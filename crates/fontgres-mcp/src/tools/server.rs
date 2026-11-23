//! The rmcp [`ServerHandler`] implementation for [`AppTools`]: server info,
//! tool listing, and the checks every tool call goes through.

use std::sync::Arc;

use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, Tool,
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::{ListToolsResult, ServerConfig},
    service::RequestContext,
};
use serde_json::{Map, Value};

use super::handler::AppTools;
use super::schema::{close_object_map, enforce_response_budget};

/// Hardens a tool's advertised schemas in place: closes the input schema and,
/// when present, the output schema (sets `additionalProperties: false` and an
/// explicit `required` on every object node). Called once per tool by
/// [`AppTools::new`], so `list_tools`, `get_tool` and `call_tool` all read the
/// same closed schemas.
pub(super) fn close_tool_schemas(tool: &mut Tool) {
    close_object_map(Arc::make_mut(&mut tool.input_schema));
    if let Some(output) = tool.output_schema.as_mut() {
        close_object_map(Arc::make_mut(output));
    }
}

/// The properties a tool's input schema declares; empty for a tool that
/// takes no arguments.
fn declared_properties(tool: &Tool) -> Option<&Map<String, Value>> {
    tool.input_schema
        .get("properties")
        .and_then(Value::as_object)
}

/// Rejects a call that sends a top-level argument key the tool's input schema
/// does not declare, including any key at all for a tool that takes none.
///
/// rmcp never deserializes the arguments of a tool without a params type, so
/// `deny_unknown_fields` cannot catch a stray key there. The check is
/// top-level only; params structs deny unknown nested fields themselves.
///
/// The result matches rmcp's own params deserialization failure: a tool error
/// whose text is `failed to deserialize parameters: ` plus serde's
/// unknown-field message. The expected fields are listed in schema order,
/// which may differ from serde's declaration order.
fn undeclared_argument_rejection(
    tool: &Tool,
    request: &CallToolRequestParams,
) -> Option<CallToolResult> {
    let declared = declared_properties(tool);
    let key = request
        .arguments
        .as_ref()?
        .keys()
        .find(|key| declared.is_none_or(|properties| !properties.contains_key(*key)))?;
    let expected: Vec<&String> = declared.into_iter().flat_map(Map::keys).collect();
    let expected = match expected.as_slice() {
        [] => "there are no fields".to_owned(),
        [only] => format!("expected `{only}`"),
        [first, second] => format!("expected `{first}` or `{second}`"),
        fields => {
            let fields: Vec<String> = fields.iter().map(|field| format!("`{field}`")).collect();
            format!("expected one of {}", fields.join(", "))
        }
    };
    Some(CallToolResult::error(vec![ContentBlock::text(format!(
        "failed to deserialize parameters: unknown field `{key}`, {expected}"
    ))]))
}

impl ServerHandler for AppTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        // Named explicitly rather than taken from `ProtocolVersion::LATEST`,
        // which follows the SDK and may move past what this server implements.
        info.protocol_version = ProtocolVersion::V_2026_07_28;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            .with_description(env!("CARGO_PKG_DESCRIPTION"));
        info.instructions = Some(include_str!("instructions.md").to_string());
        info
    }

    /// Advertises every tool to every caller.
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the handler builds its response only when the returned future is polled"
    )]
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.advertised.clone())
            .with_ttl_ms(300_000)
            .with_cache_scope(CacheScope::Private))
    }

    /// Runs a tool call through the argument check, the tool itself and the
    /// response budget. An unknown tool name skips the
    /// checks and gets the router's own error.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        tracing::info!(tool = %request.name, "call_tool");
        if let Some(tool) = self.router.get(&request.name)
            && let Some(refusal) = undeclared_argument_rejection(tool, &request)
        {
            return Ok(refusal.into());
        }
        let tool_call_context = ToolCallContext::new(self, request, context);
        let response = self.router.call(tool_call_context).await?;
        if let CallToolResponse::Complete(result) = response {
            Ok(enforce_response_budget(result, self.max_response_bytes)?.into())
        } else {
            Ok(response)
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.router.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::AppTools;
    use crate::tools::test_support::{
        Caller, call, complete, context, first_text, tools, tools_with_budget,
    };
    use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
    use rmcp::{ServerHandler, model::ToolAnnotations};
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    use std::sync::Arc;

    /// A valid call for every tool, as `(name, arguments)`.
    const VALID_CALLS: [(&str, &str); 3] = [
        ("fontgres_list_faces", r"{}"),
        ("fontgres_list_families", r"{}"),
        ("fontgres_query_font", r#"{"families":["Tuffy"]}"#),
    ];

    fn valid_arguments(name: &str) -> Value {
        let (_, raw) = VALID_CALLS
            .iter()
            .find(|(tool, _)| *tool == name)
            .unwrap_or_else(|| panic!("no valid call for {name}"));
        serde_json::from_str(raw).unwrap()
    }

    async fn listed_tools(tools: &AppTools) -> Vec<Tool> {
        ServerHandler::list_tools(tools, None, context(tools, Caller::Stdio))
            .await
            .expect("tools/list succeeds")
            .tools
    }

    fn all_tool_names(tools: &AppTools) -> Vec<String> {
        tools
            .router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }

    fn is_read_only(tool: &Tool) -> bool {
        tool.annotations
            .as_ref()
            .and_then(|annotations: &ToolAnnotations| annotations.read_only_hint)
            == Some(true)
    }

    fn assert_succeeded(result: &CallToolResult, label: &str) {
        assert_ne!(result.is_error, Some(true), "{label}: {result:?}");
    }

    /// Every listed tool rejects an argument key its input schema does not
    /// declare. The list comes from `tools/list`, so a new tool whose params
    /// accept unknown keys fails here without this test being edited.
    #[tokio::test]
    async fn every_listed_tool_rejects_an_unknown_key() {
        let tools = tools();
        let listed = listed_tools(&tools).await;
        assert_eq!(listed.len(), VALID_CALLS.len());

        for tool in &listed {
            let name = tool.name.as_ref();
            assert_succeeded(
                &complete(call(&tools, Caller::Stdio, name, valid_arguments(name)).await),
                name,
            );
            let mut arguments = valid_arguments(name);
            arguments["unexpected_key"] = json!(true);
            let result = complete(call(&tools, Caller::Stdio, name, arguments).await);
            let text = first_text(&result);
            assert_eq!(result.is_error, Some(true), "{name}: {result:?}");
            assert!(
                text.starts_with("failed to deserialize parameters: ")
                    && text.contains("unknown field `unexpected_key`"),
                "{name}: {text}"
            );
        }
    }

    #[tokio::test]
    async fn zero_argument_tool_rejects_any_key_but_accepts_empty_or_absent_arguments() {
        let tools = tools();

        let result = complete(
            call(
                &tools,
                Caller::Stdio,
                "fontgres_list_families",
                json!({ "foo": 1 }),
            )
            .await,
        );
        assert_eq!(
            first_text(&result),
            "failed to deserialize parameters: unknown field `foo`, there are no fields"
        );

        let empty =
            complete(call(&tools, Caller::Stdio, "fontgres_list_families", json!({})).await);
        let absent = complete(
            ServerHandler::call_tool(
                &tools,
                CallToolRequestParams::new("fontgres_list_families"),
                context(&tools, Caller::Stdio),
            )
            .await,
        );
        assert_succeeded(&empty, "empty arguments");
        assert_succeeded(&absent, "absent arguments");
    }

    #[tokio::test]
    async fn unknown_key_message_lists_the_declared_fields() {
        let tools = tools();

        for (name, arguments, expected) in [
            (
                "fontgres_list_faces",
                json!({ "Family": "Tuffy" }),
                "unknown field `Family`, expected `family`",
            ),
            (
                "fontgres_query_font",
                json!({ "families": ["Tuffy"], "size": 12 }),
                "unknown field `size`, expected one of `families`, `stretch`, `style`, `weight`",
            ),
        ] {
            let result = complete(call(&tools, Caller::Stdio, name, arguments).await);
            assert_eq!(
                first_text(&result),
                format!("failed to deserialize parameters: {expected}"),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn list_and_get_tool_share_the_schemas_closed_at_construction() {
        let tools = tools();

        let listed = listed_tools(&tools).await;
        assert_eq!(listed, tools.router().list_all());
        for tool in &listed {
            let fetched = ServerHandler::get_tool(&tools, &tool.name)
                .unwrap_or_else(|| panic!("{} is fetchable", tool.name));
            assert_eq!(*tool, fetched);
            assert!(Arc::ptr_eq(&tool.input_schema, &fetched.input_schema));
            match (&tool.output_schema, &fetched.output_schema) {
                (Some(listed_schema), Some(fetched_schema)) => {
                    assert!(Arc::ptr_eq(listed_schema, fetched_schema));
                }
                other => panic!("{} must advertise an output schema: {other:?}", tool.name),
            }
            assert_eq!(
                tool.input_schema.get("additionalProperties"),
                Some(&Value::Bool(false)),
                "{}",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn infeasibly_tiny_budget_propagates_as_a_protocol_error() {
        let tools = tools_with_budget(1);

        let error = call(&tools, Caller::Stdio, "fontgres_list_families", json!({}))
            .await
            .expect_err("the overflow diagnostic cannot fit a one-byte budget");

        assert_eq!(
            error.message,
            "tool response exceeds the configured response budget"
        );
        assert!(error.data.is_none());
    }

    /// The prefix every tool name carries.
    const TOOL_PREFIX: &str = "fontgres_";

    fn registered_tool_names() -> BTreeSet<String> {
        all_tool_names(&tools()).into_iter().collect()
    }

    /// Every word in `text` that starts with [`TOOL_PREFIX`], where a word is
    /// a maximal run of `[a-z0-9_]`. The bare prefix, as prose naming it,
    /// is not a tool name.
    fn mentioned_words(text: &str) -> Vec<&str> {
        text.split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .filter(|word| word.starts_with(TOOL_PREFIX) && *word != TOOL_PREFIX)
            .collect()
    }

    /// Asserts that `text` names every registered tool exactly once and
    /// nothing else that looks like a tool name.
    fn assert_names_every_tool_once(document: &str, text: &str) {
        let mentioned = mentioned_words(text);
        let registered = registered_tool_names();
        for name in &registered {
            let count = mentioned.iter().filter(|word| **word == name).count();
            assert_eq!(count, 1, "{document} must name {name} exactly once");
        }
        let unknown: Vec<&&str> = mentioned
            .iter()
            .filter(|word| !registered.contains(**word))
            .collect();
        assert!(
            unknown.is_empty(),
            "{document} names tools that are not registered: {unknown:?}"
        );
    }

    #[test]
    fn mentioned_words_split_on_markdown_and_punctuation() {
        assert_eq!(
            mentioned_words(
                "`fontgres_a`: see **fontgres_b_2**, (fontgres_c). FONTGRES_ENV, `fontgres_`"
            ),
            ["fontgres_a", "fontgres_b_2", "fontgres_c"]
        );
    }

    #[test]
    fn instructions_name_every_registered_tool_exactly_once() {
        assert_names_every_tool_once("instructions.md", include_str!("instructions.md"));
    }

    const README: &str = include_str!("../../README.md");

    /// The README's `## Tools` section: the body after its heading, up to the
    /// next `## ` heading.
    fn readme_tools_section() -> &'static str {
        let (_, rest) = README
            .split_once("\n## Tools\n")
            .expect("README has a `## Tools` section");
        rest.split_once("\n## ")
            .map_or(rest, |(section, _)| section)
    }

    #[test]
    fn readme_tool_table_matches_the_registered_tools() {
        let section = readme_tools_section();
        assert_names_every_tool_once("README.md `## Tools`", section);

        let tools = tools();
        let mut rows = 0;
        for line in section.lines() {
            // Data rows only: | `name` | description | read-only |
            let Some(row) = line.strip_prefix("| `") else {
                continue;
            };
            rows += 1;
            let name = row.split('`').next().unwrap_or_default();
            let tool = tools
                .router()
                .get(name)
                .unwrap_or_else(|| panic!("README row names unregistered tool {name}"));
            let read_only = line
                .trim_end()
                .trim_end_matches('|')
                .rsplit('|')
                .next()
                .unwrap_or_default()
                .trim();
            let expected = if is_read_only(tool) { "Yes" } else { "No" };
            assert_eq!(read_only, expected, "README read-only cell for {name}");
        }
        assert_eq!(rows, VALID_CALLS.len(), "one README row per tool");
    }
}
