//! A strict accumulator for streaming OpenAI-compatible tool calls.
//! Some providers send tool metadata in later chunks. Never invent a tool
//! name/id or execute a call before a complete validated representation exists.
use std::collections::BTreeMap;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompletedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolStreamError {
    InvalidIndex,
    ConflictingId,
    MissingId,
    MissingName,
    InvalidJsonArguments,
    NonObjectArguments,
}

#[derive(Default)]
pub(crate) struct ToolCallAccumulator {
    tools: BTreeMap<u64, PendingTool>,
}

#[derive(Default)]
struct PendingTool {
    id: Option<String>,
    name: String,
    argument_json: String,
}

impl ToolCallAccumulator {
    pub(crate) fn push_chunk(&mut self, chunk: &Value) -> Result<(),ToolStreamError> {
        let Some(calls)=chunk.pointer("/choices/0/delta/tool_calls")
            .and_then(Value::as_array) else {return Ok(());};
        for call in calls {
            let index=call.get("index").and_then(Value::as_u64)
                .ok_or(ToolStreamError::InvalidIndex)?;
            let pending=self.tools.entry(index).or_default();
            if let Some(id)=call.get("id").and_then(Value::as_str) {
                if !id.is_empty() {
                    match &pending.id {
                        Some(existing) if existing != id => return Err(ToolStreamError::ConflictingId),
                        None => pending.id=Some(id.to_owned()),
                        _ => {}
                    }
                }
            }
            if let Some(fragment)=call.pointer("/function/name").and_then(Value::as_str) {
                if !fragment.is_empty() && pending.name != fragment {
                    pending.name.push_str(fragment);
                }
            }
            if let Some(fragment)=call.pointer("/function/arguments").and_then(Value::as_str) {
                pending.argument_json.push_str(fragment);
            }
        }
        Ok(())
    }

    pub(crate) fn complete(self) -> Result<Vec<CompletedToolCall>,ToolStreamError> {
        self.tools.into_values().map(|pending| {
            let id=pending.id.ok_or(ToolStreamError::MissingId)?;
            if pending.name.is_empty(){return Err(ToolStreamError::MissingName);}
            let arguments=if pending.argument_json.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str::<Value>(&pending.argument_json)
                    .map_err(|_|ToolStreamError::InvalidJsonArguments)?
            };
            if !arguments.is_object(){return Err(ToolStreamError::NonObjectArguments);}
            Ok(CompletedToolCall {id,name:pending.name,arguments})
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn delayed_metadata_still_reconstructs_exact_tool_identity() {
        let mut acc=ToolCallAccumulator::default();
        acc.push_chunk(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,
            "function":{"arguments":r#"{"path":"#}}]}}]})).unwrap();
        acc.push_chunk(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,
            "id":"call-123","function":{"name":"Read","arguments":r#""src"}"#}}]}}]})).unwrap();
        assert_eq!(acc.complete().unwrap(), vec![CompletedToolCall {
            id:"call-123".into(),name:"Read".into(),arguments:json!({"path":"src"})
        }]);
    }

    #[test]
    fn interleaved_parallel_calls_do_not_mix() {
        let mut acc=ToolCallAccumulator::default();
        acc.push_chunk(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":1,"id":"b","function":{"name":"Write","arguments":"{}"}},
            {"index":0,"id":"a","function":{"name":"Read","arguments":"{}"}}
        ]}}]})).unwrap();
        let done=acc.complete().unwrap();
        assert_eq!(done[0].id,"a");
        assert_eq!(done[1].id,"b");
    }

    #[test]
    fn missing_tool_identity_cannot_be_masked_with_generic_default() {
        let mut acc=ToolCallAccumulator::default();
        acc.push_chunk(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,
          "function":{"arguments":"{}"}}]}}]})).unwrap();
        assert_eq!(acc.complete(),Err(ToolStreamError::MissingId));
    }

    #[test]
    fn invalid_tool_json_is_rejected_not_forwarded_as_real_command() {
        let mut acc=ToolCallAccumulator::default();
        acc.push_chunk(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,
          "id":"x","function":{"name":"Shell","arguments":"{oops"}}]}}]})).unwrap();
        assert_eq!(acc.complete(),Err(ToolStreamError::InvalidJsonArguments));
    }
}
