use crate::control::{controls_for_kind, ControlId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditEventId(pub Ulid);

/// One audit event kind. The derive below is `remote = "Self"` so the manual
/// `Serialize` and `Deserialize` impls can add the `Unknown` fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", tag = "kind", rename_all = "snake_case")]
pub enum AuditEventKind {
    ContractAttached {
        contract_id: String,
        collection: String,
        scope: String,
    },
    ContractDeleted {
        contract_id: String,
        collection: String,
    },
    ContractViolation {
        contract_id: String,
        request_path: String,
        field: String,
    },
    CollectionDeleted {
        collection: String,
    },
    CollectionExported {
        collection: String,
        destination: String,
    },
    SecretVariableWritten {
        environment: String,
        variable_key: String,
    },
    SensitiveAuthUsed {
        auth_type: String,
        collection: String,
        request_path: String,
    },
    AuditEvidenceExported {
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
        count: usize,
    },
    AuditChainBroken {
        at_event_id: AuditEventId,
        expected_hash: String,
        actual_hash: String,
    },
    NetworkDevicePaired {
        device_id: String,
    },
    NetworkDeviceUnpaired {
        device_id: String,
        by_peer: bool,
    },
    NetworkDeviceBlocked {
        device_id: String,
    },
    /// A collection left for a paired node. `device_id` is the short id (`XXXX-XXXX`).
    /// `outcome` is `completed` (the receiver confirmed), `unconfirmed` (the final ack came,
    /// no result), `failed` (the stream ended before the final ack), `cancelled`, `damaged`
    /// or `rejected` (the receiver's verdict). A row exists only for a send that ended after
    /// at least one chunk was sent.
    NetworkCollectionSent {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A collection from a paired node arrived. `device_id` is the short id. `outcome` is
    /// `completed` (imported), `damaged` (size or hash mismatch) or `rejected` (unsafe ZIP).
    NetworkCollectionReceived {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A sync this node started ended after the first chunk left the node, or was refused.
    /// `device_id` is the short id. `outcome` is `applied`, `failed`, `declined`, `cancelled`,
    /// `expired`, `base_mismatch`, `link_missing` or `unconfirmed`.
    /// The fields are the short device id, the collection name and an outcome word, never paths or
    /// file contents.
    NetworkSyncSent {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A sync a paired node started ended on this node. `device_id` is the short id. `outcome`
    /// uses the same words as `NetworkSyncSent`.
    NetworkSyncReceived {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A contract check this node started ended. `device_id` is the short id, `contract` the
    /// contract name. `outcome` is `answered`, `declined` or `busy`, as the node's result says.
    /// The row is written when the result arrives, so a check that expires unanswered writes
    /// none. Never paths or field values.
    NetworkContractCheckSent {
        device_id: String,
        contract: String,
        outcome: String,
    },
    /// A contract check a paired node started ended on this node. Same words as
    /// `NetworkContractCheckSent`.
    NetworkContractCheckReceived {
        device_id: String,
        contract: String,
        outcome: String,
    },
    /// A link was removed here (Unlink or the pairing was removed). The files stay. The fields
    /// are the short device id and the collection name, never paths or file contents.
    NetworkLinkRemoved {
        device_id: String,
        collection: String,
    },
    /// An event kind that a newer build wrote and this build does not know.
    /// It keeps the stored JSON as it was, so the log stays readable and the
    /// chain stays linked. It is never created by this build.
    #[serde(skip)]
    Unknown(UnknownEvent),
}

/// The stored JSON of an event kind this build does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEvent {
    /// The `kind` tag as written, or empty when the event had none.
    pub kind: String,
    /// The whole stored `event` object, including the `kind` tag.
    pub raw: serde_json::Value,
}

impl Serialize for AuditEventKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Unknown(unknown) => unknown.raw.serialize(serializer),
            _ => Self::serialize(self, serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AuditEventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(deserializer)?;
        match Self::deserialize(raw.clone()) {
            Ok(known) => Ok(known),
            Err(_) => {
                let kind = raw
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(Self::Unknown(UnknownEvent { kind, raw }))
            }
        }
    }
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
            Self::NetworkDevicePaired { .. } => "network_device_paired",
            Self::NetworkDeviceUnpaired { .. } => "network_device_unpaired",
            Self::NetworkDeviceBlocked { .. } => "network_device_blocked",
            Self::NetworkCollectionSent { .. } => "network_collection_sent",
            Self::NetworkCollectionReceived { .. } => "network_collection_received",
            Self::NetworkSyncSent { .. } => "network_sync_sent",
            Self::NetworkSyncReceived { .. } => "network_sync_received",
            Self::NetworkContractCheckSent { .. } => "network_contract_check_sent",
            Self::NetworkContractCheckReceived { .. } => "network_contract_check_received",
            Self::NetworkLinkRemoved { .. } => "network_link_removed",
            Self::Unknown(_) => "unknown",
        }
    }

    /// True for an event kind this build does not know.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
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
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
            "",
        );
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"collection_deleted\""));
    }

    #[test]
    fn network_kinds_have_stable_tags() {
        let paired = AuditEventKind::NetworkDevicePaired {
            device_id: "ABCD".into(),
        };
        let unpaired = AuditEventKind::NetworkDeviceUnpaired {
            device_id: "ABCD".into(),
            by_peer: true,
        };
        let blocked = AuditEventKind::NetworkDeviceBlocked {
            device_id: "ABCD".into(),
        };
        assert_eq!(paired.tag(), "network_device_paired");
        assert_eq!(unpaired.tag(), "network_device_unpaired");
        assert_eq!(blocked.tag(), "network_device_blocked");
    }

    #[test]
    fn network_kinds_map_to_soc2_cc6_1() {
        let kinds = [
            AuditEventKind::NetworkDevicePaired {
                device_id: "ABCD".into(),
            },
            AuditEventKind::NetworkDeviceUnpaired {
                device_id: "ABCD".into(),
                by_peer: false,
            },
            AuditEventKind::NetworkDeviceBlocked {
                device_id: "ABCD".into(),
            },
        ];
        for kind in kinds {
            let tag = kind.tag();
            let ev = SecurityAuditEvent::new("user", None, kind, "");
            assert!(
                ev.controls.iter().any(|c| c.code == "CC6.1"),
                "{tag} is not under CC6.1"
            );
        }
    }

    #[test]
    fn network_unpaired_serializes_with_snake_case_fields() {
        let kind = AuditEventKind::NetworkDeviceUnpaired {
            device_id: "ABCD".into(),
            by_peer: true,
        };
        let value = serde_json::to_value(&kind).expect("kind serializes");
        assert_eq!(value["kind"], "network_device_unpaired");
        assert_eq!(value["device_id"], "ABCD");
        assert_eq!(value["by_peer"], true);
        let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
        assert_eq!(back, kind);
    }

    #[test]
    fn network_collection_kinds_have_stable_tags() {
        let sent = AuditEventKind::NetworkCollectionSent {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "completed".into(),
        };
        let received = AuditEventKind::NetworkCollectionReceived {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "rejected".into(),
        };
        assert_eq!(sent.tag(), "network_collection_sent");
        assert_eq!(received.tag(), "network_collection_received");
    }

    #[test]
    fn network_collection_kinds_map_to_soc2_cc6_7_only() {
        let kinds = [
            AuditEventKind::NetworkCollectionSent {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "unconfirmed".into(),
            },
            AuditEventKind::NetworkCollectionReceived {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "damaged".into(),
            },
        ];
        for kind in kinds {
            let tag = kind.tag();
            let ev = SecurityAuditEvent::new("user", None, kind, "");
            assert!(
                ev.controls.iter().any(|c| c.code == "CC6.7"),
                "{tag} is not under CC6.7"
            );
            assert!(
                !ev.controls.iter().any(|c| c.code == "CC6.1"),
                "{tag} must not be under CC6.1"
            );
        }
    }

    #[test]
    fn network_collection_kinds_serialize_with_snake_case_fields() {
        for (kind, tag, outcome) in [
            (
                AuditEventKind::NetworkCollectionSent {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "unconfirmed".into(),
                },
                "network_collection_sent",
                "unconfirmed",
            ),
            (
                AuditEventKind::NetworkCollectionReceived {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "rejected".into(),
                },
                "network_collection_received",
                "rejected",
            ),
        ] {
            let value = serde_json::to_value(&kind).expect("kind serializes");
            assert_eq!(value["kind"], tag);
            assert_eq!(value["device_id"], "ABCD-EFGH");
            assert_eq!(value["collection"], "Orders");
            assert_eq!(value["outcome"], outcome);
            let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn network_contract_check_kinds_have_stable_tags() {
        let sent = AuditEventKind::NetworkContractCheckSent {
            device_id: "ABCD-EFGH".into(),
            contract: "Orders API".into(),
            outcome: "answered".into(),
        };
        let received = AuditEventKind::NetworkContractCheckReceived {
            device_id: "ABCD-EFGH".into(),
            contract: "Orders API".into(),
            outcome: "declined".into(),
        };
        assert_eq!(sent.tag(), "network_contract_check_sent");
        assert_eq!(received.tag(), "network_contract_check_received");
        let value = serde_json::to_value(&sent).expect("json");
        assert_eq!(value["kind"], "network_contract_check_sent");
        assert_eq!(value["contract"], "Orders API");
        let back: AuditEventKind = serde_json::from_value(value).expect("round trips");
        assert_eq!(back, sent);
    }

    #[test]
    fn network_sync_kinds_have_stable_tags() {
        let sent = AuditEventKind::NetworkSyncSent {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "applied".into(),
        };
        let received = AuditEventKind::NetworkSyncReceived {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "failed".into(),
        };
        let removed = AuditEventKind::NetworkLinkRemoved {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
        };
        assert_eq!(sent.tag(), "network_sync_sent");
        assert_eq!(received.tag(), "network_sync_received");
        assert_eq!(removed.tag(), "network_link_removed");
    }

    #[test]
    fn network_sync_kinds_map_to_the_expected_controls() {
        let sent = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkSyncSent {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "applied".into(),
            },
            "",
        );
        let received = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkSyncReceived {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "declined".into(),
            },
            "",
        );
        for ev in [&sent, &received] {
            assert!(ev.controls.iter().any(|c| c.code == "CC6.7"));
            assert!(!ev.controls.iter().any(|c| c.code == "CC6.1"));
        }
        let removed = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkLinkRemoved {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
            },
            "",
        );
        assert!(removed.controls.iter().any(|c| c.code == "CC6.1"));
    }

    #[test]
    fn network_sync_kinds_serialize_with_snake_case_fields() {
        for (kind, tag, outcome) in [
            (
                AuditEventKind::NetworkSyncSent {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "applied".into(),
                },
                "network_sync_sent",
                Some("applied"),
            ),
            (
                AuditEventKind::NetworkSyncReceived {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "declined".into(),
                },
                "network_sync_received",
                Some("declined"),
            ),
            (
                AuditEventKind::NetworkLinkRemoved {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                },
                "network_link_removed",
                None,
            ),
        ] {
            let value = serde_json::to_value(&kind).expect("kind serializes");
            assert_eq!(value["kind"], tag);
            assert_eq!(value["device_id"], "ABCD-EFGH");
            assert_eq!(value["collection"], "Orders");
            match outcome {
                Some(word) => assert_eq!(value["outcome"], word),
                None => assert!(value.get("outcome").is_none(), "the removal has no outcome"),
            }
            let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
            assert_eq!(back, kind);
        }
    }
}
