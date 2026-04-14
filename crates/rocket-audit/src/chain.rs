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
    Broken { index: usize, expected: String, actual: String },
}

/// Walks events in order, recomputing each hash and confirming prev_hash linkage.
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
        let expected = hash_event(ev);
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
            AuditEventKind::CollectionDeleted { collection: "x".into() },
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
}
