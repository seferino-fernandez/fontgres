//! The one test harness for calling MCP tools.
//!
//! Every tool test calls through [`ServerHandler::call_tool`], the path a real
//! request takes, so the arguments are read and the response budget applies
//! exactly as they do in production.

use crate::server::Server;
use fontgres_core::{Database, FaceInfo, FontsConfig, Language, Weight};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, JsonObject, NumberOrString,
};
use rmcp::service::{RequestContext, serve_directly};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The production default response budget, 1 MiB.
const DEFAULT_RESPONSE_BYTES: usize = 1024 * 1024;

/// A server over a database holding only the test font, Tuffy.ttf, with the
/// production 1 MiB budget.
pub(crate) fn server() -> Server {
    server_with_budget(DEFAULT_RESPONSE_BYTES)
}

/// Tuffy.ttf, the one font file the tests load.
fn test_font() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fontgres-core/tests/fonts/Tuffy.ttf")
}

/// [`server`] with a response budget of `max_response_bytes`.
pub(crate) fn server_with_budget(max_response_bytes: usize) -> Server {
    let fonts = FontsConfig {
        system: false,
        files: vec![test_font()],
        ..FontsConfig::default()
    };
    let database = fonts.build_database().expect("the test font loads");
    Server::new(database, max_response_bytes)
}

/// The test font's face, loaded from memory, so it has no path and
/// serializes to the same bytes on every machine.
static FACE_WITHOUT_A_PATH: LazyLock<FaceInfo> = LazyLock::new(|| {
    let mut database = Database::new();
    let report = database.load_font_data(std::fs::read(test_font()).expect("the test font reads"));
    assert!(report.failures().is_empty(), "{report:?}");
    let face = database.faces().first().expect("the test font has a face");
    face.clone()
});

/// A normal face of `family` at `weight`, with no path.
pub(crate) fn face(family: &str, weight: u16) -> FaceInfo {
    let mut face = FACE_WITHOUT_A_PATH.clone();
    face.families = vec![(family.to_owned(), Language::English_UnitedStates)];
    face.post_script_name = format!("{family}-{weight}").replace(' ', "-");
    face.weight = Weight(weight);
    face
}

/// A server over a database holding `faces`, loaded in that order, with a
/// response budget of `max_response_bytes`.
pub(crate) fn server_over(
    faces: impl IntoIterator<Item = FaceInfo>,
    max_response_bytes: usize,
) -> Server {
    let mut database = Database::new();
    for face in faces {
        database.push_face_info(face);
    }
    Server::new(database, max_response_bytes)
}

/// A request context for a call to `server`. The connection serving the peer
/// closes on return.
pub(crate) fn context(server: &Server) -> RequestContext<RoleServer> {
    let (server_transport, _client_transport) = tokio::io::duplex(4096);
    let running = serve_directly::<RoleServer, _, _, _, _>(server.clone(), server_transport, None);
    RequestContext::new(NumberOrString::Number(1), running.peer().clone())
}

/// `arguments` as the object a tool call carries.
pub(crate) fn arguments(arguments: Value) -> JsonObject {
    let Value::Object(object) = arguments else {
        panic!("tool arguments are an object");
    };
    object
}

/// Calls tool `name` through [`ServerHandler::call_tool`].
pub(crate) async fn call(
    server: &Server,
    name: &str,
    arguments: Value,
) -> Result<CallToolResponse, McpError> {
    ServerHandler::call_tool(
        server,
        CallToolRequestParams::new(name.to_string()).with_arguments(self::arguments(arguments)),
        context(server),
    )
    .await
}

/// Calls tool `name` and returns its result, panicking on a protocol error or
/// a response that is not complete.
pub(crate) async fn call_tool(server: &Server, name: &str, arguments: Value) -> CallToolResult {
    complete(call(server, name, arguments).await)
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
    use super::{call_tool, first_text, server_with_budget};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn harness_call_tool_applies_the_response_budget() {
        let server = server_with_budget(300);

        let result = call_tool(&server, "fontgres_list_faces", json!({})).await;

        let body: Value =
            serde_json::from_str(first_text(&result)).expect("the guard text is JSON");
        assert_eq!(body["error"], "response_too_large", "{body}");
        assert_eq!(body["limit"], 300, "{body}");
    }
}
