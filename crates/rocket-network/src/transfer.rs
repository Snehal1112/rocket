//! Transfer ids, the sender and receiver state machines and the offer limits.
//!
//! Everything here is pure. The caller passes the time as an `Instant`, stages the bytes and
//! sends the frames. A failed check never returns an error to the socket loop: the machine
//! moves to a final state and the caller turns that into a `TransferAbort`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::limits::{
    ACK_EVERY_CHUNKS, CHUNK_BYTES, MAX_ACTIVE_PER_PEER, MAX_PENDING_INCOMING, MAX_TRANSFER_BYTES,
    MAX_UNACKED_CHUNKS, OFFERS_PER_PEER_PER_MINUTE, TRANSFER_OFFER_TTL,
};
use crate::messages::{AbortReason, PeerMessage, ResultOutcome};
use crate::rate::SlidingWindow;

/// The longest collection name in an offer, in characters.
const NAME_MAX_CHARS: usize = 200;
/// The most base64 characters one chunk may carry: `CHUNK_BYTES` raw, padded.
const MAX_CHUNK_B64_CHARS: usize = CHUNK_BYTES.div_ceil(3) * 4;

/// Ties the frames of one transfer together. 32 lowercase hex characters, not secret.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(String);

impl TransferId {
    /// A new random 128-bit id.
    pub fn random() -> TransferId {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        let mut text = String::with_capacity(32);
        for byte in bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        TransferId(text)
    }

    /// Accepts exactly 32 lowercase hex characters.
    pub fn parse(text: &str) -> DomainResult<TransferId> {
        let valid = text.len() == 32
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(TransferId(text.to_string()))
        } else {
            Err(DomainError::InvalidInput("invalid transfer id".to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How many chunks carry `size_bytes`. Zero bytes need no chunk.
pub fn chunk_count_for(size_bytes: u64) -> u32 {
    u32::try_from(size_bytes.div_ceil(CHUNK_BYTES as u64)).unwrap_or(u32::MAX)
}

/// Which way a transfer goes, seen from this node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Send,
    Receive,
}

/// How a transfer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    /// The sender sent every chunk and the receiver acknowledged the final one, but the
    /// receiver's result never came: the peer went away after the final `ChunkAck`, or
    /// `SENDER_RESULT_TIMEOUT` passed. Not a failure. A peer lost before the final ack is `Failed`.
    Unconfirmed,
    Declined,
    Expired,
    Cancelled,
    Failed,
    TooBig,
    Busy,
    Damaged,
    Rejected,
}

impl From<AbortReason> for Outcome {
    fn from(reason: AbortReason) -> Outcome {
        match reason {
            AbortReason::Cancelled => Outcome::Cancelled,
            AbortReason::Failed => Outcome::Failed,
            AbortReason::TooBig => Outcome::TooBig,
            AbortReason::Busy => Outcome::Busy,
            AbortReason::Expired => Outcome::Expired,
        }
    }
}

impl From<ResultOutcome> for Outcome {
    fn from(result: ResultOutcome) -> Outcome {
        match result {
            ResultOutcome::Completed => Outcome::Completed,
            ResultOutcome::Damaged => Outcome::Damaged,
            ResultOutcome::Rejected => Outcome::Rejected,
            ResultOutcome::Failed => Outcome::Failed,
        }
    }
}

/// Receives the transfer frames that `NetworkService` does not handle itself.
pub trait TransferSink: Send + Sync {
    fn on_frame(&self, from: DeviceId, message: PeerMessage);

    /// Called when the peer socket ends or the pairing is removed. Both roles clean up at once.
    fn on_peer_gone(&self, device: DeviceId);
}

/// Why an offer is refused before any state exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferError {
    BadId,
    BadName,
    BadHash,
    Empty,
    TooBig,
    ChunkCountMismatch,
}

impl OfferError {
    /// The reason to put in the `TransferAbort` that answers a refused offer.
    pub fn abort_reason(self) -> AbortReason {
        match self {
            OfferError::TooBig => AbortReason::TooBig,
            _ => AbortReason::Failed,
        }
    }
}

/// Bidi controls, zero-width characters and other invisible format characters that can make
/// a name read differently from what it is.
fn is_spoofing_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{61c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// The offer name rule of the spec (section 6). The sender checks it before a send, the
/// receiver on every offer.
pub fn offer_name_is_valid(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= NAME_MAX_CHARS
        && !name
            .chars()
            .any(|c| c.is_control() || is_spoofing_format_char(c))
}

fn validate_offer(
    name: &str,
    size_bytes: u64,
    chunk_count: u32,
    sha256: &str,
) -> Result<(), OfferError> {
    if !offer_name_is_valid(name) {
        return Err(OfferError::BadName);
    }
    if size_bytes == 0 {
        return Err(OfferError::Empty);
    }
    if size_bytes > MAX_TRANSFER_BYTES {
        return Err(OfferError::TooBig);
    }
    if chunk_count != chunk_count_for(size_bytes) {
        return Err(OfferError::ChunkCountMismatch);
    }
    let hex = sha256.len() == 64
        && sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !hex {
        return Err(OfferError::BadHash);
    }
    Ok(())
}

/// The raw length chunk number `next_seq` must have, given the bytes moved so far.
fn expected_len(size_bytes: u64, moved_bytes: u64) -> u64 {
    size_bytes
        .saturating_sub(moved_bytes)
        .min(CHUNK_BYTES as u64)
}

fn offer_expired(created: Instant, now: Instant, ttl: Duration) -> bool {
    now.saturating_duration_since(created) >= ttl
}

fn abort_frame(id: &TransferId, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.as_str().to_string(),
        reason,
    }
}

fn ack_frame(id: &TransferId, next_seq: u32) -> PeerMessage {
    PeerMessage::ChunkAck {
        transfer_id: id.as_str().to_string(),
        next_seq,
    }
}

// ---- sender ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgoingState {
    Offered,
    Accepted,
    Sending,
    Done,
    Aborted,
    Expired,
}

/// The sender side of one transfer.
#[derive(Debug, Clone)]
pub struct OutgoingTransfer {
    id: TransferId,
    name: String,
    size_bytes: u64,
    chunk_count: u32,
    sha256: String,
    created: Instant,
    offer_ttl: Duration,
    state: OutgoingState,
    next_seq: u32,
    acked: u32,
    sent_bytes: u64,
    /// The receiver's final `ChunkAck` (`next_seq == chunk_count`) arrived.
    final_acked: bool,
    /// The result question is settled: a result arrived, or the wait ended `Unconfirmed`.
    result_received: bool,
    outcome: Option<Outcome>,
}

impl OutgoingTransfer {
    /// Starts in `Offered`. The same checks as the receiver apply, so a bad offer never leaves.
    pub fn new(
        id: TransferId,
        name: &str,
        size_bytes: u64,
        sha256: &str,
        now: Instant,
    ) -> Result<OutgoingTransfer, OfferError> {
        let chunk_count = chunk_count_for(size_bytes);
        validate_offer(name, size_bytes, chunk_count, sha256)?;
        Ok(OutgoingTransfer {
            id,
            name: name.to_string(),
            size_bytes,
            chunk_count,
            sha256: sha256.to_string(),
            created: now,
            offer_ttl: TRANSFER_OFFER_TTL,
            state: OutgoingState::Offered,
            next_seq: 0,
            acked: 0,
            sent_bytes: 0,
            final_acked: false,
            result_received: false,
            outcome: None,
        })
    }

    /// Test seam: shortens the offer lifetime that otherwise is `TRANSFER_OFFER_TTL`.
    #[doc(hidden)]
    pub fn with_offer_ttl(mut self, ttl: Duration) -> Self {
        self.offer_ttl = ttl;
        self
    }

    pub fn transfer_id(&self) -> &TransferId {
        &self.id
    }

    pub fn state(&self) -> OutgoingState {
        self.state
    }

    /// `None` while the transfer is active.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            OutgoingState::Offered | OutgoingState::Accepted | OutgoingState::Sending
        )
    }

    pub fn done_bytes(&self) -> u64 {
        self.sent_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Chunks sent and not yet acknowledged.
    pub fn unacked_chunks(&self) -> u32 {
        self.next_seq - self.acked
    }

    /// True when the next chunk may be sent now: accepted, chunks left and room in the window.
    pub fn window_open(&self) -> bool {
        matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending)
            && self.unacked_chunks() < MAX_UNACKED_CHUNKS
    }

    /// The receiver's `ChunkAck`. Returns `Some(Failed)` when the ack breaks a rule (before the
    /// accept, backwards, or past the chunks sent), `None` otherwise. Ignored once final.
    pub fn on_ack(&mut self, next_seq: u32) -> Option<Outcome> {
        let open = matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending)
            || self.last_chunk_built();
        if self.state == OutgoingState::Offered {
            return self.finish(Outcome::Failed);
        }
        if !open {
            return None;
        }
        if next_seq < self.acked || next_seq > self.next_seq {
            return self.finish(Outcome::Failed);
        }
        self.acked = next_seq;
        if self.acked == self.chunk_count {
            self.final_acked = true;
        }
        None
    }

    pub fn offer_message(&self) -> PeerMessage {
        PeerMessage::CollectionOffer {
            transfer_id: self.id.as_str().to_string(),
            name: self.name.clone(),
            size_bytes: self.size_bytes,
            chunk_count: self.chunk_count,
            sha256: self.sha256.clone(),
        }
    }

    pub fn abort_message(&self, reason: AbortReason) -> PeerMessage {
        abort_frame(&self.id, reason)
    }

    /// Expires an unanswered offer at exactly the offer ttl. True when it changed.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.state == OutgoingState::Offered && offer_expired(self.created, now, self.offer_ttl)
        {
            self.finish(Outcome::Expired);
            return true;
        }
        false
    }

    /// Applies the receiver's answer. Returns the outcome when this call ended the transfer.
    /// An answer that arrives at or after the TTL finds the offer expired.
    pub fn on_answer(&mut self, accept: bool, now: Instant) -> Option<Outcome> {
        if self.expire_if_due(now) {
            return Some(Outcome::Expired);
        }
        if self.state != OutgoingState::Offered {
            return None;
        }
        if accept {
            self.state = OutgoingState::Accepted;
            None
        } else {
            self.finish(Outcome::Declined)
        }
    }

    /// Builds the next chunk frame from the next slice of the ZIP. The slice must be
    /// `CHUNK_BYTES` long, or the remainder for the last chunk.
    pub fn next_chunk(&mut self, raw: &[u8]) -> DomainResult<PeerMessage> {
        if !matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending) {
            return Err(DomainError::InvalidInput(
                "the transfer is not accepted".to_string(),
            ));
        }
        if self.unacked_chunks() >= MAX_UNACKED_CHUNKS {
            return Err(DomainError::InvalidInput(
                "the ack window is full".to_string(),
            ));
        }
        let expected = expected_len(self.size_bytes, self.sent_bytes);
        if raw.len() as u64 != expected {
            return Err(DomainError::InvalidInput(
                "unexpected chunk length".to_string(),
            ));
        }
        let message = PeerMessage::Chunk {
            transfer_id: self.id.as_str().to_string(),
            seq: self.next_seq,
            data: STANDARD.encode(raw),
        };
        self.next_seq += 1;
        self.sent_bytes += expected;
        if self.next_seq == self.chunk_count {
            self.finish(Outcome::Completed);
        } else {
            self.state = OutgoingState::Sending;
        }
        Ok(message)
    }

    /// The peer stopped the transfer. Ignored once the transfer is final.
    pub fn on_abort(&mut self, reason: AbortReason) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::from(reason))
    }

    /// The user stopped the transfer. Returns the frame to send, once.
    pub fn cancel(&mut self) -> Option<PeerMessage> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::Cancelled);
        Some(abort_frame(&self.id, AbortReason::Cancelled))
    }

    /// The connection dropped. Only after the receiver's final `ChunkAck` arrived (the result
    /// is awaited) is it `Unconfirmed`: the receiver has every byte and the import may still
    /// succeed. Before the final ack, including after the last chunk was built or sent, it is
    /// `Failed` (the receiver may never have got the last chunk).
    pub fn connection_lost(&mut self) -> Option<Outcome> {
        if self.awaiting_result() {
            return self.finish(Outcome::Unconfirmed);
        }
        if !self.is_active() && !self.last_chunk_built() {
            return None;
        }
        self.finish(Outcome::Failed)
    }

    /// `SENDER_RESULT_TIMEOUT` passed. `Unconfirmed` while the result is awaited, else `None`
    /// (before the final ack there is no result timer; the idle limit applies instead).
    pub fn result_timed_out(&mut self) -> Option<Outcome> {
        if !self.awaiting_result() {
            return None;
        }
        self.finish(Outcome::Unconfirmed)
    }

    /// True after the last chunk frame was built and the receiver's final `ChunkAck` arrived,
    /// and before the receiver's result arrived. False between the last chunk and the final ack.
    pub fn awaiting_result(&self) -> bool {
        self.last_chunk_built() && self.final_acked
    }

    /// The last chunk frame was built and no result settled the transfer yet. The final
    /// `ChunkAck` may still be missing. Plan 3 uses it to know the stream is finished and to
    /// refuse a cancel; `awaiting_result()` is the stricter question.
    pub fn last_chunk_built(&self) -> bool {
        self.state == OutgoingState::Done && !self.result_received
    }

    /// The receiver reported the true outcome. Accepted once, after the last chunk was built
    /// (a result implies the final ack, so one that overtakes it counts as acked). `Completed` keeps the machine `Done`, the other results end it
    /// `Aborted` with the same-named outcome. Anything else returns `None` and changes nothing.
    pub fn on_result(&mut self, result: ResultOutcome) -> Option<Outcome> {
        if !self.last_chunk_built() {
            return None;
        }
        self.final_acked = true;
        self.result_received = true;
        match Outcome::from(result) {
            Outcome::Completed => Some(Outcome::Completed),
            failure => self.finish(failure),
        }
    }

    /// The service ends the transfer itself. Allowed from any state except an already final
    /// failure (`Aborted`, `Expired`), so it also works after `Done`: the machine is `Done` the
    /// moment the last chunk frame is built, and the result timeout fires after that. A send
    /// the receiver already confirmed stays `Completed`. `Completed` is not a failure.
    pub fn fail_with(&mut self, outcome: Outcome) -> Option<Outcome> {
        if outcome == Outcome::Completed {
            return None;
        }
        if matches!(self.state, OutgoingState::Aborted | OutgoingState::Expired)
            || (self.state == OutgoingState::Done && self.result_received)
        {
            return None;
        }
        self.finish(outcome)
    }

    fn finish(&mut self, outcome: Outcome) -> Option<Outcome> {
        self.state = match outcome {
            Outcome::Completed => OutgoingState::Done,
            // Every chunk left, so the machine stays `Done`, but the wait is over.
            Outcome::Unconfirmed => {
                self.result_received = true;
                OutgoingState::Done
            }
            Outcome::Expired => OutgoingState::Expired,
            _ => OutgoingState::Aborted,
        };
        self.outcome = Some(outcome);
        Some(outcome)
    }
}

// ---- receiver ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingState {
    Offered,
    Accepted,
    Receiving,
    Done,
    Aborted,
    Expired,
}

/// What `IncomingTransfer::on_chunk` asks the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkStep {
    /// A frame for a finished transfer. Do nothing.
    Ignored,
    /// A valid chunk. Append `bytes` to the staged file. `last` is true for the final chunk.
    /// `ack` is the `ChunkAck` frame to send after staging: `Some` on every
    /// `ACK_EVERY_CHUNKS`-th accepted chunk and on the last one.
    Data {
        bytes: Vec<u8>,
        done_bytes: u64,
        last: bool,
        ack: Option<PeerMessage>,
    },
    /// The chunk broke a rule. The transfer is final: send `TransferAbort { failed }`, discard
    /// the staged file and report `Outcome::Failed`.
    Failed,
}

/// The receiver side of one transfer.
#[derive(Debug, Clone)]
pub struct IncomingTransfer {
    id: TransferId,
    name: String,
    size_bytes: u64,
    chunk_count: u32,
    sha256: String,
    created: Instant,
    offer_ttl: Duration,
    state: IncomingState,
    next_seq: u32,
    received_bytes: u64,
    outcome: Option<Outcome>,
}

impl IncomingTransfer {
    /// Checks an offer frame and starts in `Offered`. An error carries the reason to abort with.
    pub fn from_offer(
        transfer_id: &str,
        name: &str,
        size_bytes: u64,
        chunk_count: u32,
        sha256: &str,
        now: Instant,
    ) -> Result<IncomingTransfer, OfferError> {
        let id = TransferId::parse(transfer_id).map_err(|_| OfferError::BadId)?;
        validate_offer(name, size_bytes, chunk_count, sha256)?;
        Ok(IncomingTransfer {
            id,
            name: name.to_string(),
            size_bytes,
            chunk_count,
            sha256: sha256.to_string(),
            created: now,
            offer_ttl: TRANSFER_OFFER_TTL,
            state: IncomingState::Offered,
            next_seq: 0,
            received_bytes: 0,
            outcome: None,
        })
    }

    /// Test seam: shortens the offer lifetime that otherwise is `TRANSFER_OFFER_TTL`.
    #[doc(hidden)]
    pub fn with_offer_ttl(mut self, ttl: Duration) -> Self {
        self.offer_ttl = ttl;
        self
    }

    pub fn transfer_id(&self) -> &TransferId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn state(&self) -> IncomingState {
        self.state
    }

    /// `None` while the transfer is active.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// Waiting for the user's answer.
    pub fn is_pending(&self) -> bool {
        self.state == IncomingState::Offered
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            IncomingState::Offered | IncomingState::Accepted | IncomingState::Receiving
        )
    }

    pub fn done_bytes(&self) -> u64 {
        self.received_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn abort_message(&self, reason: AbortReason) -> PeerMessage {
        abort_frame(&self.id, reason)
    }

    /// Expires an unanswered offer at exactly the offer ttl. True when it changed.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.state == IncomingState::Offered && offer_expired(self.created, now, self.offer_ttl)
        {
            self.finish(Outcome::Expired);
            return true;
        }
        false
    }

    /// The user's answer. Returns the frame to send, or `None` when the answer is not valid
    /// any more (already answered, final or expired). After a `None`, read `outcome()` to see
    /// whether the offer expired.
    pub fn respond(&mut self, accept: bool, now: Instant) -> Option<PeerMessage> {
        self.expire_if_due(now);
        if self.state != IncomingState::Offered {
            return None;
        }
        if accept {
            self.state = IncomingState::Accepted;
        } else {
            self.finish(Outcome::Declined);
        }
        Some(PeerMessage::OfferAnswer {
            transfer_id: self.id.as_str().to_string(),
            accept,
        })
    }

    /// Checks and decodes one chunk frame.
    pub fn on_chunk(&mut self, seq: u32, data: &str) -> ChunkStep {
        match self.state {
            IncomingState::Accepted | IncomingState::Receiving => {}
            // A chunk before the accept is a protocol error.
            IncomingState::Offered => {
                self.finish(Outcome::Failed);
                return ChunkStep::Failed;
            }
            IncomingState::Done | IncomingState::Aborted | IncomingState::Expired => {
                return ChunkStep::Ignored;
            }
        }
        let Some(bytes) = self.check_chunk(seq, data) else {
            self.finish(Outcome::Failed);
            return ChunkStep::Failed;
        };
        self.received_bytes += bytes.len() as u64;
        self.next_seq += 1;
        let last = self.next_seq == self.chunk_count;
        if last {
            self.finish(Outcome::Completed);
        } else {
            self.state = IncomingState::Receiving;
        }
        let ack = (last || self.next_seq.is_multiple_of(ACK_EVERY_CHUNKS))
            .then(|| ack_frame(&self.id, self.next_seq));
        ChunkStep::Data {
            bytes,
            done_bytes: self.received_bytes,
            last,
            ack,
        }
    }

    /// The decoded bytes when the chunk is the one expected, `None` for any rule break.
    fn check_chunk(&self, seq: u32, data: &str) -> Option<Vec<u8>> {
        if seq != self.next_seq || seq >= self.chunk_count {
            return None;
        }
        if data.len() > MAX_CHUNK_B64_CHARS {
            return None;
        }
        let bytes = STANDARD.decode(data).ok()?;
        let expected = expected_len(self.size_bytes, self.received_bytes);
        (bytes.len() as u64 == expected).then_some(bytes)
    }

    /// The peer stopped the transfer. Ignored once all bytes arrived or the transfer is final.
    pub fn on_abort(&mut self, reason: AbortReason) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::from(reason))
    }

    /// The connection dropped. Ignored once all bytes arrived.
    pub fn connection_lost(&mut self) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::Failed)
    }

    /// The service ended the transfer after it checked the staged file or tried the import.
    /// Allowed from any state except an already final failure. `Completed` is not a failure.
    pub fn fail_with(&mut self, outcome: Outcome) -> Option<Outcome> {
        if outcome == Outcome::Completed {
            return None;
        }
        if matches!(self.state, IncomingState::Aborted | IncomingState::Expired) {
            return None;
        }
        self.finish(outcome)
    }

    fn finish(&mut self, outcome: Outcome) -> Option<Outcome> {
        self.state = match outcome {
            Outcome::Completed => IncomingState::Done,
            Outcome::Expired => IncomingState::Expired,
            _ => IncomingState::Aborted,
        };
        self.outcome = Some(outcome);
        Some(outcome)
    }
}

// ---- limits ----

/// What `TransferLimits::admit_offer` decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Track the offer and show it to the user.
    Admit,
    /// The id is already known. Ignore the frame and leave the first transfer alone.
    Duplicate,
    /// Answer with `TransferAbort` and this reason.
    Refuse(AbortReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pending,
    Active,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    peer: DeviceId,
    direction: Direction,
    slot: Slot,
}

/// Tracks the transfers a node has open and applies the offer limits. Pure.
#[derive(Debug, Default)]
pub struct TransferLimits {
    entries: HashMap<TransferId, Entry>,
    windows: HashMap<DeviceId, SlidingWindow>,
}

impl TransferLimits {
    pub fn new() -> TransferLimits {
        TransferLimits::default()
    }

    /// Decides on an incoming offer. A duplicate id costs nothing. Every other offer counts
    /// against the peer's rate window, even a refused one, so a flood stays limited.
    pub fn admit_offer(&mut self, peer: &DeviceId, id: &TransferId, now: Instant) -> Admission {
        if self.entries.contains_key(id) {
            return Admission::Duplicate;
        }
        let window = self.windows.entry(*peer).or_insert_with(|| {
            SlidingWindow::new(OFFERS_PER_PEER_PER_MINUTE as usize, Duration::from_secs(60))
        });
        if !window.try_hit(now) {
            return Admission::Refuse(AbortReason::Busy);
        }
        if self.pending_incoming() >= MAX_PENDING_INCOMING
            || self.open_for(peer, Direction::Receive) >= MAX_ACTIVE_PER_PEER
        {
            return Admission::Refuse(AbortReason::Busy);
        }
        self.entries.insert(
            id.clone(),
            Entry {
                peer: *peer,
                direction: Direction::Receive,
                slot: Slot::Pending,
            },
        );
        Admission::Admit
    }

    /// Claims the peer's one outgoing slot. False when the peer already has one or the id is known.
    pub fn begin_outgoing(&mut self, peer: &DeviceId, id: &TransferId) -> bool {
        if self.entries.contains_key(id)
            || self.open_for(peer, Direction::Send) >= MAX_ACTIVE_PER_PEER
        {
            return false;
        }
        self.entries.insert(
            id.clone(),
            Entry {
                peer: *peer,
                direction: Direction::Send,
                slot: Slot::Active,
            },
        );
        true
    }

    /// The user accepted: the offer no longer waits, but it still holds the peer's slot.
    pub fn mark_active(&mut self, id: &TransferId) -> bool {
        match self.entries.get_mut(id) {
            Some(entry) => {
                entry.slot = Slot::Active;
                true
            }
            None => false,
        }
    }

    /// Forgets a finished transfer. False for an unknown id.
    pub fn release(&mut self, id: &TransferId) -> bool {
        self.entries.remove(id).is_some()
    }

    /// Incoming offers that wait for the user, across all peers.
    pub fn pending_incoming(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.direction == Direction::Receive && e.slot == Slot::Pending)
            .count()
    }

    fn open_for(&self, peer: &DeviceId, direction: Direction) -> usize {
        self.entries
            .values()
            .filter(|e| e.peer == *peer && e.direction == direction)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{
        ACK_EVERY_CHUNKS, CHUNK_BYTES, MAX_ACTIVE_PER_PEER, MAX_PENDING_INCOMING,
        MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS, OFFERS_PER_PEER_PER_MINUTE, TRANSFER_OFFER_TTL,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ID: &str = "0123456789abcdef0123456789abcdef";
    const CB: u64 = CHUNK_BYTES as u64;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn id() -> TransferId {
        TransferId::parse(ID).expect("valid id")
    }

    fn outgoing(size: u64, now: Instant) -> OutgoingTransfer {
        OutgoingTransfer::new(id(), "Orders", size, SHA, now).expect("valid offer")
    }

    fn incoming(size: u64, now: Instant) -> IncomingTransfer {
        IncomingTransfer::from_offer(ID, "Orders", size, chunk_count_for(size), SHA, now)
            .expect("valid offer")
    }

    fn accepted(size: u64, now: Instant) -> IncomingTransfer {
        let mut transfer = incoming(size, now);
        assert!(transfer.respond(true, now).is_some());
        transfer
    }

    fn b64(len: usize) -> String {
        STANDARD.encode(vec![7u8; len])
    }

    fn peer(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 32])
    }

    struct RecordingSink(Mutex<Vec<(&'static str, DeviceId)>>);

    impl TransferSink for RecordingSink {
        fn on_frame(&self, from: DeviceId, _message: PeerMessage) {
            self.0.lock().expect("lock").push(("frame", from));
        }

        fn on_peer_gone(&self, device: DeviceId) {
            self.0.lock().expect("lock").push(("gone", device));
        }
    }

    /// The last chunk frame is built, but the receiver's final `ChunkAck` has not arrived.
    fn last_chunk_sent(now: Instant) -> OutgoingTransfer {
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(t.state(), OutgoingState::Done);
        assert!(!t.awaiting_result(), "no final ack yet");
        t
    }

    /// The final `ChunkAck` (`next_seq == chunk_count`, 1 here) arrived: now the result is awaited.
    fn waiting_for_result(now: Instant) -> OutgoingTransfer {
        let mut t = last_chunk_sent(now);
        assert_eq!(t.on_ack(1), None);
        assert!(t.awaiting_result());
        t
    }

    // ---- ids and counts ----

    #[test]
    fn a_sink_is_object_safe_and_gets_frames_and_peer_gone() {
        let sink = Arc::new(RecordingSink(Mutex::new(Vec::new())));
        let dynamic: Arc<dyn TransferSink> = sink.clone();
        dynamic.on_frame(peer(1), PeerMessage::Ping);
        dynamic.on_peer_gone(peer(2));
        assert_eq!(
            *sink.0.lock().expect("lock"),
            vec![("frame", peer(1)), ("gone", peer(2))]
        );
    }

    #[test]
    fn random_ids_are_32_lowercase_hex_and_differ() {
        let a = TransferId::random();
        let b = TransferId::random();
        assert_eq!(a.as_str().len(), 32);
        assert!(a
            .as_str()
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        assert_ne!(a, b);
        assert_eq!(TransferId::parse(a.as_str()).expect("round trip"), a);
    }

    #[test]
    fn parse_rejects_anything_but_32_lowercase_hex() {
        assert!(TransferId::parse(ID).is_ok());
        for bad in [
            "",
            "0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcdeg",
            "0123456789abcdef0123456789abcde ",
            // 32 bytes long but 16 characters.
            "éééééééééééééééé",
        ] {
            assert!(TransferId::parse(bad).is_err(), "input {bad:?}");
        }
    }

    #[test]
    fn chunk_count_rounds_up() {
        assert_eq!(chunk_count_for(0), 0);
        assert_eq!(chunk_count_for(1), 1);
        assert_eq!(chunk_count_for(CB), 1);
        assert_eq!(chunk_count_for(CB + 1), 2);
        assert_eq!(chunk_count_for(2 * CB), 2);
        assert_eq!(chunk_count_for(MAX_TRANSFER_BYTES), 320);
        assert_eq!(chunk_count_for(MAX_TRANSFER_BYTES + 1), 321);
        assert_eq!(chunk_count_for(u64::MAX), u32::MAX);
    }

    #[test]
    fn direction_and_outcome_use_snake_case_names() {
        assert_eq!(
            serde_json::to_value(Direction::Send).expect("json"),
            json!("send")
        );
        assert_eq!(
            serde_json::to_value(Direction::Receive).expect("json"),
            json!("receive")
        );
        for (outcome, name) in [
            (Outcome::Completed, "completed"),
            (Outcome::Unconfirmed, "unconfirmed"),
            (Outcome::Declined, "declined"),
            (Outcome::Expired, "expired"),
            (Outcome::Cancelled, "cancelled"),
            (Outcome::Failed, "failed"),
            (Outcome::TooBig, "too_big"),
            (Outcome::Busy, "busy"),
            (Outcome::Damaged, "damaged"),
            (Outcome::Rejected, "rejected"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn abort_reasons_map_to_outcomes() {
        assert_eq!(Outcome::from(AbortReason::Cancelled), Outcome::Cancelled);
        assert_eq!(Outcome::from(AbortReason::Failed), Outcome::Failed);
        assert_eq!(Outcome::from(AbortReason::TooBig), Outcome::TooBig);
        assert_eq!(Outcome::from(AbortReason::Busy), Outcome::Busy);
        assert_eq!(Outcome::from(AbortReason::Expired), Outcome::Expired);
    }

    #[test]
    fn result_outcomes_map_to_outcomes() {
        assert_eq!(Outcome::from(ResultOutcome::Completed), Outcome::Completed);
        assert_eq!(Outcome::from(ResultOutcome::Damaged), Outcome::Damaged);
        assert_eq!(Outcome::from(ResultOutcome::Rejected), Outcome::Rejected);
        assert_eq!(Outcome::from(ResultOutcome::Failed), Outcome::Failed);
    }

    // ---- offer validation (receiver) ----

    #[test]
    fn an_offer_of_size_zero_is_refused() {
        let err = IncomingTransfer::from_offer(ID, "Orders", 0, 0, SHA, Instant::now())
            .expect_err("empty");
        assert_eq!(err, OfferError::Empty);
        assert_eq!(err.abort_reason(), AbortReason::Failed);
    }

    #[test]
    fn the_size_limit_is_exact() {
        let now = Instant::now();
        assert!(IncomingTransfer::from_offer(
            ID,
            "Orders",
            MAX_TRANSFER_BYTES,
            chunk_count_for(MAX_TRANSFER_BYTES),
            SHA,
            now
        )
        .is_ok());
        let err = IncomingTransfer::from_offer(
            ID,
            "Orders",
            MAX_TRANSFER_BYTES + 1,
            chunk_count_for(MAX_TRANSFER_BYTES + 1),
            SHA,
            now,
        )
        .expect_err("one byte over");
        assert_eq!(err, OfferError::TooBig);
        assert_eq!(err.abort_reason(), AbortReason::TooBig);
        let err = IncomingTransfer::from_offer(ID, "Orders", u64::MAX, u32::MAX, SHA, now)
            .expect_err("huge");
        assert_eq!(err, OfferError::TooBig);
    }

    #[test]
    fn a_chunk_count_that_does_not_match_the_size_is_refused() {
        let now = Instant::now();
        for (size, count) in [(CB + 1, 1), (CB + 1, 3), (CB, 2), (10, 0), (10, 2)] {
            let err = IncomingTransfer::from_offer(ID, "Orders", size, count, SHA, now)
                .expect_err("lying count");
            assert_eq!(
                err,
                OfferError::ChunkCountMismatch,
                "size {size} count {count}"
            );
            assert_eq!(err.abort_reason(), AbortReason::Failed);
        }
    }

    #[test]
    fn an_offer_with_a_bad_id_is_refused() {
        for bad in ["", "short", "0123456789ABCDEF0123456789ABCDEF"] {
            let err = IncomingTransfer::from_offer(bad, "Orders", 10, 1, SHA, Instant::now())
                .expect_err("bad id");
            assert_eq!(err, OfferError::BadId, "input {bad:?}");
        }
    }

    #[test]
    fn an_offer_with_a_bad_hash_or_name_is_refused() {
        let now = Instant::now();
        for bad in [&SHA[..63], &SHA.to_uppercase(), "", "zz"] {
            let err =
                IncomingTransfer::from_offer(ID, "Orders", 10, 1, bad, now).expect_err("bad hash");
            assert_eq!(err, OfferError::BadHash, "input {bad:?}");
        }
        let long = "n".repeat(201);
        for bad in [
            "",
            "   ",
            "bad\u{1b}name",
            "line\nbreak",
            "evil\u{202e}gpj.exe",
            "zero\u{200b}width",
            "\u{feff}Orders",
            long.as_str(),
        ] {
            let err = IncomingTransfer::from_offer(ID, bad, 10, 1, SHA, now).expect_err("bad name");
            assert_eq!(err, OfferError::BadName, "input {bad:?}");
        }
        let longest = "n".repeat(200);
        assert!(IncomingTransfer::from_offer(ID, &longest, 10, 1, SHA, now).is_ok());
    }

    #[test]
    fn offer_name_is_valid_follows_the_spec_rule() {
        for good in [
            "Orders",
            "Orders: v2",
            "Café API",
            "NUL",
            "注文",
            &"n".repeat(200),
        ] {
            assert!(offer_name_is_valid(good), "input {good:?}");
        }
        // 200 multi-byte characters still pass: the limit counts characters, not bytes.
        assert!(offer_name_is_valid(&"é".repeat(200)));
        assert!(!offer_name_is_valid(&"n".repeat(201)));
        assert!(!offer_name_is_valid(""));
        assert!(!offer_name_is_valid(" \t "));
        for bad in [
            '\u{0}', '\u{7}', '\u{7f}', '\u{85}', '\u{ad}', '\u{61c}', '\u{180e}', '\u{200b}',
            '\u{200d}', '\u{200f}', '\u{202a}', '\u{202e}', '\u{2060}', '\u{2064}', '\u{2066}',
            '\u{2069}', '\u{feff}',
        ] {
            let name = format!("Ord{bad}ers");
            assert!(!offer_name_is_valid(&name), "char U+{:04X}", bad as u32);
        }
        // The sender refuses the same names.
        assert_eq!(
            OutgoingTransfer::new(id(), "evil\u{202e}txt", 10, SHA, Instant::now())
                .expect_err("bidi"),
            OfferError::BadName
        );
    }

    #[test]
    fn the_sender_refuses_the_same_offers() {
        let now = Instant::now();
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", 0, SHA, now).expect_err("empty"),
            OfferError::Empty
        );
        assert!(OutgoingTransfer::new(id(), "Orders", MAX_TRANSFER_BYTES, SHA, now).is_ok());
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", MAX_TRANSFER_BYTES + 1, SHA, now)
                .expect_err("one byte over"),
            OfferError::TooBig
        );
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", 10, "nothex", now).expect_err("hash"),
            OfferError::BadHash
        );
    }

    // ---- sender ----

    #[test]
    fn the_sender_builds_the_offer_frame() {
        let t = outgoing(2 * CB + 10, Instant::now());
        assert_eq!(
            t.offer_message(),
            PeerMessage::CollectionOffer {
                transfer_id: ID.into(),
                name: "Orders".into(),
                size_bytes: 2 * CB + 10,
                chunk_count: 3,
                sha256: SHA.into(),
            }
        );
        assert_eq!(t.state(), OutgoingState::Offered);
        assert_eq!(t.outcome(), None);
        assert_eq!(t.total_bytes(), 2 * CB + 10);
    }

    #[test]
    fn the_sender_streams_every_chunk_after_an_accept() {
        let now = Instant::now();
        let mut t = outgoing(2 * CB + 10, now);
        assert_eq!(t.on_answer(true, now), None);
        assert_eq!(t.state(), OutgoingState::Accepted);

        let first = t.next_chunk(&vec![1u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(
            first,
            PeerMessage::Chunk {
                transfer_id: ID.into(),
                seq: 0,
                data: STANDARD.encode(vec![1u8; CHUNK_BYTES]),
            }
        );
        assert_eq!(t.state(), OutgoingState::Sending);
        assert_eq!(t.done_bytes(), CB);

        t.next_chunk(&vec![2u8; CHUNK_BYTES]).expect("chunk 1");
        assert_eq!(t.state(), OutgoingState::Sending);
        let last = t.next_chunk(&[3u8; 10]).expect("chunk 2");
        assert!(matches!(last, PeerMessage::Chunk { seq: 2, .. }));
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
        assert_eq!(t.done_bytes(), 2 * CB + 10);
        assert!(!t.is_active());
    }

    #[test]
    fn the_sender_refuses_chunks_it_may_not_send() {
        let now = Instant::now();
        let mut t = outgoing(CB + 10, now);
        assert!(
            t.next_chunk(&vec![0u8; CHUNK_BYTES]).is_err(),
            "before the accept"
        );
        t.on_answer(true, now);
        assert!(t.next_chunk(&[0u8; 5]).is_err(), "a short non-last chunk");
        assert!(
            t.next_chunk(&vec![0u8; CHUNK_BYTES + 1]).is_err(),
            "too long"
        );
        assert_eq!(t.done_bytes(), 0, "a refused chunk changes nothing");
        t.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert!(t.next_chunk(&[0u8; 11]).is_err(), "beyond the size");
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert!(t.next_chunk(&[0u8; 1]).is_err(), "after the last chunk");
    }

    #[test]
    fn a_decline_ends_the_sender_transfer() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        assert_eq!(t.on_answer(false, now), Some(Outcome::Declined));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Declined));
        assert!(t.next_chunk(&[0u8; 10]).is_err());
    }

    #[test]
    fn an_offer_expires_exactly_at_the_ttl() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert_eq!(t.state(), OutgoingState::Offered);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.state(), OutgoingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 2), "only once");
    }

    #[test]
    fn a_late_answer_finds_the_offer_expired() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert_eq!(
            t.on_answer(true, t0 + TRANSFER_OFFER_TTL),
            Some(Outcome::Expired)
        );
        assert_eq!(t.state(), OutgoingState::Expired);

        let mut early = outgoing(10, t0);
        assert_eq!(early.on_answer(true, t0 + TRANSFER_OFFER_TTL - ms(1)), None);
        assert_eq!(early.state(), OutgoingState::Accepted);
    }

    #[test]
    fn an_accepted_transfer_does_not_expire() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        t.on_answer(true, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 10));
        assert_eq!(t.state(), OutgoingState::Accepted);
    }

    #[test]
    fn a_second_answer_is_ignored() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        assert_eq!(t.on_answer(false, now), None);
        assert_eq!(t.state(), OutgoingState::Accepted);
    }

    #[test]
    fn a_peer_abort_ends_an_active_sender_transfer_once() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.on_abort(AbortReason::Busy), Some(Outcome::Busy));
        assert_eq!(offered.on_abort(AbortReason::Failed), None, "already final");
        assert_eq!(offered.outcome(), Some(Outcome::Busy));

        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending
            .next_chunk(&vec![0u8; CHUNK_BYTES])
            .expect("chunk 0");
        assert_eq!(sending.on_abort(AbortReason::TooBig), Some(Outcome::TooBig));

        let mut done = outgoing(10, now);
        done.on_answer(true, now);
        done.next_chunk(&[0u8; 10]).expect("only chunk");
        assert_eq!(
            done.on_abort(AbortReason::Failed),
            None,
            "a finished send stays done"
        );
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn fail_with_ends_an_offered_or_sending_transfer() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(offered.state(), OutgoingState::Aborted);
        assert_eq!(offered.outcome(), Some(Outcome::Failed));
        assert_eq!(offered.fail_with(Outcome::Failed), None, "idempotent");

        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending
            .next_chunk(&vec![0u8; CHUNK_BYTES])
            .expect("chunk 0");
        assert_eq!(sending.state(), OutgoingState::Sending);
        assert_eq!(
            sending.fail_with(Outcome::Cancelled),
            Some(Outcome::Cancelled)
        );
        assert_eq!(sending.state(), OutgoingState::Aborted);
        assert_eq!(sending.outcome(), Some(Outcome::Cancelled));
        assert_eq!(sending.fail_with(Outcome::Failed), None, "already final");
        assert_eq!(sending.outcome(), Some(Outcome::Cancelled));
    }

    #[test]
    fn the_service_can_fail_a_sender_transfer_after_the_last_chunk() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.cancel(), None, "cancel is too late after Done");
        assert_eq!(t.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(
            t.fail_with(Outcome::Failed),
            None,
            "a second call is a no-op"
        );
        assert_eq!(t.fail_with(Outcome::Completed), None, "not a failure");
        assert_eq!(t.outcome(), Some(Outcome::Failed));
    }

    #[test]
    fn fail_with_completed_is_refused_and_changes_nothing() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.fail_with(Outcome::Completed), None);
        assert_eq!(offered.state(), OutgoingState::Offered);
        assert_eq!(offered.outcome(), None);

        let mut done = outgoing(10, now);
        done.on_answer(true, now);
        done.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(done.fail_with(Outcome::Completed), None);
        assert_eq!(done.state(), OutgoingState::Done);
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn fail_with_after_expiry_returns_none() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.fail_with(Outcome::Failed), None);
        assert_eq!(t.state(), OutgoingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn a_short_sender_offer_ttl_expires_the_offer_at_exactly_that_ttl() {
        let t0 = Instant::now();
        let one = Duration::from_secs(1);
        let mut t = outgoing(10, t0).with_offer_ttl(one);
        assert!(!t.expire_if_due(t0 + one - ms(1)));
        assert_eq!(t.state(), OutgoingState::Offered);
        assert!(t.expire_if_due(t0 + one));
        assert_eq!(t.state(), OutgoingState::Expired);

        let mut late = outgoing(10, t0).with_offer_ttl(one);
        assert_eq!(late.on_answer(true, t0 + one), Some(Outcome::Expired));
    }

    #[test]
    fn the_default_sender_offer_ttl_is_the_constant() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(
            !t.expire_if_due(t0 + Duration::from_secs(1)),
            "not the short test ttl"
        );
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
    }

    #[test]
    fn a_result_while_done_ends_the_sender_transfer_with_the_real_outcome() {
        for (result, outcome, state) in [
            (
                ResultOutcome::Completed,
                Outcome::Completed,
                OutgoingState::Done,
            ),
            (
                ResultOutcome::Damaged,
                Outcome::Damaged,
                OutgoingState::Aborted,
            ),
            (
                ResultOutcome::Rejected,
                Outcome::Rejected,
                OutgoingState::Aborted,
            ),
            (
                ResultOutcome::Failed,
                Outcome::Failed,
                OutgoingState::Aborted,
            ),
        ] {
            let mut t = waiting_for_result(Instant::now());
            assert!(t.awaiting_result(), "result {result:?}");
            assert_eq!(t.on_result(result), Some(outcome), "result {result:?}");
            assert_eq!(t.state(), state, "result {result:?}");
            assert_eq!(t.outcome(), Some(outcome), "result {result:?}");
            assert!(!t.awaiting_result(), "result {result:?}");
        }
    }

    #[test]
    fn a_second_result_is_ignored() {
        let mut confirmed = waiting_for_result(Instant::now());
        assert_eq!(
            confirmed.on_result(ResultOutcome::Completed),
            Some(Outcome::Completed)
        );
        assert_eq!(confirmed.on_result(ResultOutcome::Failed), None);
        assert_eq!(confirmed.state(), OutgoingState::Done);
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));

        let mut damaged = waiting_for_result(Instant::now());
        assert_eq!(
            damaged.on_result(ResultOutcome::Damaged),
            Some(Outcome::Damaged)
        );
        assert_eq!(damaged.on_result(ResultOutcome::Completed), None);
        assert_eq!(damaged.outcome(), Some(Outcome::Damaged));
    }

    #[test]
    fn a_result_before_the_last_chunk_is_ignored() {
        let now = Instant::now();
        let mut t = outgoing(CB + 1, now);
        assert!(!t.awaiting_result());
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "offered");
        t.on_answer(true, now);
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "accepted");
        t.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "sending");
        assert_eq!(t.state(), OutgoingState::Sending);
        assert_eq!(t.outcome(), None);
        // The transfer still finishes normally and then waits for a real result.
        t.next_chunk(&[0u8; 1]).expect("last chunk");
        assert!(!t.awaiting_result(), "the final ack is still missing");
        assert_eq!(t.on_ack(2), None);
        assert!(t.awaiting_result());
        assert_eq!(
            t.on_result(ResultOutcome::Completed),
            Some(Outcome::Completed)
        );
    }

    #[test]
    fn a_result_after_abort_or_expiry_is_ignored() {
        let t0 = Instant::now();
        let mut cancelled = outgoing(10, t0);
        cancelled.cancel();
        assert_eq!(cancelled.on_result(ResultOutcome::Completed), None);
        assert_eq!(cancelled.outcome(), Some(Outcome::Cancelled));

        let mut expired = outgoing(10, t0);
        expired.expire_if_due(t0 + TRANSFER_OFFER_TTL);
        assert_eq!(expired.on_result(ResultOutcome::Completed), None);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));

        // The result timeout fired first: the late result changes nothing.
        let mut timed_out = waiting_for_result(t0);
        assert_eq!(timed_out.result_timed_out(), Some(Outcome::Unconfirmed));
        assert!(!timed_out.awaiting_result());
        assert_eq!(timed_out.on_result(ResultOutcome::Completed), None);
        assert_eq!(timed_out.state(), OutgoingState::Done);
        assert_eq!(timed_out.outcome(), Some(Outcome::Unconfirmed));

        // The service failed it after the last chunk (for example a failed send of that
        // chunk): the late result changes nothing either.
        let mut failed = waiting_for_result(t0);
        assert_eq!(failed.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(failed.on_result(ResultOutcome::Completed), None);
        assert_eq!(failed.state(), OutgoingState::Aborted);
    }

    #[test]
    fn a_confirmed_send_cannot_be_failed_later() {
        let mut t = waiting_for_result(Instant::now());
        t.on_result(ResultOutcome::Completed);
        assert_eq!(t.fail_with(Outcome::Failed), None);
        assert_eq!(t.connection_lost(), None);
        assert_eq!(t.cancel(), None);
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn the_sender_window_closes_after_32_unacked_chunks_and_an_ack_reopens_it() {
        let now = Instant::now();
        let chunk = vec![0u8; CHUNK_BYTES];
        let mut t = outgoing(40 * CB, now);
        assert!(!t.window_open(), "no window before the accept");
        t.on_answer(true, now);
        for n in 0..MAX_UNACKED_CHUNKS {
            assert!(t.window_open(), "chunk {n}");
            t.next_chunk(&chunk).expect("inside the window");
        }
        assert_eq!(t.unacked_chunks(), MAX_UNACKED_CHUNKS);
        assert!(!t.window_open());
        assert!(
            t.next_chunk(&chunk).is_err(),
            "the 33rd unacked chunk is refused"
        );
        assert_eq!(t.done_bytes(), 32 * CB, "a refused chunk changes nothing");

        assert_eq!(t.on_ack(ACK_EVERY_CHUNKS), None);
        assert_eq!(t.unacked_chunks(), 16);
        assert!(t.window_open());
        assert_eq!(
            t.on_ack(ACK_EVERY_CHUNKS),
            None,
            "a repeated ack is harmless"
        );
        for _ in 0..8 {
            t.next_chunk(&chunk).expect("reopened");
        }
        assert_eq!(t.state(), OutgoingState::Done);
        assert!(
            !t.awaiting_result(),
            "all chunks are built but the final ack is missing"
        );
        // The final ack arrives: only now the sender waits for the result.
        assert_eq!(t.on_ack(40), None);
        assert_eq!(t.unacked_chunks(), 0);
        assert!(t.awaiting_result());
    }

    #[test]
    fn a_bad_ack_fails_the_sender_and_a_late_one_is_ignored() {
        let now = Instant::now();
        let chunk = vec![0u8; CHUNK_BYTES];

        let mut early = outgoing(40 * CB, now);
        assert_eq!(
            early.on_ack(0),
            Some(Outcome::Failed),
            "an ack before the accept"
        );

        let mut too_far = outgoing(40 * CB, now);
        too_far.on_answer(true, now);
        for _ in 0..5 {
            too_far.next_chunk(&chunk).expect("chunk");
        }
        assert_eq!(
            too_far.on_ack(6),
            Some(Outcome::Failed),
            "past the chunks sent"
        );
        assert_eq!(too_far.state(), OutgoingState::Aborted);

        let mut backwards = outgoing(40 * CB, now);
        backwards.on_answer(true, now);
        for _ in 0..20 {
            backwards.next_chunk(&chunk).expect("chunk");
        }
        assert_eq!(backwards.on_ack(16), None);
        assert_eq!(backwards.on_ack(15), Some(Outcome::Failed), "backwards");
        assert_eq!(backwards.on_ack(16), None, "ignored once final");
        assert_eq!(backwards.outcome(), Some(Outcome::Failed));

        let mut confirmed = waiting_for_result(now);
        confirmed.on_result(ResultOutcome::Completed);
        assert_eq!(confirmed.on_ack(5), None, "ignored after the result");
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn losing_the_peer_after_the_final_ack_is_unconfirmed() {
        let mut t = waiting_for_result(Instant::now());
        assert_eq!(t.connection_lost(), Some(Outcome::Unconfirmed));
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
        assert!(!t.awaiting_result());
        assert_eq!(t.connection_lost(), None, "only once");
        assert_eq!(t.result_timed_out(), None);
        assert_eq!(t.fail_with(Outcome::Failed), None, "unconfirmed is final");
        assert_eq!(t.on_result(ResultOutcome::Completed), None);
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
    }

    #[test]
    fn losing_the_peer_after_the_last_chunk_but_before_the_final_ack_is_failed() {
        let mut t = last_chunk_sent(Instant::now());
        assert_eq!(t.connection_lost(), Some(Outcome::Failed));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert!(!t.awaiting_result());
        assert_eq!(t.connection_lost(), None, "only once");
        assert_eq!(t.on_ack(1), None, "a late final ack changes nothing");
        assert_eq!(t.on_result(ResultOutcome::Completed), None);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
    }

    #[test]
    fn the_result_timeout_after_the_final_ack_is_unconfirmed() {
        let now = Instant::now();
        let mut t = waiting_for_result(now);
        assert_eq!(t.result_timed_out(), Some(Outcome::Unconfirmed));
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
        assert_eq!(t.result_timed_out(), None, "only once");

        // Before the last chunk there is no result to wait for.
        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending
            .next_chunk(&vec![0u8; CHUNK_BYTES])
            .expect("chunk 0");
        assert_eq!(sending.result_timed_out(), None);
        assert_eq!(sending.state(), OutgoingState::Sending);

        // Between the last chunk and the final ack the result timeout is ignored: it cannot
        // turn a send the receiver never acknowledged into `Unconfirmed`.
        let mut unacked = last_chunk_sent(now);
        assert_eq!(unacked.result_timed_out(), None);
        assert_eq!(unacked.state(), OutgoingState::Done);
        assert_eq!(
            unacked.outcome(),
            Some(Outcome::Completed),
            "unchanged by the timeout"
        );

        // A confirmed send cannot time out.
        let mut confirmed = waiting_for_result(now);
        confirmed.on_result(ResultOutcome::Completed);
        assert_eq!(confirmed.result_timed_out(), None);
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn a_result_that_overtakes_the_final_ack_is_accepted() {
        let mut t = last_chunk_sent(Instant::now());
        assert_eq!(
            t.on_result(ResultOutcome::Completed),
            Some(Outcome::Completed)
        );
        assert_eq!(t.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn cancel_and_connection_loss_end_the_sender_transfer() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        assert_eq!(
            t.cancel(),
            Some(PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Cancelled,
            })
        );
        assert_eq!(t.outcome(), Some(Outcome::Cancelled));
        assert_eq!(t.cancel(), None, "only once");

        let mut lost = outgoing(10, now);
        assert_eq!(
            lost.connection_lost(),
            Some(Outcome::Failed),
            "lost before the last chunk"
        );
        assert_eq!(lost.connection_lost(), None);
    }

    // ---- receiver ----

    #[test]
    fn the_receiver_starts_offered_and_pending() {
        let t = incoming(2 * CB + 10, Instant::now());
        assert_eq!(t.state(), IncomingState::Offered);
        assert_eq!(t.transfer_id().as_str(), ID);
        assert_eq!(t.name(), "Orders");
        assert_eq!(t.sha256(), SHA);
        assert_eq!(t.total_bytes(), 2 * CB + 10);
        assert_eq!(t.done_bytes(), 0);
        assert!(t.is_pending());
        assert!(t.is_active());
        assert_eq!(t.outcome(), None);
    }

    #[test]
    fn accept_and_decline_produce_the_answer_frames() {
        let now = Instant::now();
        let mut yes = incoming(10, now);
        assert_eq!(
            yes.respond(true, now),
            Some(PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            })
        );
        assert_eq!(yes.state(), IncomingState::Accepted);
        assert!(!yes.is_pending());
        assert!(yes.is_active());
        assert_eq!(yes.respond(false, now), None, "answered once");
        assert_eq!(yes.state(), IncomingState::Accepted);

        let mut no = incoming(10, now);
        assert_eq!(
            no.respond(false, now),
            Some(PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: false,
            })
        );
        assert_eq!(no.state(), IncomingState::Aborted);
        assert_eq!(no.outcome(), Some(Outcome::Declined));
        assert!(!no.is_active());
    }

    #[test]
    fn the_receiver_offer_expires_exactly_at_the_ttl() {
        let t0 = Instant::now();
        let mut t = incoming(10, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert_eq!(t.state(), IncomingState::Offered);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.state(), IncomingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 2));
    }

    #[test]
    fn answering_at_the_ttl_is_too_late_and_one_millisecond_before_is_not() {
        let t0 = Instant::now();
        let mut late = incoming(10, t0);
        assert_eq!(late.respond(true, t0 + TRANSFER_OFFER_TTL), None);
        assert_eq!(late.state(), IncomingState::Expired);
        assert_eq!(late.outcome(), Some(Outcome::Expired));

        let mut in_time = incoming(10, t0);
        assert!(in_time
            .respond(true, t0 + TRANSFER_OFFER_TTL - ms(1))
            .is_some());
        assert_eq!(in_time.state(), IncomingState::Accepted);
        assert!(
            !in_time.expire_if_due(t0 + TRANSFER_OFFER_TTL * 10),
            "no timer after accept"
        );
    }

    #[test]
    fn a_chunk_before_the_accept_aborts_as_failed() {
        let now = Instant::now();
        let mut t = incoming(10, now);
        assert_eq!(t.on_chunk(0, &b64(10)), ChunkStep::Failed);
        assert_eq!(t.state(), IncomingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(t.done_bytes(), 0);
    }

    #[test]
    fn chunks_out_of_order_or_repeated_abort_as_failed() {
        let now = Instant::now();
        let mut skip = accepted(2 * CB, now);
        assert_eq!(skip.on_chunk(1, &b64(CHUNK_BYTES)), ChunkStep::Failed);
        assert_eq!(skip.outcome(), Some(Outcome::Failed));

        let mut repeat = accepted(2 * CB, now);
        assert!(matches!(
            repeat.on_chunk(0, &b64(CHUNK_BYTES)),
            ChunkStep::Data { .. }
        ));
        assert_eq!(repeat.on_chunk(0, &b64(CHUNK_BYTES)), ChunkStep::Failed);
        assert_eq!(repeat.state(), IncomingState::Aborted);
    }

    #[test]
    fn a_seq_at_or_past_the_chunk_count_aborts_as_failed() {
        let now = Instant::now();
        for seq in [2, 3, u32::MAX] {
            let mut t = accepted(2 * CB, now);
            assert_eq!(
                t.on_chunk(seq, &b64(CHUNK_BYTES)),
                ChunkStep::Failed,
                "seq {seq}"
            );
        }
        // A surplus chunk after the last one is a frame for a finished transfer.
        let mut t = accepted(CB, now);
        assert!(matches!(
            t.on_chunk(0, &b64(CHUNK_BYTES)),
            ChunkStep::Data { last: true, .. }
        ));
        assert_eq!(t.on_chunk(1, &b64(CHUNK_BYTES)), ChunkStep::Ignored);
        assert_eq!(
            t.outcome(),
            Some(Outcome::Completed),
            "the surplus chunk changes nothing"
        );
    }

    #[test]
    fn data_beyond_the_size_or_a_short_chunk_aborts_as_failed() {
        let now = Instant::now();
        // The size is 10 and the chunk carries 11.
        let mut over = accepted(10, now);
        assert_eq!(over.on_chunk(0, &b64(11)), ChunkStep::Failed);

        // The last chunk carries one byte more than the remainder.
        let mut last = accepted(CB + 10, now);
        assert!(matches!(
            last.on_chunk(0, &b64(CHUNK_BYTES)),
            ChunkStep::Data { .. }
        ));
        assert_eq!(last.on_chunk(1, &b64(11)), ChunkStep::Failed);

        // The last chunk is one byte short.
        let mut short_last = accepted(CB + 10, now);
        short_last.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(short_last.on_chunk(1, &b64(9)), ChunkStep::Failed);

        // A non-last chunk is short.
        let mut short = accepted(2 * CB, now);
        assert_eq!(short.on_chunk(0, &b64(CHUNK_BYTES - 1)), ChunkStep::Failed);

        // An empty chunk.
        let mut empty = accepted(10, now);
        assert_eq!(empty.on_chunk(0, ""), ChunkStep::Failed);
    }

    #[test]
    fn bad_base64_aborts_as_failed() {
        let now = Instant::now();
        for bad in ["not base64!", "AAE", "AAEC====", "AA EC"] {
            let mut t = accepted(3, now);
            assert_eq!(t.on_chunk(0, bad), ChunkStep::Failed, "input {bad:?}");
        }
        let mut ok = accepted(3, now);
        assert!(matches!(
            ok.on_chunk(0, "AAEC"),
            ChunkStep::Data { last: true, .. }
        ));
    }

    #[test]
    fn an_oversize_data_string_aborts_without_decoding() {
        let now = Instant::now();
        let mut t = accepted(CB, now);
        let huge = "A".repeat(MAX_CHUNK_B64_CHARS + 4);
        assert_eq!(t.on_chunk(0, &huge), ChunkStep::Failed);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(MAX_CHUNK_B64_CHARS, 43_692);
    }

    #[test]
    fn the_last_chunk_completes_the_receive() {
        let now = Instant::now();
        let mut t = accepted(2 * CB + 10, now);
        let first = t.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(
            first,
            ChunkStep::Data {
                bytes: vec![7u8; CHUNK_BYTES],
                done_bytes: CB,
                last: false,
                ack: None,
            }
        );
        assert_eq!(t.state(), IncomingState::Receiving);
        let second = t.on_chunk(1, &b64(CHUNK_BYTES));
        assert!(
            matches!(second, ChunkStep::Data { done_bytes, last: false, .. } if done_bytes == 2 * CB)
        );
        let third = t.on_chunk(2, &b64(10));
        assert_eq!(
            third,
            ChunkStep::Data {
                bytes: vec![7u8; 10],
                done_bytes: 2 * CB + 10,
                last: true,
                ack: Some(PeerMessage::ChunkAck {
                    transfer_id: ID.into(),
                    next_seq: 3,
                }),
            }
        );
        assert_eq!(t.state(), IncomingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
        assert_eq!(t.done_bytes(), t.total_bytes());
        assert!(!t.is_active());
    }

    #[test]
    fn the_receiver_acks_every_16th_chunk_and_the_last() {
        let now = Instant::now();
        // 34 chunks: acks after chunk 16, chunk 32 and the last one.
        let mut t = accepted(33 * CB + 10, now);
        let mut acks = Vec::new();
        for seq in 0..34u32 {
            let data = if seq == 33 { b64(10) } else { b64(CHUNK_BYTES) };
            match t.on_chunk(seq, &data) {
                ChunkStep::Data {
                    ack: Some(frame), ..
                } => acks.push((seq, frame)),
                ChunkStep::Data { ack: None, .. } => {}
                other => panic!("seq {seq}: {other:?}"),
            }
        }
        let ack = |next_seq| PeerMessage::ChunkAck {
            transfer_id: ID.into(),
            next_seq,
        };
        assert_eq!(
            acks,
            vec![
                (ACK_EVERY_CHUNKS - 1, ack(ACK_EVERY_CHUNKS)),
                (2 * ACK_EVERY_CHUNKS - 1, ack(2 * ACK_EVERY_CHUNKS)),
                (33, ack(34)),
            ]
        );
        // A single-chunk transfer acks its only (last) chunk.
        let mut one = accepted(10, now);
        assert!(matches!(
            one.on_chunk(0, &b64(10)),
            ChunkStep::Data {
                ack: Some(PeerMessage::ChunkAck { next_seq: 1, .. }),
                last: true,
                ..
            }
        ));
    }

    #[test]
    fn frames_after_a_final_state_are_ignored() {
        let now = Instant::now();
        let t0 = now;

        let mut declined = incoming(10, now);
        declined.respond(false, now);
        assert_eq!(declined.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(declined.outcome(), Some(Outcome::Declined));

        let mut expired = incoming(10, t0);
        expired.expire_if_due(t0 + TRANSFER_OFFER_TTL);
        assert_eq!(expired.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));

        let mut aborted = accepted(10, now);
        aborted.on_abort(AbortReason::Cancelled);
        assert_eq!(aborted.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(aborted.on_abort(AbortReason::Failed), None);
        assert_eq!(aborted.outcome(), Some(Outcome::Cancelled));

        let mut failed = accepted(10, now);
        assert_eq!(failed.on_chunk(5, &b64(10)), ChunkStep::Failed);
        assert_eq!(
            failed.on_chunk(0, &b64(10)),
            ChunkStep::Ignored,
            "a failed transfer stays failed"
        );
    }

    #[test]
    fn a_peer_abort_maps_to_an_outcome() {
        let now = Instant::now();
        let mut offered = incoming(10, now);
        assert_eq!(
            offered.on_abort(AbortReason::Cancelled),
            Some(Outcome::Cancelled)
        );
        let mut receiving = accepted(CB + 1, now);
        receiving.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(
            receiving.on_abort(AbortReason::Failed),
            Some(Outcome::Failed)
        );
        let mut done = accepted(10, now);
        done.on_chunk(0, &b64(10));
        assert_eq!(
            done.on_abort(AbortReason::Failed),
            None,
            "all bytes already arrived"
        );
    }

    #[test]
    fn the_service_can_fail_a_finished_receive() {
        let now = Instant::now();
        let mut t = accepted(10, now);
        t.on_chunk(0, &b64(10));
        assert_eq!(t.fail_with(Outcome::Completed), None, "not a failure");
        assert_eq!(t.fail_with(Outcome::Damaged), Some(Outcome::Damaged));
        assert_eq!(t.state(), IncomingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Damaged));
        assert_eq!(t.fail_with(Outcome::Rejected), None, "already final");

        let mut expired = incoming(10, now);
        expired.expire_if_due(now + TRANSFER_OFFER_TTL);
        assert_eq!(expired.fail_with(Outcome::Failed), None);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn a_short_receiver_offer_ttl_expires_the_offer_at_exactly_that_ttl() {
        let t0 = Instant::now();
        let one = Duration::from_secs(1);
        let mut t = incoming(10, t0).with_offer_ttl(one);
        assert!(!t.expire_if_due(t0 + one - ms(1)));
        assert_eq!(t.state(), IncomingState::Offered);
        assert!(t.expire_if_due(t0 + one));
        assert_eq!(t.state(), IncomingState::Expired);

        let mut late = incoming(10, t0).with_offer_ttl(one);
        assert_eq!(late.respond(true, t0 + one), None);
        assert_eq!(late.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn the_default_receiver_offer_ttl_is_the_constant() {
        let t0 = Instant::now();
        let mut t = incoming(10, t0);
        assert!(
            !t.expire_if_due(t0 + Duration::from_secs(1)),
            "not the short test ttl"
        );
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
    }

    #[test]
    fn connection_loss_fails_an_active_receive_only() {
        let now = Instant::now();
        let mut receiving = accepted(CB + 1, now);
        receiving.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(receiving.connection_lost(), Some(Outcome::Failed));
        assert_eq!(receiving.connection_lost(), None);

        let mut done = accepted(10, now);
        done.on_chunk(0, &b64(10));
        assert_eq!(
            done.connection_lost(),
            None,
            "the import does not depend on the socket"
        );
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn the_abort_frame_names_the_transfer() {
        let t = incoming(10, Instant::now());
        assert_eq!(
            t.abort_message(AbortReason::Busy),
            PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Busy,
            }
        );
    }

    // ---- limits ----

    #[test]
    fn a_duplicate_offer_id_is_ignored_and_costs_no_rate_hit() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let first = TransferId::random();
        assert_eq!(limits.admit_offer(&peer(1), &first, t0), Admission::Admit);
        for _ in 0..20 {
            assert_eq!(
                limits.admit_offer(&peer(1), &first, t0),
                Admission::Duplicate
            );
        }
        // The same id from another peer is still a duplicate.
        assert_eq!(
            limits.admit_offer(&peer(2), &first, t0),
            Admission::Duplicate
        );
        // The duplicates did not use the rate window: the id is released and 4 more fit.
        assert!(limits.release(&first));
        for _ in 0..(OFFERS_PER_PEER_PER_MINUTE - 1) {
            let next = TransferId::random();
            assert_eq!(limits.admit_offer(&peer(1), &next, t0), Admission::Admit);
            assert!(limits.release(&next));
        }
    }

    #[test]
    fn a_peer_gets_one_incoming_transfer_at_a_time() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let (a1, a2, b1) = (
            TransferId::random(),
            TransferId::random(),
            TransferId::random(),
        );
        assert_eq!(MAX_ACTIVE_PER_PEER, 1);
        assert_eq!(limits.admit_offer(&peer(1), &a1, t0), Admission::Admit);
        assert_eq!(
            limits.admit_offer(&peer(1), &a2, t0),
            Admission::Refuse(AbortReason::Busy)
        );
        assert_eq!(limits.admit_offer(&peer(2), &b1, t0), Admission::Admit);
        // An accepted transfer still occupies the peer.
        assert!(limits.mark_active(&a1));
        assert_eq!(
            limits.admit_offer(&peer(1), &a2, t0),
            Admission::Refuse(AbortReason::Busy)
        );
        // The refused id was not remembered.
        assert!(limits.release(&a1));
        assert_eq!(limits.admit_offer(&peer(1), &a2, t0), Admission::Admit);
    }

    #[test]
    fn the_fourth_pending_offer_across_peers_is_refused() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        assert_eq!(MAX_PENDING_INCOMING, 3);
        let ids: Vec<TransferId> = (0..4).map(|_| TransferId::random()).collect();
        for (n, id) in ids.iter().take(3).enumerate() {
            assert_eq!(
                limits.admit_offer(&peer(n as u8 + 1), id, t0),
                Admission::Admit
            );
        }
        assert_eq!(limits.pending_incoming(), 3);
        assert_eq!(
            limits.admit_offer(&peer(4), &ids[3], t0),
            Admission::Refuse(AbortReason::Busy)
        );
        // Accepting one frees a pending slot, because it is no longer waiting for the user.
        assert!(limits.mark_active(&ids[0]));
        assert_eq!(limits.pending_incoming(), 2);
        assert_eq!(limits.admit_offer(&peer(4), &ids[3], t0), Admission::Admit);
    }

    #[test]
    fn offers_per_peer_per_minute_are_limited_and_the_window_slides() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        for _ in 0..OFFERS_PER_PEER_PER_MINUTE {
            let id = TransferId::random();
            assert_eq!(limits.admit_offer(&peer(1), &id, t0), Admission::Admit);
            assert!(limits.release(&id));
        }
        let sixth = TransferId::random();
        assert_eq!(
            limits.admit_offer(&peer(1), &sixth, t0 + Duration::from_secs(59)),
            Admission::Refuse(AbortReason::Busy)
        );
        // Another peer has its own window.
        assert_eq!(
            limits.admit_offer(
                &peer(2),
                &TransferId::random(),
                t0 + Duration::from_secs(59)
            ),
            Admission::Admit
        );
        assert_eq!(
            limits.admit_offer(&peer(1), &sixth, t0 + Duration::from_secs(60)),
            Admission::Admit
        );
    }

    #[test]
    fn outgoing_transfers_are_limited_per_peer_and_do_not_block_incoming() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let (out1, out2, inc) = (
            TransferId::random(),
            TransferId::random(),
            TransferId::random(),
        );
        assert!(limits.begin_outgoing(&peer(1), &out1));
        assert!(!limits.begin_outgoing(&peer(1), &out2), "one send per peer");
        assert!(!limits.begin_outgoing(&peer(1), &out1), "a known id");
        assert!(limits.begin_outgoing(&peer(2), &out2));
        assert_eq!(limits.admit_offer(&peer(1), &inc, t0), Admission::Admit);
        assert_eq!(
            limits.pending_incoming(),
            1,
            "an outgoing transfer is never pending"
        );
        assert!(limits.release(&out1));
        assert!(limits.begin_outgoing(&peer(1), &TransferId::random()));
    }

    #[test]
    fn unknown_ids_are_not_found_in_the_limits() {
        let mut limits = TransferLimits::new();
        let unknown = TransferId::random();
        assert!(!limits.release(&unknown));
        assert!(!limits.mark_active(&unknown));
        assert_eq!(limits.pending_incoming(), 0);
    }
}
