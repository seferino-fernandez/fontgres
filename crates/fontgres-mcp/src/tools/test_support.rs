//! The one test harness for calling MCP tools.
//!
//! Every tool test calls through [`ServerHandler::call_tool`], the path a real
//! request takes, so the argument check and the response budget apply exactly
//! as they do in production.

use super::handler::AppTools;
use fontgres_core::FontsConfig;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Extensions, JsonObject, NumberOrString,
};
use rmcp::service::{RequestContext, serve_directly};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;
use std::path::Path;

/// The production default response budget, 1 MiB.
const DEFAULT_RESPONSE_BYTES: usize = 1024 * 1024;

/// How a request was authenticated, as the tool handler sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Caller {
    /// A local client using standard input and output.
    Stdio,
}

/// Tools over a database holding only the test font, Tuffy.ttf, with the
/// production 1 MiB budget.
pub(crate) fn tools() -> AppTools {
    tools_with_budget(DEFAULT_RESPONSE_BYTES)
}

/// [`tools`] with a response budget of `max_response_bytes`.
pub(crate) fn tools_with_budget(max_response_bytes: usize) -> AppTools {
    let fonts = FontsConfig {
        system: false,
        files: vec![
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fontgres-core/tests/fonts/Tuffy.ttf"),
        ],
        ..FontsConfig::default()
    };
    let database = fonts.build_database().expect("the test font loads");
    AppTools::new(database, max_response_bytes)
}

/// A request context for the selected caller. The connection serving the peer
/// closes on return.
pub(crate) fn context(tools: &AppTools, caller: Caller) -> RequestContext<RoleServer> {
    let (server_transport, _client_transport) = tokio::io::duplex(4096);
    let running = serve_directly::<RoleServer, _, _, _, _>(tools.clone(), server_transport, None);
    let mut context = RequestContext::new(NumberOrString::Number(1), running.peer().clone());
    context.extensions = request_extensions(caller);
    context
}

/// The extensions a request from `caller` carries: none over stdio; HTTP
/// request parts otherwise.
fn request_extensions(caller: Caller) -> Extensions {
    match caller {
        Caller::Stdio => Extensions::default(),
    }
}

/// `arguments` as the object a tool call carries.
pub(crate) fn arguments(arguments: Value) -> JsonObject {
    let Value::Object(object) = arguments else {
        panic!("tool arguments are an object");
    };
    object
}

/// Calls tool `name` as `caller` through [`ServerHandler::call_tool`].
pub(crate) async fn call(
    tools: &AppTools,
    caller: Caller,
    name: &str,
    arguments: Value,
) -> Result<CallToolResponse, McpError> {
    ServerHandler::call_tool(
        tools,
        CallToolRequestParams::new(name.to_string()).with_arguments(self::arguments(arguments)),
        context(tools, caller),
    )
    .await
}

/// Calls tool `name` over stdio and returns its result, panicking on a
/// protocol error or a response that is not complete.
pub(crate) async fn call_tool(tools: &AppTools, name: &str, arguments: Value) -> CallToolResult {
    complete(call(tools, Caller::Stdio, name, arguments).await)
}

/// The result a [`call`] completed with, panicking on a protocol error or a
/// response that is not complete.
pub(crate) fn complete(response: Result<CallToolResponse, McpError>) -> CallToolResult {
    let response = response.unwrap_or_else(|error| panic!("tools/call is answered: {error:?}"));
    if let CallToolResponse::Complete(result) = response {
        result
    } else {
        panic!("expected a complete response, got {response:?}")
    }
}

/// The structured content of a successful tool result.
pub(crate) fn structured(result: CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "tool call failed: {result:?}");
    result
        .structured_content
        .expect("typed tools return structured content")
}

/// The text of a result's first content block.
pub(crate) fn first_text(result: &CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map_or("", |text| text.text.as_str())
}

#[cfg(test)]
mod tests {
    use super::{call_tool, first_text, tools_with_budget};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn harness_call_tool_applies_the_response_budget() {
        let tools = tools_with_budget(300);

        let result = call_tool(&tools, "fontgres_list_faces", json!({})).await;

        let body: Value =
            serde_json::from_str(first_text(&result)).expect("the guard text is JSON");
        assert_eq!(body["error"], "response_too_large", "{body}");
        assert_eq!(body["limit"], 300, "{body}");
    }
}
