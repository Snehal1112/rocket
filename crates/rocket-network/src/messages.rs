//! The WebSocket frame type. One serde enum tagged by `type`.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::{
    CONTRACT_CHECK_MAX_CHANGES, CONTRACT_FIELD_MAX, CONTRACT_METHOD_MAX, CONTRACT_NAME_MAX,
    CONTRACT_PATH_MAX, CONTRACT_VALUE_MAX, CONTRACT_VERSION_MAX, MAX_FRAME_BYTES,
};
use crate::link::ContractCheckId;
use crate::profile::NodeProfile;

/// Why a transfer stopped. Sent inside `TransferAbort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortReason {
    Cancelled,
    Failed,
    TooBig,
    Busy,
    Expired,
}

/// What the receiver tells the sender once the import is done. Sent inside `TransferResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Completed,
    Damaged,
    Rejected,
    Failed,
}

/// What a sync transfer carries. The offer's `kind` tells the receiver how to treat the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPayloadKind {
    /// JSON of a `Manifest`.
    Manifest,
    /// JSON of a `SyncPlanPayload`.
    Plan,
    /// A ZIP of collection files.
    Files,
}

impl SyncPayloadKind {
    /// The name the transfer machines carry in the offer. At most `SYNC_PAYLOAD_NAME_MAX`.
    pub fn transfer_name(self) -> &'static str {
        match self {
            SyncPayloadKind::Manifest => "manifest",
            SyncPayloadKind::Plan => "plan",
            SyncPayloadKind::Files => "files",
        }
    }
}

/// Why a sync stopped. Sent inside `SyncAbort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAbortReason {
    BaseMismatch,
    LinkMissing,
    Busy,
    Cancelled,
    Declined,
    Expired,
    Failed,
    TooBig,
}

/// What a node reports once it has applied, or failed to apply, a sync. Sent inside `SyncResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Applied,
    Failed,
}

/// How strictly the provider treats changes. Sent inside `ContractCheckOffer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractPolicyWire {
    Strict,
    Lenient,
    AdditiveOk,
}

/// What happened to one contract field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractChangeType {
    Changed,
    Added,
    Removed,
}

/// The consumer's verdict on one change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractVerdict {
    Pass,
    Break,
    NotUsed,
}

/// How a node answered a contract check. Sent inside `ContractCheckResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractCheckOutcome {
    Answered,
    Declined,
    Busy,
}

/// One contract change in an offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractChange {
    pub method: String,
    pub path_template: String,
    pub field: String,
    pub change_type: ContractChangeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    pub breaking: bool,
}

impl ContractChange {
    /// Cuts `old_value` and `new_value` to `CONTRACT_VALUE_MAX` characters.
    pub fn cut_values(&mut self) {
        for value in [&mut self.old_value, &mut self.new_value]
            .into_iter()
            .flatten()
        {
            if let Some((byte, _)) = value.char_indices().nth(CONTRACT_VALUE_MAX) {
                value.truncate(byte);
            }
        }
    }
}

/// The verdict for the change at `index` in the offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCheckRow {
    pub index: u32,
    pub verdict: ContractVerdict,
}

/// Serde helper: skips `keep_in_sync` when it is false, so an offer without a link looks the
/// same as in protocol version 2.
fn is_false(value: &bool) -> bool {
    !*value
}

/// A text frame on the peer WebSocket. Later projects add variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerMessage {
    Hello {
        version: u32,
        profile: NodeProfile,
    },
    Ping,
    Pong,
    Unpair,
    /// The sender offers a collection ZIP. `sha256` is lowercase hex.
    CollectionOffer {
        transfer_id: String,
        name: String,
        size_bytes: u64,
        chunk_count: u32,
        sha256: String,
        /// The sender wants a link, so the two copies can be synced later.
        #[serde(default, skip_serializing_if = "is_false")]
        keep_in_sync: bool,
        /// The link id the sender made, when `keep_in_sync` is true.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_id: Option<String>,
    },
    /// The receiver accepts or declines an offer.
    OfferAnswer {
        transfer_id: String,
        accept: bool,
    },
    /// One piece of the ZIP. `data` is standard base64, padded.
    Chunk {
        transfer_id: String,
        seq: u32,
        data: String,
    },
    /// Either side stops the transfer.
    TransferAbort {
        transfer_id: String,
        reason: AbortReason,
    },
    /// The receiver acknowledges chunks: `next_seq` is how many it accepted in order. Sent
    /// after every `ACK_EVERY_CHUNKS` chunks and after the last one.
    ChunkAck {
        transfer_id: String,
        next_seq: u32,
    },
    /// The receiver reports the true outcome after the last chunk and the import.
    TransferResult {
        transfer_id: String,
        outcome: ResultOutcome,
    },
    /// A node starts a sync of a linked collection. The payload of `kind` follows as the
    /// transfer `transfer_id` on the transfer machines. `base_hash` is the sender's base.
    SyncOffer {
        sync_id: String,
        link_id: String,
        base_hash: String,
        kind: SyncPayloadKind,
        transfer_id: String,
        size_bytes: u64,
        chunk_count: u32,
        sha256: String,
    },
    /// Either node stops a sync.
    SyncAbort {
        sync_id: String,
        reason: SyncAbortReason,
    },
    /// A node reports whether it applied the sync.
    SyncResult {
        sync_id: String,
        outcome: SyncOutcome,
    },
    /// A provider asks a node whether its contract changes break the node's requests.
    ContractCheckOffer {
        check_id: String,
        contract: String,
        version: String,
        policy: ContractPolicyWire,
        changes: Vec<ContractChange>,
    },
    /// The node answers a contract check. `rows` is only set for `answered`.
    ContractCheckResult {
        check_id: String,
        outcome: ContractCheckOutcome,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rows: Vec<ContractCheckRow>,
    },
}

/// Whether the text is at most `max` characters.
fn within(text: &str, max: usize) -> bool {
    text.chars().count() <= max
}

/// Whether the text is non-empty, at most `max` characters and has no control characters.
fn plain_text(text: &str, max: usize) -> bool {
    !text.is_empty() && within(text, max) && !text.chars().any(char::is_control)
}

/// Whether one change in an offer is within every limit.
fn change_is_valid(c: &ContractChange) -> bool {
    let value_ok = |v: &Option<String>| v.as_deref().is_none_or(|v| within(v, CONTRACT_VALUE_MAX));
    plain_text(&c.method, CONTRACT_METHOD_MAX)
        && plain_text(&c.path_template, CONTRACT_PATH_MAX)
        && plain_text(&c.field, CONTRACT_FIELD_MAX)
        && value_ok(&c.old_value)
        && value_ok(&c.new_value)
}

/// Checks the shape of a contract check frame. Other frames pass. Errors use the fixed
/// decode text and never repeat the frame.
fn validate_contract_check(message: &PeerMessage) -> DomainResult<()> {
    let bad = || DomainError::InvalidInput("invalid peer message".to_string());
    match message {
        PeerMessage::ContractCheckOffer {
            check_id,
            contract,
            version,
            changes,
            ..
        } => {
            ContractCheckId::parse(check_id).map_err(|_| bad())?;
            let count_ok = (1..=CONTRACT_CHECK_MAX_CHANGES).contains(&changes.len());
            if plain_text(contract, CONTRACT_NAME_MAX)
                && within(version, CONTRACT_VERSION_MAX)
                && count_ok
                && changes.iter().all(change_is_valid)
            {
                Ok(())
            } else {
                Err(bad())
            }
        }
        PeerMessage::ContractCheckResult { check_id, rows, .. } => {
            ContractCheckId::parse(check_id).map_err(|_| bad())?;
            if rows.len() <= CONTRACT_CHECK_MAX_CHANGES {
                Ok(())
            } else {
                Err(bad())
            }
        }
        _ => Ok(()),
    }
}

impl PeerMessage {
    /// The frame as JSON text.
    pub fn encode(&self) -> String {
        // Serializing this enum cannot fail. The fallback is a frame the peer rejects.
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Parses a text frame.
    ///
    /// A text above `MAX_FRAME_BYTES` is refused before parsing. Unknown types
    /// and malformed JSON are `InvalidInput`, and the error never repeats the
    /// frame. A `Hello` profile is re-validated and its initials rebuilt.
    pub fn decode(text: &str) -> DomainResult<PeerMessage> {
        if text.len() > MAX_FRAME_BYTES {
            return Err(DomainError::InvalidInput("frame too large".to_string()));
        }
        let message: PeerMessage = serde_json::from_str(text)
            .map_err(|_| DomainError::InvalidInput("invalid peer message".to_string()))?;
        match message {
            PeerMessage::Hello { version, profile } => Ok(PeerMessage::Hello {
                version,
                profile: profile.validated()?,
            }),
            other => {
                validate_contract_check(&other)?;
                Ok(other)
            }
        }
    }

    /// Whether the encoded frame is within `MAX_FRAME_BYTES`, so the peer will accept it.
    pub fn fits_frame(&self) -> bool {
        self.encode().len() <= MAX_FRAME_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{CHUNK_BYTES, DISPLAY_NAME_MAX_CHARS, SYNC_PAYLOAD_NAME_MAX};
    use crate::transfer::offer_name_is_valid;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const SYNC_ID: &str = "fedcba9876543210fedcba9876543210";
    const LINK_ID: &str = "00112233445566778899aabbccddeeff";

    fn sync_messages() -> Vec<PeerMessage> {
        vec![
            PeerMessage::SyncOffer {
                sync_id: SYNC_ID.into(),
                link_id: LINK_ID.into(),
                base_hash: SHA.into(),
                kind: SyncPayloadKind::Manifest,
                transfer_id: ID.into(),
                size_bytes: 70_000,
                chunk_count: 3,
                sha256: SHA.into(),
            },
            PeerMessage::SyncAbort {
                sync_id: SYNC_ID.into(),
                reason: SyncAbortReason::BaseMismatch,
            },
            PeerMessage::SyncResult {
                sync_id: SYNC_ID.into(),
                outcome: SyncOutcome::Applied,
            },
        ]
    }

    const CHECK_ID: &str = "aabbccddeeff00112233445566778899";

    fn change() -> ContractChange {
        ContractChange {
            method: "GET".into(),
            path_template: "/users/{id}".into(),
            field: "status".into(),
            change_type: ContractChangeType::Changed,
            old_value: Some("200".into()),
            new_value: Some("204".into()),
            breaking: true,
        }
    }

    fn contract_offer(changes: Vec<ContractChange>) -> PeerMessage {
        PeerMessage::ContractCheckOffer {
            check_id: CHECK_ID.into(),
            contract: "Orders API".into(),
            version: "1.2.0".into(),
            policy: ContractPolicyWire::AdditiveOk,
            changes,
        }
    }

    fn contract_result(rows: Vec<ContractCheckRow>) -> PeerMessage {
        PeerMessage::ContractCheckResult {
            check_id: CHECK_ID.into(),
            outcome: ContractCheckOutcome::Answered,
            rows,
        }
    }

    fn contract_messages() -> Vec<PeerMessage> {
        vec![
            contract_offer(vec![change()]),
            contract_result(vec![ContractCheckRow {
                index: 0,
                verdict: ContractVerdict::Break,
            }]),
            PeerMessage::ContractCheckResult {
                check_id: CHECK_ID.into(),
                outcome: ContractCheckOutcome::Declined,
                rows: Vec::new(),
            },
        ]
    }

    fn transfer_messages() -> Vec<PeerMessage> {
        vec![
            PeerMessage::CollectionOffer {
                transfer_id: ID.into(),
                name: "Orders".into(),
                size_bytes: 70_000,
                chunk_count: 3,
                sha256: SHA.into(),
                keep_in_sync: false,
                link_id: None,
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: false,
            },
            PeerMessage::Chunk {
                transfer_id: ID.into(),
                seq: 2,
                data: "AAEC".into(),
            },
            PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Cancelled,
            },
            PeerMessage::ChunkAck {
                transfer_id: ID.into(),
                next_seq: 16,
            },
            PeerMessage::TransferResult {
                transfer_id: ID.into(),
                outcome: ResultOutcome::Completed,
            },
        ]
    }

    fn hello() -> PeerMessage {
        PeerMessage::Hello {
            version: 3,
            profile: NodeProfile::new("Ada Lovelace").expect("valid name"),
        }
    }

    #[test]
    fn every_variant_round_trips() {
        let mut all = vec![
            hello(),
            PeerMessage::Ping,
            PeerMessage::Pong,
            PeerMessage::Unpair,
        ];
        all.extend(transfer_messages());
        all.extend(sync_messages());
        all.extend(contract_messages());
        for outcome in [
            ContractCheckOutcome::Answered,
            ContractCheckOutcome::Declined,
            ContractCheckOutcome::Busy,
        ] {
            all.push(PeerMessage::ContractCheckResult {
                check_id: CHECK_ID.into(),
                outcome,
                rows: Vec::new(),
            });
        }
        for kind in [
            SyncPayloadKind::Manifest,
            SyncPayloadKind::Plan,
            SyncPayloadKind::Files,
        ] {
            all.push(PeerMessage::SyncOffer {
                sync_id: SYNC_ID.into(),
                link_id: LINK_ID.into(),
                base_hash: SHA.into(),
                kind,
                transfer_id: ID.into(),
                size_bytes: 1,
                chunk_count: 1,
                sha256: SHA.into(),
            });
        }
        for reason in [
            SyncAbortReason::BaseMismatch,
            SyncAbortReason::LinkMissing,
            SyncAbortReason::Busy,
            SyncAbortReason::Cancelled,
            SyncAbortReason::Declined,
            SyncAbortReason::Expired,
            SyncAbortReason::Failed,
            SyncAbortReason::TooBig,
        ] {
            all.push(PeerMessage::SyncAbort {
                sync_id: SYNC_ID.into(),
                reason,
            });
        }
        for outcome in [SyncOutcome::Applied, SyncOutcome::Failed] {
            all.push(PeerMessage::SyncResult {
                sync_id: SYNC_ID.into(),
                outcome,
            });
        }
        all.push(PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "Orders".into(),
            size_bytes: 70_000,
            chunk_count: 3,
            sha256: SHA.into(),
            keep_in_sync: true,
            link_id: Some(LINK_ID.into()),
        });
        for reason in [
            AbortReason::Cancelled,
            AbortReason::Failed,
            AbortReason::TooBig,
            AbortReason::Busy,
            AbortReason::Expired,
        ] {
            all.push(PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason,
            });
        }
        for outcome in [
            ResultOutcome::Completed,
            ResultOutcome::Damaged,
            ResultOutcome::Rejected,
            ResultOutcome::Failed,
        ] {
            all.push(PeerMessage::TransferResult {
                transfer_id: ID.into(),
                outcome,
            });
        }
        for message in all {
            let text = message.encode();
            assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);
        }
    }

    #[test]
    fn transfer_frames_have_the_expected_shape() {
        let messages = transfer_messages();
        assert_eq!(
            messages[0].encode(),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
            )
        );
        assert_eq!(
            messages[1].encode(),
            format!(r#"{{"type":"offer_answer","transfer_id":"{ID}","accept":true}}"#)
        );
        assert_eq!(
            messages[2].encode(),
            format!(r#"{{"type":"offer_answer","transfer_id":"{ID}","accept":false}}"#)
        );
        assert_eq!(
            messages[3].encode(),
            format!(r#"{{"type":"chunk","transfer_id":"{ID}","seq":2,"data":"AAEC"}}"#)
        );
        assert_eq!(
            messages[4].encode(),
            format!(r#"{{"type":"transfer_abort","transfer_id":"{ID}","reason":"cancelled"}}"#)
        );
        assert_eq!(
            messages[5].encode(),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{ID}","next_seq":16}}"#)
        );
        assert_eq!(
            messages[6].encode(),
            format!(r#"{{"type":"transfer_result","transfer_id":"{ID}","outcome":"completed"}}"#)
        );
        assert_eq!(messages.len(), 7, "six variants, the answer twice");
    }

    #[test]
    fn result_outcomes_use_snake_case_names() {
        for (outcome, name) in [
            (ResultOutcome::Completed, "completed"),
            (ResultOutcome::Damaged, "damaged"),
            (ResultOutcome::Rejected, "rejected"),
            (ResultOutcome::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn abort_reasons_use_snake_case_names() {
        for (reason, name) in [
            (AbortReason::Cancelled, "cancelled"),
            (AbortReason::Failed, "failed"),
            (AbortReason::TooBig, "too_big"),
            (AbortReason::Busy, "busy"),
            (AbortReason::Expired, "expired"),
        ] {
            assert_eq!(serde_json::to_value(reason).expect("json"), json!(name));
        }
    }

    #[test]
    fn transfer_frames_reject_unknown_fields_and_bad_values() {
        let id = ID;
        for text in [
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}","accept":true,"extra":1}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":0,"data":"","extra":1}}"#),
            format!(
                r#"{{"type":"transfer_abort","transfer_id":"{id}","reason":"failed","extra":1}}"#
            ),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{id}","name":"A","size_bytes":1,"chunk_count":1,"sha256":"{SHA}","extra":1}}"#
            ),
            format!(
                r#"{{"type":"transfer_result","transfer_id":"{id}","outcome":"completed","extra":1}}"#
            ),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":16,"extra":1}}"#),
            // Missing fields.
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}"}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":0}}"#),
            r#"{"type":"transfer_abort","reason":"failed"}"#.to_string(),
            format!(r#"{{"type":"transfer_result","transfer_id":"{id}"}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}"}}"#),
            // Wrong types and values.
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}","accept":"yes"}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":-1,"data":""}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":4294967296,"data":""}}"#),
            format!(r#"{{"type":"transfer_abort","transfer_id":"{id}","reason":"bogus"}}"#),
            format!(r#"{{"type":"transfer_result","transfer_id":"{id}","outcome":"cancelled"}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":-1}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":"16"}}"#),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{id}","name":"A","size_bytes":-1,"chunk_count":1,"sha256":"{SHA}"}}"#
            ),
        ] {
            let err = PeerMessage::decode(&text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text}");
        }
    }

    #[test]
    fn a_full_chunk_frame_fits_and_decodes_at_exactly_the_frame_limit() {
        let raw = vec![0xABu8; CHUNK_BYTES];
        let message = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: u32::MAX,
            data: STANDARD.encode(&raw),
        };
        let text = message.encode();
        assert!(text.len() < MAX_FRAME_BYTES, "a full chunk leaves room");
        assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);

        // JSON allows trailing white space, so pad the same frame to the exact limit.
        let padded = format!("{text}{}", " ".repeat(MAX_FRAME_BYTES - text.len()));
        assert_eq!(padded.len(), MAX_FRAME_BYTES);
        assert_eq!(
            PeerMessage::decode(&padded).expect("limit is allowed"),
            message
        );

        let over = format!("{padded} ");
        assert!(PeerMessage::decode(&over).is_err());
    }

    #[test]
    fn chunk_frame_boundary_is_exact() {
        let empty = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: String::new(),
        };
        let base = empty.encode().len();
        let fits = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES - base),
        };
        assert_eq!(fits.encode().len(), MAX_FRAME_BYTES);
        assert_eq!(PeerMessage::decode(&fits.encode()).expect("fits"), fits);

        let over = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES - base + 1),
        };
        assert_eq!(over.encode().len(), MAX_FRAME_BYTES + 1);
        assert!(PeerMessage::decode(&over.encode()).is_err());
    }

    #[test]
    fn a_chunk_with_an_oversize_data_field_is_rejected() {
        let message = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES * 2),
        };
        let err = PeerMessage::decode(&message.encode()).expect_err("too large");
        match err {
            DomainError::InvalidInput(text) => {
                assert_eq!(text, "frame too large");
                assert!(!text.contains("AAAA"), "the error never echoes the frame");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn a_name_cannot_push_an_offer_past_the_frame_limit() {
        let offer = PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "n".repeat(MAX_FRAME_BYTES),
            size_bytes: 1,
            chunk_count: 1,
            sha256: SHA.into(),
            keep_in_sync: false,
            link_id: None,
        };
        assert!(PeerMessage::decode(&offer.encode()).is_err());
    }

    #[test]
    fn encoded_frames_have_the_expected_shape() {
        assert_eq!(PeerMessage::Ping.encode(), r#"{"type":"ping"}"#);
        assert_eq!(PeerMessage::Pong.encode(), r#"{"type":"pong"}"#);
        assert_eq!(PeerMessage::Unpair.encode(), r#"{"type":"unpair"}"#);
        let value: serde_json::Value = serde_json::from_str(&hello().encode()).expect("json");
        assert_eq!(
            value,
            json!({
                "type": "hello",
                "version": 3,
                "profile": {"display_name": "Ada Lovelace", "initials": "AL"}
            })
        );
    }

    #[test]
    fn decode_rejects_unknown_type_without_echoing_it() {
        for text in [
            r#"{"type":"send_collection"}"#,
            r#"{"type":"secret-token-123"}"#,
            r#"{"type":""}"#,
        ] {
            let err = PeerMessage::decode(text).expect_err("unknown type");
            match err {
                DomainError::InvalidInput(message) => {
                    assert!(!message.contains("secret-token"));
                    assert!(!message.contains("send_collection"));
                }
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }
    }

    #[test]
    fn decode_rejects_malformed_frames() {
        for text in [
            "",
            "not json",
            "[]",
            "42",
            "null",
            "{}",
            r#"{"kind":"ping"}"#,
            r#"{"type":"hello"}"#,
            r#"{"type":"hello","version":"1","profile":{"display_name":"A","initials":"A"}}"#,
        ] {
            let err = PeerMessage::decode(text).expect_err("rejected");
            assert!(
                matches!(err, DomainError::InvalidInput(_)),
                "input {text:?}"
            );
        }
    }

    #[test]
    fn decode_rejects_oversize_frame_and_accepts_the_limit() {
        let base = r#"{"type":"ping"}"#;
        let at_limit = format!("{base}{}", " ".repeat(MAX_FRAME_BYTES - base.len()));
        assert_eq!(at_limit.len(), MAX_FRAME_BYTES);
        assert_eq!(
            PeerMessage::decode(&at_limit).expect("limit is allowed"),
            PeerMessage::Ping
        );

        let over = format!("{at_limit} ");
        assert_eq!(over.len(), MAX_FRAME_BYTES + 1);
        let err = PeerMessage::decode(&over).expect_err("too large");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        let huge = "x".repeat(MAX_FRAME_BYTES * 4);
        assert!(PeerMessage::decode(&huge).is_err());
    }

    #[test]
    fn decode_normalizes_hello_profile() {
        let forged = r#"{"type":"hello","version":3,"profile":{"display_name":"Ada Lovelace","initials":"ZZZZZZ"}}"#;
        match PeerMessage::decode(forged).expect("decodes") {
            PeerMessage::Hello { profile, .. } => assert_eq!(profile.initials, "AL"),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_a_hello_with_a_bad_name() {
        let control = r#"{"type":"hello","version":3,"profile":{"display_name":"Ada\u001b[31m","initials":"A"}}"#;
        assert!(PeerMessage::decode(control).is_err());
        let empty = r#"{"type":"hello","version":3,"profile":{"display_name":"  ","initials":""}}"#;
        assert!(PeerMessage::decode(empty).is_err());
        let long = format!(
            r#"{{"type":"hello","version":3,"profile":{{"display_name":"{}","initials":"A"}}}}"#,
            "a".repeat(DISPLAY_NAME_MAX_CHARS + 1)
        );
        assert!(PeerMessage::decode(&long).is_err());
    }

    #[test]
    fn hello_with_an_unknown_field_is_rejected() {
        let text = r#"{"type":"hello","version":3,"profile":{"display_name":"Ada","initials":"A"},"extra":1}"#;
        assert!(PeerMessage::decode(text).is_err());
    }

    #[test]
    fn sync_frames_have_the_expected_shape() {
        let messages = sync_messages();
        assert_eq!(
            messages[0].encode(),
            format!(
                r#"{{"type":"sync_offer","sync_id":"{SYNC_ID}","link_id":"{LINK_ID}","base_hash":"{SHA}","kind":"manifest","transfer_id":"{ID}","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
            )
        );
        assert_eq!(
            messages[1].encode(),
            format!(r#"{{"type":"sync_abort","sync_id":"{SYNC_ID}","reason":"base_mismatch"}}"#)
        );
        assert_eq!(
            messages[2].encode(),
            format!(r#"{{"type":"sync_result","sync_id":"{SYNC_ID}","outcome":"applied"}}"#)
        );
    }

    #[test]
    fn sync_enums_use_snake_case_names() {
        for (kind, name) in [
            (SyncPayloadKind::Manifest, "manifest"),
            (SyncPayloadKind::Plan, "plan"),
            (SyncPayloadKind::Files, "files"),
        ] {
            assert_eq!(serde_json::to_value(kind).expect("json"), json!(name));
        }
        for (reason, name) in [
            (SyncAbortReason::BaseMismatch, "base_mismatch"),
            (SyncAbortReason::LinkMissing, "link_missing"),
            (SyncAbortReason::Busy, "busy"),
            (SyncAbortReason::Cancelled, "cancelled"),
            (SyncAbortReason::Declined, "declined"),
            (SyncAbortReason::Expired, "expired"),
            (SyncAbortReason::Failed, "failed"),
            (SyncAbortReason::TooBig, "too_big"),
        ] {
            assert_eq!(serde_json::to_value(reason).expect("json"), json!(name));
        }
        for (outcome, name) in [
            (SyncOutcome::Applied, "applied"),
            (SyncOutcome::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn sync_frames_reject_unknown_fields_and_bad_values() {
        let (sid, lid) = (SYNC_ID, LINK_ID);
        let offer = |tail: &str| {
            format!(
                r#"{{"type":"sync_offer","sync_id":"{sid}","link_id":"{lid}","base_hash":"{SHA}","kind":"manifest","transfer_id":"{ID}",{tail}}}"#
            )
        };
        for text in [
            // Unknown fields.
            offer(&format!(
                r#""size_bytes":1,"chunk_count":1,"sha256":"{SHA}","extra":1"#
            )),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}","reason":"failed","extra":1}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"applied","extra":1}}"#),
            // Missing fields.
            offer(r#""size_bytes":1,"chunk_count":1"#),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}"}}"#),
            r#"{"type":"sync_result","outcome":"applied"}"#.to_string(),
            // Wrong values.
            offer(&format!(
                r#""size_bytes":-1,"chunk_count":1,"sha256":"{SHA}""#
            )),
            offer(&format!(
                r#""size_bytes":1,"chunk_count":4294967296,"sha256":"{SHA}""#
            )),
            offer(&format!(
                r#""size_bytes":1,"chunk_count":1,"sha256":"{SHA}""#
            ))
            .replace(r#""kind":"manifest""#, r#""kind":"bogus""#),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}","reason":"bogus"}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"completed"}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"damaged"}}"#),
        ] {
            let err = PeerMessage::decode(&text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text}");
        }
    }

    #[test]
    fn a_version_2_offer_without_sync_fields_still_decodes() {
        let text = format!(
            r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
        );
        match PeerMessage::decode(&text).expect("decodes") {
            PeerMessage::CollectionOffer {
                keep_in_sync,
                link_id,
                ..
            } => {
                assert!(!keep_in_sync);
                assert_eq!(link_id, None);
            }
            other => panic!("expected an offer, got {other:?}"),
        }
        // A null link id reads as none too.
        let with_null = text.replace(r#""sha256":"#, r#""link_id":null,"sha256":"#);
        assert!(PeerMessage::decode(&with_null).is_ok());
    }

    #[test]
    fn an_offer_for_a_link_carries_both_fields() {
        let offer = PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "Orders".into(),
            size_bytes: 70_000,
            chunk_count: 3,
            sha256: SHA.into(),
            keep_in_sync: true,
            link_id: Some(LINK_ID.into()),
        };
        assert_eq!(
            offer.encode(),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}","keep_in_sync":true,"link_id":"{LINK_ID}"}}"#
            )
        );
        assert_eq!(
            PeerMessage::decode(&offer.encode()).expect("decodes"),
            offer
        );
    }

    #[test]
    fn an_offer_without_a_link_encodes_as_in_version_2() {
        let text = transfer_messages()[0].encode();
        assert!(!text.contains("keep_in_sync"));
        assert!(!text.contains("link_id"));
    }

    #[test]
    fn an_offer_with_a_bad_keep_in_sync_is_rejected() {
        for bad in [
            r#""keep_in_sync":"yes""#,
            r#""keep_in_sync":1"#,
            r#""link_id":5"#,
        ] {
            let text = format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"A","size_bytes":1,"chunk_count":1,"sha256":"{SHA}",{bad}}}"#
            );
            assert!(PeerMessage::decode(&text).is_err(), "input {bad}");
        }
    }

    #[test]
    fn sync_payload_names_fit_the_offer_name_rule() {
        for (kind, name) in [
            (SyncPayloadKind::Manifest, "manifest"),
            (SyncPayloadKind::Plan, "plan"),
            (SyncPayloadKind::Files, "files"),
        ] {
            assert_eq!(kind.transfer_name(), name);
            assert!(name.chars().count() <= SYNC_PAYLOAD_NAME_MAX);
            assert!(offer_name_is_valid(name));
        }
    }

    #[test]
    fn every_sync_frame_fits_the_frame_limit_with_room_to_spare() {
        let widest = PeerMessage::SyncOffer {
            sync_id: SYNC_ID.into(),
            link_id: LINK_ID.into(),
            base_hash: SHA.into(),
            kind: SyncPayloadKind::Manifest,
            transfer_id: ID.into(),
            size_bytes: u64::MAX,
            chunk_count: u32::MAX,
            sha256: SHA.into(),
        };
        let text = widest.encode();
        assert!(text.len() < 512, "a sync offer is a small frame");
        assert_eq!(PeerMessage::decode(&text).expect("decodes"), widest);
    }

    #[test]
    fn contract_check_frames_round_trip() {
        for message in contract_messages() {
            let text = message.encode();
            assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);
        }
    }

    #[test]
    fn contract_check_frames_have_the_expected_shape() {
        let messages = contract_messages();
        assert_eq!(
            messages[0].encode(),
            format!(
                r#"{{"type":"contract_check_offer","check_id":"{CHECK_ID}","contract":"Orders API","version":"1.2.0","policy":"additive_ok","changes":[{{"method":"GET","path_template":"/users/{{id}}","field":"status","change_type":"changed","old_value":"200","new_value":"204","breaking":true}}]}}"#
            )
        );
        assert_eq!(
            messages[1].encode(),
            format!(
                r#"{{"type":"contract_check_result","check_id":"{CHECK_ID}","outcome":"answered","rows":[{{"index":0,"verdict":"break"}}]}}"#
            )
        );
        assert_eq!(
            messages[2].encode(),
            format!(
                r#"{{"type":"contract_check_result","check_id":"{CHECK_ID}","outcome":"declined"}}"#
            )
        );
        let mut added = change();
        added.change_type = ContractChangeType::Added;
        added.old_value = None;
        added.new_value = None;
        let text = serde_json::to_string(&added).expect("json");
        assert!(!text.contains("old_value") && !text.contains("new_value"));
    }

    #[test]
    fn contract_check_enums_use_snake_case_names() {
        for (policy, name) in [
            (ContractPolicyWire::Strict, "strict"),
            (ContractPolicyWire::Lenient, "lenient"),
            (ContractPolicyWire::AdditiveOk, "additive_ok"),
        ] {
            assert_eq!(serde_json::to_value(policy).expect("json"), json!(name));
        }
        for (kind, name) in [
            (ContractChangeType::Changed, "changed"),
            (ContractChangeType::Added, "added"),
            (ContractChangeType::Removed, "removed"),
        ] {
            assert_eq!(serde_json::to_value(kind).expect("json"), json!(name));
        }
        for (verdict, name) in [
            (ContractVerdict::Pass, "pass"),
            (ContractVerdict::Break, "break"),
            (ContractVerdict::NotUsed, "not_used"),
        ] {
            assert_eq!(serde_json::to_value(verdict).expect("json"), json!(name));
        }
        for (outcome, name) in [
            (ContractCheckOutcome::Answered, "answered"),
            (ContractCheckOutcome::Declined, "declined"),
            (ContractCheckOutcome::Busy, "busy"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn contract_check_frames_reject_bad_values() {
        let bad_offer = |edit: &dyn Fn(&mut ContractChange), count: usize| {
            let mut c = change();
            edit(&mut c);
            contract_offer(vec![c; count]).encode()
        };
        let named = |name: String| {
            let mut m = contract_offer(vec![change()]);
            if let PeerMessage::ContractCheckOffer { contract, .. } = &mut m {
                *contract = name;
            }
            m.encode()
        };
        let row = ContractCheckRow {
            index: 0,
            verdict: ContractVerdict::Pass,
        };
        for text in [
            contract_offer(vec![change()])
                .encode()
                .replace(r#""policy""#, r#""extra":1,"policy""#),
            contract_result(vec![])
                .encode()
                .replace(r#""outcome""#, r#""extra":1,"outcome""#),
            bad_offer(&|_| {}, 0),
            bad_offer(&|_| {}, 201),
            named("a".repeat(121)),
            named("bad\nname".into()),
            bad_offer(&|c| c.path_template = "p".repeat(513), 1),
            bad_offer(&|c| c.field = "f".repeat(129), 1),
            contract_offer(vec![change()])
                .encode()
                .replace(CHECK_ID, "nope"),
            contract_result(vec![row.clone(); 201]).encode(),
        ] {
            let err = PeerMessage::decode(&text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)));
        }
        // The limits themselves are allowed.
        assert!(PeerMessage::decode(&named("a".repeat(120))).is_ok());
        assert!(PeerMessage::decode(&bad_offer(&|c| c.field = "f".repeat(128), 1)).is_ok());
        assert!(PeerMessage::decode(&contract_result(vec![row; 200]).encode()).is_ok());
    }

    #[test]
    fn contract_check_values_are_cut_at_256_on_a_char_boundary() {
        let mut c = change();
        c.old_value = Some("é".repeat(200));
        c.new_value = Some("a".repeat(300));
        c.cut_values();
        assert_eq!(c.old_value.as_deref().map(|v| v.chars().count()), Some(200));
        assert_eq!(c.new_value.as_deref().map(|v| v.chars().count()), Some(256));
        c.old_value = Some("é".repeat(300));
        c.cut_values();
        let old = c.old_value.expect("kept");
        assert_eq!(old.chars().count(), 256);
        assert!(old.chars().all(|ch| ch == 'é'));
    }

    #[test]
    fn contract_check_offer_fits_the_frame_limit_check() {
        let mut big = change();
        big.path_template = "p".repeat(512);
        big.field = "f".repeat(128);
        big.old_value = Some("o".repeat(256));
        big.new_value = Some("n".repeat(256));
        assert!(!contract_offer(vec![big; 200]).fits_frame());
        assert!(contract_offer(vec![change(); 5]).fits_frame());
        assert!(PeerMessage::Ping.fits_frame());
    }

    #[test]
    fn contract_check_decode_checks_every_field() {
        let offer_with = |edit: &dyn Fn(&mut ContractChange)| {
            let mut c = change();
            edit(&mut c);
            contract_offer(vec![c]).encode()
        };
        let named = |contract: &str, version: &str| {
            let mut m = contract_offer(vec![change()]);
            if let PeerMessage::ContractCheckOffer {
                contract: c,
                version: v,
                ..
            } = &mut m
            {
                *c = contract.into();
                *v = version.into();
            }
            m.encode()
        };
        let good = contract_offer(vec![change()]).encode();
        let row = contract_result(vec![ContractCheckRow {
            index: 0,
            verdict: ContractVerdict::Pass,
        }])
        .encode();
        for text in [
            good.replace(r#""breaking""#, r#""extra":1,"breaking""#),
            row.replace(r#""verdict""#, r#""extra":1,"verdict""#),
            named("Orders", &"v".repeat(33)),
            named("", "1"),
            offer_with(&|c| c.old_value = Some("x".repeat(257))),
            offer_with(&|c| c.new_value = Some("x".repeat(257))),
            offer_with(&|c| c.method = "M".repeat(17)),
            offer_with(&|c| c.method = String::new()),
            offer_with(&|c| c.method = "G\tT".into()),
            offer_with(&|c| c.field = "a\nb".into()),
            offer_with(&|c| c.path_template = "/a\u{7}".into()),
        ] {
            match PeerMessage::decode(&text).expect_err("rejected") {
                DomainError::InvalidInput(m) => {
                    assert!(m == "invalid peer message", "fixed text for {text}")
                }
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }
        assert!(PeerMessage::decode(&named("Orders", &"v".repeat(32))).is_ok());
        assert!(PeerMessage::decode(&offer_with(&|c| {
            c.old_value = Some("x".repeat(256));
            c.new_value = Some("é".repeat(256));
            c.method = "M".repeat(16);
        }))
        .is_ok());
        // A bad check id gives the fixed text too.
        let bad_id = good.replace(CHECK_ID, "nope");
        match PeerMessage::decode(&bad_id).expect_err("rejected") {
            DomainError::InvalidInput(m) => assert_eq!(m, "invalid peer message"),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
