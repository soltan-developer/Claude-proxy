//! Usage contracts. Unknown/cache-unreported is different from zero cached tokens.
//! OpenAI prompt_tokens includes cached reads; Anthropic input_tokens excludes
//! cache creation and cache reads. Do not conflate these semantics.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UsageProvenance {
    Provider,
    Unreported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UsageBreakdown {
    pub total_input_tokens: Option<u64>,
    pub uncached_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_write_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub provenance: UsageProvenance,
}

impl UsageBreakdown {
    pub(crate) fn openai(value: &Value) -> Self {
        let prompt = value.get("prompt_tokens").and_then(Value::as_u64);
        let read = value
            .pointer("/prompt_tokens_details/cached_tokens")
            .or_else(|| value.pointer("/input_tokens_details/cached_tokens"))
            .and_then(Value::as_u64);
        // A provider may report cache writes; they are already included in
        // prompt_tokens, not additive to it.
        let write = value
            .pointer("/prompt_tokens_details/cache_write_tokens")
            .or_else(|| value.pointer("/input_tokens_details/cache_write_tokens"))
            .and_then(Value::as_u64);
        let uncached = prompt.and_then(|total| {
            let read = read.unwrap_or(0);
            let write = write.unwrap_or(0);
            total.checked_sub(read)?.checked_sub(write)
        });
        Self {
            total_input_tokens: prompt,
            uncached_input_tokens: uncached,
            cache_read_input_tokens: read,
            cache_write_input_tokens: write,
            output_tokens: value.get("completion_tokens").and_then(Value::as_u64),
            provenance: if prompt.is_some() {
                UsageProvenance::Provider
            } else {
                UsageProvenance::Unreported
            },
        }
    }

    pub(crate) fn anthropic(value: &Value) -> Self {
        let fresh = value.get("input_tokens").and_then(Value::as_u64);
        let read = value.get("cache_read_input_tokens").and_then(Value::as_u64);
        let write = value
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64);
        let total = fresh.map(|n| {
            n.saturating_add(read.unwrap_or(0))
                .saturating_add(write.unwrap_or(0))
        });
        Self {
            total_input_tokens: total,
            uncached_input_tokens: fresh,
            cache_read_input_tokens: read,
            cache_write_input_tokens: write,
            output_tokens: value.get("output_tokens").and_then(Value::as_u64),
            provenance: if fresh.is_some() {
                UsageProvenance::Provider
            } else {
                UsageProvenance::Unreported
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_cached_tokens_are_not_double_counted() {
        let u = UsageBreakdown::openai(&json!({
            "prompt_tokens":1000,"completion_tokens":30,
            "prompt_tokens_details":{"cached_tokens":800}
        }));
        assert_eq!(u.total_input_tokens, Some(1000));
        assert_eq!(u.uncached_input_tokens, Some(200));
        assert_eq!(u.cache_read_input_tokens, Some(800));
        assert_eq!(u.cache_write_input_tokens, None);
    }
    #[test]
    fn anthropic_input_is_additive_with_cache() {
        let u = UsageBreakdown::anthropic(&json!({
            "input_tokens":200,"cache_read_input_tokens":800,
            "cache_creation_input_tokens":100,"output_tokens":30
        }));
        assert_eq!(u.total_input_tokens, Some(1100));
        assert_eq!(u.uncached_input_tokens, Some(200));
        assert_eq!(u.cache_read_input_tokens, Some(800));
    }
    #[test]
    fn provider_without_usage_does_not_claim_zero_cache_hits() {
        let u = UsageBreakdown::openai(&json!({}));
        assert_eq!(u.provenance, UsageProvenance::Unreported);
        assert_eq!(u.cache_read_input_tokens, None);
        assert_eq!(u.total_input_tokens, None);
    }
    #[test]
    fn invalid_openai_cached_overflow_is_not_misreported_as_uncached() {
        let u = UsageBreakdown::openai(&json!({
            "prompt_tokens":100,"prompt_tokens_details":{"cached_tokens":150}
        }));
        assert_eq!(u.total_input_tokens, Some(100));
        assert_eq!(u.uncached_input_tokens, None);
    }
}
