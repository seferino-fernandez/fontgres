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

impl AppTools {
    /// Runs the checks a call to the known `tool` goes through before it runs:
    /// undeclared arguments. Returns the response that ends the call early, or
    /// `None` when the tool may run.
    fn precheck(&self, tool: &Tool, request: &CallToolRequestParams) -> Option<CallToolResponse> {
        undeclared_argument_rejection(tool, request).map(Into::into)
    }
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
            && let Some(refusal) = self.precheck(tool, &request)
        {
            return Ok(refusal);
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
