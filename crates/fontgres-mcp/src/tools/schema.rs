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
/// returns a small result describing the overflow so the model can narrow its
/// request. If even that diagnostic exceeds `max_bytes`, returns a protocol
/// error rather than break the configured cap. Returns the original result
/// otherwise.
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
        "hint": "narrow the query (smaller page size, fewer results, or a more specific filter)"
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
fn serialized_len(result: &CallToolResult) -> Result<usize, McpError> {
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
    fn preserves_existing_additional_properties() {
        let mut schema = json!({"type": "object", "additionalProperties": true});
        close_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(true));
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

    #[test]
    fn replaces_oversized_results() {
        let big = "x".repeat(2048);
        let original = CallToolResult::success(vec![ContentBlock::text(big)]);
        let guarded = enforce_response_budget(original, 256).unwrap();
        assert!(serde_json::to_vec(&guarded).unwrap().len() <= 256);
        let text = serde_json::to_string(&guarded.content).unwrap();
        assert!(text.contains("response_too_large"), "got: {text}");
    }

    #[test]
    fn guards_oversized_structured_results() {
        let big = serde_json::json!({ "blob": "x".repeat(2048) });
        let original = CallToolResult::structured(big);
        let guarded = enforce_response_budget(original, 256).unwrap();
        assert!(serde_json::to_vec(&guarded).unwrap().len() <= 256);
        let text = serde_json::to_string(&guarded.content).unwrap();
        assert!(text.contains("response_too_large"), "got: {text}");
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
