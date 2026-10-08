//! Opt-in gateway contract preflight. No changes to legacy behavior in Off mode.
//! Observe records only numeric request metadata. Enforce refuses potentially
//! lossy requests *before* the legacy translator drops blocks.
use super::anthropic_contract::{validate_request, AdapterKind, ContractError};
use super::request_shape::RequestShape;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContractMode {
    Off,
    Observe,
    Enforce,
}

impl ContractMode {
    pub(crate) fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "observe" | "shadow" => Self::Observe,
            "enforce" => Self::Enforce,
            _ => Self::Off,
        }
    }
}

pub(crate) struct ContractAssessment {
    pub(crate) shape: RequestShape,
    pub(crate) error: Option<ContractError>,
}

impl ContractAssessment {
    pub(crate) fn failed(&self) -> bool {
        self.error.is_some()
    }

    // Deliberately excludes block values, model IDs, tool arguments, and
    // error text (which could contain an attacker-controlled block type).
    pub(crate) fn sanitized_summary(&self) -> String {
        format!(
            "messages={} tools={} tool_uses={} tool_results={} \
             unsupported_types={} request_bytes={} stream={} compatible={}",
            self.shape.messages,
            self.shape.tool_definitions,
            self.shape.assistant_tool_uses,
            self.shape.user_tool_results,
            self.shape.unknown_block_types,
            self.shape.serialized_request_bytes,
            self.shape.streaming,
            !self.failed()
        )
    }
}

pub(crate) fn check_chat_completions(
    mode: ContractMode,
    request: &Value,
) -> Option<ContractAssessment> {
    if mode == ContractMode::Off {
        return None;
    }
    Some(ContractAssessment {
        shape: RequestShape::inspect(request),
        error: validate_request(request, AdapterKind::ChatCompletions).err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flag_defaults_off_and_requires_explicit_enable() {
        assert_eq!(ContractMode::from_setting(""), ContractMode::Off);
        assert_eq!(ContractMode::from_setting("yes"), ContractMode::Off);
        assert_eq!(ContractMode::from_setting("shadow"), ContractMode::Observe);
        assert_eq!(
            ContractMode::from_setting(" ENFORCE "),
            ContractMode::Enforce
        );
    }

    #[test]
    fn default_mode_does_not_interfere_with_legacy_gateway() {
        let request = json!({"model":"model", "messages":[
          {"role":"user","content":[{"type":"document","text":"secret"}]}
        ]});
        assert!(check_chat_completions(ContractMode::Off, &request).is_none());
        let observed = check_chat_completions(ContractMode::Observe, &request).unwrap();
        assert!(observed.failed());
        let enforce = check_chat_completions(ContractMode::Enforce, &request).unwrap();
        assert!(enforce.failed());
        let summary = observed.sanitized_summary();
        assert!(!summary.contains("secret"));
        assert!(!summary.contains("document"));
        assert!(summary.contains("compatible=false"));
    }

    #[test]
    fn valid_tool_turn_passes_enforce_mode() {
        let request = json!({"model":"model", "messages":[
          {"role":"assistant","content":[{"type":"tool_use","id":"x","name":"Read","input":{}}]},
          {"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"ok"}]}
        ]});
        let assessment = check_chat_completions(ContractMode::Enforce, &request).unwrap();
        assert!(!assessment.failed());
        assert_eq!(assessment.shape.user_tool_results, 1);
    }
}
