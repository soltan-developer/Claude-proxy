//! Verify that a provider adapter can represent the incoming Anthropic request.
//! No serialization, trimming or mutation is performed here. When a provider's
//! Chat Completions representation would discard a block, refuse that adapter
//! instead of silently throwing away client context.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterKind {
    NativeAnthropic,
    ChatCompletions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContractError {
    MissingModel,
    MissingMessages,
    InvalidMessage { index: usize },
    InvalidContent { location: String },
    MissingToolField { location: String },
    UnsupportedBlock { location: String, kind: String },
}

fn fail_content(location: &str) -> ContractError {
    ContractError::InvalidContent {
        location: location.to_string(),
    }
}

fn nonempty_string(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn check_tool_result(value: &Value, location: &str) -> Result<(), ContractError> {
    if !nonempty_string(value.get("tool_use_id")) || value.get("content").is_none() {
        return Err(ContractError::MissingToolField {
            location: location.to_string(),
        });
    }
    // Chat Completions cannot represent rich tool_result blocks with images or
    // documents via the current gateway translator, which only extracts text.
    if let Some(blocks) = value.get("content").and_then(Value::as_array) {
        for (index, block) in blocks.iter().enumerate() {
            if block.get("type").and_then(Value::as_str) != Some("text")
                || !block.get("text").is_some_and(Value::is_string)
            {
                return Err(ContractError::UnsupportedBlock {
                    location: format!("{location}/content/{index}"),
                    kind: block
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("missing_type")
                        .to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Assert that a request is representable by the selected upstream adapter.
/// Native requests preserve the complete raw request, even for future block
/// types; Chat Completions must reject blocks it cannot represent losslessly.
pub(crate) fn validate_request(request: &Value, adapter: AdapterKind) -> Result<(), ContractError> {
    if !nonempty_string(request.get("model")) {
        return Err(ContractError::MissingModel);
    }
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty())
        .ok_or(ContractError::MissingMessages)?;

    if adapter == AdapterKind::ChatCompletions {
        if let Some(system) = request.get("system") {
            match system {
                Value::String(_) => {}
                Value::Array(blocks) => {
                    for (i, block) in blocks.iter().enumerate() {
                        if block.get("type").and_then(Value::as_str) != Some("text") {
                            return Err(ContractError::UnsupportedBlock {
                                location: format!("/system/{i}"),
                                kind: block
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("missing_type")
                                    .to_owned(),
                            });
                        }
                        if !block.get("text").is_some_and(Value::is_string) {
                            return Err(fail_content(&format!("/system/{i}/text")));
                        }
                    }
                }
                _ => return Err(fail_content("/system")),
            }
        }
        if let Some(tools) = request.get("tools").and_then(Value::as_array) {
            for (i, tool) in tools.iter().enumerate() {
                let kind = tool.get("type").and_then(Value::as_str).unwrap_or("custom");
                if kind != "custom" && kind != "function" {
                    return Err(ContractError::UnsupportedBlock {
                        location: format!("/tools/{i}"),
                        kind: kind.to_owned(),
                    });
                }
            }
        }
    }

    for (i, message) in messages.iter().enumerate() {
        let Some(role) = message.get("role").and_then(Value::as_str) else {
            return Err(ContractError::InvalidMessage { index: i });
        };
        if role != "user" && role != "assistant" {
            return Err(ContractError::InvalidMessage { index: i });
        }
        let Some(content) = message.get("content") else {
            return Err(ContractError::InvalidMessage { index: i });
        };
        let location = format!("/messages/{i}/content");
        match content {
            Value::String(_) => {}
            Value::Array(blocks) => {
                for (j, block) in blocks.iter().enumerate() {
                    let path = format!("{location}/{j}");
                    let kind = block
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("missing_type");
                    if adapter == AdapterKind::NativeAnthropic {
                        continue;
                    }
                    match kind {
                        "text" if block.get("text").is_some_and(Value::is_string) => {}
                        "image" if block.get("source").is_some_and(Value::is_object) => {}
                        "tool_use" if role == "assistant" => {
                            if !nonempty_string(block.get("id"))
                                || !nonempty_string(block.get("name"))
                                || !block.get("input").is_some_and(Value::is_object)
                            {
                                return Err(ContractError::MissingToolField { location: path });
                            }
                        }
                        "tool_result" if role == "user" => check_tool_result(block, &path)?,
                        _ => {
                            return Err(ContractError::UnsupportedBlock {
                                location: path,
                                kind: kind.to_owned(),
                            });
                        }
                    }
                }
            }
            _ => return Err(fail_content(&location)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_adapter_keeps_future_document_and_thinking_blocks() {
        let request = json!({"model":"claude-sonnet-5-5","messages":[
            {"role":"user","content":[{"type":"document","source":{"type":"base64","data":"private"}},
                                      {"type":"text","text":"analyze this"}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"private"}]}
        ]});
        let original = request.clone();
        assert_eq!(
            validate_request(&request, AdapterKind::NativeAnthropic),
            Ok(())
        );
        assert_eq!(request, original);
        assert!(matches!(
            validate_request(&request, AdapterKind::ChatCompletions),
            Err(ContractError::UnsupportedBlock { location, kind })
                if location == "/messages/0/content/0" && kind == "document"
        ));
    }

    #[test]
    fn chat_adapter_accepts_complete_client_tool_round_trip() {
        let request = json!({"model":"claude-sonnet-5-5",
        "tools":[{"name":"Read","input_schema":{"type":"object"}}],
        "messages":[
          {"role":"user","content":"inspect repository"},
          {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"Read",
                       "input":{"path":"README.md"}}]},
          {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1",
                      "content":[{"type":"text","text":"repo readme"}]}]}
        ]});
        assert_eq!(
            validate_request(&request, AdapterKind::ChatCompletions),
            Ok(())
        );
    }

    #[test]
    fn chat_adapter_rejects_rich_tool_results_instead_of_dropping_them() {
        let request = json!({"model":"claude-sonnet-5-5","messages":[
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1",
                    "content":[{"type":"image","source":{"type":"base64","data":"secret"}}]}]}
        ]});
        assert!(matches!(
            validate_request(&request, AdapterKind::ChatCompletions),
            Err(ContractError::UnsupportedBlock { location, kind })
                if location == "/messages/0/content/0/content/0" && kind == "image"
        ));
    }

    #[test]
    fn server_tools_are_not_passed_off_as_client_functions() {
        let request = json!({"model":"claude-sonnet-5-5",
          "tools":[{"type":"web_search_20250305","name":"web_search"}],
          "messages":[{"role":"user","content":"search online"}]});
        assert!(matches!(
            validate_request(&request, AdapterKind::ChatCompletions),
            Err(ContractError::UnsupportedBlock { location, .. })
                if location == "/tools/0"
        ));
    }

    #[test]
    fn unrelated_requests_cannot_be_mutated_by_contract_validation() {
        let chinese = json!({"model":"claude-sonnet-5-5","messages":[
            {"role":"user","content":"查找 Claude Agent SDK"}]});
        let persian = json!({"model":"claude-sonnet-5-5","messages":[
            {"role":"user","content":"سلام"}]});
        let snapshot = persian.clone();
        for _ in 0..10 {
            assert_eq!(
                validate_request(&chinese, AdapterKind::ChatCompletions),
                Ok(())
            );
            assert_eq!(
                validate_request(&persian, AdapterKind::ChatCompletions),
                Ok(())
            );
        }
        assert_eq!(persian, snapshot);
        assert_ne!(persian, chinese);
    }
}
