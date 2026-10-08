//! Collects what one Flow step saw: the value on each wire into it, how a
//! routing node decided, and which wire failed. Every value is masked first
//! and capped second, so a cap can never cut a secret in half and show the rest.

use std::collections::HashSet;

use rocket_shared::events::FlowStepTrace;

use crate::flow_debug::cap_text;
use crate::redaction::redact_secrets;

/// The largest value an Input, Transform or Output step reports, in bytes.
pub(crate) const STEP_VALUE_LIMIT: usize = crate::flow_debug::EXCHANGE_BODY_LIMIT;

/// Masks `raw` with `masks`, then cuts it to `limit` bytes. Returns the text
/// and whether it was cut.
pub(crate) fn mask_then_cap(raw: &str, masks: &HashSet<String>, limit: usize) -> (String, bool) {
    let mut text = redact_secrets(raw, masks);
    let cut = cap_text(&mut text, limit);
    (text, cut)
}

/// The trace of the node that is running. `execute_node` fills it as it goes,
/// so what was recorded before an error still reaches the step.
#[derive(Debug, Default)]
pub(crate) struct NodeTrace {
    pub(crate) step: FlowStepTrace,
}

impl NodeTrace {
    /// The finished trace, or `None` when nothing was recorded.
    pub(crate) fn into_trace(self) -> Option<FlowStepTrace> {
        (self.step != FlowStepTrace::default()).then_some(self.step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::REDACTED;

    fn masks(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn an_untouched_trace_is_none() {
        assert_eq!(NodeTrace::default().into_trace(), None);
    }

    #[test]
    fn a_trace_with_only_a_value_cut_is_kept() {
        let mut trace = NodeTrace::default();
        trace.step.value_truncated = true;
        assert!(trace.into_trace().is_some_and(|t| t.value_truncated));
    }

    #[test]
    fn mask_then_cap_masks_before_it_cuts() {
        // Cut first, the limit would keep "sk-l" of the secret.
        let raw = format!("{}sk-live-123456", "a".repeat(20));
        let (text, cut) = mask_then_cap(&raw, &masks(&["sk-live-123456"]), 24);
        assert!(cut);
        assert!(!text.contains("sk-"), "{text}");
        assert!(text.len() <= 24);
        assert!(text.starts_with(&"a".repeat(20)));
    }

    #[test]
    fn mask_then_cap_leaves_short_text_alone() {
        let (text, cut) = mask_then_cap("token sk-live-123456", &masks(&["sk-live-123456"]), 100);
        assert!(!cut);
        assert_eq!(text, format!("token {REDACTED}"));
    }
}
