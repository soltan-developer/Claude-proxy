//! Explicit, per-upstream-model capability declarations. No optimistic
//! inference from a model name or an OpenAI-compatible /models response.
//! All gate failures are static strings safe for logs and client errors.
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityMode {
    Off,
    Observe,
    Enforce,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capabilities {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub parallel_tools: bool,
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct CapabilitySettings {
    pub mode: CapabilityMode,
    profiles: Option<BTreeMap<String, Capabilities>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityFailure {
    MissingOrInvalidProfile,
    ToolsUnavailable,
    StreamingUnavailable,
    VisionUnavailable,
    ReasoningUnavailable,
    ParallelToolsUnavailable,
    OutputLimitExceeded,
}

impl CapabilityFailure {
    pub(crate) fn user_message(self) -> &'static str {
        match self {
            Self::MissingOrInvalidProfile => "No verified capability profile is configured for the selected upstream model.",
            Self::ToolsUnavailable => "The selected provider model is not verified for tool calling. Code and Cowork require a tool-capable model.",
            Self::StreamingUnavailable => "The selected provider model is not verified for streaming responses.",
            Self::VisionUnavailable => "The selected provider model is not verified for image input.",
            Self::ReasoningUnavailable => "The selected provider model is not verified for requested reasoning controls.",
            Self::ParallelToolsUnavailable => "The selected provider model is not verified for parallel tool calls.",
            Self::OutputLimitExceeded => "The provider model's verified output limit is lower than the requested upstream output limit.",
        }
    }
}

impl CapabilitySettings {
    pub(crate) fn from_strings(mode: &str, json: &str) -> Self {
        let mode = match mode.trim().to_ascii_lowercase().as_str() {
            "observe" | "shadow" => CapabilityMode::Observe,
            "enforce" => CapabilityMode::Enforce,
            _ => CapabilityMode::Off,
        };
        // Reject missing, malformed or ambiguously duplicated model profiles in
        // enforce mode; do not grant capabilities based on the route's name.
        let profiles = serde_json::from_str::<BTreeMap<String, Capabilities>>(json)
            .ok()
            .filter(|profiles| !profiles.is_empty());
        Self { mode, profiles }
    }

    pub(crate) fn assess(
        &self,
        upstream_model: &str,
        payload: &Value,
    ) -> Option<Result<(), CapabilityFailure>> {
        if self.mode == CapabilityMode::Off {
            return None;
        }
        let result = self
            .profiles
            .as_ref()
            .and_then(|profiles| profiles.get(upstream_model))
            .ok_or(CapabilityFailure::MissingOrInvalidProfile)
            .and_then(|capability| capability.check(payload));
        Some(result)
    }
}

impl Capabilities {
    fn check(&self, payload: &Value) -> Result<(), CapabilityFailure> {
        let messages = payload.get("messages").and_then(Value::as_array);
        let past_tools = messages.is_some_and(|messages| {
            messages.iter().any(|message| {
                message.get("role").and_then(Value::as_str) == Some("tool")
                    || message.get("tool_calls").and_then(Value::as_array)
                        .is_some_and(|calls| !calls.is_empty())
            })
        });
        let declared_tools = payload.get("tools").and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty());
        if (past_tools || declared_tools || payload.get("tool_choice").is_some())
            && !self.tools
        {
            return Err(CapabilityFailure::ToolsUnavailable);
        }
        if payload.get("stream").and_then(Value::as_bool) == Some(true)
            && !self.streaming
        {
            return Err(CapabilityFailure::StreamingUnavailable);
        }
        let has_vision = messages.is_some_and(|messages| {
            messages.iter().any(|message| {
                message.get("content").and_then(Value::as_array).is_some_and(|parts| {
                    parts.iter().any(|part| part.get("type").and_then(Value::as_str)
                        == Some("image_url"))
                })
            })
        });
        if has_vision && !self.vision {
            return Err(CapabilityFailure::VisionUnavailable);
        }
        if payload.get("reasoning_effort").is_some()
            || payload.pointer("/chat_template_kwargs/enable_thinking")
                .and_then(Value::as_bool) == Some(true)
        {
            if !self.reasoning {
                return Err(CapabilityFailure::ReasoningUnavailable);
            }
        }
        if payload.get("parallel_tool_calls").and_then(Value::as_bool) == Some(true)
            && !self.parallel_tools
        {
            return Err(CapabilityFailure::ParallelToolsUnavailable);
        }
        if let (Some(requested), Some(max)) = (
            payload.get("max_tokens").and_then(Value::as_u64),
            self.max_output_tokens,
        ) {
            if max == 0 || requested > max {
                return Err(CapabilityFailure::OutputLimitExceeded);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROFILES: &str = r#"{
        "upstream-code":{"tools":true,"streaming":true,"vision":false,
          "reasoning":false,"parallel_tools":true,"max_output_tokens":8000},
        "upstream-text":{"tools":false,"streaming":true,"vision":false,
          "reasoning":false,"parallel_tools":false,"max_output_tokens":2048}
    }"#;

    #[test]
    fn off_mode_is_safe_default_even_with_invalid_profiles() {
        let settings = CapabilitySettings::from_strings("", "{broken");
        assert!(settings.assess("unknown", &json!({"tools":[{}]})).is_none());
    }

    #[test]
    fn strict_mode_does_not_infer_tool_calling_from_model_name() {
        let gate = CapabilitySettings::from_strings("enforce", PROFILES);
        assert_eq!(
            gate.assess("unverified-claude-opus", &json!({"tools":[{}]})),
            Some(Err(CapabilityFailure::MissingOrInvalidProfile))
        );
        assert_eq!(
            gate.assess("upstream-text", &json!({"tools":[{"type":"function"}]})),
            Some(Err(CapabilityFailure::ToolsUnavailable))
        );
    }

    #[test]
    fn provider_verified_for_tools_accepts_a_code_request() {
        let gate = CapabilitySettings::from_strings("enforce", PROFILES);
        let request = json!({"stream":true,"max_tokens":4096,"tools":[
            {"type":"function","function":{"name":"Read"}}
        ],"messages":[{"role":"user","content":"inspect repo"}]});
        assert_eq!(gate.assess("upstream-code", &request), Some(Ok(())));
    }

    #[test]
    fn detects_history_tool_use_even_if_no_current_tools_are_declared() {
        let gate = CapabilitySettings::from_strings("enforce", PROFILES);
        let request = json!({"messages":[
            {"role":"assistant","tool_calls":[{"id":"a"}]},
            {"role":"tool","tool_call_id":"a","content":"result"}
        ]});
        assert_eq!(
            gate.assess("upstream-text", &request),
            Some(Err(CapabilityFailure::ToolsUnavailable))
        );
    }

    #[test]
    fn catches_vision_parallel_reasoning_and_output_limits() {
        let gate = CapabilitySettings::from_strings("enforce", PROFILES);
        assert_eq!(gate.assess("upstream-code", &json!({"messages":[{"content":[
            {"type":"image_url","image_url":{"url":"data:image/png;base64,..."}}
        ]}]})), Some(Err(CapabilityFailure::VisionUnavailable)));
        assert_eq!(gate.assess("upstream-text", &json!({"stream":false,
            "parallel_tool_calls":true})), Some(Err(CapabilityFailure::ParallelToolsUnavailable)));
        assert_eq!(gate.assess("upstream-code", &json!({"reasoning_effort":"high"})),
            Some(Err(CapabilityFailure::ReasoningUnavailable)));
        assert_eq!(gate.assess("upstream-code", &json!({"max_tokens":32000})),
            Some(Err(CapabilityFailure::OutputLimitExceeded)));
    }

    #[test]
    fn malformed_profile_fails_closed_in_enforce_mode() {
        let gate = CapabilitySettings::from_strings("enforce", r#"{
          "upstream-code":{"tools":"yes","streaming":true}
        }"#);
        assert_eq!(gate.assess("upstream-code", &json!({})),
            Some(Err(CapabilityFailure::MissingOrInvalidProfile)));
    }

    #[test]
    fn shadow_mode_observes_but_caller_decides_whether_to_block() {
        let gate = CapabilitySettings::from_strings("shadow", PROFILES);
        assert_eq!(gate.mode, CapabilityMode::Observe);
        assert_eq!(gate.assess("upstream-text", &json!({"tools":[{}]})),
            Some(Err(CapabilityFailure::ToolsUnavailable)));
    }
}
