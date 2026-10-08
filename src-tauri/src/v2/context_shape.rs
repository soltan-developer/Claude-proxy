//! Privacy-safe byte attribution for context growth. This is NOT a tokenizer.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ContextShape {
    pub request_bytes: usize,
    pub system_bytes: usize,
    pub tools_bytes: usize,
    pub prior_messages_bytes: usize,
    pub latest_message_bytes: usize,
    pub messages_count: usize,
    pub tools_count: usize,
}

fn size(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |v| v.len())
}

impl ContextShape {
    pub(crate) fn inspect(value: &Value) -> Self {
        let messages = value.get("messages").and_then(Value::as_array);
        let tools = value.get("tools").and_then(Value::as_array);
        let mut prior_messages_bytes = 0;
        let mut latest_message_bytes = 0;
        if let Some(messages) = messages {
            for (index, item) in messages.iter().enumerate() {
                if index + 1 == messages.len() {
                    latest_message_bytes = size(item);
                } else {
                    prior_messages_bytes += size(item);
                }
            }
        }
        Self {
            request_bytes: size(value),
            system_bytes: value.get("system").map_or(0, size),
            tools_bytes: value.get("tools").map_or(0, size),
            prior_messages_bytes,
            latest_message_bytes,
            messages_count: messages.map_or(0, Vec::len),
            tools_count: tools.map_or(0, Vec::len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exposes_byte_counts_not_prompt_contents_or_fabricated_tokens() {
        let request = json!({
            "system":"system policy",
            "tools":[{"name":"Read","description":"read a local file"}],
            "messages":[
                {"role":"user","content":"first"},
                {"role":"assistant","content":"reply"},
                {"role":"user","content":"last"}
            ]
        });
        let shape = ContextShape::inspect(&request);
        assert_eq!(shape.messages_count, 3);
        assert_eq!(shape.tools_count, 1);
        assert!(shape.prior_messages_bytes > shape.latest_message_bytes);
        assert!(shape.system_bytes > 0);
        assert!(shape.tools_bytes > 0);
        let output = serde_json::to_string(&shape).unwrap();
        assert!(!output.contains("system policy"));
        assert!(!output.contains("read a local file"));
        assert!(!output.contains("first"));
        assert!(!output.contains("last"));
    }
}
