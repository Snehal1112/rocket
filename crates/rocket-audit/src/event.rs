use crate::control::{controls_for_kind, ControlId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditEventId(pub Ulid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditEventKind {
    ContractAttached { contract_id: String, collection: String, scope: String },
    ContractDeleted { contract_id: String, collection: String },
    ContractViolation { contract_id: String, request_path: String, field: String },
    CollectionDeleted { collection: String },
    CollectionExported { collection: String, destination: String },
    SecretVariableWritten { environment: String, variable_key: String },
    SensitiveAuthUsed { auth_type: String, collection: String, request_path: String },
    AuditEvidenceExported { range_start: DateTime<Utc>, range_end: DateTime<Utc>, count: usize },
    AuditChainBroken { at_event_id: AuditEventId, expected_hash: String, actual_hash: String },
}

impl AuditEventKind {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::ContractAttached { .. } => "contract_attached",
            Self::ContractDeleted { .. } => "contract_deleted",
            Self::ContractViolation { .. } => "contract_violation",
            Self::CollectionDeleted { .. } => "collection_deleted",
            Self::CollectionExported { .. } => "collection_exported",
            Self::SecretVariableWritten { .. } => "secret_variable_written",
            Self::SensitiveAuthUsed { .. } => "sensitive_auth_used",
            Self::AuditEvidenceExported { .. } => "audit_evidence_exported",
            Self::AuditChainBroken { .. } => "audit_chain_broken",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAuditEvent {
    pub id: AuditEventId,
    pub occurred_at: DateTime<Utc>,
    pub actor: String,
    pub workspace_id: Option<String>,
    pub event: AuditEventKind,
    pub controls: Vec<ControlId>,
    pub prev_hash: String,
    pub hash: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl SecurityAuditEvent {
    pub fn new(
        actor: impl Into<String>,
        workspace_id: Option<String>,
        event: AuditEventKind,
        prev_hash: impl Into<String>,
    ) -> Self {
        let controls = controls_for_kind(event.tag());
        Self {
            id: AuditEventId(Ulid::new()),
            occurred_at: Utc::now(),
            actor: actor.into(),
            workspace_id,
            event,
            controls,
            prev_hash: prev_hash.into(),
            hash: String::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_populates_controls_from_kind() {
        let ev = SecurityAuditEvent::new(
            "user@example.com",
            Some("ws-1".into()),
            AuditEventKind::SensitiveAuthUsed {
                auth_type: "bearer".into(),
                collection: "api".into(),
                request_path: "get.yml".into(),
            },
            "",
        );
        assert!(ev.controls.iter().any(|c| c.code == "CC6.1"));
        assert!(ev.controls.iter().any(|c| c.code == "IAM-09"));
    }

    #[test]
    fn kind_tag_matches_catalog_key() {
        let kind = AuditEventKind::ContractViolation {
            contract_id: "c1".into(),
            request_path: "a.yml".into(),
            field: "method".into(),
        };
        assert_eq!(kind.tag(), "contract_violation");
    }

    #[test]
    fn serializes_tagged_event() {
        let ev = SecurityAuditEvent::new(
            "a",
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
            "",
        );
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"collection_deleted\""));
    }
}
