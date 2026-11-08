//! Events the network context publishes.
//!
//! The tag is camelCase and the variant fields stay snake_case on purpose.
//! Do not rename them: the frontend listeners read these keys.
//! `TransferEvent` and `SyncEvent` follow the same rule on their own channels.

use crate::planner::ConflictKind;
use crate::transfer::{Direction, Outcome};
use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NetworkEvent {
    /// The node list changed. The frontend refetches it.
    NodesChanged,
    /// The network status changed. The frontend refetches it.
    StatusChanged,
    /// A pairing was removed here or by the peer.
    PairedRemoved {
        device_id: String,
        display_name: String,
        by_peer: bool,
    },
    /// A stranger asked to pair and the code is ready. `code` is six digits.
    PairingIncoming {
        pairing_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        code: String,
    },
    /// A pairing ended. `outcome` is `PairingOutcome::as_str()`.
    PairingFinished {
        pairing_id: String,
        outcome: String,
        display_name: String,
    },
}

impl PublishedEvent for NetworkEvent {
    fn channel(&self) -> &'static str {
        match self {
            NetworkEvent::NodesChanged
            | NetworkEvent::StatusChanged
            | NetworkEvent::PairedRemoved { .. } => "network-changed",
            NetworkEvent::PairingIncoming { .. } | NetworkEvent::PairingFinished { .. } => {
                "network-pairing"
            }
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Events about collection transfers. Channel `network-transfer`.
///
/// Same wire rule as `NetworkEvent`: camelCase tag, snake_case fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEvent {
    /// A paired node offered a collection. The frontend asks the user.
    OfferIncoming {
        transfer_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        size_bytes: u64,
    },
    /// Bytes moved. The frontend hides this for a transfer that ends in under 300 ms.
    Progress {
        transfer_id: String,
        direction: Direction,
        done_bytes: u64,
        total_bytes: u64,
    },
    /// A transfer ended. `outcome` is the snake_case `Outcome` (`unconfirmed` included).
    /// `message` is extra text for the receiving side (the fixed text "The collection could
    /// not be imported." for an importer failure, or "The other node went offline."), `null`
    /// when there is none.
    Finished {
        transfer_id: String,
        direction: Direction,
        outcome: Outcome,
        display_name: String,
        collection_name: String,
        message: Option<String>,
    },
}

impl PublishedEvent for TransferEvent {
    fn channel(&self) -> &'static str {
        "network-transfer"
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Counts and lists for the review and confirm dialogs. Counts are files, lists are paths.
///
/// A helper DTO, so its keys are camelCase. "Here" is the node that shows the dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub added_here: u32,
    pub changed_here: u32,
    pub deleted_here: u32,
    pub sent_added: u32,
    pub sent_changed: u32,
    pub sent_deleted: u32,
    /// Changed files that contain scripts.
    pub scripts_changed: Vec<String>,
    /// Changed files that contain auth.
    pub auth_changed: Vec<String>,
}

/// One conflict as the review dialog shows it. A hash is `None` for a deleted or absent file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub path: String,
    pub kind: ConflictKind,
    pub mine_hash: Option<String>,
    pub theirs_hash: Option<String>,
}

/// Which side of a sync this node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncRole {
    /// Received the offer and decides the conflicts. Every review event carries this role.
    Planner,
    /// Pressed Sync. Kept for serialization only, no review event uses it.
    Starter,
}

/// How a sync ended, for the dialog and the toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncFinish {
    Applied,
    Declined,
    Expired,
    Cancelled,
    Failed,
    BaseMismatch,
    LinkMissing,
    /// This node applied, but the peer's result never came.
    Unconfirmed,
}

/// Events about syncing linked collections. Channel `network-sync`.
///
/// Same wire rule as `NetworkEvent`: camelCase tag, snake_case fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncEvent {
    /// The plan is ready and the user decides: accept, decline and each conflict.
    ReviewReady {
        sync_id: String,
        link_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        summary: SyncSummary,
        conflicts: Vec<ConflictView>,
        role: SyncRole,
    },
    /// The starter confirms before anything is written.
    ConfirmNeeded {
        sync_id: String,
        link_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        summary: SyncSummary,
    },
    /// Bytes moved. The frontend hides this for a sync that ends in under 300 ms.
    Progress {
        sync_id: String,
        done_bytes: u64,
        total_bytes: u64,
    },
    /// A sync ended. `message` is a fixed sentence or `null`, never file content.
    Finished {
        sync_id: String,
        link_id: String,
        display_name: String,
        collection_name: String,
        outcome: SyncFinish,
        message: Option<String>,
    },
    /// A link was created, removed or changed state. The frontend refetches the list.
    LinkChanged { link_id: String },
}

impl PublishedEvent for SyncEvent {
    fn channel(&self) -> &'static str {
        "network-sync"
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
    use serde_json::json;

    fn cases() -> Vec<(NetworkEvent, &'static str, &'static [&'static str])> {
        vec![
            (NetworkEvent::NodesChanged, "network-changed", &["type"][..]),
            (
                NetworkEvent::StatusChanged,
                "network-changed",
                &["type"][..],
            ),
            (
                NetworkEvent::PairedRemoved {
                    device_id: "ID".into(),
                    display_name: "Ada".into(),
                    by_peer: true,
                },
                "network-changed",
                &["type", "device_id", "display_name", "by_peer"][..],
            ),
            (
                NetworkEvent::PairingIncoming {
                    pairing_id: "P".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    code: "048213".into(),
                },
                "network-pairing",
                &[
                    "type",
                    "pairing_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "code",
                ][..],
            ),
            (
                NetworkEvent::PairingFinished {
                    pairing_id: "P".into(),
                    outcome: "accepted".into(),
                    display_name: "Ada".into(),
                },
                "network-pairing",
                &["type", "pairing_id", "outcome", "display_name"][..],
            ),
        ]
    }

    fn sorted_keys(event: &NetworkEvent) -> Vec<String> {
        let value = event.to_json().expect("serializes");
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn channels_are_pinned() {
        for (event, channel, _) in cases() {
            assert_eq!(event.channel(), channel, "event {event:?}");
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_network_events() {
        for (event, _, expected) in cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            assert_eq!(sorted_keys(&event), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_tags_are_camel_case() {
        let tags: Vec<String> = cases()
            .iter()
            .map(|(event, _, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(
            tags,
            [
                "nodesChanged",
                "statusChanged",
                "pairedRemoved",
                "pairingIncoming",
                "pairingFinished"
            ]
        );
    }

    // The envelope keys the bus adds must not collide with a top-level field.
    #[test]
    fn wire_contract_envelope_keys_are_free() {
        for (event, _, _) in cases() {
            let keys = sorted_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn pairing_incoming_json_carries_the_values() {
        let event = NetworkEvent::PairingIncoming {
            pairing_id: "P1".into(),
            device_id: "DEV".into(),
            short_id: "ABCD-EFGH".into(),
            display_name: "Ada".into(),
            code: "048213".into(),
        };
        assert_eq!(
            event.to_json().expect("serializes"),
            json!({
                "type": "pairingIncoming",
                "pairing_id": "P1",
                "device_id": "DEV",
                "short_id": "ABCD-EFGH",
                "display_name": "Ada",
                "code": "048213"
            })
        );
    }

    #[test]
    fn events_have_no_extra_channels_or_written_collections() {
        for (event, _, _) in cases() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn events_can_be_downcast_from_the_trait_object() {
        let event: Box<dyn PublishedEvent> = Box::new(NetworkEvent::NodesChanged);
        assert_eq!(
            event.as_any().downcast_ref::<NetworkEvent>(),
            Some(&NetworkEvent::NodesChanged)
        );
    }

    fn transfer_cases() -> Vec<(TransferEvent, &'static [&'static str])> {
        vec![
            (
                TransferEvent::OfferIncoming {
                    transfer_id: "T".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    size_bytes: 1234,
                },
                &[
                    "type",
                    "transfer_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "size_bytes",
                ][..],
            ),
            (
                TransferEvent::Progress {
                    transfer_id: "T".into(),
                    direction: Direction::Send,
                    done_bytes: 10,
                    total_bytes: 20,
                },
                &[
                    "type",
                    "transfer_id",
                    "direction",
                    "done_bytes",
                    "total_bytes",
                ][..],
            ),
            (
                TransferEvent::Finished {
                    transfer_id: "T".into(),
                    direction: Direction::Receive,
                    outcome: Outcome::TooBig,
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    message: None,
                },
                &[
                    "type",
                    "transfer_id",
                    "direction",
                    "outcome",
                    "display_name",
                    "collection_name",
                    "message",
                ][..],
            ),
        ]
    }

    fn transfer_keys(event: &TransferEvent) -> Vec<String> {
        let value = event.to_json().expect("serializes");
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn transfer_events_use_one_channel() {
        for (event, _) in transfer_cases() {
            assert_eq!(event.channel(), "network-transfer", "event {event:?}");
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_transfer_events() {
        for (event, expected) in transfer_cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            assert_eq!(transfer_keys(&event), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_transfer_tags_are_camel_case() {
        let tags: Vec<String> = transfer_cases()
            .iter()
            .map(|(event, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(tags, ["offerIncoming", "progress", "finished"]);
    }

    #[test]
    fn wire_contract_transfer_envelope_keys_are_free() {
        for (event, _) in transfer_cases() {
            let keys = transfer_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn transfer_event_json_carries_the_values() {
        let progress = TransferEvent::Progress {
            transfer_id: "T1".into(),
            direction: Direction::Send,
            done_bytes: 10,
            total_bytes: 20,
        };
        assert_eq!(
            progress.to_json().expect("serializes"),
            json!({
                "type": "progress",
                "transfer_id": "T1",
                "direction": "send",
                "done_bytes": 10,
                "total_bytes": 20
            })
        );
        let finished = TransferEvent::Finished {
            transfer_id: "T1".into(),
            direction: Direction::Receive,
            outcome: Outcome::TooBig,
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        assert_eq!(
            finished.to_json().expect("serializes"),
            json!({
                "type": "finished",
                "transfer_id": "T1",
                "direction": "receive",
                "outcome": "too_big",
                "display_name": "Ada",
                "collection_name": "Orders",
                "message": null
            })
        );
    }

    #[test]
    fn finished_carries_a_null_or_a_string_message() {
        let finished = |message: Option<&str>| TransferEvent::Finished {
            transfer_id: "T1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Failed,
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            message: message.map(str::to_string),
        };
        let none = finished(None).to_json().expect("serializes");
        assert!(none.as_object().expect("an object").contains_key("message"));
        assert!(none["message"].is_null());
        let some = finished(Some("The bundle has no collection."))
            .to_json()
            .expect("serializes");
        assert_eq!(some["message"], "The bundle has no collection.");
        // The event is cloneable, the service fans it out to the bus and its own state.
        assert_eq!(finished(Some("x")).clone(), finished(Some("x")));
    }

    #[test]
    fn transfer_events_can_be_downcast_and_are_not_network_events() {
        let event: Box<dyn PublishedEvent> = Box::new(TransferEvent::Progress {
            transfer_id: "T".into(),
            direction: Direction::Send,
            done_bytes: 0,
            total_bytes: 1,
        });
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_some());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }

    fn summary() -> SyncSummary {
        SyncSummary {
            added_here: 1,
            changed_here: 2,
            deleted_here: 3,
            sent_added: 4,
            sent_changed: 5,
            sent_deleted: 6,
            scripts_changed: vec!["a/pre.js".into()],
            auth_changed: vec!["b.yml".into()],
        }
    }

    fn sync_cases() -> Vec<(SyncEvent, &'static [&'static str])> {
        vec![
            (
                SyncEvent::ReviewReady {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    summary: summary(),
                    conflicts: vec![ConflictView {
                        path: "r.yml".into(),
                        kind: ConflictKind::BothChanged,
                        mine_hash: Some("m".into()),
                        theirs_hash: None,
                    }],
                    role: SyncRole::Planner,
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "summary",
                    "conflicts",
                    "role",
                ][..],
            ),
            (
                SyncEvent::ConfirmNeeded {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    summary: summary(),
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "summary",
                ][..],
            ),
            (
                SyncEvent::Progress {
                    sync_id: "S".into(),
                    done_bytes: 10,
                    total_bytes: 20,
                },
                &["type", "sync_id", "done_bytes", "total_bytes"][..],
            ),
            (
                SyncEvent::Finished {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    outcome: SyncFinish::BaseMismatch,
                    message: None,
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "display_name",
                    "collection_name",
                    "outcome",
                    "message",
                ][..],
            ),
            (
                SyncEvent::LinkChanged {
                    link_id: "L".into(),
                },
                &["type", "link_id"][..],
            ),
        ]
    }

    fn value_keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn sync_events_use_one_channel() {
        for (event, _) in sync_cases() {
            assert_eq!(event.channel(), "network-sync", "event {event:?}");
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_sync_events() {
        for (event, expected) in sync_cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            let value = event.to_json().expect("serializes");
            assert_eq!(value_keys(&value), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_sync_tags_are_camel_case() {
        let tags: Vec<String> = sync_cases()
            .iter()
            .map(|(event, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(
            tags,
            [
                "reviewReady",
                "confirmNeeded",
                "progress",
                "finished",
                "linkChanged"
            ]
        );
    }

    #[test]
    fn wire_contract_sync_envelope_keys_are_free() {
        for (event, _) in sync_cases() {
            let keys = value_keys(&event.to_json().expect("serializes"));
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn wire_contract_sync_helper_dtos_are_camel_case() {
        let value = serde_json::to_value(summary()).expect("serializes");
        assert_eq!(
            value_keys(&value),
            [
                "addedHere",
                "authChanged",
                "changedHere",
                "deletedHere",
                "scriptsChanged",
                "sentAdded",
                "sentChanged",
                "sentDeleted"
            ]
        );
        let view = ConflictView {
            path: "r.yml".into(),
            kind: ConflictKind::DeleteVsEdit,
            mine_hash: None,
            theirs_hash: Some("t".into()),
        };
        let value = serde_json::to_value(view).expect("serializes");
        assert_eq!(
            value_keys(&value),
            ["kind", "mineHash", "path", "theirsHash"]
        );
        assert!(value["mineHash"].is_null());
        assert_eq!(value["kind"], "delete_vs_edit");
    }

    #[test]
    fn sync_event_json_carries_the_values() {
        let progress = SyncEvent::Progress {
            sync_id: "S1".into(),
            done_bytes: 10,
            total_bytes: 20,
        };
        assert_eq!(
            progress.to_json().expect("serializes"),
            json!({"type": "progress", "sync_id": "S1", "done_bytes": 10, "total_bytes": 20})
        );
        let finished = SyncEvent::Finished {
            sync_id: "S1".into(),
            link_id: "L1".into(),
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            outcome: SyncFinish::LinkMissing,
            message: None,
        };
        assert_eq!(
            finished.to_json().expect("serializes"),
            json!({
                "type": "finished",
                "sync_id": "S1",
                "link_id": "L1",
                "display_name": "Ada",
                "collection_name": "Orders",
                "outcome": "link_missing",
                "message": null
            })
        );
        let review = SyncEvent::ReviewReady {
            sync_id: "S1".into(),
            link_id: "L1".into(),
            device_id: "ID".into(),
            short_id: "ABCD-EFGH".into(),
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            summary: SyncSummary::default(),
            conflicts: Vec::new(),
            role: SyncRole::Starter,
        };
        let value = review.to_json().expect("serializes");
        assert_eq!(value["role"], "starter");
        assert_eq!(value["summary"]["addedHere"], 0);
        assert_eq!(value["summary"]["scriptsChanged"], json!([]));
        assert_eq!(value["conflicts"], json!([]));
    }

    #[test]
    fn sync_enums_serialize_as_snake_case() {
        for (role, name) in [
            (SyncRole::Planner, "planner"),
            (SyncRole::Starter, "starter"),
        ] {
            assert_eq!(serde_json::to_value(role).expect("json"), json!(name));
        }
        for (finish, name) in [
            (SyncFinish::Applied, "applied"),
            (SyncFinish::Declined, "declined"),
            (SyncFinish::Expired, "expired"),
            (SyncFinish::Cancelled, "cancelled"),
            (SyncFinish::Failed, "failed"),
            (SyncFinish::BaseMismatch, "base_mismatch"),
            (SyncFinish::LinkMissing, "link_missing"),
            (SyncFinish::Unconfirmed, "unconfirmed"),
        ] {
            assert_eq!(serde_json::to_value(finish).expect("json"), json!(name));
        }
        for (kind, name) in [
            (ConflictKind::BothChanged, "both_changed"),
            (ConflictKind::DeleteVsEdit, "delete_vs_edit"),
            (ConflictKind::BothAdded, "both_added"),
            (ConflictKind::RenameVsEdit, "rename_vs_edit"),
        ] {
            assert_eq!(serde_json::to_value(kind).expect("json"), json!(name));
        }
    }

    #[test]
    fn sync_events_can_be_downcast_and_are_not_other_events() {
        let event: Box<dyn PublishedEvent> = Box::new(SyncEvent::LinkChanged {
            link_id: "L".into(),
        });
        assert!(event.as_any().downcast_ref::<SyncEvent>().is_some());
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_none());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }
}
