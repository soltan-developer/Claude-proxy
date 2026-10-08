//! Opt-in output guard for Claude Desktop/Cowork's observed 32k token cap.
//! The cap is negotiated BEFORE provider calls, including follow-up rounds.
//! Provider-reported usage is mandatory for a VERIFIED output-cap assertion.
use serde_json::{json, Value};

pub(crate) const DESKTOP_HARD_CAP: u64 = 32_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputGuardError {
    MissingOrInvalidClientLimit,
    InvalidProviderBudget,
    MissingProviderUsage,
    ProviderExceededBudget,
}

impl OutputGuardError {
    pub(crate) fn user_message(self) -> &'static str {
        match self {
            Self::MissingOrInvalidClientLimit => "A positive max_tokens value is required by the experimental Desktop output guard.",
            Self::InvalidProviderBudget => "The selected provider returned an invalid output-budget configuration.",
            Self::MissingProviderUsage => "The provider omitted measured completion token usage; the strict Desktop output limit cannot be verified.",
            Self::ProviderExceededBudget => "The provider exceeded the negotiated Desktop output-token budget; its response was not forwarded.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClientOutputGuard {
    pub limit: u64,
}

impl ClientOutputGuard {
    pub(crate) fn enabled(strict_contract: bool, flag: &str) -> bool {
        strict_contract && flag.trim().eq_ignore_ascii_case("on")
    }

    pub(crate) fn negotiate(
        request: &Value,
        upstream_payload: &Value,
        route_output_limit: u64,
    ) -> Result<Self, OutputGuardError> {
        let requested = request.get("max_tokens").and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or(OutputGuardError::MissingOrInvalidClientLimit)?;
        let mut limit = requested.min(DESKTOP_HARD_CAP);
        if route_output_limit > 0 {
            limit = limit.min(route_output_limit);
        }
        if let Some(already_capped) = upstream_payload.get("max_tokens") {
            let value = already_capped.as_u64()
                .filter(|n| *n > 0)
                .ok_or(OutputGuardError::InvalidProviderBudget)?;
            limit = limit.min(value);
        }
        if limit == 0 {
            return Err(OutputGuardError::InvalidProviderBudget);
        }
        Ok(Self { limit })
    }

    /// Called directly before every provider call, not only the first one.
    /// Tool / retry code may otherwise enlarge the output after negotiation.
    pub(crate) fn apply(self, payload: &mut Value) {
        let budget = payload.get("max_tokens").and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .map_or(self.limit, |value| value.min(self.limit));
        payload["max_tokens"] = json!(budget);
    }

    pub(crate) fn verify_reported_output(self, measured: Option<u64>)
        -> Result<(), OutputGuardError>
    {
        let actual = measured.ok_or(OutputGuardError::MissingProviderUsage)?;
        if actual > self.limit {
            return Err(OutputGuardError::ProviderExceededBudget);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn disabled_by_default_and_requires_strict_contract() {
        assert!(!ClientOutputGuard::enabled(false, "on"));
        assert!(!ClientOutputGuard::enabled(true, ""));
        assert!(!ClientOutputGuard::enabled(true, "true"));
        assert!(ClientOutputGuard::enabled(true, " ON "));
    }

    #[test]
    fn client_64000_is_limited_to_32000_before_provider() {
        let guard = ClientOutputGuard::negotiate(
            &json!({"max_tokens":64_000}),
            &json!({"max_tokens":64_000}),
            128_000,
        ).unwrap();
        assert_eq!(guard.limit, DESKTOP_HARD_CAP);
        let mut provider = json!({"max_tokens":64_000});
        guard.apply(&mut provider);
        assert_eq!(provider["max_tokens"], 32_000);
    }

    #[test]
    fn a_small_client_budget_is_respected() {
        let guard = ClientOutputGuard::negotiate(
            &json!({"max_tokens":512}), &json!({"max_tokens":256}), 4096,
        ).unwrap();
        assert_eq!(guard.limit, 256);
        let mut next = json!({"max_tokens":40_000});
        guard.apply(&mut next);
        assert_eq!(next["max_tokens"], 256);
    }

    #[test]
    fn an_unreported_provider_count_is_not_verified_as_zero() {
        let guard = ClientOutputGuard { limit:32_000 };
        assert_eq!(guard.verify_reported_output(None),
            Err(OutputGuardError::MissingProviderUsage));
        assert_eq!(guard.verify_reported_output(Some(32_001)),
            Err(OutputGuardError::ProviderExceededBudget));
        assert_eq!(guard.verify_reported_output(Some(32_000)), Ok(()));
    }

    #[test]
    fn invalid_client_request_fails_instead_of_guessing() {
        assert_eq!(ClientOutputGuard::negotiate(
            &json!({}), &json!({}), 0,
        ), Err(OutputGuardError::MissingOrInvalidClientLimit));
        assert_eq!(ClientOutputGuard::negotiate(
            &json!({"max_tokens":0}), &json!({}), 0,
        ), Err(OutputGuardError::MissingOrInvalidClientLimit));
    }

    #[test]
    fn invalid_provider_max_tokens_never_turns_into_one_token() {
        assert_eq!(ClientOutputGuard::negotiate(
            &json!({"max_tokens":1024}), &json!({"max_tokens":0}), 0,
        ), Err(OutputGuardError::InvalidProviderBudget));
    }
}
