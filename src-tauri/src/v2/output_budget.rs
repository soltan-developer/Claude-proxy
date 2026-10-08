//! Safe v2 output negotiation. No guessed context size, hidden history
//! deletion or invisible auto-continuation. Callers must supply measured usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputBudget {
    pub allowed_output_tokens: u64,
    pub measured_input_tokens: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetError {
    NeedMeasuredInput,
    InvalidLimits,
    ContextExhausted,
}

pub(crate) fn negotiate(
    requested: u64,
    client_hard_cap: u64,
    provider_cap: u64,
    context_window: u64,
    measured_input: Option<u64>,
    reserve: u64,
) -> Result<OutputBudget, BudgetError> {
    if requested == 0 || client_hard_cap == 0 || provider_cap == 0 || context_window == 0 {
        return Err(BudgetError::InvalidLimits);
    }
    let input = measured_input.ok_or(BudgetError::NeedMeasuredInput)?;
    let room = context_window
        .checked_sub(input)
        .and_then(|v| v.checked_sub(reserve))
        .ok_or(BudgetError::ContextExhausted)?;
    if room == 0 {
        return Err(BudgetError::ContextExhausted);
    }
    Ok(OutputBudget {
        allowed_output_tokens: requested.min(client_hard_cap).min(provider_cap).min(room),
        measured_input_tokens: input,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_limit_is_never_violated_by_provider_budget() {
        let budget = negotiate(64000, 32000, 128000, 128000, Some(40000), 512).unwrap();
        assert_eq!(budget.allowed_output_tokens, 32000);
    }
    #[test]
    fn no_estimate_is_silently_presented_as_real_count() {
        assert_eq!(
            negotiate(8000, 32000, 32000, 128000, None, 512),
            Err(BudgetError::NeedMeasuredInput)
        );
    }
    #[test]
    fn reserved_context_cannot_be_consumed_by_output() {
        let budget = negotiate(32000, 32000, 32000, 128000, Some(127000), 512).unwrap();
        assert_eq!(budget.allowed_output_tokens, 488);
    }
    #[test]
    fn fully_exhausted_context_is_explicit_error() {
        assert_eq!(
            negotiate(4000, 32000, 32000, 10000, Some(9900), 512),
            Err(BudgetError::ContextExhausted)
        );
    }
}
