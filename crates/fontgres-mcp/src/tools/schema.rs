//! Schema post-processing: close every object schema so MCP tool inputs reject
//! unexpected properties, and a response-size guard helper.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{Map, Value};

/// Recursively sets `additionalProperties: false` on every object schema node
/// within a JSON Schema value: the root, nested `properties`, `items`, and any
/// `$defs`/`definitions`. Idempotent.
pub(super) fn close_object_schemas(schema: &mut Value) {
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
pub(super) fn close_object_map(map: &mut Map<String, Value>) {
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
pub(super) fn enforce_response_budget(
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

/// The length of `result` serialized as JSON, as it goes on the wire.
pub(super) fn serialized_len(result: &CallToolResult) -> Result<usize, McpError> {
    serde_json::to_vec(result)
        .map(|bytes| bytes.len())
        .map_err(|_error| McpError::internal_error("failed to measure tool response", None))
}

#[cfg(test)]
mod tests {
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
    use crate::tools::test_support::tools;
    use rmcp::ServerHandler;
    use rmcp::model::Tool;
    use serde_json::Value;

    /// Every registered tool, from a handler that is never called.
    fn listed_tools() -> Vec<Tool> {
        tools().router().list_all()
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

        let tools = tools();
        assert_eq!(
            tools.get_info().protocol_version,
            ProtocolVersion::V_2026_07_28
        );
    }

    /// The server must identify itself, not the SDK. `Implementation::default()`
    /// and `from_build_env()` both expand `env!` inside rmcp, so they report
    /// `"rmcp"` at rmcp's version which would tell clients this server is
    /// version 3.0.0. Guards against a well-meaning revert to either.
    #[test]
    fn identifies_itself_rather_than_the_sdk() {
        let tools = tools();
        let server_info = tools.get_info().server_info;

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
