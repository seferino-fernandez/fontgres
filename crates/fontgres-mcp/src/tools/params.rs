//! Parameter structs for MCP tools. Each derives `JsonSchema` so rmcp can
//! advertise an input schema, plus `Deserialize` for argument parsing.
//! `fontgres_query_font` takes core's
//! [`FontQuery`](fontgres_core::models::FontQuery) as its parameters.
//!
//! Every struct denies unknown fields, so a misspelled argument fails the call
//! instead of silently falling back to a default.

use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for `fontgres_list_faces`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListFacesParams {
    /// Only list faces of this family (any of its names, case-insensitive).
    #[serde(default)]
    pub family: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::ListFacesParams;
    use serde_json::json;

    #[test]
    fn every_params_struct_denies_unknown_fields() {
        let message = serde_json::from_value::<ListFacesParams>(json!({ "extra": 1 }))
            .expect_err("an unknown field is refused")
            .to_string();
        assert!(message.contains("unknown field `extra`"), "{message}");
    }
}
