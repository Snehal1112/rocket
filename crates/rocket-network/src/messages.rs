//! The WebSocket frame type. One serde enum tagged by `type`.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::MAX_FRAME_BYTES;
use crate::profile::NodeProfile;

/// A text frame on the peer WebSocket. Later projects add variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerMessage {
    Hello { version: u32, profile: NodeProfile },
    Ping,
    Pong,
    Unpair,
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
    use crate::limits::DISPLAY_NAME_MAX_CHARS;
    use serde_json::json;

    fn hello() -> PeerMessage {
        PeerMessage::Hello {
            version: 1,
            profile: NodeProfile::new("Ada Lovelace").expect("valid name"),
        }
    }

    #[test]
    fn every_variant_round_trips() {
        for message in [
            hello(),
            PeerMessage::Ping,
            PeerMessage::Pong,
            PeerMessage::Unpair,
        ] {
            let text = message.encode();
            assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);
        }
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
                "version": 1,
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
        let forged = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada Lovelace","initials":"ZZZZZZ"}}"#;
        match PeerMessage::decode(forged).expect("decodes") {
            PeerMessage::Hello { profile, .. } => assert_eq!(profile.initials, "AL"),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_a_hello_with_a_bad_name() {
        let control = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada\u001b[31m","initials":"A"}}"#;
        assert!(PeerMessage::decode(control).is_err());
        let empty = r#"{"type":"hello","version":1,"profile":{"display_name":"  ","initials":""}}"#;
        assert!(PeerMessage::decode(empty).is_err());
        let long = format!(
            r#"{{"type":"hello","version":1,"profile":{{"display_name":"{}","initials":"A"}}}}"#,
            "a".repeat(DISPLAY_NAME_MAX_CHARS + 1)
        );
        assert!(PeerMessage::decode(&long).is_err());
    }

    #[test]
    fn hello_with_an_unknown_field_is_rejected() {
        let text = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada","initials":"A"},"extra":1}"#;
        assert!(PeerMessage::decode(text).is_err());
    }
}
