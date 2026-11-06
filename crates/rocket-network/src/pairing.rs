//! Pairing: the commitment and code functions and the session state machine.
//!
//! No clock is read here. The caller passes `Instant` values in.

use std::fmt;
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::PeerIdentity;
use crate::limits::PAIRING_TTL;
use crate::profile::NodeProfile;

const COMMIT_LABEL: &[u8] = b"rocket-pair-commit-v1";
const CODE_LABEL: &[u8] = b"rocket-pair-code-v1";
/// Base64 length of 32 bytes with padding.
const BASE64_LEN_32: usize = 44;

/// Feeds a byte string into the hash with a big-endian u32 length in front.
fn update_prefixed(hasher: &mut Sha256, data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    hasher.update(len.to_be_bytes());
    hasher.update(data);
}

/// Compares two 32-byte values without an early exit.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn decode_32(s: &str, what: &str) -> DomainResult<[u8; 32]> {
    let invalid = || DomainError::InvalidInput(format!("invalid {what}"));
    if s.len() != BASE64_LEN_32 {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(s).map_err(|_| invalid())?;
    bytes.try_into().map_err(|_| invalid())
}

/// 32 random bytes. No `Debug`, so a nonce cannot be logged by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Nonce(pub [u8; 32]);

impl Nonce {
    pub fn random() -> Nonce {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Nonce(bytes)
    }

    pub fn to_base64(&self) -> String {
        STANDARD.encode(self.0)
    }

    /// Accepts exactly 32 bytes in standard padded base64.
    pub fn parse_base64(s: &str) -> DomainResult<Nonce> {
        decode_32(s, "nonce").map(Nonce)
    }
}

/// Encodes a commitment for the REST body.
pub fn commitment_to_base64(commitment: &[u8; 32]) -> String {
    STANDARD.encode(commitment)
}

/// Decodes a commitment from the REST body.
pub fn commitment_from_base64(s: &str) -> DomainResult<[u8; 32]> {
    decode_32(s, "commitment")
}

/// The id of one pairing session. A UUID v4 string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PairingId(pub String);

impl PairingId {
    pub fn new() -> PairingId {
        PairingId(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for PairingId {
    fn default() -> Self {
        PairingId::new()
    }
}

/// The 6-digit code both users compare. The value is 0 to 999 999.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingCode(pub u32);

impl PairingCode {
    /// Six digits with leading zeros, for example `048213`.
    pub fn digits(&self) -> String {
        format!("{:06}", self.0)
    }
}

impl fmt::Display for PairingCode {
    /// Two groups of three, for example `048 213`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.digits();
        write!(f, "{} {}", &digits[..3], &digits[3..])
    }
}

/// `Cb = SHA-256("rocket-pair-commit-v1" || PkB || PkA || Nb)`, keys length-prefixed.
pub fn commitment(responder_spki: &[u8], initiator_spki: &[u8], nb: &Nonce) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(COMMIT_LABEL);
    update_prefixed(&mut hasher, responder_spki);
    update_prefixed(&mut hasher, initiator_spki);
    hasher.update(nb.0);
    hasher.finalize().into()
}

/// The pairing code from both keys and both nonces.
pub fn pairing_code(
    initiator_spki: &[u8],
    responder_spki: &[u8],
    na: &Nonce,
    nb: &Nonce,
) -> PairingCode {
    let mut hasher = Sha256::new();
    hasher.update(CODE_LABEL);
    update_prefixed(&mut hasher, initiator_spki);
    update_prefixed(&mut hasher, responder_spki);
    hasher.update(na.0);
    hasher.update(nb.0);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    let value = u64::from_be_bytes(head) % 1_000_000;
    PairingCode(u32::try_from(value).unwrap_or(0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingRole {
    Initiator,
    Responder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    Committed,
    CodeShown,
    Paired,
    Declined,
    Expired,
    Cancelled,
    Failed,
}

/// What the initiator sees when it polls `GET /pair/{id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingOutcome {
    Accepted,
    Declined,
    Expired,
    Cancelled,
    Failed,
}

impl PairingOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            PairingOutcome::Accepted => "accepted",
            PairingOutcome::Declined => "declined",
            PairingOutcome::Expired => "expired",
            PairingOutcome::Cancelled => "cancelled",
            PairingOutcome::Failed => "failed",
        }
    }
}

/// One pairing, seen from one side. Lives in memory only.
pub struct PairingSession {
    id: PairingId,
    role: PairingRole,
    peer: PeerIdentity,
    peer_profile: NodeProfile,
    local_spki: Vec<u8>,
    state: PairingState,
    created: Instant,
    /// `nb` for a responder, `na` for an initiator.
    local_nonce: Nonce,
    /// The commitment the initiator was given. `None` for a responder.
    expected_commitment: Option<[u8; 32]>,
    code: Option<PairingCode>,
    local_decision: Option<bool>,
    remote_decision: Option<bool>,
    outcome: Option<PairingOutcome>,
}

impl PairingSession {
    /// Starts a session for the side that received `POST /pair/request`.
    /// Returns the session and the commitment to send back.
    pub fn new_responder(
        id: PairingId,
        peer: PeerIdentity,
        peer_profile: NodeProfile,
        local_spki: Vec<u8>,
        nb: Nonce,
        now: Instant,
    ) -> (PairingSession, [u8; 32]) {
        let commit = commitment(&local_spki, &peer.spki_der, &nb);
        let session = PairingSession {
            id,
            role: PairingRole::Responder,
            peer,
            peer_profile,
            local_spki,
            state: PairingState::Committed,
            created: now,
            local_nonce: nb,
            expected_commitment: None,
            code: None,
            local_decision: None,
            remote_decision: None,
            outcome: None,
        };
        (session, commit)
    }

    /// Starts a session for the side that sent `POST /pair/request` and got a commitment.
    pub fn new_initiator(
        id: PairingId,
        peer: PeerIdentity,
        peer_profile: NodeProfile,
        local_spki: Vec<u8>,
        commitment: [u8; 32],
        na: Nonce,
        now: Instant,
    ) -> PairingSession {
        PairingSession {
            id,
            role: PairingRole::Initiator,
            peer,
            peer_profile,
            local_spki,
            state: PairingState::Committed,
            created: now,
            local_nonce: na,
            expected_commitment: Some(commitment),
            code: None,
            local_decision: None,
            remote_decision: None,
            outcome: None,
        }
    }

    /// Responder, step 2: takes `na`, shows the code, returns `nb` to reveal.
    pub fn receive_nonce(&mut self, na: Nonce) -> DomainResult<(Nonce, PairingCode)> {
        if self.role != PairingRole::Responder {
            return Err(DomainError::InvalidInput(
                "only the responder receives the initiator nonce".to_string(),
            ));
        }
        self.require_committed()?;
        let code = pairing_code(
            &self.peer.spki_der,
            &self.local_spki,
            &na,
            &self.local_nonce,
        );
        self.code = Some(code);
        self.state = PairingState::CodeShown;
        Ok((self.local_nonce.clone(), code))
    }

    /// Initiator, step 3: checks the revealed `nb` against the commitment.
    /// A mismatch ends the session as `Failed`.
    pub fn receive_reveal(&mut self, nb: Nonce) -> DomainResult<PairingCode> {
        if self.role != PairingRole::Initiator {
            return Err(DomainError::InvalidInput(
                "only the initiator receives the revealed nonce".to_string(),
            ));
        }
        self.require_committed()?;
        let expected = self
            .expected_commitment
            .ok_or_else(|| DomainError::Internal("initiator has no commitment".to_string()))?;
        let actual = commitment(&self.peer.spki_der, &self.local_spki, &nb);
        if !constant_time_eq(&expected, &actual) {
            self.finish(PairingState::Failed, PairingOutcome::Failed);
            return Err(DomainError::InvalidInput("pairing failed".to_string()));
        }
        let code = pairing_code(
            &self.local_spki,
            &self.peer.spki_der,
            &self.local_nonce,
            &nb,
        );
        self.code = Some(code);
        self.state = PairingState::CodeShown;
        Ok(code)
    }

    /// The local user decided.
    pub fn decide_local(&mut self, accept: bool) -> DomainResult<()> {
        self.decide(true, accept)
    }

    /// The peer's decision arrived.
    pub fn decide_remote(&mut self, accept: bool) -> DomainResult<()> {
        self.decide(false, accept)
    }

    /// Ends a live session. A finished session stays as it is.
    pub fn cancel(&mut self) {
        if !self.is_terminal() {
            self.finish(PairingState::Cancelled, PairingOutcome::Cancelled);
        }
    }

    /// Ends a live session as failed, for example when the peer cannot be
    /// reached partway through. A finished session stays as it is.
    pub fn fail(&mut self) {
        if !self.is_terminal() {
            self.finish(PairingState::Failed, PairingOutcome::Failed);
        }
    }

    /// Expires a live session once `PAIRING_TTL` has passed since creation.
    /// Returns true when this call expired it. Call it before every decision.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.is_terminal() {
            return false;
        }
        if now.saturating_duration_since(self.created) >= PAIRING_TTL {
            self.finish(PairingState::Expired, PairingOutcome::Expired);
            return true;
        }
        false
    }

    pub fn id(&self) -> &PairingId {
        &self.id
    }

    pub fn role(&self) -> PairingRole {
        self.role
    }

    pub fn state(&self) -> PairingState {
        self.state
    }

    pub fn peer(&self) -> &PeerIdentity {
        &self.peer
    }

    pub fn peer_profile(&self) -> &NodeProfile {
        &self.peer_profile
    }

    pub fn code(&self) -> Option<PairingCode> {
        self.code
    }

    /// The local nonce in base64, for the wire.
    pub fn local_nonce_b64(&self) -> String {
        self.local_nonce.to_base64()
    }

    pub fn status(&self) -> PairingStatus {
        match self.state {
            PairingState::Paired => PairingStatus::Accepted,
            PairingState::Declined | PairingState::Cancelled | PairingState::Failed => {
                PairingStatus::Declined
            }
            PairingState::Expired => PairingStatus::Expired,
            PairingState::Committed | PairingState::CodeShown => PairingStatus::Pending,
        }
    }

    /// Set once, when the session becomes terminal.
    pub fn outcome(&self) -> Option<PairingOutcome> {
        self.outcome
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            PairingState::Paired
                | PairingState::Declined
                | PairingState::Expired
                | PairingState::Cancelled
                | PairingState::Failed
        )
    }

    fn finish(&mut self, state: PairingState, outcome: PairingOutcome) {
        self.state = state;
        self.outcome = Some(outcome);
    }

    fn require_committed(&self) -> DomainResult<()> {
        match self.state {
            PairingState::Committed => Ok(()),
            _ => Err(DomainError::Conflict(
                "pairing is not waiting for this step".to_string(),
            )),
        }
    }

    fn decide(&mut self, from_local: bool, accept: bool) -> DomainResult<()> {
        if self.is_terminal() {
            return Err(DomainError::Conflict(
                "pairing is already finished".to_string(),
            ));
        }
        if self.state != PairingState::CodeShown {
            return Err(DomainError::Conflict("pairing has no code yet".to_string()));
        }
        let slot = if from_local {
            &mut self.local_decision
        } else {
            &mut self.remote_decision
        };
        if slot.is_some() {
            return Err(DomainError::Conflict(
                "decision already recorded".to_string(),
            ));
        }
        *slot = Some(accept);
        if !accept {
            self.finish(PairingState::Declined, PairingOutcome::Declined);
        } else if self.local_decision == Some(true) && self.remote_decision == Some(true) {
            self.finish(PairingState::Paired, PairingOutcome::Accepted);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;
    use rocket_shared::error::DomainError;
    use std::time::Duration;

    fn spki(byte: u8) -> Vec<u8> {
        vec![byte; 44]
    }

    fn nonce(byte: u8) -> Nonce {
        Nonce([byte; 32])
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn profile() -> NodeProfile {
        NodeProfile::new("Ada Lovelace").expect("valid name")
    }

    /// Responder B (key 0xB2) and initiator A (key 0xA1) after step 1.
    fn pair_at(now: Instant, na: Nonce, nb: Nonce) -> (PairingSession, PairingSession) {
        let (responder, c) = PairingSession::new_responder(
            PairingId::new(),
            PeerIdentity::from_spki(spki(0xA1)),
            profile(),
            spki(0xB2),
            nb,
            now,
        );
        let initiator = PairingSession::new_initiator(
            responder.id().clone(),
            PeerIdentity::from_spki(spki(0xB2)),
            profile(),
            spki(0xA1),
            c,
            na,
            now,
        );
        (responder, initiator)
    }

    /// Both sessions after steps 2 and 3, with the code shown.
    fn shown(now: Instant) -> (PairingSession, PairingSession) {
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let (nb, _) = responder
            .receive_nonce(nonce(1))
            .expect("responder takes na");
        initiator.receive_reveal(nb).expect("commitment matches");
        (responder, initiator)
    }

    // Nonce

    #[test]
    fn nonce_base64_round_trips() {
        let original = nonce(9);
        let text = original.to_base64();
        assert_eq!(text.len(), 44);
        let parsed = Nonce::parse_base64(&text).expect("round trip");
        assert!(parsed == original);
    }

    #[test]
    fn nonce_parse_rejects_wrong_length_and_garbage() {
        let short = STANDARD.encode([1u8; 31]);
        let long = STANDARD.encode([1u8; 33]);
        for bad in ["", "!!!!", short.as_str(), long.as_str(), "AAAA"] {
            let err = Nonce::parse_base64(bad).err().expect("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    #[test]
    fn random_nonces_differ() {
        assert!(Nonce::random() != Nonce::random());
    }

    // Pairing id and code formatting

    #[test]
    fn pairing_id_is_a_unique_uuid_string() {
        let a = PairingId::new();
        let b = PairingId::default();
        assert_ne!(a, b);
        assert_eq!(a.0.len(), 36);
        assert!(uuid::Uuid::parse_str(&a.0).is_ok());
    }

    #[test]
    fn code_digits_pad_with_zeros() {
        assert_eq!(PairingCode(4821).digits(), "004821");
        assert_eq!(PairingCode(0).digits(), "000000");
        assert_eq!(PairingCode(999_999).digits(), "999999");
        assert_eq!(PairingCode(48_213).digits(), "048213");
    }

    #[test]
    fn code_display_groups_three_and_three() {
        assert_eq!(PairingCode(48_213).to_string(), "048 213");
        assert_eq!(PairingCode(4821).to_string(), "004 821");
        assert_eq!(PairingCode(999_999).to_string(), "999 999");
    }

    // Commitment and code functions

    #[test]
    fn commitment_matches_the_known_vector() {
        let c = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        assert_eq!(
            hex(&c),
            "b57ff7f7441d6019f770d430fd2e8e4ede27c8fd7835315a0131943b7f3d527a"
        );
    }

    #[test]
    fn commitment_depends_on_every_input() {
        let base = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        assert_ne!(base, commitment(&spki(0xB3), &spki(0xA1), &nonce(2)));
        assert_ne!(base, commitment(&spki(0xB2), &spki(0xA2), &nonce(2)));
        assert_ne!(base, commitment(&spki(0xB2), &spki(0xA1), &nonce(3)));
        // Swapping the two keys is a different commitment.
        assert_ne!(base, commitment(&spki(0xA1), &spki(0xB2), &nonce(2)));
    }

    #[test]
    fn length_prefix_stops_boundary_shifting() {
        // Without a length prefix these two inputs would hash the same bytes.
        let a = commitment(&[1, 2, 3], &[4, 5], &nonce(2));
        let b = commitment(&[1, 2], &[3, 4, 5], &nonce(2));
        assert_ne!(a, b);
    }

    #[test]
    fn pairing_code_matches_the_known_vector() {
        let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        assert_eq!(code, PairingCode(162_483));
    }

    #[test]
    fn pairing_code_pads_leading_zeros() {
        let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(60), &nonce(2));
        assert_eq!(code, PairingCode(5_263));
        assert_eq!(code.digits(), "005263");
        assert_eq!(code.to_string(), "005 263");
    }

    #[test]
    fn swapping_either_spki_changes_the_code() {
        let base = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        let swapped = pairing_code(&spki(0xB2), &spki(0xA1), &nonce(1), &nonce(2));
        assert_eq!(swapped, PairingCode(595_548));
        assert_ne!(base, swapped);
        let other_initiator = pairing_code(&spki(0xC3), &spki(0xB2), &nonce(1), &nonce(2));
        let other_responder = pairing_code(&spki(0xA1), &spki(0xC3), &nonce(1), &nonce(2));
        assert_ne!(base, other_initiator);
        assert_ne!(base, other_responder);
    }

    #[test]
    fn changing_either_nonce_changes_the_code() {
        let base = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        assert_ne!(
            base,
            pairing_code(&spki(0xA1), &spki(0xB2), &nonce(5), &nonce(2))
        );
        assert_ne!(
            base,
            pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(6))
        );
    }

    #[test]
    fn code_is_always_below_one_million() {
        for byte in 0..=255u8 {
            let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(byte), &nonce(2));
            assert!(code.0 < 1_000_000);
            assert_eq!(code.digits().len(), 6);
            assert!(code.digits().chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn commitment_base64_round_trips_and_checks_length() {
        let c = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        let text = commitment_to_base64(&c);
        assert_eq!(commitment_from_base64(&text).expect("round trip"), c);
        let short = STANDARD.encode([1u8; 31]);
        for bad in ["", "!!!!", short.as_str()] {
            let err = commitment_from_base64(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    // Session: the happy path

    #[test]
    fn responder_returns_the_commitment_and_starts_committed() {
        let now = Instant::now();
        let (responder, initiator) = pair_at(now, nonce(1), nonce(2));
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
        assert_eq!(responder.role(), PairingRole::Responder);
        assert_eq!(initiator.role(), PairingRole::Initiator);
        assert_eq!(responder.status(), PairingStatus::Pending);
        assert!(responder.code().is_none());
        assert!(responder.outcome().is_none());
        assert!(!responder.is_terminal());
        assert_eq!(responder.id(), initiator.id());
        assert_eq!(responder.peer().device_id, DeviceId::from_spki(&spki(0xA1)));
        assert_eq!(responder.peer_profile().display_name, "Ada Lovelace");
    }

    #[test]
    fn new_responder_commitment_matches_the_function() {
        let (_, c) = PairingSession::new_responder(
            PairingId::new(),
            PeerIdentity::from_spki(spki(0xA1)),
            profile(),
            spki(0xB2),
            nonce(2),
            Instant::now(),
        );
        assert_eq!(c, commitment(&spki(0xB2), &spki(0xA1), &nonce(2)));
    }

    #[test]
    fn end_to_end_codes_match() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let (nb, responder_code) = responder.receive_nonce(nonce(1)).expect("step 2");
        assert!(nb == nonce(2));
        assert_eq!(responder.state(), PairingState::CodeShown);
        let initiator_code = initiator.receive_reveal(nb).expect("step 3");
        assert_eq!(initiator.state(), PairingState::CodeShown);
        assert_eq!(responder_code, initiator_code);
        assert_eq!(responder_code, PairingCode(162_483));
        assert_eq!(responder.code(), Some(responder_code));
        assert_eq!(initiator.code(), Some(initiator_code));
    }

    #[test]
    fn end_to_end_codes_match_with_random_nonces() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, Nonce::random(), Nonce::random());
        let na = Nonce::parse_base64(&initiator.local_nonce_b64()).expect("na on the wire");
        let (nb, responder_code) = responder.receive_nonce(na).expect("step 2");
        assert_eq!(nb.to_base64(), responder.local_nonce_b64());
        let initiator_code = initiator.receive_reveal(nb).expect("step 3");
        assert_eq!(responder_code, initiator_code);
    }

    #[test]
    fn a_man_in_the_middle_sees_different_codes() {
        let now = Instant::now();
        // A talks to Mallory (key 0xC3), who talks to B (key 0xB2).
        let (mut mallory_for_a, mut a) = {
            let (r, c) = PairingSession::new_responder(
                PairingId::new(),
                PeerIdentity::from_spki(spki(0xA1)),
                profile(),
                spki(0xC3),
                nonce(7),
                now,
            );
            let a = PairingSession::new_initiator(
                r.id().clone(),
                PeerIdentity::from_spki(spki(0xC3)),
                profile(),
                spki(0xA1),
                c,
                nonce(1),
                now,
            );
            (r, a)
        };
        let (mut b, mut mallory_for_b) = {
            let (r, c) = PairingSession::new_responder(
                PairingId::new(),
                PeerIdentity::from_spki(spki(0xC3)),
                profile(),
                spki(0xB2),
                nonce(2),
                now,
            );
            let m = PairingSession::new_initiator(
                r.id().clone(),
                PeerIdentity::from_spki(spki(0xB2)),
                profile(),
                spki(0xC3),
                c,
                nonce(8),
                now,
            );
            (r, m)
        };
        let (nb_m, code_a_side) = mallory_for_a.receive_nonce(nonce(1)).expect("a side");
        a.receive_reveal(nb_m).expect("a checks");
        let (nb, code_b_side) = b.receive_nonce(nonce(8)).expect("b side");
        mallory_for_b.receive_reveal(nb).expect("m checks");
        assert_ne!(a.code(), b.code());
        assert_ne!(code_a_side, code_b_side);
    }

    #[test]
    fn wrong_revealed_nonce_fails_the_session() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        responder.receive_nonce(nonce(1)).expect("step 2");
        let err = initiator
            .receive_reveal(nonce(9))
            .expect_err("commitment mismatch");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(initiator.state(), PairingState::Failed);
        assert!(initiator.is_terminal());
        assert!(initiator.code().is_none());
        assert_eq!(initiator.outcome(), Some(PairingOutcome::Failed));
        assert_eq!(initiator.status(), PairingStatus::Declined);
        // A later correct reveal cannot revive the session.
        let err = initiator
            .receive_reveal(nonce(2))
            .expect_err("already failed");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(initiator.state(), PairingState::Failed);
    }

    #[test]
    fn roles_cannot_use_the_other_roles_steps() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let err = initiator.receive_nonce(nonce(1)).err().expect("initiator");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = responder.receive_reveal(nonce(2)).expect_err("responder");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
    }

    #[test]
    fn receive_nonce_twice_is_a_conflict() {
        let now = Instant::now();
        let (mut responder, _) = pair_at(now, nonce(1), nonce(2));
        responder.receive_nonce(nonce(1)).expect("first");
        let err = responder.receive_nonce(nonce(4)).err().expect("second");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(responder.code(), Some(PairingCode(162_483)));
    }

    // Session: decisions

    #[test]
    fn accept_and_accept_pairs_in_either_order() {
        let (mut responder, _) = shown(Instant::now());
        responder.decide_local(true).expect("local");
        assert_eq!(responder.state(), PairingState::CodeShown);
        assert_eq!(responder.status(), PairingStatus::Pending);
        responder.decide_remote(true).expect("remote");
        assert_eq!(responder.state(), PairingState::Paired);
        assert_eq!(responder.status(), PairingStatus::Accepted);
        assert_eq!(responder.outcome(), Some(PairingOutcome::Accepted));
        assert!(responder.is_terminal());

        let (_, mut initiator) = shown(Instant::now());
        initiator.decide_remote(true).expect("remote first");
        initiator.decide_local(true).expect("local second");
        assert_eq!(initiator.state(), PairingState::Paired);
    }

    #[test]
    fn any_decline_declines_the_session() {
        let (mut a, _) = shown(Instant::now());
        a.decide_local(false).expect("local decline");
        assert_eq!(a.state(), PairingState::Declined);
        assert_eq!(a.status(), PairingStatus::Declined);
        assert_eq!(a.outcome(), Some(PairingOutcome::Declined));

        let (mut b, _) = shown(Instant::now());
        b.decide_remote(false).expect("remote decline");
        assert_eq!(b.state(), PairingState::Declined);

        // A decline after the other side accepted still declines.
        let (mut c, _) = shown(Instant::now());
        c.decide_local(true).expect("local accept");
        c.decide_remote(false).expect("remote decline");
        assert_eq!(c.state(), PairingState::Declined);
        assert_eq!(c.status(), PairingStatus::Declined);
    }

    #[test]
    fn decision_before_the_code_is_shown_is_an_error() {
        let (mut responder, mut initiator) = pair_at(Instant::now(), nonce(1), nonce(2));
        assert!(responder.decide_local(true).is_err());
        assert!(responder.decide_remote(true).is_err());
        assert!(initiator.decide_local(true).is_err());
        assert!(initiator.decide_remote(false).is_err());
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
    }

    #[test]
    fn decision_after_a_terminal_state_is_a_conflict() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        for result in [
            session.decide_local(true),
            session.decide_local(false),
            session.decide_remote(true),
            session.decide_remote(false),
        ] {
            assert!(matches!(result, Err(DomainError::Conflict(_))));
        }
        assert_eq!(session.state(), PairingState::Paired);

        let (mut declined, _) = shown(Instant::now());
        declined.decide_remote(false).expect("decline");
        assert!(matches!(
            declined.decide_local(true),
            Err(DomainError::Conflict(_))
        ));
        assert_eq!(declined.state(), PairingState::Declined);
    }

    #[test]
    fn the_same_side_cannot_decide_twice() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("first");
        assert!(matches!(
            session.decide_local(false),
            Err(DomainError::Conflict(_))
        ));
        assert_eq!(session.state(), PairingState::CodeShown);
    }

    // Session: expiry and cancel

    #[test]
    fn session_expires_exactly_at_the_ttl() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        assert!(!session.expire_if_due(start));
        assert!(!session.expire_if_due(start + PAIRING_TTL - Duration::from_secs(1)));
        assert_eq!(session.state(), PairingState::CodeShown);
        assert!(session.expire_if_due(start + PAIRING_TTL));
        assert_eq!(session.state(), PairingState::Expired);
        assert_eq!(session.status(), PairingStatus::Expired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Expired));
        assert!(session.is_terminal());
    }

    #[test]
    fn expiry_counts_from_creation_not_from_the_code() {
        let start = Instant::now();
        let (mut session, _) = pair_at(start, nonce(1), nonce(2));
        assert!(session.expire_if_due(start + PAIRING_TTL + Duration::from_secs(5)));
        assert_eq!(session.state(), PairingState::Expired);
    }

    #[test]
    fn a_clock_before_creation_never_expires() {
        let later = Instant::now() + Duration::from_secs(30);
        let (mut session, _) = pair_at(later, nonce(1), nonce(2));
        assert!(!session.expire_if_due(Instant::now()));
        assert_eq!(session.state(), PairingState::Committed);
    }

    #[test]
    fn expiry_does_not_touch_a_terminal_session() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        assert!(!session.expire_if_due(start + PAIRING_TTL * 2));
        assert_eq!(session.state(), PairingState::Paired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Accepted));
    }

    #[test]
    fn decision_after_expiry_is_conflict() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        session
            .decide_local(true)
            .expect("local accept before expiry");
        assert!(session.expire_if_due(start + PAIRING_TTL));
        let err = session.decide_remote(true).expect_err("expired");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(session.state(), PairingState::Expired);
        assert_ne!(session.status(), PairingStatus::Accepted);
    }

    #[test]
    fn steps_after_expiry_are_conflicts() {
        let start = Instant::now();
        let (mut responder, _) = pair_at(start, nonce(1), nonce(2));
        assert!(responder.expire_if_due(start + PAIRING_TTL));
        assert!(matches!(
            responder.receive_nonce(nonce(1)),
            Err(DomainError::Conflict(_))
        ));
        assert!(responder.code().is_none());
    }

    #[test]
    fn cancel_ends_a_live_session() {
        let (mut session, _) = shown(Instant::now());
        session.cancel();
        assert_eq!(session.state(), PairingState::Cancelled);
        assert_eq!(session.status(), PairingStatus::Declined);
        assert_eq!(session.outcome(), Some(PairingOutcome::Cancelled));
        assert!(session.is_terminal());
        assert!(matches!(
            session.decide_local(true),
            Err(DomainError::Conflict(_))
        ));
    }

    #[test]
    fn cancel_does_not_change_a_finished_session() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        session.cancel();
        assert_eq!(session.state(), PairingState::Paired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Accepted));
    }

    #[test]
    fn fail_ends_a_live_session_as_failed() {
        let (mut session, _) = shown(Instant::now());
        session.fail();
        assert_eq!(session.state(), PairingState::Failed);
        assert_eq!(session.status(), PairingStatus::Declined);
        assert_eq!(session.outcome(), Some(PairingOutcome::Failed));
        assert!(session.is_terminal());
    }

    #[test]
    fn fail_does_not_change_a_finished_session() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(false).expect("local");
        session.fail();
        assert_eq!(session.outcome(), Some(PairingOutcome::Declined));
    }

    #[test]
    fn outcome_names_are_stable() {
        assert_eq!(PairingOutcome::Accepted.as_str(), "accepted");
        assert_eq!(PairingOutcome::Declined.as_str(), "declined");
        assert_eq!(PairingOutcome::Expired.as_str(), "expired");
        assert_eq!(PairingOutcome::Cancelled.as_str(), "cancelled");
        assert_eq!(PairingOutcome::Failed.as_str(), "failed");
    }

    #[test]
    fn status_serializes_in_snake_case() {
        for (status, text) in [
            (PairingStatus::Pending, "\"pending\""),
            (PairingStatus::Accepted, "\"accepted\""),
            (PairingStatus::Declined, "\"declined\""),
            (PairingStatus::Expired, "\"expired\""),
        ] {
            assert_eq!(serde_json::to_string(&status).expect("serializes"), text);
        }
    }
}
