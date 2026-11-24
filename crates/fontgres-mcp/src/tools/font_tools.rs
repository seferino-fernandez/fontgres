//! The font tools: list faces, list families and query a font.
//!
//! Every tool name carries the `fontgres_` prefix, so a client running several
//! MCP servers can tell this server's tools apart. Every tool reads the
//! database the server loaded at startup and changes nothing.
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
use serde::Serialize;

use fontgres_core::models::{self, Face, FontQuery, QueryError};

use super::error::ToolError;
use super::handler::AppTools;
use super::params::{ListFacesParams, ListFamiliesParams};
use super::types::Page;

impl AppTools {
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

#[tool_router(router = font_router, vis = "pub(super)")]
impl AppTools {
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
    ) -> Result<Json<Face>, ToolError> {
        query.run(&self.database).map(Json).map_err(|error| {
            let message = match error {
                QueryError::NoMatch(_) => {
                    format!("{error}; use fontgres_list_families to see the loaded families")
                }
                QueryError::NoFamilies | QueryError::WeightOutOfRange(_) => error.to_string(),
            };
            message.into()
        })
    }
}

/// Each tool's structured result, called through
/// [`ServerHandler::call_tool`](rmcp::ServerHandler::call_tool).
#[cfg(test)]
mod tests {
    use crate::tools::test_support::{
        call_tool, face, first_text, structured, tools, tools_over, tools_with_budget,
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
        let result = call_tool(&tools(), "fontgres_list_faces", json!({})).await;
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
        let tools = tools();
        let matched = structured(
            call_tool(&tools, "fontgres_list_faces", json!({ "family": "TUFFY" })).await,
        );
        assert_eq!(matched["total"], 1, "{matched}");
        assert_eq!(families_of(&matched), ["Tuffy"]);
        let other = structured(
            call_tool(&tools, "fontgres_list_faces", json!({ "family": "Other" })).await,
        );
        assert_eq!(
            other,
            json!({ "items": [], "total": 0, "next_offset": null })
        );
    }

    #[tokio::test]
    async fn list_families_lists_each_family_once() {
        let result = call_tool(&tools(), "fontgres_list_families", json!({})).await;
        assert_eq!(
            structured(result),
            json!({ "items": ["Tuffy"], "total": 1, "next_offset": null })
        );
    }

    #[tokio::test]
    async fn list_faces_pages_follow_next_offset_to_the_end() {
        let tools = tools_over(one_face_each(5), ROOMY);
        let mut offset = json!(0);
        let mut pages = Vec::new();
        while !offset.is_null() {
            assert!(pages.len() < 5, "next_offset must end the listing");
            let arguments = json!({ "offset": offset, "limit": 2 });
            let page = structured(call_tool(&tools, "fontgres_list_faces", arguments).await);
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
        let tools = tools_over(one_face_each(101), ROOMY);
        let first = structured(call_tool(&tools, "fontgres_list_faces", json!({})).await);
        assert_eq!(families_of(&first).len(), 100);
        assert_eq!(first["total"], 101);
        assert_eq!(first["next_offset"], 100);

        let all =
            structured(call_tool(&tools, "fontgres_list_faces", json!({ "limit": 500 })).await);
        assert_eq!(families_of(&all).len(), 101);
        assert_eq!(all["next_offset"], Value::Null);
    }

    #[tokio::test]
    async fn list_families_returns_1000_names_unless_a_limit_is_given() {
        let tools = tools_over(one_face_each(1001), ROOMY);
        let first = structured(call_tool(&tools, "fontgres_list_families", json!({})).await);
        assert_eq!(first["items"].as_array().map(Vec::len), Some(1000));
        assert_eq!(first["items"][0], "Family 0000");
        assert_eq!(first["total"], 1001);
        assert_eq!(first["next_offset"], 1000);

        let rest = structured(
            call_tool(
                &tools,
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
        let tools = tools_over(faces, ROOMY);
        let arguments = json!({ "family": "wanted", "offset": 1, "limit": 1 });
        let page = structured(call_tool(&tools, "fontgres_list_faces", arguments).await);
        assert_eq!(page["total"], 3, "{page}");
        assert_eq!(page["next_offset"], 2, "{page}");
        assert_eq!(page["items"][0]["weight"], 400, "{page}");
        assert_eq!(families_of(&page), ["Wanted"]);
    }

    #[tokio::test]
    async fn an_offset_past_the_end_is_an_empty_last_page() {
        let tools = tools_over(one_face_each(3), ROOMY);
        for name in ["fontgres_list_faces", "fontgres_list_families"] {
            for offset in [3, 4, usize::MAX] {
                let page = structured(call_tool(&tools, name, json!({ "offset": offset })).await);
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
        let tools = tools();
        for name in ["fontgres_list_faces", "fontgres_list_families"] {
            let result = call_tool(&tools, name, json!({ "limit": 0 })).await;
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
        let roomy = tools_over(faces.clone(), ROOMY);
        // The last ten faces make the largest page of ten: its `next_offset` is
        // null, which is longer than a two-digit offset.
        let last_ten = json!({ "offset": 30, "limit": 10 });
        let ten = call_tool(&roomy, "fontgres_list_faces", last_ten).await;
        let eleven = call_tool(&roomy, "fontgres_list_faces", json!({ "limit": 11 })).await;
        let budget = serde_json::to_vec(&ten).unwrap().len();
        assert!(serde_json::to_vec(&eleven).unwrap().len() > budget);

        let tight = tools_over(faces, budget);
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
            &tools_with_budget(300),
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
            &tools(),
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
            &tools(),
            "fontgres_query_font",
            json!({ "families": ["tuffy"] }),
        )
        .await;
        assert_eq!(structured(result)["family"], "Tuffy");
    }

    #[tokio::test]
    async fn query_font_without_a_match_is_a_tool_error_with_a_hint() {
        let result = call_tool(
            &tools(),
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
        let tools = tools();
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
            let result = call_tool(&tools, "fontgres_query_font", arguments.clone()).await;
            assert_eq!(result.is_error, Some(true), "{arguments}: {result:?}");
            assert_eq!(first_text(&result), expected, "{arguments}");
        }
    }
}
