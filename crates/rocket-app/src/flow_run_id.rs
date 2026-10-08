//! Run ids for Flow runs. The client may choose the id, so it can match every
//! event of the run to the tab that asked for it (roadmap F-03).

use rocket_shared::error::{DomainError, DomainResult};
use ulid::Ulid;

/// Longest run id a client may choose. A UUID has 36 characters.
pub(crate) const MAX_RUN_ID_LEN: usize = 64;

/// The client's run id when it is well formed, or a new ULID when the client
/// sent none. Whether the id is free is checked when the run registers.
pub(crate) fn choose_run_id(requested: Option<String>) -> DomainResult<String> {
    let Some(id) = requested else {
        return Ok(Ulid::new().to_string());
    };
    if is_well_formed(&id) {
        Ok(id)
    } else {
        // The id is not quoted, because it can hold anything.
        Err(DomainError::InvalidInput(format!(
            "a flow run id must be 1 to {MAX_RUN_ID_LEN} letters, digits, '-' or '_'"
        )))
    }
}

fn is_well_formed(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_RUN_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_uuid_and_a_ulid() {
        let uuid = "0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c";
        assert_eq!(choose_run_id(Some(uuid.to_string())).expect("uuid"), uuid);
        let ulid = Ulid::new().to_string();
        assert_eq!(choose_run_id(Some(ulid.clone())).expect("ulid"), ulid);
        let longest = "x".repeat(MAX_RUN_ID_LEN);
        assert_eq!(choose_run_id(Some(longest.clone())).expect("64 chars"), longest);
    }

    #[test]
    fn generates_a_ulid_when_the_client_sent_none() {
        let id = choose_run_id(None).expect("generated");
        assert!(Ulid::from_string(&id).is_ok(), "got {id}");
        assert_ne!(choose_run_id(None).expect("second"), id);
    }

    #[test]
    fn rejects_empty_long_and_odd_ids_without_quoting_them() {
        let long = "x".repeat(MAX_RUN_ID_LEN + 1);
        for bad in ["", "has space", "line\nbreak", "../etc", "ü-umlaut", long.as_str()] {
            match choose_run_id(Some(bad.to_string())) {
                Err(DomainError::InvalidInput(message)) => {
                    if !bad.is_empty() {
                        assert!(!message.contains(bad), "{bad:?} quoted in {message}");
                    }
                }
                other => panic!("{bad:?} must be refused, got {other:?}"),
            }
        }
    }
}
