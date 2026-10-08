//! Provider Chat Completions validation BEFORE legacy JSON translation.
//! Prevent fabricated tool names, IDs or empty arguments from a malformed
//! upstream response. This is opt-in with V2 strict contracts.
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseFailure {
    MissingChoices,
    MissingMessage,
    MissingFinishReason,
    UnsupportedFinishReason,
    MissingToolCalls,
    ContradictoryToolFinish,
    ToolIdentityMissing,
    RepeatedToolId,
    InvalidToolArguments,
    PartialToolCalls,
}

impl ResponseFailure {
    pub(crate) fn user_message(self) -> &'static str {
        "The provider returned an incomplete or invalid tool response. No tool call can safely be forwarded."
    }
}

pub(crate) fn validate_completion(value: &Value) -> Result<(), ResponseFailure> {
    let choice = value.pointer("/choices/0").ok_or(ResponseFailure::MissingChoices)?;
    let message = choice.get("message").ok_or(ResponseFailure::MissingMessage)?;
    let finish = choice.get("finish_reason").and_then(Value::as_str)
        .ok_or(ResponseFailure::MissingFinishReason)?;
    if !matches!(finish, "stop" | "length" | "tool_calls") {
        return Err(ResponseFailure::UnsupportedFinishReason);
    }
    let calls = message.get("tool_calls").and_then(Value::as_array);
    let has_calls = calls.is_some_and(|calls| !calls.is_empty());
    if finish == "tool_calls" && !has_calls {
        return Err(ResponseFailure::MissingToolCalls);
    }
    if has_calls && finish == "length" {
        return Err(ResponseFailure::PartialToolCalls);
    }
    if has_calls && finish != "tool_calls" {
        return Err(ResponseFailure::ContradictoryToolFinish);
    }
    if let Some(calls) = calls {
        let mut ids = HashSet::new();
        for call in calls {
            let id = call.get("id").and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(ResponseFailure::ToolIdentityMissing)?;
            let name = call.pointer("/function/name").and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(ResponseFailure::ToolIdentityMissing)?;
            if !ids.insert(id) {
                return Err(ResponseFailure::RepeatedToolId);
            }
            if name.len() > 256 {
                return Err(ResponseFailure::ToolIdentityMissing);
            }
            let arguments = call.pointer("/function/arguments").and_then(Value::as_str)
                .ok_or(ResponseFailure::InvalidToolArguments)?;
            if arguments.len() > 2 * 1024 * 1024
                || serde_json::from_str::<Value>(arguments)
                    .ok().is_none_or(|parsed| !parsed.is_object())
            {
                return Err(ResponseFailure::InvalidToolArguments);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_complete_parallel_client_tool_calls() {
        let reply = json!({"choices":[{"finish_reason":"tool_calls","message":{
            "role":"assistant","tool_calls":[
                {"id":"call-1","function":{"name":"Read","arguments":"{\"path\":\"a\"}"}},
                {"id":"call-2","function":{"name":"Grep","arguments":"{\"pattern\":\"b\"}"}}
            ]}}]});
        assert_eq!(validate_completion(&reply), Ok(()));
    }
    #[test]
    fn rejects_missing_identity_instead_of_fabricating_tool_call() {
        let reply = json!({"choices":[{"finish_reason":"tool_calls","message":{
            "tool_calls":[{"function":{"name":"Read","arguments":"{}"}}]
        }}]});
        assert_eq!(validate_completion(&reply), Err(ResponseFailure::ToolIdentityMissing));
    }
    #[test]
    fn refuses_broken_json_or_duplicate_ids() {
        let broken = json!({"choices":[{"finish_reason":"tool_calls","message":{
            "tool_calls":[{"id":"call","function":{"name":"Read","arguments":"{bad"}}]
        }}]});
        assert_eq!(validate_completion(&broken), Err(ResponseFailure::InvalidToolArguments));
        let dup = json!({"choices":[{"finish_reason":"tool_calls","message":{
            "tool_calls":[
                {"id":"duplicate","function":{"name":"Read","arguments":"{}"}},
                {"id":"duplicate","function":{"name":"Edit","arguments":"{}"}}
            ]
        }}]});
        assert_eq!(validate_completion(&dup), Err(ResponseFailure::RepeatedToolId));
    }
    #[test]
    fn length_is_not_a_valid_end_for_a_partial_tool_call() {
        let r = json!({"choices":[{"finish_reason":"length","message":{
            "tool_calls":[{"id":"id","function":{"name":"Read","arguments":"{}"}}]
        }}]});
        assert_eq!(validate_completion(&r), Err(ResponseFailure::PartialToolCalls));
    }
    #[test]
    fn text_truncation_remains_distinguishable_from_success() {
        assert_eq!(validate_completion(&json!({"choices":[{"finish_reason":"length",
            "message":{"role":"assistant","content":"partial"}}]})), Ok(()));
    }
    #[test]
    fn tool_finish_without_calls_is_rejected() {
        assert_eq!(validate_completion(&json!({"choices":[{"finish_reason":"tool_calls",
            "message":{"content":"hello"}}]})), Err(ResponseFailure::MissingToolCalls));
    }
}
