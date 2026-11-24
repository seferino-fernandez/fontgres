//! The MCP server: the rmcp [`ServerHandler`] over the font tools.
//!
//! [`Server`] holds the tools and their router, closes every schema the
//! server advertises, and holds every tool result to the response budget.

use std::sync::Arc;

use rmcp::handler::server::tool::{ToolCallContext, ToolRouter};
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

use fontgres_core::Database;

use crate::tools::{FontTools, serialized_len};

/// The MCP server for one font database.
///
/// A cheap handle: cloning it bumps one reference count, so every request
/// shares the tools, the tool router and the advertised tools built by
/// [`Server::new`].
#[derive(Clone)]
pub(crate) struct Server(Arc<Inner>);

/// The state every [`Server`] handle shares.
struct Inner {
    /// The tools, with the fonts they read.
    tools: FontTools,
    /// Every tool route, with its input and output schemas already closed.
    router: ToolRouter<FontTools>,
    /// The tools `tools/list` advertises: the router's tools, sorted by name.
    /// They share their schemas with the router.
    advertised: Vec<Tool>,
}

impl Server {
    /// Builds a server around an already loaded font database.
    ///
    /// Closes each tool's schemas once, so requests never rebuild routes or
    /// copy schemas. Build once per process and clone the result.
    ///
    /// `max_response_bytes` caps the serialized size of each tool result; see
    /// [`McpConfig::max_response_bytes`](crate::config::McpConfig).
    pub(crate) fn new(database: Database, max_response_bytes: usize) -> Self {
        let mut router = FontTools::tool_router();
        for route in router.map.values_mut() {
            close_tool_schemas(&mut route.attr);
        }
        let advertised = router.list_all();
        Self(Arc::new(Inner {
            tools: FontTools::new(database, max_response_bytes),
            router,
            advertised,
        }))
    }

    /// The tool router, schemas already closed.
    #[cfg(test)]
    pub(crate) fn router(&self) -> &ToolRouter<FontTools> {
        &self.0.router
    }
}

/// Hardens a tool's advertised schemas in place: closes the input schema and,
/// when present, the output schema (sets `additionalProperties: false` and an
/// explicit `required` on every object node). Called once per tool by
/// [`Server::new`], so `list_tools`, `get_tool` and `call_tool` all read the
/// same closed schemas.
fn close_tool_schemas(tool: &mut Tool) {
    close_object_map(Arc::make_mut(&mut tool.input_schema));
    if let Some(output) = tool.output_schema.as_mut() {
        close_object_map(Arc::make_mut(output));
    }
}

/// Recursively sets `additionalProperties: false` on every object schema node
/// within a JSON Schema value: the root, nested `properties`, `items`, and any
/// `$defs`/`definitions`. Idempotent.
fn close_object_schemas(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            close_object_map(map);
        }
        Value::Array(items) => {
            for item in items {
                close_object_schemas(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// Closes a single object schema map in place: for every object schema node it
/// sets `additionalProperties: false` and `required: []` when those keys are
/// absent, then recurses into nested values. Idempotent.
///
/// The empty `required: []` truthfully declares "no fields are required" for
/// parameterless/all-optional tools (schemars omits the key entirely when a
/// struct has no mandatory fields), satisfying scanners that require an explicit
/// `required` declaration. An existing `required` array is preserved verbatim.
fn close_object_map(map: &mut Map<String, Value>) {
    let is_object_schema =
        map.get("type").and_then(Value::as_str) == Some("object") || map.contains_key("properties");
    if is_object_schema {
        if !map.contains_key("additionalProperties") {
            map.insert("additionalProperties".to_string(), Value::Bool(false));
        }
        if !map.contains_key("required") {
            map.insert("required".to_string(), Value::Array(Vec::new()));
        }
    }
    for (_key, value) in map.iter_mut() {
        close_object_schemas(value);
    }
}

/// Replaces an oversized tool result with a valid, bounded JSON error.
///
/// Measures the exact serialized [`CallToolResult`]: `content`,
/// `structured_content` and every other field. If it exceeds `max_bytes`,
/// returns a small result describing the overflow. If even that diagnostic
/// exceeds `max_bytes`, returns a protocol error rather than break the
/// configured cap. Returns the original result otherwise.
///
/// The list tools cut each page to `max_bytes` themselves, so a result
/// reaches the limit only when one item, one face or one error message is
/// larger than it. Asking for less cannot help then, and the hint says to
/// raise the limit.
fn enforce_response_budget(
    result: CallToolResult,
    max_bytes: usize,
) -> Result<CallToolResult, McpError> {
    let size = serialized_len(&result)?;
    if size <= max_bytes {
        return Ok(result);
    }
    // Preserve the original error flag: a tool returning `Err(String)` arrives
    // here as `Ok(CallToolResult { is_error: Some(true), .. })`, so relabeling it
    // as a success would silently hide the failure.
    let was_error = result.is_error;
    let body = serde_json::json!({
        "error": "response_too_large",
        "bytes": size,
        "limit": max_bytes,
        "hint": "one result alone exceeds the limit; raise max_response_bytes"
    });
    let mut replacement = CallToolResult::success(vec![ContentBlock::text(body.to_string())]);
    replacement.is_error = was_error;
    if serialized_len(&replacement)? <= max_bytes {
        Ok(replacement)
    } else {
        Err(McpError::internal_error(
            "tool response exceeds the configured response budget",
            None,
        ))
    }
}

impl ServerHandler for Server {
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
        Ok(ListToolsResult::with_all_items(self.0.advertised.clone())
            .with_ttl_ms(300_000)
            .with_cache_scope(CacheScope::Private))
    }

    /// Runs the tool, then holds its result to the response budget.
    ///
    /// rmcp reads the arguments into the tool's parameter type, which denies
    /// unknown fields, so an argument the tool does not declare is a tool
    /// error that names it. An unknown tool name gets the router's own
    /// error.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        tracing::info!(tool = %request.name, "call_tool");
        let Inner { tools, router, .. } = &*self.0;
        let response = router
            .call(ToolCallContext::new(tools, request, context))
            .await?;
        if let CallToolResponse::Complete(result) = response {
            Ok(enforce_response_budget(result, tools.max_response_bytes())?.into())
        } else {
            Ok(response)
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.0.router.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::Server;
    use crate::test_support::{call, complete, context, first_text, server, server_with_budget};
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

    async fn listed_tools(server: &Server) -> Vec<Tool> {
        ServerHandler::list_tools(server, None, context(server))
            .await
            .expect("tools/list succeeds")
            .tools
    }

    fn all_tool_names(server: &Server) -> Vec<String> {
        server
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
        let server = server();
        let listed = listed_tools(&server).await;
        assert_eq!(listed.len(), VALID_CALLS.len());

        for tool in &listed {
            let name = tool.name.as_ref();
            assert_succeeded(
                &complete(call(&server, name, valid_arguments(name)).await),
                name,
            );
            let mut arguments = valid_arguments(name);
            arguments["unexpected_key"] = json!(true);
            let result = complete(call(&server, name, arguments).await);
            let text = first_text(&result);
            assert_eq!(result.is_error, Some(true), "{name}: {result:?}");
            assert!(
                text.starts_with("failed to deserialize parameters: ")
                    && text.contains("unknown field `unexpected_key`"),
                "{name}: {text}"
            );
        }
    }

    /// Every argument of the list tools has a default, so a call may send an
    /// empty object or no arguments at all.
    #[tokio::test]
    async fn a_tool_with_only_optional_arguments_accepts_empty_or_absent_arguments() {
        let server = server();

        let empty = complete(call(&server, "fontgres_list_families", json!({})).await);
        let absent = complete(
            ServerHandler::call_tool(
                &server,
                CallToolRequestParams::new("fontgres_list_families"),
                context(&server),
            )
            .await,
        );
        assert_succeeded(&empty, "empty arguments");
        assert_succeeded(&absent, "absent arguments");
        assert_eq!(empty.structured_content, absent.structured_content);
        assert_eq!(
            empty.structured_content,
            Some(json!({ "items": ["Tuffy"], "total": 1, "next_offset": null }))
        );
    }

    #[tokio::test]
    async fn unknown_key_message_lists_the_declared_fields() {
        let server = server();

        for (name, arguments, expected) in [
            (
                "fontgres_list_faces",
                json!({ "Family": "Tuffy" }),
                "unknown field `Family`, expected one of `family`, `offset`, `limit`",
            ),
            (
                "fontgres_query_font",
                json!({ "families": ["Tuffy"], "size": 12 }),
                "unknown field `size`, expected one of `families`, `weight`, `style`, `stretch`",
            ),
        ] {
            let result = complete(call(&server, name, arguments).await);
            assert_eq!(
                first_text(&result),
                format!("failed to deserialize parameters: {expected}"),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn list_and_get_tool_share_the_schemas_closed_at_construction() {
        let server = server();

        let listed = listed_tools(&server).await;
        assert_eq!(listed, server.router().list_all());
        for tool in &listed {
            let fetched = ServerHandler::get_tool(&server, &tool.name)
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
        let server = server_with_budget(1);

        let error = call(&server, "fontgres_list_families", json!({}))
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
        all_tool_names(&server()).into_iter().collect()
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

    const README: &str = include_str!("../README.md");

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

        let server = server();
        let mut rows = 0;
        for line in section.lines() {
            // Data rows only: | `name` | description | read-only |
            let Some(row) = line.strip_prefix("| `") else {
                continue;
            };
            rows += 1;
            let name = row.split('`').next().unwrap_or_default();
            let tool = server
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

#[cfg(test)]
mod schema_tests {
    use super::close_object_schemas;
    use serde_json::json;

    #[test]
    fn closes_top_level_object() {
        let mut schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        close_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn closes_nested_object_and_defs() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {"type": "object", "properties": {"x": {"type": "number"}}}
                }
            },
            "$defs": {"Inner": {"type": "object", "properties": {"y": {"type": "string"}}}}
        });
        close_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!([]));
        assert_eq!(
            schema["properties"]["items"]["items"]["additionalProperties"],
            json!(false)
        );
        assert_eq!(
            schema["properties"]["items"]["items"]["required"],
            json!([])
        );
        assert_eq!(
            schema["$defs"]["Inner"]["additionalProperties"],
            json!(false)
        );
        assert_eq!(schema["$defs"]["Inner"]["required"], json!([]));
    }

    #[test]
    fn closes_empty_no_arg_object() {
        let mut schema = json!({"type": "object", "properties": {}});
        close_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn inserts_empty_required_when_absent() {
        // An object schema with no `required` key gets an explicit `required: []`.
        let mut schema = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        assert!(schema.get("required").is_none());
        close_object_schemas(&mut schema);
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn preserves_existing_non_empty_required() {
        // A populated `required` (e.g. from schemars for a mandatory field) must
        // be preserved verbatim, never overwritten or emptied.
        let mut schema = json!({
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"]
        });
        close_object_schemas(&mut schema);
        assert_eq!(schema["required"], json!(["id"]));
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn closes_a_schema_with_only_an_object_type_or_only_properties() {
        for mut schema in [
            json!({"type": "object"}),
            json!({"properties": {"a": {"type": "string"}}}),
        ] {
            close_object_schemas(&mut schema);
            assert_eq!(schema["additionalProperties"], json!(false), "{schema}");
            assert_eq!(schema["required"], json!([]), "{schema}");
        }
    }

    #[test]
    fn preserves_existing_additional_properties() {
        let mut schema = json!({"type": "object", "additionalProperties": true});
        close_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(true));
    }
}

#[cfg(test)]
mod router_tests {
    use crate::test_support::server;
    use rmcp::ServerHandler;
    use rmcp::model::Tool;
    use serde_json::Value;

    /// Every registered tool, from a handler that is never called.
    fn listed_tools() -> Vec<Tool> {
        server().router().list_all()
    }

    #[test]
    fn every_listed_tool_schema_is_closed() {
        for tool in listed_tools() {
            let v = &tool.input_schema;
            assert_eq!(
                v.get("additionalProperties"),
                Some(&Value::Bool(false)),
                "tool {} root schema must be closed",
                tool.name
            );
            // Every advertised tool's root schema must declare `required`
            // explicitly (either `[]` for all-optional/no-arg tools or a
            // populated array), so deterministic scanners don't flag a missing
            // `required` key.
            let required = v.get("required").unwrap_or_else(|| {
                panic!("tool {} root schema must declare `required`", tool.name)
            });
            assert!(
                required.is_array(),
                "tool {} root `required` must be an array, got {required}",
                tool.name
            );
        }
    }

    #[test]
    fn every_listed_tool_has_a_closed_output_schema() {
        // Every tool advertises an `outputSchema`: rmcp derives it for a tool
        // that returns `Json<T>`, and the list tools name theirs. The MCP spec
        // requires an object root, and the server lists it closed.
        for tool in listed_tools() {
            let output = tool
                .output_schema
                .as_ref()
                .unwrap_or_else(|| panic!("tool {} must advertise an output schema", tool.name));
            assert!(
                output.get("type").and_then(Value::as_str) == Some("object")
                    || output.get("properties").is_some(),
                "tool {} output schema must have an object root: {output:?}",
                tool.name
            );
            assert_eq!(
                output.get("additionalProperties"),
                Some(&Value::Bool(false)),
                "tool {} output schema must be closed: {output:?}",
                tool.name
            );
        }
    }

    #[test]
    fn list_output_schemas_require_items_total_and_next_offset() {
        use serde_json::json;

        for (name, item) in [
            ("fontgres_list_faces", json!({ "$ref": "#/$defs/Face" })),
            ("fontgres_list_families", json!({ "type": "string" })),
        ] {
            let tool = listed_tools()
                .into_iter()
                .find(|tool| tool.name == name)
                .unwrap();
            let schema = tool.output_schema.unwrap();
            let required = schema["required"].as_array().unwrap();
            assert_eq!(required.len(), 3, "{name}");
            assert!(required.contains(&json!("items")), "{name}");
            assert!(required.contains(&json!("total")), "{name}");
            assert!(required.contains(&json!("next_offset")), "{name}");
            assert_eq!(schema["additionalProperties"], false, "{name}");
            let properties = &schema["properties"];
            assert_eq!(properties["items"]["items"], item, "{name}");
            assert_eq!(
                properties["next_offset"]["type"],
                json!(["integer", "null"]),
                "{name}"
            );
        }
    }

    /// Every tool, in listing order, with the annotation hints it must carry:
    /// read-only, destructive (left unchecked when `None`) and idempotent.
    const EXPECTED_HINTS: [(&str, bool, Option<bool>, bool); 3] = [
        ("fontgres_list_faces", true, Some(false), true),
        ("fontgres_list_families", true, Some(false), true),
        ("fontgres_query_font", true, Some(false), true),
    ];

    #[test]
    fn tool_annotations_are_complete() {
        let tools = listed_tools();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        let expected: Vec<&str> = EXPECTED_HINTS.iter().map(|(name, ..)| *name).collect();
        assert_eq!(names, expected);
        for (tool, (_, read_only, destructive, idempotent)) in tools.iter().zip(EXPECTED_HINTS) {
            assert!(
                tool.name.starts_with("fontgres_"),
                "tool {} must carry the server prefix",
                tool.name
            );
            let ann = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("tool {} must have annotations", tool.name));
            assert!(
                ann.title.as_ref().is_some_and(|t| !t.is_empty()),
                "tool {} must have a non-empty title",
                tool.name
            );
            assert_eq!(
                ann.read_only_hint,
                Some(read_only),
                "{} read-only hint",
                tool.name
            );
            if destructive.is_some() {
                assert_eq!(
                    ann.destructive_hint, destructive,
                    "{} destructive hint",
                    tool.name
                );
            }
            assert_eq!(
                ann.idempotent_hint,
                Some(idempotent),
                "{} idempotent hint",
                tool.name
            );
        }
    }

    /// Pins the advertised protocol version. `ProtocolVersion::LATEST` follows
    /// the SDK and may move past what this server implements, so this server
    /// names its version explicitly.
    #[test]
    fn advertises_current_protocol_version() {
        use rmcp::model::ProtocolVersion;

        let server = server();
        assert_eq!(
            server.get_info().protocol_version,
            ProtocolVersion::V_2026_07_28
        );
    }

    /// The server must identify itself, not the SDK. `Implementation::default()`
    /// and `from_build_env()` both expand `env!` inside rmcp, so they report
    /// `"rmcp"` at rmcp's version which would tell clients this server is
    /// version 3.0.0. Guards against a well-meaning revert to either.
    #[test]
    fn identifies_itself_rather_than_the_sdk() {
        let server = server();
        let server_info = server.get_info().server_info;

        assert_eq!(server_info.name, env!("CARGO_PKG_NAME"));
        assert_eq!(server_info.version, env!("CARGO_PKG_VERSION"));
        assert_ne!(
            server_info.name, "rmcp",
            "server_info must not report the SDK's identity"
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::enforce_response_budget;
    use rmcp::model::{CallToolResult, ContentBlock};

    #[test]
    fn passes_small_results_through() {
        let original =
            CallToolResult::success(vec![ContentBlock::text("{\"ok\":true}".to_string())]);
        let guarded = enforce_response_budget(original, 1024).unwrap();
        let text = serde_json::to_string(&guarded.content).unwrap();
        assert!(text.contains("ok"));
    }

    /// Guards `original` with a 256-byte budget, asserts that the result fits
    /// and reports `response_too_large`, and returns it.
    fn guard_oversized(original: CallToolResult) -> CallToolResult {
        let guarded = enforce_response_budget(original, 256).unwrap();
        assert!(serde_json::to_vec(&guarded).unwrap().len() <= 256);
        let text = serde_json::to_string(&guarded.content).unwrap();
        assert!(text.contains("response_too_large"), "got: {text}");
        guarded
    }

    #[test]
    fn replaces_oversized_results() {
        let big = "x".repeat(2048);
        guard_oversized(CallToolResult::success(vec![ContentBlock::text(big)]));
    }

    #[test]
    fn guards_oversized_structured_results() {
        let big = serde_json::json!({ "blob": "x".repeat(2048) });
        let guarded = guard_oversized(CallToolResult::structured(big));
        assert_eq!(
            guarded.is_error,
            Some(false),
            "a successful structured result stays non-error after guarding"
        );
    }

    /// `CallToolResult::structured` carries the payload twice, as text content
    /// and as `structured_content`, plus the other result fields. The cap
    /// applies to all of it, not to `content` alone.
    #[test]
    fn budget_measures_the_whole_serialized_result() {
        let payload = serde_json::json!({ "blob": "x".repeat(200) });
        let original = CallToolResult::structured(payload);
        let content_size = serde_json::to_vec(&original.content).unwrap().len();
        let exact_size = serde_json::to_vec(&original).unwrap().len();
        assert!(exact_size > 2 * 200, "{exact_size}");

        let at_the_boundary = enforce_response_budget(original.clone(), exact_size).unwrap();
        assert_eq!(
            at_the_boundary, original,
            "a result of exactly the cap passes"
        );

        for cap in [content_size, exact_size - 1] {
            let guarded = enforce_response_budget(original.clone(), cap).unwrap();
            let text = serde_json::to_string(&guarded.content).unwrap();
            assert!(
                text.contains("response_too_large"),
                "the {exact_size}-byte result must not pass a {cap}-byte cap"
            );
        }
    }

    #[test]
    fn overflow_replacement_never_exceeds_even_a_tiny_configured_cap() {
        let original = CallToolResult::success(vec![ContentBlock::text("x".repeat(2048))]);
        let error = enforce_response_budget(original, 1)
            .expect_err("an infeasibly tiny cap must produce a protocol error");

        assert_eq!(
            error.message,
            "tool response exceeds the configured response budget"
        );
        assert!(error.data.is_none());
    }

    #[test]
    fn preserves_error_flag_on_replacement() {
        let mut original = CallToolResult::success(vec![ContentBlock::text("x".repeat(2048))]);
        original.is_error = Some(true);
        let guarded = enforce_response_budget(original, 256).unwrap();
        assert_eq!(
            guarded.is_error,
            Some(true),
            "a failed call must stay flagged as an error after guarding"
        );
        let text = serde_json::to_string(&guarded.content).unwrap();
        assert!(text.contains("response_too_large"), "got: {text}");
    }
}
