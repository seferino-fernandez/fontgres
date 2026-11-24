//! The font tools: list faces, list families and query a font, with their
//! parameters and the page the list tools return.
//!
//! Every tool name carries the `fontgres_` prefix, so a client running several
//! MCP servers can tell this server's tools apart. Every tool reads the
//! database the server loaded at startup and changes nothing.
//!
//! Each parameter type derives `JsonSchema`, which rmcp advertises as the
//! tool's input schema, and `Deserialize`, which reads the arguments. Each
//! denies unknown fields, so a misspelled argument fails the call instead of
//! falling back to a default. `fontgres_query_font` takes core's
//! [`FontQuery`] as its parameters.
//!
//! The two list tools return one [`Page`] a call. They sort the whole
//! listing on every call and copy out only the page, which is cut to fit the
//! response budget.

use std::num::NonZeroUsize;

use rmcp::{
    ErrorData as McpError,
    handler::server::{
        tool::schema_for_output,
        wrapper::{Json, Parameters},
    },
    model::CallToolResult,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use fontgres_core::Database;
use fontgres_core::models::{self, Face, FontQuery, QueryError};

/// What the tools answer from: the fonts loaded at startup, and the size a
/// tool result may have.
pub(crate) struct FontTools {
    /// The fonts every tool reads.
    database: Database,
    /// The most bytes a serialized tool result may take. The list tools cut
    /// each page to it.
    max_response_bytes: usize,
}

impl FontTools {
    /// Tools over an already loaded font database, with results of at most
    /// `max_response_bytes`; see
    /// [`McpConfig::max_response_bytes`](crate::config::McpConfig).
    pub(crate) fn new(database: Database, max_response_bytes: usize) -> Self {
        Self {
            database,
            max_response_bytes,
        }
    }

    /// The most bytes a serialized tool result may take.
    pub(crate) fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }

    /// Returns the page of `listing` that starts at `offset` and holds at
    /// most `limit` items, each as `view` shows it, cut to the response
    /// budget. An offset past the end gives an empty last page.
    fn page<S, T: Serialize>(
        &self,
        listing: &[S],
        offset: usize,
        limit: NonZeroUsize,
        view: impl Fn(&S) -> T,
    ) -> Result<CallToolResult, McpError> {
        let wanted: Vec<T> = listing
            .iter()
            .skip(offset)
            .take(limit.get())
            .map(view)
            .collect();
        Page::cut(&wanted, offset, listing.len(), self.max_response_bytes)
    }
}

#[tool_router(vis = "pub(crate)")]
impl FontTools {
    #[tool(
        name = "fontgres_list_faces",
        description = "List the loaded font faces a page at a time, optionally only one family",
        output_schema = schema_for_output::<Page<'static, Face>>(),
        annotations(
            title = "List font faces",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_faces(
        &self,
        Parameters(params): Parameters<ListFacesParams>,
    ) -> Result<CallToolResult, McpError> {
        let faces = models::faces(&self.database, params.family.as_deref());
        self.page(&faces, params.offset, params.limit, |info| {
            Face::from(*info)
        })
    }

    #[tool(
        name = "fontgres_list_families",
        description = "List the distinct font family names loaded, a page at a time",
        output_schema = schema_for_output::<Page<'static, String>>(),
        annotations(
            title = "List font families",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_families(
        &self,
        Parameters(params): Parameters<ListFamiliesParams>,
    ) -> Result<CallToolResult, McpError> {
        let names = models::family_names(&self.database);
        self.page(&names, params.offset, params.limit, |name| *name)
    }

    #[tool(
        name = "fontgres_query_font",
        description = "Find the face that best matches a CSS-like font query",
        annotations(
            title = "Query a font",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query_font(
        &self,
        Parameters(query): Parameters<FontQuery>,
    ) -> Result<Json<Face>, String> {
        query
            .run(&self.database)
            .map(Json)
            .map_err(|error| match error {
                QueryError::NoMatch(_) => {
                    format!("{error}; use fontgres_list_families to see the loaded families")
                }
                QueryError::NoFamilies | QueryError::WeightOutOfRange(_) => error.to_string(),
            })
    }
}

/// The faces in a page of `fontgres_list_faces` when a call names no
/// `limit`: about 55 KB of result.
const DEFAULT_FACE_LIMIT: NonZeroUsize = NonZeroUsize::new(100).unwrap();

/// The names in a page of `fontgres_list_families` when a call names no
/// `limit`. A machine's families are few and their names short, so one page
/// usually holds them all.
const DEFAULT_FAMILY_LIMIT: NonZeroUsize = NonZeroUsize::new(1000).unwrap();

fn default_face_limit() -> NonZeroUsize {
    DEFAULT_FACE_LIMIT
}

fn default_family_limit() -> NonZeroUsize {
    DEFAULT_FAMILY_LIMIT
}

/// Parameters for `fontgres_list_faces`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFacesParams {
    /// Only list faces of this family (any of its names, case-insensitive).
    #[serde(default)]
    pub family: Option<String>,
    /// How many faces to skip. Pass a page's `next_offset` to get the page
    /// after it.
    #[serde(default)]
    pub offset: usize,
    /// The most faces to return. A page holds fewer when more would exceed
    /// the server's response size limit.
    #[serde(default = "default_face_limit")]
    pub limit: NonZeroUsize,
}

/// Parameters for `fontgres_list_families`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFamiliesParams {
    /// How many family names to skip. Pass a page's `next_offset` to get the
    /// page after it.
    #[serde(default)]
    pub offset: usize,
    /// The most family names to return. A page holds fewer when more would
    /// exceed the server's response size limit.
    #[serde(default = "default_family_limit")]
    pub limit: NonZeroUsize,
}

/// One page of a listing.
///
/// The MCP spec requires a tool's `outputSchema` to have an `object` root,
/// so a list tool returns its items inside this object and not as a bare
/// array.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(rename = "Page")]
struct Page<'a, T> {
    /// The items on this page, in listing order.
    pub items: &'a [T],
    /// How many items the listing holds, on this page and every other.
    pub total: usize,
    /// The `offset` that gets the next page, or null on the last page.
    pub next_offset: Option<usize>,
}

impl<T: Serialize> Page<'_, T> {
    /// Returns the tool result for the page that starts `offset` items into
    /// a listing of `total` items.
    ///
    /// `wanted` holds the items the caller asked for, from `offset` on. The
    /// page holds the longest run of them, from the first, whose result is
    /// at most `max_bytes` when serialized. It always holds the first one,
    /// so the next page starts further on: when that one alone is too large,
    /// the result is too, and the server's budget guard replaces it.
    fn cut(
        wanted: &[T],
        offset: usize,
        total: usize,
        max_bytes: usize,
    ) -> Result<CallToolResult, McpError> {
        let result_of = |count: usize| {
            let end = offset.saturating_add(count);
            let page = Page {
                items: wanted.get(..count).unwrap_or(wanted),
                total,
                next_offset: (end < total).then_some(end),
            };
            serde_json::to_value(&page)
                .map(CallToolResult::structured)
                .map_err(|_error| {
                    McpError::internal_error("failed to serialize tool response", None)
                })
        };
        let everything = result_of(wanted.len())?;
        if wanted.len() <= 1 || serialized_len(&everything)? <= max_bytes {
            return Ok(everything);
        }
        // `short` items fit, or are the one item a page always holds, and
        // `long` items do not fit. A result grows with its items, so the
        // longest run that fits lies between them. Each round halves the gap
        // between the two, so as many rounds as a `usize` has bits close it.
        let (mut short, mut long) = (1_usize, wanted.len());
        // The result for `short` items, once a run has been found to fit.
        let mut fitting = None;
        for _ in 0..usize::BITS {
            let middle = short.midpoint(long);
            if middle == short {
                break;
            }
            let result = result_of(middle)?;
            if serialized_len(&result)? <= max_bytes {
                short = middle;
                fitting = Some(result);
            } else {
                long = middle;
            }
        }
        fitting.map_or_else(|| result_of(short), Ok)
    }
}

/// The length of `result` serialized as JSON, as it goes on the wire. The page
/// cut and the server's budget guard both measure with it.
pub(crate) fn serialized_len(result: &CallToolResult) -> Result<usize, McpError> {
    serde_json::to_vec(result)
        .map(|bytes| bytes.len())
        .map_err(|_error| McpError::internal_error("failed to measure tool response", None))
}

/// Each tool's result, called through
/// [`ServerHandler::call_tool`](rmcp::ServerHandler::call_tool).
#[cfg(test)]
mod tests {
    use crate::test_support::{
        call_tool, face, first_text, server, server_over, server_with_budget, structured,
    };
    use fontgres_core::FaceInfo;
    use serde_json::{Value, json};

    /// The production default response budget, which none of these listings
    /// reaches.
    const ROOMY: usize = 1024 * 1024;

    /// `count` faces, one to a family, named so the listing is in the order
    /// they are given: `Family 0000`, `Family 0001` and so on.
    fn one_face_each(count: usize) -> Vec<FaceInfo> {
        (0..count)
            .map(|number| face(&format!("Family {number:04}"), 400))
            .collect()
    }

    fn families_of(page: &Value) -> Vec<&str> {
        page["items"]
            .as_array()
            .expect("a page has items")
            .iter()
            .map(|face| face["family"].as_str().expect("a face has a family"))
            .collect()
    }

    #[tokio::test]
    async fn list_faces_lists_every_loaded_face() {
        let result = call_tool(&server(), "fontgres_list_faces", json!({})).await;
        let body = structured(result);
        assert_eq!(body["total"], 1, "{body}");
        assert_eq!(body["next_offset"], Value::Null, "{body}");
        let face = &body["items"][0];
        assert_eq!(face["family"], "Tuffy");
        assert_eq!(face["weight"], 500);
        assert_eq!(face["style"], "normal");
        assert_eq!(face["stretch"], "normal");
        assert!(
            face["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("Tuffy.ttf"))
        );
    }

    #[tokio::test]
    async fn list_faces_family_filter_ignores_case() {
        let server = server();
        let matched = structured(
            call_tool(&server, "fontgres_list_faces", json!({ "family": "TUFFY" })).await,
        );
        assert_eq!(matched["total"], 1, "{matched}");
        assert_eq!(families_of(&matched), ["Tuffy"]);
        let other = structured(
            call_tool(&server, "fontgres_list_faces", json!({ "family": "Other" })).await,
        );
        assert_eq!(
            other,
            json!({ "items": [], "total": 0, "next_offset": null })
        );
    }

    #[tokio::test]
    async fn list_families_lists_each_family_once() {
        let result = call_tool(&server(), "fontgres_list_families", json!({})).await;
        assert_eq!(
            structured(result),
            json!({ "items": ["Tuffy"], "total": 1, "next_offset": null })
        );
    }

    #[tokio::test]
    async fn list_faces_pages_follow_next_offset_to_the_end() {
        let server = server_over(one_face_each(5), ROOMY);
        let mut offset = json!(0);
        let mut pages = Vec::new();
        while !offset.is_null() {
            assert!(pages.len() < 5, "next_offset must end the listing");
            let arguments = json!({ "offset": offset, "limit": 2 });
            let page = structured(call_tool(&server, "fontgres_list_faces", arguments).await);
            assert_eq!(page["total"], 5, "{page}");
            offset = page["next_offset"].clone();
            pages.push(page);
        }
        let families: Vec<Vec<&str>> = pages.iter().map(families_of).collect();
        assert_eq!(
            families,
            [
                vec!["Family 0000", "Family 0001"],
                vec!["Family 0002", "Family 0003"],
                vec!["Family 0004"],
            ]
        );
        let offsets: Vec<&Value> = pages.iter().map(|page| &page["next_offset"]).collect();
        assert_eq!(offsets, [&json!(2), &json!(4), &Value::Null]);
    }

    #[tokio::test]
    async fn list_faces_returns_100_faces_unless_a_limit_is_given() {
        let server = server_over(one_face_each(101), ROOMY);
        let first = structured(call_tool(&server, "fontgres_list_faces", json!({})).await);
        assert_eq!(families_of(&first).len(), 100);
        assert_eq!(first["total"], 101);
        assert_eq!(first["next_offset"], 100);

        let all =
            structured(call_tool(&server, "fontgres_list_faces", json!({ "limit": 500 })).await);
        assert_eq!(families_of(&all).len(), 101);
        assert_eq!(all["next_offset"], Value::Null);
    }

    #[tokio::test]
    async fn list_families_returns_1000_names_unless_a_limit_is_given() {
        let server = server_over(one_face_each(1001), ROOMY);
        let first = structured(call_tool(&server, "fontgres_list_families", json!({})).await);
        assert_eq!(first["items"].as_array().map(Vec::len), Some(1000));
        assert_eq!(first["items"][0], "Family 0000");
        assert_eq!(first["total"], 1001);
        assert_eq!(first["next_offset"], 1000);

        let rest = structured(
            call_tool(
                &server,
                "fontgres_list_families",
                json!({ "offset": 1000, "limit": 1 }),
            )
            .await,
        );
        assert_eq!(
            rest,
            json!({ "items": ["Family 1000"], "total": 1001, "next_offset": null })
        );
    }

    /// `total` counts the faces of the family asked for, not every face.
    #[tokio::test]
    async fn list_faces_pages_one_family() {
        let mut faces = one_face_each(3);
        faces.extend([300, 400, 700].map(|weight| face("Wanted", weight)));
        let server = server_over(faces, ROOMY);
        let arguments = json!({ "family": "wanted", "offset": 1, "limit": 1 });
        let page = structured(call_tool(&server, "fontgres_list_faces", arguments).await);
        assert_eq!(page["total"], 3, "{page}");
        assert_eq!(page["next_offset"], 2, "{page}");
        assert_eq!(page["items"][0]["weight"], 400, "{page}");
        assert_eq!(families_of(&page), ["Wanted"]);
    }

    #[tokio::test]
    async fn an_offset_past_the_end_is_an_empty_last_page() {
        let server = server_over(one_face_each(3), ROOMY);
        for name in ["fontgres_list_faces", "fontgres_list_families"] {
            for offset in [3, 4, usize::MAX] {
                let page = structured(call_tool(&server, name, json!({ "offset": offset })).await);
                assert_eq!(
                    page,
                    json!({ "items": [], "total": 3, "next_offset": null }),
                    "{name} from {offset}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_limit_of_zero_is_refused_when_the_arguments_are_read() {
        let server = server();
        for name in ["fontgres_list_faces", "fontgres_list_families"] {
            let result = call_tool(&server, name, json!({ "limit": 0 })).await;
            assert_eq!(result.is_error, Some(true), "{name}: {result:?}");
            let text = first_text(&result);
            assert!(
                text.starts_with("failed to deserialize parameters: ")
                    && text.contains("expected a nonzero usize"),
                "{name}: {text}"
            );
        }
    }

    /// A page stops where one more face would exceed the budget, and the
    /// pages still add up to the whole listing.
    #[tokio::test]
    async fn pages_are_cut_to_the_response_budget_and_lose_no_face() {
        let faces = one_face_each(40);
        let roomy = server_over(faces.clone(), ROOMY);
        // The last ten faces make the largest page of ten: its `next_offset` is
        // null, which is longer than a two-digit offset.
        let last_ten = json!({ "offset": 30, "limit": 10 });
        let ten = call_tool(&roomy, "fontgres_list_faces", last_ten).await;
        let eleven = call_tool(&roomy, "fontgres_list_faces", json!({ "limit": 11 })).await;
        let budget = serde_json::to_vec(&ten).unwrap().len();
        assert!(serde_json::to_vec(&eleven).unwrap().len() > budget);

        let tight = server_over(faces, budget);
        let mut offset = json!(0);
        let mut listed = Vec::new();
        let mut lengths = Vec::new();
        while !offset.is_null() {
            assert!(lengths.len() < 40, "next_offset must end the listing");
            let arguments = json!({ "offset": offset, "limit": 40 });
            let result = call_tool(&tight, "fontgres_list_faces", arguments).await;
            assert!(serde_json::to_vec(&result).unwrap().len() <= budget);
            let page = structured(result);
            assert_eq!(page["total"], 40, "{page}");
            lengths.push(families_of(&page).len());
            listed.extend(families_of(&page).into_iter().map(str::to_owned));
            offset = page["next_offset"].clone();
        }
        assert_eq!(lengths, [10, 10, 10, 10]);
        let whole =
            structured(call_tool(&roomy, "fontgres_list_faces", json!({ "limit": 40 })).await);
        assert_eq!(listed, families_of(&whole));
    }

    /// One face larger than the budget is still the page, and the budget
    /// guard then replaces the result.
    #[tokio::test]
    async fn one_face_larger_than_the_budget_is_reported_as_too_large() {
        let result = call_tool(
            &server_with_budget(300),
            "fontgres_list_faces",
            json!({ "limit": 1 }),
        )
        .await;
        let body: Value = serde_json::from_str(first_text(&result)).unwrap();
        assert_eq!(body["error"], "response_too_large", "{body}");
        assert_eq!(
            body["hint"],
            "one result alone exceeds the limit; raise max_response_bytes"
        );
    }

    #[tokio::test]
    async fn query_font_returns_the_best_match() {
        let result = call_tool(
            &server(),
            "fontgres_query_font",
            json!({ "families": ["Missing", "Tuffy"], "weight": 700, "style": "italic" }),
        )
        .await;
        let face = structured(result);
        assert_eq!(face["family"], "Tuffy");
        assert_eq!(face["post_script_name"], "Tuffy");
    }

    #[tokio::test]
    async fn query_font_matches_family_names_ignoring_case() {
        let result = call_tool(
            &server(),
            "fontgres_query_font",
            json!({ "families": ["tuffy"] }),
        )
        .await;
        assert_eq!(structured(result)["family"], "Tuffy");
    }

    #[tokio::test]
    async fn query_font_without_a_match_is_a_tool_error_with_a_hint() {
        let result = call_tool(
            &server(),
            "fontgres_query_font",
            json!({ "families": ["Missing"] }),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "{result:?}");
        assert_eq!(
            first_text(&result),
            "no face matches Missing; use fontgres_list_families to see the loaded families"
        );
    }

    #[tokio::test]
    async fn query_font_rejects_no_families_and_out_of_range_weights() {
        let server = server();
        for (arguments, expected) in [
            (
                json!({ "families": [] }),
                "families must name at least one family",
            ),
            (
                json!({ "families": ["Tuffy"], "weight": 0 }),
                "weight must be between 1 and 1000, got 0",
            ),
            (
                json!({ "families": ["Tuffy"], "weight": 1001 }),
                "weight must be between 1 and 1000, got 1001",
            ),
        ] {
            let result = call_tool(&server, "fontgres_query_font", arguments.clone()).await;
            assert_eq!(result.is_error, Some(true), "{arguments}: {result:?}");
            assert_eq!(first_text(&result), expected, "{arguments}");
        }
    }
}

#[cfg(test)]
mod params_tests {
    use super::{ListFacesParams, ListFamiliesParams};
    use serde_json::json;

    #[test]
    fn every_params_struct_denies_unknown_fields() {
        let unknown = |result: Result<(), serde_json::Error>| {
            let message = result.expect_err("an unknown field is refused").to_string();
            assert!(message.contains("unknown field `extra`"), "{message}");
        };
        unknown(serde_json::from_value::<ListFacesParams>(json!({ "extra": 1 })).map(drop));
        unknown(serde_json::from_value::<ListFamiliesParams>(json!({ "extra": 1 })).map(drop));
    }

    #[test]
    fn list_params_default_to_the_first_page() {
        let faces: ListFacesParams = serde_json::from_value(json!({})).unwrap();
        assert_eq!((faces.family, faces.offset), (None, 0));
        assert_eq!(faces.limit.get(), 100);
        let families: ListFamiliesParams = serde_json::from_value(json!({})).unwrap();
        assert_eq!(families.offset, 0);
        assert_eq!(families.limit.get(), 1000);
    }

    #[test]
    fn list_schemas_advertise_the_default_limit_and_refuse_zero() {
        for (schema, limit) in [
            (schemars::schema_for!(ListFacesParams), 100),
            (schemars::schema_for!(ListFamiliesParams), 1000),
        ] {
            let schema = serde_json::to_value(schema).unwrap();
            let properties = &schema["properties"];
            assert_eq!(properties["limit"]["default"], json!(limit));
            assert_eq!(properties["limit"]["minimum"], json!(1));
            assert_eq!(properties["offset"]["default"], json!(0));
        }
    }
}

#[cfg(test)]
mod page_tests {
    use super::Page;
    use rmcp::model::CallToolResult;
    use serde_json::{Value, json};

    /// Ten items of one size, so every result one item longer is longer by
    /// the same number of bytes.
    const ITEMS: [&str; 10] = ["item"; 10];

    fn cut(wanted: &[&str], offset: usize, total: usize, max_bytes: usize) -> CallToolResult {
        Page::cut(wanted, offset, total, max_bytes).expect("a page of strings serializes")
    }

    fn body(result: &CallToolResult) -> &Value {
        result
            .structured_content
            .as_ref()
            .expect("a page is structured content")
    }

    fn size(result: &CallToolResult) -> usize {
        serde_json::to_vec(result).unwrap().len()
    }

    #[test]
    fn a_page_that_fits_holds_everything_wanted() {
        let result = cut(&["a", "b", "c"], 0, 3, usize::MAX);
        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            *body(&result),
            json!({ "items": ["a", "b", "c"], "total": 3, "next_offset": null })
        );
    }

    /// The same page is in the text block, for clients that read only text.
    #[test]
    fn a_page_is_also_its_text_content() {
        let result = cut(&["a"], 0, 1, usize::MAX);
        let text = result.content.first().and_then(|block| block.as_text());
        let parsed: Value = serde_json::from_str(&text.unwrap().text).unwrap();
        assert_eq!(parsed, *body(&result));
    }

    #[test]
    fn next_offset_is_where_the_page_ends_until_the_listing_does() {
        let middle = cut(&["c", "d"], 2, 5, usize::MAX);
        assert_eq!(
            *body(&middle),
            json!({ "items": ["c", "d"], "total": 5, "next_offset": 4 })
        );
        let last = cut(&["d", "e"], 3, 5, usize::MAX);
        assert_eq!(body(&last)["next_offset"], Value::Null);
    }

    #[test]
    fn nothing_wanted_is_an_empty_last_page() {
        let result = cut(&[], 9, 5, usize::MAX);
        assert_eq!(
            *body(&result),
            json!({ "items": [], "total": 5, "next_offset": null })
        );
    }

    /// For every length a page can have, a budget of exactly that page's
    /// size gives that page, and one byte less gives the page one item
    /// shorter.
    #[test]
    fn a_page_is_the_longest_run_that_fits_the_budget() {
        for count in 1..=ITEMS.len() {
            // A listing longer than what is wanted, so every page has a
            // `next_offset`.
            let exact = size(&cut(&ITEMS[..count], 0, 100, usize::MAX));

            let at_the_budget = cut(&ITEMS, 0, 100, exact);
            assert_eq!(
                *body(&at_the_budget),
                json!({ "items": ITEMS[..count], "total": 100, "next_offset": count }),
                "a budget of {exact} bytes"
            );
            assert_eq!(size(&at_the_budget), exact);

            let below = cut(&ITEMS, 0, 100, exact - 1);
            let expected = (count - 1).max(1);
            assert_eq!(
                body(&below)["items"].as_array().map(Vec::len),
                Some(expected),
                "a budget of {} bytes",
                exact - 1
            );
            assert_eq!(body(&below)["next_offset"], json!(expected));
        }
    }

    /// A cut page of a listing that ends with what was wanted still says
    /// where the rest starts.
    #[test]
    fn a_cut_page_of_the_last_items_has_a_next_offset() {
        let two = size(&cut(&ITEMS[..2], 90, 100, usize::MAX));
        let result = cut(&ITEMS, 90, 100, two);
        assert_eq!(
            *body(&result),
            json!({ "items": ["item", "item"], "total": 100, "next_offset": 92 })
        );
    }

    /// One item is returned whatever the budget, so a caller following
    /// `next_offset` always moves on.
    #[test]
    fn a_page_holds_one_item_even_when_it_does_not_fit() {
        for wanted in [&ITEMS[..1], &ITEMS[..2], &ITEMS[..]] {
            let result = cut(wanted, 0, 100, 0);
            assert_eq!(
                *body(&result),
                json!({ "items": ["item"], "total": 100, "next_offset": 1 })
            );
        }
    }
}
