//! Parameter structs for MCP tools. Each derives `JsonSchema` so rmcp can
//! advertise an input schema, plus `Deserialize` for argument parsing.
//!
//! Every struct denies unknown fields, so a misspelled argument fails the call
//! instead of silently falling back to a default.

use fontgres_core::models::{FontStretch, FontStyle};
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

/// Parameters for `fontgres_query_font`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryFontParams {
    /// Families in priority order. Names match exactly, case included. The CSS
    /// generic names `serif`, `sans-serif`, `cursive`, `fantasy` and
    /// `monospace` resolve through the server's configured generic families.
    #[schemars(length(min = 1))]
    pub families: Vec<String>,
    /// Wanted weight: 100 (thin) to 900 (black); 400 is normal, 700 bold.
    #[serde(default = "default_weight")]
    #[schemars(range(min = 1, max = 1000))]
    pub weight: u16,
    /// Wanted style.
    #[serde(default)]
    pub style: FontStyle,
    /// Wanted stretch.
    #[serde(default)]
    pub stretch: FontStretch,
}

/// The CSS `normal` weight.
fn default_weight() -> u16 {
    400
}

#[cfg(test)]
mod tests {
    use super::{ListFacesParams, QueryFontParams};
    use fontgres_core::models::{FontStretch, FontStyle};
    use schemars::schema_for;
    use serde_json::json;

    #[test]
    fn every_params_struct_denies_unknown_fields() {
        let unknown = |result: Result<(), serde_json::Error>| {
            let message = result.expect_err("an unknown field is refused").to_string();
            assert!(message.contains("unknown field `extra`"), "{message}");
        };
        unknown(serde_json::from_value::<ListFacesParams>(json!({ "extra": 1 })).map(drop));
        unknown(
            serde_json::from_value::<QueryFontParams>(json!({ "families": ["A"], "extra": 1 }))
                .map(drop),
        );
    }

    #[test]
    fn query_params_default_to_a_normal_face() {
        let params: QueryFontParams =
            serde_json::from_value(json!({ "families": ["Tuffy"] })).unwrap();
        assert_eq!(params.weight, 400);
        assert_eq!(params.style, FontStyle::Normal);
        assert_eq!(params.stretch, FontStretch::Normal);
    }

    #[test]
    fn query_params_read_css_keywords() {
        let params: QueryFontParams = serde_json::from_value(json!({
            "families": ["Tuffy"], "style": "oblique", "stretch": "semi-expanded"
        }))
        .unwrap();
        assert_eq!(params.style, FontStyle::Oblique);
        assert_eq!(params.stretch, FontStretch::SemiExpanded);
    }

    #[test]
    fn query_schema_bounds_weight_and_requires_a_family() {
        let schema = serde_json::to_value(schema_for!(QueryFontParams)).unwrap();
        let properties = &schema["properties"];
        assert_eq!(properties["weight"]["minimum"], json!(1));
        assert_eq!(properties["weight"]["maximum"], json!(1000));
        assert_eq!(properties["families"]["minItems"], json!(1));
        assert_eq!(schema["required"], json!(["families"]));
    }
}
