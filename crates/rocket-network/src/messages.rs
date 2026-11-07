//! The WebSocket frame type. One serde enum tagged by `type`.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::MAX_FRAME_BYTES;
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
            other => Ok(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{CHUNK_BYTES, DISPLAY_NAME_MAX_CHARS};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn transfer_messages() -> Vec<PeerMessage> {
        vec![
            PeerMessage::CollectionOffer {
                transfer_id: ID.into(),
                name: "Orders".into(),
                size_bytes: 70_000,
                chunk_count: 3,
                sha256: SHA.into(),
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
            version: 2,
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
                "version": 2,
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
        let forged = r#"{"type":"hello","version":2,"profile":{"display_name":"Ada Lovelace","initials":"ZZZZZZ"}}"#;
        match PeerMessage::decode(forged).expect("decodes") {
            PeerMessage::Hello { profile, .. } => assert_eq!(profile.initials, "AL"),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_a_hello_with_a_bad_name() {
        let control = r#"{"type":"hello","version":2,"profile":{"display_name":"Ada\u001b[31m","initials":"A"}}"#;
        assert!(PeerMessage::decode(control).is_err());
        let empty = r#"{"type":"hello","version":2,"profile":{"display_name":"  ","initials":""}}"#;
        assert!(PeerMessage::decode(empty).is_err());
        let long = format!(
            r#"{{"type":"hello","version":2,"profile":{{"display_name":"{}","initials":"A"}}}}"#,
            "a".repeat(DISPLAY_NAME_MAX_CHARS + 1)
        );
        assert!(PeerMessage::decode(&long).is_err());
    }

    #[test]
    fn hello_with_an_unknown_field_is_rejected() {
        let text = r#"{"type":"hello","version":2,"profile":{"display_name":"Ada","initials":"A"},"extra":1}"#;
        assert!(PeerMessage::decode(text).is_err());
    }
}
