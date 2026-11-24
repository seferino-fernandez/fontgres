//! The page a list tool returns, and the cut that fits it to the response
//! budget.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Serialize;

use super::schema::serialized_len;

/// One page of a listing.
///
/// The MCP spec requires a tool's `outputSchema` to have an `object` root,
/// so a list tool returns its items inside this object and not as a bare
/// array.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(rename = "Page")]
pub(crate) struct Page<'a, T> {
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
    /// the result is too, and
    /// [`enforce_response_budget`](super::schema::enforce_response_budget)
    /// replaces it.
    pub(super) fn cut(
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

#[cfg(test)]
mod tests {
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
