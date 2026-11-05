//! Events the ACP agent and assistant conversations publish.

use crate::{ConfigOption, PermissionOption};
use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// An ACP agent session changed, or a stored assistant conversation changed.
// Only the unit tests below read events back, so no Deserialize is derived.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AcpEvent {
    /// Emitted once a spawned agent process completes its ACP handshake.
    AcpSessionStarted { session_id: String },
    /// Emitted per streamed text chunk while a prompt is being answered.
    AcpSessionChunk { session_id: String, text: String },
    /// Emitted per streamed reasoning chunk while a prompt is being answered.
    AcpThoughtChunk { session_id: String, text: String },
    /// Emitted once a prompt's turn ends. `stop_reason` is the raw ACP
    /// `stopReason` string (`end_turn`, `max_tokens`, `max_turn_requests`,
    /// `refusal`, `cancelled`, or an agent-specific custom reason) — kept as
    /// a plain string rather than a fixed enum because ACP's own
    /// `StopReason` type is `#[non_exhaustive]`.
    AcpSessionFinished {
        session_id: String,
        stop_reason: String,
    },
    /// Emitted when a session ends abnormally: the agent process crashed,
    /// a protocol-level error occurred, or `send_prompt` timed out.
    AcpSessionFailed { session_id: String, error: String },
    /// Emitted by `McpToolService` after a successful call of a read tool or
    /// `run_request`, so agent-driven actions are auditable and
    /// distinguishable from manual ones. Refused or failed calls publish
    /// nothing. `propose_changes` and `list_proposals` do not publish it;
    /// proposals have `AcpProposalCreated` and `AcpProposalResolved`. No
    /// timestamp field, matching every other event —
    /// ordering comes from emission order, not a payload timestamp.
    AcpToolInvoked {
        session_id: String,
        tool: String,
        summary: String,
    },
    /// Emitted once per proposal the assistant queues. Nothing was written.
    AcpProposalCreated {
        session_id: String,
        proposal_id: String,
        summary: String,
    },
    /// Emitted when a proposal leaves `pending`. `status` is `accepted`,
    /// `rejected`, `stale` or `failed`.
    AcpProposalResolved {
        session_id: String,
        proposal_id: String,
        status: String,
    },
    /// Emitted for every tool call start or change during a prompt turn.
    /// `title` and `status` are the last known values, so the UI can upsert
    /// by `call_id`. `status` is `pending`, `in_progress`, `completed` or
    /// `failed`.
    AcpToolActivity {
        session_id: String,
        call_id: String,
        title: String,
        status: String,
    },
    /// Emitted with the agent's full option list whenever it changes, during
    /// a turn or after `set_config_option`.
    AcpConfigOptionsChanged {
        session_id: String,
        options: Vec<ConfigOption>,
    },
    /// Emitted when the agent reports context window use. `cost_usd` is the
    /// cumulative session cost, present only when reported in US dollars.
    AcpUsage {
        session_id: String,
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
    /// Emitted when the agent asks the user to approve something. The turn
    /// waits for the answer. `kind` is the ACP tool kind in snake_case;
    /// `summary` is short, masked text about the call.
    AcpPermissionRequested {
        session_id: String,
        request_id: String,
        call_id: String,
        title: String,
        kind: String,
        summary: String,
        options: Vec<PermissionOption>,
    },
    /// Emitted when a permission request was answered. `option_id` is the
    /// chosen option, or `None` when the request was cancelled.
    AcpPermissionResolved {
        session_id: String,
        request_id: String,
        option_id: Option<String>,
    },
    /// Emitted after an assistant conversation was saved. Carries the id
    /// only, never message text.
    AssistantConversationSaved { conversation_id: String },
    /// Emitted after one stored assistant conversation was deleted.
    AssistantConversationDeleted { conversation_id: String },
    /// Emitted after every stored conversation of the workspace was deleted.
    AssistantConversationsCleared,
}

impl PublishedEvent for AcpEvent {
    fn channel(&self) -> &'static str {
        match self {
            AcpEvent::AcpSessionStarted { .. } => "agent-session-started",
            AcpEvent::AcpSessionChunk { .. } => "agent-session-chunk",
            AcpEvent::AcpThoughtChunk { .. } => "agent-session-thought",
            AcpEvent::AcpSessionFinished { .. } => "agent-session-finished",
            AcpEvent::AcpSessionFailed { .. } => "agent-session-failed",
            AcpEvent::AcpToolInvoked { .. } => "agent-tool-invoked",
            AcpEvent::AcpProposalCreated { .. } => "agent-proposal-created",
            AcpEvent::AcpProposalResolved { .. } => "agent-proposal-resolved",
            AcpEvent::AcpToolActivity { .. } => "agent-session-tool-activity",
            AcpEvent::AcpConfigOptionsChanged { .. } => "agent-session-config-options",
            AcpEvent::AcpUsage { .. } => "agent-session-usage",
            AcpEvent::AcpPermissionRequested { .. } => "agent-permission-requested",
            AcpEvent::AcpPermissionResolved { .. } => "agent-permission-resolved",
            AcpEvent::AssistantConversationSaved { .. } => "assistant-conversation-saved",
            AcpEvent::AssistantConversationDeleted { .. } => "assistant-conversation-deleted",
            AcpEvent::AssistantConversationsCleared => "assistant-conversations-cleared",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfigChoice, ConfigOption, PermissionOption, PermissionOptionKind};
    use rocket_shared::published_event::PublishedEvent;
    use serde_json::json;

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn config_option() -> ConfigOption {
        ConfigOption {
            id: s("model"),
            name: s("Model"),
            category: Some(s("model")),
            current_value: s("opus"),
            choices: vec![ConfigChoice {
                value: s("opus"),
                name: s("Opus"),
                description: None,
            }],
        }
    }

    fn permission_option() -> PermissionOption {
        PermissionOption {
            id: s("proceed_once"),
            name: s("Allow once"),
            kind: PermissionOptionKind::AllowOnce,
        }
    }

    /// Every variant (with the `None` forms too), its JSON and its channel.
    /// The JSON was captured from the old `DomainEvent` variants.
    fn cases() -> Vec<(AcpEvent, serde_json::Value, &'static str)> {
        vec![
            (
                AcpEvent::AcpSessionStarted {
                    session_id: s("sess-1"),
                },
                json!({"type":"acpSessionStarted","session_id":"sess-1"}),
                "agent-session-started",
            ),
            (
                AcpEvent::AcpSessionChunk {
                    session_id: s("sess-1"),
                    text: s("Hello, "),
                },
                json!({"type":"acpSessionChunk","session_id":"sess-1","text":"Hello, "}),
                "agent-session-chunk",
            ),
            (
                AcpEvent::AcpThoughtChunk {
                    session_id: s("sess-1"),
                    text: s("Checking the auth header. "),
                },
                json!({"type":"acpThoughtChunk","session_id":"sess-1","text":"Checking the auth header. "}),
                "agent-session-thought",
            ),
            (
                AcpEvent::AcpSessionFinished {
                    session_id: s("sess-1"),
                    stop_reason: s("_custom_agent_reason"),
                },
                json!({"type":"acpSessionFinished","session_id":"sess-1","stop_reason":"_custom_agent_reason"}),
                "agent-session-finished",
            ),
            (
                AcpEvent::AcpSessionFailed {
                    session_id: s("sess-1"),
                    error: s("agent process exited unexpectedly"),
                },
                json!({"type":"acpSessionFailed","session_id":"sess-1","error":"agent process exited unexpectedly"}),
                "agent-session-failed",
            ),
            (
                AcpEvent::AcpToolInvoked {
                    session_id: s("sess-1"),
                    tool: s("run_request"),
                    summary: s("Ran GET /users"),
                },
                json!({"type":"acpToolInvoked","session_id":"sess-1","tool":"run_request","summary":"Ran GET /users"}),
                "agent-tool-invoked",
            ),
            (
                AcpEvent::AcpProposalCreated {
                    session_id: s("sess-1"),
                    proposal_id: s("p-1"),
                    summary: s("Create folder 'reports' in demo"),
                },
                json!({"type":"acpProposalCreated","session_id":"sess-1","proposal_id":"p-1","summary":"Create folder 'reports' in demo"}),
                "agent-proposal-created",
            ),
            (
                AcpEvent::AcpProposalResolved {
                    session_id: s("sess-1"),
                    proposal_id: s("p-1"),
                    status: s("stale"),
                },
                json!({"type":"acpProposalResolved","session_id":"sess-1","proposal_id":"p-1","status":"stale"}),
                "agent-proposal-resolved",
            ),
            (
                AcpEvent::AcpToolActivity {
                    session_id: s("sess-1"),
                    call_id: s("call-1"),
                    title: s("Read file"),
                    status: s("in_progress"),
                },
                json!({"type":"acpToolActivity","session_id":"sess-1","call_id":"call-1","title":"Read file","status":"in_progress"}),
                "agent-session-tool-activity",
            ),
            (
                AcpEvent::AcpConfigOptionsChanged {
                    session_id: s("sess-1"),
                    options: vec![config_option()],
                },
                json!({"type":"acpConfigOptionsChanged","session_id":"sess-1","options":[{"id":"model","name":"Model","category":"model","current_value":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}]}),
                "agent-session-config-options",
            ),
            (
                AcpEvent::AcpUsage {
                    session_id: s("sess-1"),
                    used: 53_000,
                    size: 200_000,
                    cost_usd: Some(0.045),
                },
                json!({"type":"acpUsage","session_id":"sess-1","used":53000,"size":200000,"cost_usd":0.045}),
                "agent-session-usage",
            ),
            (
                AcpEvent::AcpUsage {
                    session_id: s("sess-1"),
                    used: 1,
                    size: 2,
                    cost_usd: None,
                },
                json!({"type":"acpUsage","session_id":"sess-1","used":1,"size":2,"cost_usd":null}),
                "agent-session-usage",
            ),
            (
                AcpEvent::AcpPermissionRequested {
                    session_id: s("sess-1"),
                    request_id: s("perm-1"),
                    call_id: s("call-1"),
                    title: s("propose_changes"),
                    kind: s("other"),
                    summary: s("{}"),
                    options: vec![permission_option()],
                },
                json!({"type":"acpPermissionRequested","session_id":"sess-1","request_id":"perm-1","call_id":"call-1","title":"propose_changes","kind":"other","summary":"{}","options":[{"id":"proceed_once","name":"Allow once","kind":"allow_once"}]}),
                "agent-permission-requested",
            ),
            (
                AcpEvent::AcpPermissionResolved {
                    session_id: s("sess-1"),
                    request_id: s("perm-1"),
                    option_id: Some(s("proceed_once")),
                },
                json!({"type":"acpPermissionResolved","session_id":"sess-1","request_id":"perm-1","option_id":"proceed_once"}),
                "agent-permission-resolved",
            ),
            (
                AcpEvent::AcpPermissionResolved {
                    session_id: s("sess-1"),
                    request_id: s("perm-1"),
                    option_id: None,
                },
                json!({"type":"acpPermissionResolved","session_id":"sess-1","request_id":"perm-1","option_id":null}),
                "agent-permission-resolved",
            ),
            (
                AcpEvent::AssistantConversationSaved {
                    conversation_id: s("c1"),
                },
                json!({"type":"assistantConversationSaved","conversation_id":"c1"}),
                "assistant-conversation-saved",
            ),
            (
                AcpEvent::AssistantConversationDeleted {
                    conversation_id: s("c1"),
                },
                json!({"type":"assistantConversationDeleted","conversation_id":"c1"}),
                "assistant-conversation-deleted",
            ),
            (
                AcpEvent::AssistantConversationsCleared,
                json!({"type":"assistantConversationsCleared"}),
                "assistant-conversations-cleared",
            ),
        ]
    }

    #[test]
    fn acp_events_json_are_unchanged() {
        for (event, want, _) in cases() {
            assert_eq!(event.to_json().expect("serializes"), want);
        }
    }

    #[test]
    fn acp_events_keep_their_channels() {
        for (event, _, channel) in cases() {
            assert_eq!(event.channel(), channel);
        }
    }

    #[test]
    fn every_variant_has_a_case() {
        let tags: std::collections::BTreeSet<String> = cases()
            .iter()
            .map(|(event, _, _)| event.to_json().expect("serializes")["type"].to_string())
            .collect();
        assert_eq!(tags.len(), 16);
    }

    #[test]
    fn assistant_conversations_cleared_is_a_bare_tagged_object() {
        assert_eq!(
            serde_json::to_string(&AcpEvent::AssistantConversationsCleared).expect("serialize"),
            r#"{"type":"assistantConversationsCleared"}"#
        );
    }

    #[test]
    fn acp_events_carry_ids_only_for_conversations() {
        let saved = AcpEvent::AssistantConversationSaved {
            conversation_id: s("c1"),
        };
        assert_eq!(
            serde_json::to_string(&saved).expect("serialize"),
            r#"{"type":"assistantConversationSaved","conversation_id":"c1"}"#
        );
    }

    #[test]
    fn acp_events_have_no_extra_channels_or_written_collections() {
        for (event, _, _) in cases() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn wire_contract_keys_stay_snake_case_with_a_camel_tag() {
        let json = AcpEvent::AcpUsage {
            session_id: s("s"),
            used: 1,
            size: 2,
            cost_usd: None,
        }
        .to_json()
        .expect("serializes");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["cost_usd", "session_id", "size", "type", "used"]);
    }

    #[test]
    fn envelope_keys_are_free() {
        for (event, _, _) in cases() {
            let json = event.to_json().expect("serializes");
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(json.get(reserved).is_none());
            }
        }
    }
}
