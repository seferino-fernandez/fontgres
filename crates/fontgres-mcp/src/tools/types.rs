//! Shared result types for MCP tool handlers.

use schemars::JsonSchema;
use serde::Serialize;

/// A list result: the items plus their count.
///
/// The MCP spec requires a tool's `outputSchema` to have an `object` root, so
/// a bare array is not allowed; wrapping a `Vec<T>` as
/// `{ "items": [...], "count": N }` keeps list tools spec-compliant.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(rename = "Items")]
pub(crate) struct Listed<T> {
    /// The listed items.
    pub items: Vec<T>,
    /// The number of items in `items`.
    pub count: usize,
}

impl<T> Listed<T> {
    /// Wraps a list, recording its length as `count`.
    pub(super) fn new(items: Vec<T>) -> Self {
        Self {
            count: items.len(),
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Listed;
    use serde_json::json;

    #[test]
    fn list_carries_its_count() {
        let items = Listed::new(vec![1, 2, 3]);
        assert_eq!(
            serde_json::to_value(items).unwrap(),
            json!({ "items": [1, 2, 3], "count": 3 })
        );
    }
}
