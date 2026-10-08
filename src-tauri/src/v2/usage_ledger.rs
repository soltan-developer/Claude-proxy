//! Privacy-safe usage accounting: context size on one request is NOT the
//! cumulative total of provider-billed input tokens across requests.
use super::usage::UsageBreakdown;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LedgerError {
    DuplicateRound,
    TooManyRounds,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UsageLedger {
    rounds: Vec<UsageBreakdown>,
    ids: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UsageTotals {
    pub rounds: usize,
    pub billed_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub current_context_tokens: Option<u64>,
}

impl UsageLedger {
    pub(crate) fn append(
        &mut self,
        unique_round_id: &str,
        report: UsageBreakdown,
    ) -> Result<(), LedgerError> {
        if self.ids.contains(unique_round_id) {
            return Err(LedgerError::DuplicateRound);
        }
        if self.rounds.len() >= 10_000 {
            return Err(LedgerError::TooManyRounds);
        }
        self.ids.insert(unique_round_id.to_owned());
        self.rounds.push(report);
        Ok(())
    }

    pub(crate) fn totals(&self) -> UsageTotals {
        fn sum_reported(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
            values.fold(Some(0u64), |sum, next| {
                sum.and_then(|sum| sum.checked_add(next?))
            })
        }
        UsageTotals {
            rounds: self.rounds.len(),
            billed_input_tokens: sum_reported(self.rounds.iter().map(|r| r.total_input_tokens)),
            output_tokens: sum_reported(self.rounds.iter().map(|r| r.output_tokens)),
            cache_read_tokens: sum_reported(self.rounds.iter().map(|r| r.cache_read_input_tokens)),
            cache_write_tokens: sum_reported(self.rounds.iter().map(|r| r.cache_write_input_tokens)),
            current_context_tokens: self.rounds.last().and_then(|r| r.total_input_tokens),
        }
    }
}

/// These are byte-count diagnostics, **not** token counts. Provider-side model
/// templates, tool overhead and tokenizers make exact per-category token
/// attribution impossible from JSON bytes alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContextShape {
    pub total_request_bytes: usize,
    pub system_serialized_bytes: usize,
    pub tool_schemas_serialized_bytes: usize,
    pub preceding_messages_serialized_bytes: usize,
    pub last_message_serialized_bytes: usize,
    pub messages: usize,
    pub declared_tools: usize,
}

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |data| data.len())
}

impl ContextShape {
    pub(crate) fn inspect(request: &Value) -> Self {
        let messages = request.get("messages").and_then(Value::as_array);
        let tool_schemas = request.get("tools").and_then(Value::as_array);
        let system_bytes = request.get("system").map_or(0, serialized_len);
        let tool_bytes = request.get("tools").map_or(0, serialized_len);
        let mut history = 0usize;
        let mut last = 0usize;
        if let Some(messages) = messages {
            for (index, message) in messages.iter().enumerate() {
                let bytes = serialized_len(message);
                if index + 1 == messages.len() {
                    last = bytes;
                } else {
                    history = history.saturating_add(bytes);
                }
            }
        }
        Self {
            total_request_bytes: serialized_len(request),
            system_serialized_bytes: system_bytes,
            tool_schemas_serialized_bytes: tool_bytes,
            preceding_messages_serialized_bytes: history,
            last_message_serialized_bytes: last,
            messages: messages.map_or(0, Vec::len),
            declared_tools: tool_schemas.map_or(0, Vec::len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::usage::{UsageBreakdown, UsageProvenance};
    use serde_json::json;

    #[test]
    fn total_billed_input_is_not_the_current_context_window() {
        let mut ledger = UsageLedger::default();
        let first = UsageBreakdown::openai(&json!({
            "prompt_tokens":2000, "completion_tokens":150,
            "prompt_tokens_details":{"cached_tokens":0}
        }));
        let second = UsageBreakdown::openai(&json!({
            "prompt_tokens":2200, "completion_tokens":80,
            "prompt_tokens_details":{"cached_tokens":1800}
        }));
        ledger.append("req-a/round-1", first).unwrap();
        ledger.append("req-b/round-1", second).unwrap();
        let totals = ledger.totals();
        assert_eq!(totals.billed_input_tokens, Some(4200));
        assert_eq!(totals.current_context_tokens, Some(2200));
        assert_eq!(totals.cache_read_tokens, Some(1800));
        assert_eq!(totals.output_tokens, Some(230));
    }

    #[test]
    fn missing_cache_metadata_propagates_as_unknown_not_zero() {
        let mut ledger = UsageLedger::default();
        ledger.append("a", UsageBreakdown::openai(&json!({
            "prompt_tokens":100,"completion_tokens":20,
            "prompt_tokens_details":{"cached_tokens":0}
        }))).unwrap();
        ledger.append("b", UsageBreakdown::openai(&json!({
            "prompt_tokens":150,"completion_tokens":40
        }))).unwrap();
        assert_eq!(ledger.totals().billed_input_tokens, Some(250));
        assert_eq!(ledger.totals().cache_read_tokens, None);
        assert_eq!(ledger.totals().output_tokens, Some(60));
    }

    #[test]
    fn duplicate_round_cannot_be_counted_twice() {
        let mut ledger = UsageLedger::default();
        let empty = UsageBreakdown {
            total_input_tokens: None,
            uncached_input_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            output_tokens: None,
            provenance: UsageProvenance::Unreported,
        };
        ledger.append("same-request", empty.clone()).unwrap();
        assert_eq!(ledger.append("same-request", empty), Err(LedgerError::DuplicateRound));
        assert_eq!(ledger.totals().rounds, 1);
        assert_eq!(ledger.totals().billed_input_tokens, None);
    }

    #[test]
    fn request_breakdown_is_byte_only_and_never_exposes_secret_prompts() {
        let request = json!({
            "system":"SYSTEM_SECRET",
            "tools":[{"name":"Read","description":"TOOL_SECRET","input_schema":{
                "type":"object","properties":{"path":{"type":"string"}}
            }}],
            "messages":[
                {"role":"user","content":"HISTORY_SECRET"},
                {"role":"assistant","content":"answer"},
                {"role":"user","content":"CURRENT_SECRET"}
            ]
        });
        let shape = ContextShape::inspect(&request);
        assert_eq!(shape.messages, 3);
        assert_eq!(shape.declared_tools, 1);
        assert!(shape.system_serialized_bytes > 0);
        assert!(shape.tool_schemas_serialized_bytes > 0);
        assert!(shape.preceding_messages_serialized_bytes > shape.last_message_serialized_bytes);
        let diagnostic = serde_json::to_string(&shape).unwrap();
        for secret in ["SYSTEM_SECRET","TOOL_SECRET","HISTORY_SECRET","CURRENT_SECRET"] {
            assert!(!diagnostic.contains(secret));
        }
    }
}
