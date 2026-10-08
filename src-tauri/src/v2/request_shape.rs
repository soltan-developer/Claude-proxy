//! Privacy-preserving request instrumentation; never logs prompt contents.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct RequestShape {
    pub messages: usize,
    pub tool_definitions: usize,
    pub assistant_tool_uses: usize,
    pub user_tool_results: usize,
    pub system_blocks: usize,
    pub last_user_characters: usize,
    pub unknown_block_types: usize,
    pub has_cache_control: bool,
    pub has_thinking: bool,
    pub streaming: bool,
    pub requested_max_output_tokens: Option<u64>,
    pub serialized_request_bytes: usize,
}

impl RequestShape {
    pub(crate) fn inspect(request: &Value) -> Self {
        let messages = request.get("messages").and_then(Value::as_array);
        let mut shape = Self {
            messages: messages.map_or(0, Vec::len),
            tool_definitions: request.get("tools").and_then(Value::as_array).map_or(0, Vec::len),
            assistant_tool_uses: 0,
            user_tool_results: 0,
            system_blocks: match request.get("system") {
                Some(Value::Array(parts)) => parts.len(),
                Some(Value::String(_)) => 1,
                _ => 0,
            },
            last_user_characters: 0,
            unknown_block_types: 0,
            has_cache_control: has_marker(request, "cache_control"),
            has_thinking: request.get("thinking").is_some()
                || request.get("output_config").is_some(),
            streaming: request.get("stream").and_then(Value::as_bool) == Some(true),
            requested_max_output_tokens: request.get("max_tokens").and_then(Value::as_u64),
            serialized_request_bytes: serde_json::to_vec(request).map_or(0, |v| v.len()),
        };

        if let Some(messages) = messages {
            for message in messages {
                let is_user = message.get("role").and_then(Value::as_str) == Some("user");
                if let Some(parts) = message.get("content").and_then(Value::as_array) {
                    for part in parts {
                        match part.get("type").and_then(Value::as_str) {
                            Some("tool_use") => shape.assistant_tool_uses += 1,
                            Some("tool_result") => shape.user_tool_results += 1,
                            Some("text" | "image" | "thinking" | "redacted_thinking") => {}
                            Some(_) | None => shape.unknown_block_types += 1,
                        }
                    }
                }
                if is_user {
                    shape.last_user_characters = visible_text_length(
                        message.get("content").unwrap_or(&Value::Null),
                    );
                }
            }
        }
        shape
    }
}

fn visible_text_length(content: &Value) -> usize {
    match content {
        Value::String(text) => text.chars().count(),
        Value::Array(parts) => parts.iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .map(|text| text.chars().count())
            .sum(),
        _ => 0,
    }
}

fn has_marker(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => object.contains_key(key)
            || object.values().any(|child| has_marker(child, key)),
        Value::Array(items) => items.iter().any(|child| has_marker(child, key)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_shape_not_secrets_or_previous_prompt_text() {
        let request = json!({
            "model":"claude-sonnet-5-5", "stream":true, "max_tokens":32000,
            "system":[{"type":"text","text":"SECRET_SYSTEM","cache_control":{"type":"ephemeral"}}],
            "tools":[{"name":"Bash","input_schema":{"type":"object"}}],
            "messages":[
                {"role":"user","content":"old private prompt"},
                {"role":"assistant","content":[{"type":"tool_use","id":"tool1","name":"Bash","input":{"command":"pwd"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"ok"},
                   {"type":"text","text":"hello"}]}
            ]
        });
        let shape=RequestShape::inspect(&request);
        assert_eq!(shape.messages,3);
        assert_eq!(shape.tool_definitions,1);
        assert_eq!(shape.assistant_tool_uses,1);
        assert_eq!(shape.user_tool_results,1);
        assert_eq!(shape.last_user_characters,5);
        assert!(shape.streaming);
        assert!(shape.has_cache_control);
        assert_eq!(shape.requested_max_output_tokens,Some(32000));
        let diagnostic=serde_json::to_string(&shape).unwrap();
        assert!(!diagnostic.contains("SECRET_SYSTEM"));
        assert!(!diagnostic.contains("old private prompt"));
        assert!(!diagnostic.contains("pwd"));
    }

    #[test]
    fn detects_unsupported_content_instead_of_silently_discarding_it() {
        let request=json!({"messages":[{"role":"user","content":[{"type":"document","source":{}},
          {"type":"text","text":"check this"}]}]});
        assert_eq!(RequestShape::inspect(&request).unknown_block_types,1);
    }
}
