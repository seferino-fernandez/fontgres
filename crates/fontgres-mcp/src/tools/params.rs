//! Parameter structs for MCP tools. Each derives `JsonSchema` so rmcp can
//! advertise an input schema, plus `Deserialize` for argument parsing.
//! `fontgres_query_font` takes core's
//! [`FontQuery`](fontgres_core::models::FontQuery) as its parameters.
//!
//! Every struct denies unknown fields, so a misspelled argument fails the call
//! instead of silently falling back to a default.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::Deserialize;

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
pub(super) struct ListFacesParams {
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
pub(super) struct ListFamiliesParams {
    /// How many family names to skip. Pass a page's `next_offset` to get the
    /// page after it.
    #[serde(default)]
    pub offset: usize,
    /// The most family names to return. A page holds fewer when more would
    /// exceed the server's response size limit.
    #[serde(default = "default_family_limit")]
    pub limit: NonZeroUsize,
}

#[cfg(test)]
mod tests {
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
