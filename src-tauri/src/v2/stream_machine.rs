//! Opt-in Anthropic SSE translator for OpenAI-compatible Chat Completions.
//!
//! Emit text incrementally but delay tool blocks until all metadata and JSON
//! fragments are complete. This avoids fabricated tool names/IDs in legacy SSE.
//! No synthetic Continue prompts, and no falsified successful stop reasons.
use super::tool_stream::{ToolCallAccumulator, ToolStreamError};
use serde_json::{json, Value};

pub(crate) type Event = (String, Value);
const MAX_TOOL_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamContractError {
    InvalidTool(ToolStreamError),
    ExcessiveToolArguments,
    ExcessiveText,
    MissingFinishReason,
    ConflictingFinishReason,
    MissingToolCalls,
    TruncatedToolCalls,
    InvalidFinishReason,
    VerifiedOutputOverBudget,
}

#[derive(Default)]
pub(crate) struct StreamMachine {
    next_index: usize,
    text_index: Option<usize>,
    tools: ToolCallAccumulator,
    tool_json_bytes: usize,
    text_bytes: usize,
    finish_reason: Option<String>,
}

impl StreamMachine {
    pub(crate) fn ingest(&mut self, chunk: &Value) -> Result<Vec<Event>, StreamContractError> {
        let delta = chunk.pointer("/choices/0/delta").unwrap_or(&Value::Null);
        let mut events = Vec::new();
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                self.text_bytes = self.text_bytes.saturating_add(text.len());
                if self.text_bytes > MAX_TEXT_BYTES {
                    return Err(StreamContractError::ExcessiveText);
                }
                let index = match self.text_index {
                    Some(index) => index,
                    None => {
                        let index = self.next_index;
                        self.next_index += 1;
                        self.text_index = Some(index);
                        events.push(("content_block_start".into(), json!({
                            "type": "content_block_start", "index": index,
                            "content_block": {"type": "text", "text": ""}
                        })));
                        index
                    }
                };
                events.push(("content_block_delta".into(), json!({
                    "type": "content_block_delta", "index": index,
                    "delta": {"type": "text_delta", "text": text}
                })));
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                if let Some(fragment) = call.pointer("/function/arguments").and_then(Value::as_str) {
                    self.tool_json_bytes = self.tool_json_bytes.saturating_add(fragment.len());
                    if self.tool_json_bytes > MAX_TOOL_JSON_BYTES {
                        return Err(StreamContractError::ExcessiveToolArguments);
                    }
                }
            }
            self.tools
                .push_chunk(chunk)
                .map_err(StreamContractError::InvalidTool)?;
        }
        if let Some(reason) = chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            if let Some(previous) = &self.finish_reason {
                if previous != reason {
                    return Err(StreamContractError::ConflictingFinishReason);
                }
            } else {
                self.finish_reason = Some(reason.to_owned());
            }
        }
        Ok(events)
    }

    pub(crate) fn finish(
        mut self,
        reported_output_tokens: Option<u64>,
        fallback_output_tokens: u64,
        client_limit: Option<u64>,
    ) -> Result<Vec<Event>, StreamContractError> {
        let reason = self
            .finish_reason
            .take()
            .ok_or(StreamContractError::MissingFinishReason)?;
        if let (Some(actual), Some(limit)) = (reported_output_tokens, client_limit) {
            if actual > limit {
                return Err(StreamContractError::VerifiedOutputOverBudget);
            }
        }
        let tools = self
            .tools
            .complete()
            .map_err(StreamContractError::InvalidTool)?;
        if reason == "tool_calls" && tools.is_empty() {
            return Err(StreamContractError::MissingToolCalls);
        }
        if reason == "length" && !tools.is_empty() {
            return Err(StreamContractError::TruncatedToolCalls);
        }
        if !matches!(reason.as_str(), "stop" | "tool_calls" | "length") {
            return Err(StreamContractError::InvalidFinishReason);
        }
        let mut events = Vec::new();
        if let Some(index) = self.text_index {
            events.push(("content_block_stop".into(), json!({
                "type": "content_block_stop", "index": index
            })));
        }
        let has_tools = !tools.is_empty();
        for tool in tools {
            let index = self.next_index;
            self.next_index += 1;
            events.push(("content_block_start".into(), json!({
                "type": "content_block_start", "index": index,
                "content_block": {
                    "type": "tool_use", "id": tool.id,
                    "name": tool.name, "input": {}
                }
            })));
            events.push(("content_block_delta".into(), json!({
                "type": "content_block_delta", "index": index,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": tool.arguments.to_string()
                }
            })));
            events.push(("content_block_stop".into(), json!({
                "type": "content_block_stop", "index": index
            })));
        }
        let stop_reason = if has_tools {
            "tool_use"
        } else if reason == "length" {
            "max_tokens"
        } else {
            "end_turn"
        };
        events.push(("message_delta".into(), json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason, "stop_sequence": null},
            "usage": {"output_tokens": reported_output_tokens.unwrap_or(fallback_output_tokens)}
        })));
        events.push(("message_stop".into(), json!({"type": "message_stop"})));
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn late_tool_identity_and_interleaved_parallel_calls_are_exact() {
        let mut machine = StreamMachine::default();
        let a = machine.ingest(&json!({"choices":[{"delta":{"content":"Looking up two files."}}]})).unwrap();
        assert_eq!(a[0].0, "content_block_start");
        assert_eq!(a[1].1["delta"]["text"], "Looking up two files.");
        machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":1,"function":{"arguments":"{\"pattern\":"}},
            {"index":0,"function":{"arguments":"{\"path\":"}}
        ]}}]})).unwrap();
        machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"call-read","function":{"name":"Re","arguments":"\"README.md\"}"}},
            {"index":1,"id":"call-grep","function":{"name":"Grep","arguments":"\"TODO\"}"}}
        ]}}]})).unwrap();
        machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"function":{"name":"ad"}}
        ]},"finish_reason":"tool_calls"}]})).unwrap();
        let events = machine.finish(Some(55), 1, Some(512)).unwrap();
        assert_eq!(events[0].0, "content_block_stop");
        assert_eq!(events[1].1["content_block"]["id"], "call-read");
        assert_eq!(events[1].1["content_block"]["name"], "Read");
        assert_eq!(events[2].1["delta"]["partial_json"], r#"{"path":"README.md"}"#);
        assert_eq!(events[4].1["content_block"]["id"], "call-grep");
        assert_eq!(events[5].1["delta"]["partial_json"], r#"{"pattern":"TODO"}"#);
        assert_eq!(events[7].1["delta"]["stop_reason"], "tool_use");
        assert_eq!(events[8].0, "message_stop");
    }

    #[test]
    fn missing_late_metadata_is_an_error_not_fake_tool_name() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"function":{"arguments":"{}"}}
        ]},"finish_reason":"tool_calls"}]})).unwrap();
        assert_eq!(
            machine.finish(Some(1), 1, Some(10)),
            Err(StreamContractError::InvalidTool(ToolStreamError::MissingId))
        );
    }

    #[test]
    fn truncated_json_does_not_emit_tool_use() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"a","function":{"name":"Read","arguments":"{\"file\":"}}
        ]},"finish_reason":"tool_calls"}]})).unwrap();
        assert_eq!(
            machine.finish(Some(1), 1, Some(10)),
            Err(StreamContractError::InvalidTool(ToolStreamError::InvalidJsonArguments))
        );
    }

    #[test]
    fn capped_output_is_not_falsely_reported_as_success() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{"content":"many tokens"},"finish_reason":"stop"}]})).unwrap();
        assert_eq!(
            machine.finish(Some(32_001), 0, Some(32_000)),
            Err(StreamContractError::VerifiedOutputOverBudget)
        );
    }

    #[test]
    fn length_remains_max_tokens_and_never_auto_continues() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{"content":"part"},"finish_reason":"length"}]})).unwrap();
        let events = machine.finish(Some(4), 4, Some(32_000)).unwrap();
        assert_eq!(events[1].1["delta"]["stop_reason"], "max_tokens");
        assert_eq!(events[2].0, "message_stop");
    }

    #[test]
    fn missing_completion_marker_is_an_error() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{"content":"partial"}}]})).unwrap();
        assert_eq!(
            machine.finish(None, 3, Some(32_000)),
            Err(StreamContractError::MissingFinishReason)
        );
    }

    #[test]
    fn provider_usage_only_chunk_is_accepted() {
        let mut machine = StreamMachine::default();
        machine.ingest(&json!({"choices":[{"delta":{},"finish_reason":"stop"}]})).unwrap();
        assert!(machine.ingest(&json!({"choices":[],"usage":{"completion_tokens":5}})).unwrap().is_empty());
        assert_eq!(machine.finish(Some(5), 0, Some(32_000)).unwrap()[0].1["usage"]["output_tokens"], 5);
    }

    #[test]
    fn binary_oversized_arguments_fail_without_allocating_unbounded_buffers() {
        let mut machine = StreamMachine::default();
        let long = "x".repeat(MAX_TOOL_JSON_BYTES + 1);
        assert_eq!(
            machine.ingest(&json!({"choices":[{"delta":{"tool_calls":[
                {"index":0,"id":"a","function":{"name":"Read","arguments":long}}
            ]}}]})),
            Err(StreamContractError::ExcessiveToolArguments)
        );
    }
}
