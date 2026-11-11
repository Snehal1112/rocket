use crate::event::SecurityAuditEvent;
use sha2::{Digest, Sha256};

/// Computes the SHA-256 hash of the canonical JSON serialisation of an event's
/// identity + prev_hash. The hash field itself is excluded from the input.
pub fn hash_event(ev: &SecurityAuditEvent) -> String {
    let mut clone = ev.clone();
    clone.hash = String::new();
    let canonical = serde_json::to_vec(&clone).expect("event must serialize");
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChainVerification {
    Ok,
    Broken {
        index: usize,
        expected: String,
        actual: String,
    },
}

/// Walks events in order, recomputing each hash and confirming prev_hash linkage.
/// An event of an unknown kind keeps its stored hash, since this build cannot recompute it.
pub fn verify_chain(events: &[SecurityAuditEvent]) -> ChainVerification {
    let mut prev = String::new();
    for (i, ev) in events.iter().enumerate() {
        if ev.prev_hash != prev {
            return ChainVerification::Broken {
                index: i,
                expected: prev,
                actual: ev.prev_hash.clone(),
            };
        }
        // The hash of an event kind this build does not know cannot be recomputed.
        // Its link to the next event is still checked, through `prev_hash`.
        let expected = if ev.event.is_unknown() {
            ev.hash.clone()
        } else {
            hash_event(ev)
        };
        if expected != ev.hash {
            return ChainVerification::Broken {
                index: i,
                expected,
                actual: ev.hash.clone(),
            };
        }
        prev = ev.hash.clone();
    }
    ChainVerification::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AuditEventKind, SecurityAuditEvent};

    fn mk(prev: &str) -> SecurityAuditEvent {
        let mut ev = SecurityAuditEvent::new(
            "actor",
            None,
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
            prev,
        );
        ev.hash = hash_event(&ev);
        ev
    }

    #[test]
    fn hash_is_deterministic_for_same_event() {
        let ev = mk("");
        assert_eq!(hash_event(&ev), ev.hash);
    }

    #[test]
    fn verify_accepts_well_formed_chain() {
        let a = mk("");
        let b = mk(&a.hash);
        assert_eq!(verify_chain(&[a, b]), ChainVerification::Ok);
    }

    #[test]
    fn verify_rejects_tampered_event() {
        let mut a = mk("");
        let b = mk(&a.hash);
        // Tamper: change actor without recomputing hash.
        a.actor = "attacker".into();
        match verify_chain(&[a, b]) {
            ChainVerification::Broken { index, .. } => assert_eq!(index, 0),
            _ => panic!("expected broken chain"),
        }
    }

    #[test]
    fn verify_rejects_broken_prev_hash() {
        let a = mk("");
        let mut b = mk(&a.hash);
        b.prev_hash = "deadbeef".into();
        b.hash = hash_event(&b);
        match verify_chain(&[a, b]) {
            ChainVerification::Broken { index, .. } => assert_eq!(index, 1),
            _ => panic!("expected broken chain at index 1"),
        }
    }

    fn unknown_event(prev: &str, hash: &str) -> SecurityAuditEvent {
        let line = format!(
            r#"{{"id":"01M74GZKNT1Y8Q3FH01WDMAFFC","occurredAt":"2026-11-11T05:23:38Z","actor":"user","workspaceId":null,"event":{{"kind":"from_the_future","x":1}},"controls":[{{"framework":"hipaa","code":"164","title":"t"}}],"prevHash":"{prev}","hash":"{hash}"}}"#
        );
        serde_json::from_str(&line).expect("unknown kinds and frameworks must still parse")
    }

    #[test]
    fn verify_accepts_an_unknown_event_in_the_middle() {
        let a = mk("");
        let u = unknown_event(&a.hash, "abc123");
        let c = mk("abc123");
        assert!(u.event.is_unknown());
        assert_eq!(verify_chain(&[a, u, c]), ChainVerification::Ok);
    }

    #[test]
    fn verify_still_catches_a_broken_link_after_an_unknown_event() {
        let a = mk("");
        let u = unknown_event(&a.hash, "abc123");
        let c = mk("not-the-hash");
        match verify_chain(&[a, u, c]) {
            ChainVerification::Broken { index, .. } => assert_eq!(index, 2),
            _ => panic!("expected broken chain at index 2"),
        }
    }

    #[test]
    fn an_unknown_event_is_written_back_as_it_was_read() {
        let u = unknown_event("", "abc123");
        let json = serde_json::to_value(&u).unwrap();
        assert_eq!(json["event"]["kind"], "from_the_future");
        assert_eq!(json["event"]["x"], 1);
    }
}
