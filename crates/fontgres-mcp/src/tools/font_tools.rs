//! The font tools: list faces, list families and query a font.
//!
//! Every tool name carries the `fontgres_` prefix, so a client running several
//! MCP servers can tell this server's tools apart. Every tool reads the
//! database the server loaded at startup and changes nothing.

use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};

use fontgres_core::models::{self, Face, FontQuery};

use super::error::ToolError;
use super::handler::AppTools;
use super::params::{ListFacesParams, QueryFontParams};
use super::types::Listed;

#[tool_router(router = font_router, vis = "pub(super)")]
impl AppTools {
    #[tool(
        name = "fontgres_list_faces",
        description = "List the loaded font faces, optionally only one family",
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
    ) -> Json<Listed<Face>> {
        Json(Listed::new(models::faces(
            &self.database,
            params.family.as_deref(),
        )))
    }

    #[tool(
        name = "fontgres_list_families",
        description = "List the distinct font family names loaded",
        annotations(
            title = "List font families",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_families(&self) -> Json<Listed<String>> {
        Json(Listed::new(models::family_names(&self.database)))
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
        Parameters(params): Parameters<QueryFontParams>,
    ) -> Result<Json<Face>, ToolError> {
        if params.families.is_empty() {
            return Err("families must name at least one family".to_string().into());
        }
        if !(1..=1000).contains(&params.weight) {
            return Err(format!("weight must be between 1 and 1000, got {}", params.weight).into());
        }
        let query = FontQuery {
            families: params.families,
            weight: params.weight,
            style: params.style,
            stretch: params.stretch,
        };
        query.run(&self.database).map(Json).ok_or_else(|| {
            format!(
                "no face matches {}; use fontgres_list_families to see the loaded families",
                query.families.join(", ")
            )
            .into()
        })
    }
}

/// Each tool's structured result, called through
/// [`ServerHandler::call_tool`](rmcp::ServerHandler::call_tool).
#[cfg(test)]
mod tests {
    use crate::tools::test_support::{call_tool, first_text, structured, tools};
    use serde_json::json;

    #[tokio::test]
    async fn list_faces_lists_every_loaded_face() {
        let result = call_tool(&tools(), "fontgres_list_faces", json!({})).await;
        let body = structured(result);
        assert_eq!(body["count"], 1, "{body}");
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
        assert_eq!(matched["count"], 1, "{matched}");
        let other = structured(
            call_tool(&tools, "fontgres_list_faces", json!({ "family": "Other" })).await,
        );
        assert_eq!(other, json!({ "items": [], "count": 0 }));
    }

    #[tokio::test]
    async fn list_families_lists_each_family_once() {
        let result = call_tool(&tools(), "fontgres_list_families", json!({})).await;
        assert_eq!(
            structured(result),
            json!({ "items": ["Tuffy"], "count": 1 })
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
