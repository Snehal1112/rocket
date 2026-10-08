//! Collects what one Flow step saw: the value on each wire into it, how a
//! routing node decided, and which wire failed. Every value is masked first
//! and capped second, so a cap can never cut a secret in half and show the rest.

use std::collections::HashSet;

use rocket_flow::FlowEdge;
use rocket_shared::events::{FlowRouteEval, FlowStepTrace, FlowWireValue};

use crate::flow_debug::cap_text;
use crate::redaction::redact_secrets;

/// The largest value an Input, Transform or Output step reports, in bytes.
pub(crate) const STEP_VALUE_LIMIT: usize = crate::flow_debug::EXCHANGE_BODY_LIMIT;

/// The largest value one wire record keeps, in bytes.
pub(crate) const WIRE_VALUE_LIMIT: usize = 16_384;
/// The most wire value bytes one step keeps over all its wires.
pub(crate) const WIRE_TOTAL_LIMIT: usize = 65_536;
/// The largest Switch value a route record keeps, in bytes.
pub(crate) const ROUTE_VALUE_LIMIT: usize = 1_024;

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
    /// Wire value bytes recorded so far, for `WIRE_TOTAL_LIMIT`.
    wire_bytes: usize,
}

impl NodeTrace {
    /// The finished trace, or `None` when nothing was recorded.
    pub(crate) fn into_trace(self) -> Option<FlowStepTrace> {
        (self.step != FlowStepTrace::default()).then_some(self.step)
    }

    /// Records the value `edge` delivered, masked, then cut to the wire limit
    /// and to what is left of the step total.
    pub(crate) fn record_wire(&mut self, edge: &FlowEdge, raw: &str, masks: &HashSet<String>) {
        let (mut value, mut truncated) = mask_then_cap(raw, masks, WIRE_VALUE_LIMIT);
        let room = WIRE_TOTAL_LIMIT.saturating_sub(self.wire_bytes);
        if cap_text(&mut value, room) {
            truncated = true;
        }
        self.wire_bytes += value.len();
        self.step.wires.push(FlowWireValue {
            value: Some(value),
            truncated,
            ..blank_wire(edge)
        });
    }

    /// Records an `auth` wire. Its credential is never read or recorded.
    pub(crate) fn record_credential_wire(&mut self, edge: &FlowEdge) {
        self.step.wires.push(FlowWireValue {
            credential: true,
            ..blank_wire(edge)
        });
    }

    /// Records how a routing node decided. `raw` is masked, then capped.
    pub(crate) fn record_route(
        &mut self,
        kind: &str,
        raw: &str,
        matched_case: Option<String>,
        masks: &HashSet<String>,
    ) {
        let (value, _) = mask_then_cap(raw, masks, ROUTE_VALUE_LIMIT);
        self.step.route = Some(FlowRouteEval {
            kind: kind.to_string(),
            value,
            matched_case,
        });
    }
}

/// A wire record that names `edge` and holds nothing else yet.
fn blank_wire(edge: &FlowEdge) -> FlowWireValue {
    FlowWireValue {
        edge_id: edge.id.clone(),
        source_node_id: edge.source_node_id.clone(),
        target_field: edge.target_field.clone(),
        ..Default::default()
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

    fn edge(id: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: "src".to_string(),
            target_node_id: "dst".to_string(),
            target_field: field.to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        }
    }

    #[test]
    fn record_wire_masks_and_names_the_wire() {
        let mut trace = NodeTrace::default();
        trace.record_wire(
            &edge("e1", "body"),
            "key=sk-live-123456",
            &masks(&["sk-live-123456"]),
        );
        let wire = &trace.step.wires[0];
        assert_eq!(wire.edge_id, "e1");
        assert_eq!(wire.source_node_id, "src");
        assert_eq!(wire.target_field, "body");
        assert_eq!(
            wire.value.as_deref(),
            Some(format!("key={REDACTED}").as_str())
        );
        assert!(!wire.truncated && !wire.credential && wire.error.is_none());
    }

    #[test]
    fn record_wire_cuts_one_value_at_the_wire_limit() {
        let mut trace = NodeTrace::default();
        trace.record_wire(
            &edge("e1", "body"),
            &"x".repeat(WIRE_VALUE_LIMIT + 1),
            &masks(&[]),
        );
        let wire = &trace.step.wires[0];
        assert!(wire.truncated);
        assert_eq!(wire.value.as_ref().map(String::len), Some(WIRE_VALUE_LIMIT));
    }

    #[test]
    fn record_wire_stops_at_the_step_total() {
        let mut trace = NodeTrace::default();
        let big = "x".repeat(WIRE_VALUE_LIMIT);
        for i in 0..5 {
            trace.record_wire(&edge(&format!("e{i}"), "body"), &big, &masks(&[]));
        }
        let total: usize = trace
            .step
            .wires
            .iter()
            .filter_map(|w| w.value.as_ref())
            .map(String::len)
            .sum();
        assert_eq!(total, WIRE_TOTAL_LIMIT);
        assert_eq!(trace.step.wires.len(), 5, "every wire is still listed");
        assert!(trace.step.wires[4].truncated);
        assert_eq!(trace.step.wires[4].value.as_deref(), Some(""));
    }

    #[test]
    fn a_credential_wire_has_no_value() {
        let mut trace = NodeTrace::default();
        trace.record_credential_wire(&edge("ea", rocket_flow::handle::AUTH));
        let wire = &trace.step.wires[0];
        assert!(wire.credential);
        assert_eq!(wire.value, None);
    }

    #[test]
    fn record_route_masks_and_caps_the_value() {
        let mut trace = NodeTrace::default();
        let raw = format!("sk-live-123456{}", "y".repeat(ROUTE_VALUE_LIMIT));
        trace.record_route(
            "switch",
            &raw,
            Some("c1".into()),
            &masks(&["sk-live-123456"]),
        );
        let route = trace.step.route.expect("route");
        assert_eq!(route.kind, "switch");
        assert!(route.value.starts_with(REDACTED));
        assert!(route.value.len() <= ROUTE_VALUE_LIMIT);
        assert_eq!(route.matched_case.as_deref(), Some("c1"));
    }
}
