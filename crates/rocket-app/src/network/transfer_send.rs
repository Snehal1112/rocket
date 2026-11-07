//! The sender side of a transfer, around one `OutgoingTransfer` per send.
//!
//! The machine decides what is allowed. The service exports and hashes the ZIP, sends the
//! frames the machine builds, runs the timers and reports the result. After the last chunk
//! the machine is `Done` and waits for the receiver's final `ChunkAck`, then for its
//! `TransferResult`; the entry stays until the result, an abort, a gone peer, the idle limit
//! (no final ack: `Failed`) or `Timing::result_timeout` (after the final ack: `Unconfirmed`).

use std::sync::Arc;
use std::time::Instant;

use rocket_audit::AuditEventKind;
use rocket_export::{ExportAction, ExportReport};
use rocket_network::limits::{CHUNK_BYTES, MAX_TRANSFER_BYTES};
use rocket_network::{
    offer_name_is_valid, AbortReason, DeviceId, Direction, OfferError, Outcome, OutgoingState,
    OutgoingTransfer, PeerMessage, ResultOutcome, TransferEvent, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::transfer::{outcome_name, TransferService};

const MIB: u64 = 1024 * 1024;

/// What `send` returns: the transfer id and one line per thing the export left out or blanked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendStarted {
    pub transfer_id: String,
    pub notes: Vec<String>,
}

/// One outgoing transfer.
pub(super) struct Out {
    pub device: DeviceId,
    pub peer_name: String,
    pub collection: String,
    pub zip: Vec<u8>,
    pub machine: OutgoingTransfer,
    /// The offer, the accept or the last good `ChunkAck`. The idle limit counts from here.
    pub last_ack: Instant,
    /// Wakes the stream when an ack opens the window. `notify_one` keeps a permit.
    pub ack_signal: Arc<Notify>,
    /// The highest `next_seq` the receiver acked. Only a higher one restarts the idle clock.
    pub acked: u32,
    /// True while the offer frame is being written. A peer-gone notice in that time only marks
    /// `lost`, so `send` can return an error without a `Finished` for an id nobody got.
    pub offer_pending: bool,
    pub lost: bool,
    /// The 60 s result timer runs only once the receiver's final `ChunkAck` arrived.
    pub result_timer_started: bool,
}

/// What the stream does next.
enum StreamStep {
    /// Send this frame; then `done` of `total` bytes are out, and `last` says it was the last.
    Send {
        device: DeviceId,
        frame: PeerMessage,
        done: u64,
        total: u64,
        last: bool,
    },
    /// The ack window is full. Wait for the signal, or give up at the deadline.
    Wait(Arc<Notify>, Instant),
}

/// The words for an offer the machine refused. `len` is the ZIP size, rounded up to one decimal.
pub(super) fn offer_error(error: OfferError, len: usize) -> DomainError {
    let text = match error {
        OfferError::Empty => "There is nothing to send in that collection.".to_string(),
        OfferError::TooBig => {
            let tenths = (len as u64 * 10).div_ceil(MIB);
            format!(
                "This collection is too big to send ({}.{} MiB, limit {}).",
                tenths / 10,
                tenths % 10,
                MAX_TRANSFER_BYTES / MIB
            )
        }
        OfferError::BadName => {
            "This collection name cannot be sent. Rename it and try again.".to_string()
        }
        OfferError::BadId | OfferError::BadHash | OfferError::ChunkCountMismatch => {
            return DomainError::Internal("The offer could not be built.".to_string());
        }
    };
    DomainError::InvalidInput(text)
}

/// One line per report item that was skipped or blanked, formatted `<feature>: <detail>` (the
/// action word when there is no detail). Kept and translated items are left out, and the same
/// line is listed once.
pub(super) fn export_notes(report: &ExportReport) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for note in &report.items {
        let word = match note.action {
            ExportAction::Skipped => "skipped",
            ExportAction::Blanked => "blanked",
            ExportAction::Preserved | ExportAction::Translated => continue,
        };
        let line = format!(
            "{}: {}",
            note.feature,
            note.detail.as_deref().unwrap_or(word)
        );
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

/// The outcome a peer abort gives once the machine ignores it. `Expired` and `Unconfirmed`
/// belong to the state machine, so `fail_with` never gets them (rule R4).
fn failure_outcome(reason: AbortReason) -> Outcome {
    match Outcome::from(reason) {
        Outcome::Expired | Outcome::Unconfirmed => Outcome::Failed,
        other => other,
    }
}

/// The text of `Finished.message` for a failed send. It never carries peer or importer text.
pub(super) const SEND_FAILED_MESSAGE: &str = "The transfer failed.";

fn already_running() -> DomainError {
    DomainError::InvalidInput("A transfer to that node is already running.".to_string())
}

fn not_paired() -> DomainError {
    DomainError::InvalidInput("That node is not paired.".to_string())
}

fn offline() -> DomainError {
    DomainError::InvalidInput("That node is offline.".to_string())
}

fn already_sent() -> DomainError {
    DomainError::InvalidInput("Already sent. Waiting for the result.".to_string())
}

impl TransferService {
    /// Builds the ZIP, offers it to `device` and returns the transfer id and the export notes.
    /// The stream starts when the peer accepts.
    pub async fn send(
        &self,
        device: &DeviceId,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<SendStarted> {
        self.ensure_worker();
        let collection = collection.trim().to_string();
        if collection.is_empty() {
            return Err(DomainError::InvalidInput(
                "Pick a collection to send.".to_string(),
            ));
        }
        // The same name rule as the receiver's, before any export work (ruling 13).
        if !offer_name_is_valid(&collection) {
            return Err(offer_error(OfferError::BadName, 0));
        }
        if self.deps.network.peer_label(device).is_none() {
            return Err(not_paired());
        }
        // Refuse an offline node before any export work (ruling 14).
        if !self.deps.network.is_online(device) {
            return Err(offline());
        }
        let id = self.claim_send(device)?;
        let sent = self
            .export_and_offer(device, &id, &collection, include_environments)
            .await;
        match sent {
            Ok(notes) => Ok(SendStarted {
                transfer_id: id.as_str().to_string(),
                notes,
            }),
            Err(e) => {
                // A failure after the entry exists already freed the slot. This is for the rest.
                self.lock().limits.release(&id);
                Err(e)
            }
        }
    }

    /// Claims the peer's one outgoing slot. A second send to the same peer is refused here.
    pub(super) fn claim_send(&self, device: &DeviceId) -> DomainResult<TransferId> {
        let id = TransferId::random();
        if self.lock().limits.begin_outgoing(device, &id) {
            Ok(id)
        } else {
            Err(already_running())
        }
    }

    /// Test hook for Plan 4: offers raw bytes (not an export) and returns the transfer id.
    /// It claims the peer's slot, offers and frees the slot again when the offer fails.
    #[cfg(test)]
    pub(crate) async fn offer_raw(
        &self,
        device: &DeviceId,
        name: &str,
        bytes: Vec<u8>,
    ) -> DomainResult<String> {
        self.ensure_worker();
        let id = self.claim_send(device)?;
        match self.offer_bytes(device, &id, name, bytes).await {
            Ok(()) => Ok(id.as_str().to_string()),
            Err(e) => {
                self.lock().limits.release(&id);
                Err(e)
            }
        }
    }

    async fn export_and_offer(
        &self,
        device: &DeviceId,
        id: &TransferId,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<Vec<String>> {
        let export = Arc::clone(&self.deps.export);
        let name = collection.to_string();
        let (zip, notes) =
            tokio::task::spawn_blocking(move || -> DomainResult<(Vec<u8>, Vec<String>)> {
                let out = export.export_bruno(&name, include_environments)?;
                if out.report.exported == 0 {
                    return Err(DomainError::InvalidInput(
                        "There is nothing to send in that collection.".to_string(),
                    ));
                }
                let notes = export_notes(&out.report);
                Ok((out.bytes, notes))
            })
            .await
            .map_err(|e| DomainError::Internal(format!("The export task failed: {e}")))??;
        self.offer_bytes(device, id, collection, zip).await?;
        Ok(notes)
    }

    /// Everything after the export: hash, build the machine, register, send the offer, start
    /// the offer timer. The caller has claimed the slot for `id` and frees it on an error.
    pub(super) async fn offer_bytes(
        &self,
        device: &DeviceId,
        id: &TransferId,
        collection: &str,
        zip: Vec<u8>,
    ) -> DomainResult<()> {
        let Some((peer_name, _short)) = self.deps.network.peer_label(device) else {
            return Err(not_paired());
        };
        let machine_id = id.clone();
        let name = collection.to_string();
        // Read the clock and the ttl here: the closure runs on a blocking thread.
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        let (zip, machine) = tokio::task::spawn_blocking(move || {
            let len = zip.len();
            let sha256 = format!("{:x}", Sha256::digest(&zip));
            OutgoingTransfer::new(machine_id, &name, len as u64, &sha256, now)
                .map(|machine| (zip, machine.with_offer_ttl(ttl)))
                .map_err(|e| offer_error(e, len))
        })
        .await
        .map_err(|e| DomainError::Internal(format!("The hash task failed: {e}")))??;

        let offer = machine.offer_message();
        self.lock().outgoing.insert(
            id.clone(),
            Out {
                device: *device,
                peer_name,
                collection: collection.to_string(),
                zip,
                machine,
                last_ack: now,
                ack_signal: Arc::new(Notify::new()),
                acked: 0,
                offer_pending: true,
                lost: false,
                result_timer_started: false,
            },
        );
        let sent = self.deps.network.send_to(device, offer).await;
        let lost = {
            let mut inner = self.lock();
            match inner.outgoing.get_mut(id) {
                Some(out) => {
                    out.offer_pending = false;
                    out.lost
                }
                None => false,
            }
        };
        if let Err(e) = sent {
            self.take_out(id);
            return Err(e);
        }
        if lost {
            // The peer went away while the offer was written: nothing is reported for this id.
            self.take_out(id);
            return Err(offline());
        }
        self.spawn_offer_timer(id.clone());
        Ok(())
    }

    /// Cancels a transfer this node runs. An unknown or finished id is not an error. After the
    /// last chunk a send can no longer be cancelled.
    pub async fn cancel(&self, transfer_id: &str) -> DomainResult<()> {
        let Ok(id) = TransferId::parse(transfer_id) else {
            return Ok(());
        };
        self.cancel_outgoing(&id).await?;
        Ok(())
    }

    // ----- frames from the peer -----

    /// The peer answered the offer. The machine decides: accepted, declined or too late.
    pub(super) fn on_answer(&self, from: &DeviceId, id: &TransferId, accept: bool) {
        enum Next {
            Ignore,
            Stream,
            End,
        }
        let now = Self::now();
        let next = {
            let mut inner = self.lock();
            match inner.outgoing.get_mut(id) {
                Some(out) if out.device == *from => {
                    let was_offered = out.machine.state() == OutgoingState::Offered;
                    if out.machine.on_answer(accept, now).is_some() {
                        Next::End
                    } else if was_offered && out.machine.state() == OutgoingState::Accepted {
                        // The idle limit for the first window counts from the accept.
                        out.last_ack = now;
                        Next::Stream
                    } else {
                        Next::Ignore
                    }
                }
                _ => Next::Ignore,
            }
        };
        match next {
            Next::Ignore => {}
            Next::Stream => self.spawn_stream(id.clone()),
            Next::End => {
                if let Some(out) = self.take_out(id) {
                    // Only a late accept needs a word to the peer. A decline came from it.
                    let expired = out.machine.outcome() == Some(Outcome::Expired);
                    let notice = (out.device, out.machine.abort_message(AbortReason::Expired));
                    self.finish_out(id, out);
                    if expired {
                        self.tell_later(notice.0, notice.1);
                    }
                }
            }
        }
    }

    /// The peer aborted one of our transfers. Returns false when the id is not ours.
    pub(super) fn abort_outgoing(
        &self,
        from: &DeviceId,
        id: &TransferId,
        reason: AbortReason,
    ) -> bool {
        let ended = {
            let mut inner = self.lock();
            let Some(out) = inner.outgoing.get_mut(id) else {
                return false;
            };
            if out.device != *from {
                return false;
            }
            // Once every chunk is out the receiver holds the whole collection, so any abort is
            // a failure, whatever its reason, and the egress is audited.
            if out.machine.last_chunk_built() {
                out.machine.fail_with(Outcome::Failed).is_some()
            } else {
                out.machine
                    .on_abort(reason)
                    .or_else(|| out.machine.fail_with(failure_outcome(reason)))
                    .is_some()
            }
        };
        if ended {
            if let Some(out) = self.take_out(id) {
                self.finish_out(id, out);
            }
        }
        true
    }

    /// The receiver's verdict after the import. The machine accepts it once, and only after the
    /// last chunk. A result from another device is ignored.
    pub(super) fn on_result(&self, from: &DeviceId, id: &TransferId, outcome: ResultOutcome) {
        let ended = {
            let mut inner = self.lock();
            match inner.outgoing.get_mut(id) {
                Some(out) if out.device == *from => out.machine.on_result(outcome).is_some(),
                _ => false,
            }
        };
        if ended {
            if let Some(out) = self.take_out(id) {
                self.finish_out(id, out);
            }
        }
    }

    /// The receiver confirmed `next_seq` chunks. The machine checks the number: a good ack
    /// restarts the idle clock and wakes the stream, a bad one ends the send as `Failed`.
    pub(super) fn on_ack(&self, from: &DeviceId, id: &TransferId, next_seq: u32) {
        let now = Self::now();
        let (bad, start_result_timer) = {
            let mut inner = self.lock();
            let Some(out) = inner.outgoing.get_mut(id).filter(|o| o.device == *from) else {
                return;
            };
            if out.machine.on_ack(next_seq).is_some() {
                (true, false)
            } else {
                if next_seq > out.acked {
                    out.acked = next_seq;
                    out.last_ack = now;
                    out.ack_signal.notify_one();
                }
                // Only the final ack starts the wait for the result (and so the only
                // `Unconfirmed` end). Before it a lost peer is `Failed`.
                let start = out.machine.awaiting_result() && !out.result_timer_started;
                out.result_timer_started |= start;
                (false, start)
            }
        };
        if start_result_timer {
            self.spawn_result_timer(id.clone());
        }
        if bad {
            if let Some(out) = self.take_out(id) {
                let device = out.device;
                let frame = out.machine.abort_message(AbortReason::Failed);
                self.finish_out(id, out);
                self.tell_later(device, frame);
            }
        }
    }

    /// The device's connection ended or its pairing was removed. Every send to it ends now:
    /// `Unconfirmed` only after the receiver's final `ChunkAck`, `Failed` before it (also after
    /// the last chunk was sent). Nothing is told to the gone peer.
    pub(super) fn outgoing_gone(&self, device: &DeviceId) {
        let gone: Vec<(TransferId, Out)> = {
            let mut inner = self.lock();
            let mut ids: Vec<TransferId> = Vec::new();
            for (id, out) in inner.outgoing.iter_mut() {
                if out.device != *device {
                    continue;
                }
                if out.offer_pending {
                    out.lost = true;
                } else {
                    ids.push(id.clone());
                }
            }
            ids.into_iter()
                .filter_map(|id| inner.take_out(&id).map(|out| (id, out)))
                .collect()
        };
        for (id, mut out) in gone {
            if out.machine.connection_lost().is_none() {
                out.machine.fail_with(Outcome::Failed);
            }
            self.finish_out(&id, out);
        }
    }

    // ----- helpers -----

    fn take_out(&self, id: &TransferId) -> Option<Out> {
        self.lock().take_out(id)
    }

    /// Removes the transfer only while `wanted` accepts it.
    fn take_out_if(&self, id: &TransferId, wanted: impl Fn(&Out) -> bool) -> Option<Out> {
        let mut inner = self.lock();
        let ready = inner.outgoing.get(id).is_some_and(wanted);
        if ready {
            inner.take_out(id)
        } else {
            None
        }
    }

    /// The one place a send ends. The caller has already removed the transfer, and the machine
    /// holds the outcome. One audit row is written when at least one chunk went out, whatever
    /// the outcome; a send that ended before the first chunk writes none.
    fn finish_out(&self, id: &TransferId, out: Out) {
        let outcome = out.machine.outcome().unwrap_or(Outcome::Failed);
        self.emit(TransferEvent::Finished {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Send,
            outcome,
            display_name: out.peer_name.clone(),
            collection_name: out.collection.clone(),
            message: (outcome == Outcome::Failed).then(|| SEND_FAILED_MESSAGE.to_string()),
        });
        if out.machine.done_bytes() > 0 {
            self.audit(AuditEventKind::NetworkCollectionSent {
                device_id: out.device.short(),
                collection: out.collection,
                outcome: outcome_name(outcome).to_string(),
            });
        }
    }

    /// Ends a send the service gave up on (a failed send of a frame).
    fn fail_out(&self, id: &TransferId, outcome: Outcome) {
        if let Some(mut out) = self.take_out(id) {
            out.machine.fail_with(outcome);
            self.finish_out(id, out);
        }
    }

    /// Cancels a send. `Ok(false)` means there was no such send. After the last chunk the
    /// cancel is refused, because the receiver may already be importing.
    async fn cancel_outgoing(&self, id: &TransferId) -> DomainResult<bool> {
        let taken = {
            let mut inner = self.lock();
            let waiting = match inner.outgoing.get(id) {
                None => return Ok(false),
                Some(out) => out.machine.last_chunk_built(),
            };
            if waiting {
                return Err(already_sent());
            }
            inner.take_out(id)
        };
        let Some(mut out) = taken else {
            return Ok(false);
        };
        let frame = out.machine.cancel();
        let device = out.device;
        self.finish_out(id, out);
        if let Some(frame) = frame {
            self.tell(&device, frame).await;
        }
        Ok(true)
    }

    // ----- timers and the stream -----

    fn spawn_offer_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let ttl = self.timing.offer_ttl;
        drop(tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            me.expire_out(&id);
        }));
    }

    /// Ends an unanswered offer when the machine says it is due.
    fn expire_out(&self, id: &TransferId) {
        let now = Self::now();
        let due = self
            .lock()
            .outgoing
            .get_mut(id)
            .is_some_and(|out| out.machine.expire_if_due(now));
        if !due {
            return;
        }
        if let Some(out) = self.take_out(id) {
            let device = out.device;
            let frame = out.machine.abort_message(AbortReason::Expired);
            self.finish_out(id, out);
            self.tell_later(device, frame);
        }
    }

    fn spawn_stream(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move { me.stream(&id).await }));
    }

    /// Gives up on a result that never comes. Started by the receiver's final `ChunkAck`, so
    /// the receiver has every byte and the send ends `Unconfirmed`.
    fn spawn_result_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let wait = self.timing.result_timeout;
        drop(tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            if let Some(mut out) = me.take_out_if(&id, |o| o.machine.awaiting_result()) {
                out.machine.result_timed_out();
                me.finish_out(&id, out);
            }
        }));
    }

    /// Guards the wait for the final `ChunkAck` after the last chunk. When no ack of any kind
    /// arrives for `timing.idle_limit` the send ends `Failed` (audited `failed`), never
    /// `Unconfirmed`. Stops as soon as the final ack arrived or the send ended.
    fn spawn_final_ack_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move {
            loop {
                let deadline = {
                    let inner = me.lock();
                    match inner.outgoing.get(&id) {
                        Some(o) if !o.machine.awaiting_result() => {
                            o.last_ack + me.timing.idle_limit
                        }
                        _ => return,
                    }
                };
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                if me.ack_overdue(&id) {
                    return;
                }
            }
        }));
    }

    /// Sends every chunk in order, at most `MAX_UNACKED_CHUNKS` ahead of the receiver's acks
    /// (ruling 19). Each send is bounded by the connection's own timeout.
    async fn stream(&self, id: &TransferId) {
        loop {
            #[cfg(test)]
            {
                let delay = *self.chunk_delay.lock().unwrap_or_else(|e| e.into_inner());
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
            #[cfg(test)]
            let tamper = *self.tamper.lock().unwrap_or_else(|e| e.into_inner());
            #[cfg(not(test))]
            let tamper: Option<u32> = None;
            let step = {
                let mut inner = self.lock();
                let Some(out) = inner.outgoing.get_mut(id) else {
                    return;
                };
                if !out.machine.window_open() && out.machine.is_active() {
                    StreamStep::Wait(
                        out.ack_signal.clone(),
                        out.last_ack + self.timing.idle_limit,
                    )
                } else {
                    let start = out.machine.done_bytes() as usize;
                    let end = (start + CHUNK_BYTES).min(out.zip.len());
                    // The tamper hook sends a changed copy of one chunk; the ZIP stays intact.
                    let mut copy: Vec<u8>;
                    let raw: &[u8] = if tamper == Some((start / CHUNK_BYTES) as u32) {
                        copy = out.zip[start..end].to_vec();
                        if let Some(byte) = copy.first_mut() {
                            *byte ^= 0xFF;
                        }
                        &copy
                    } else {
                        &out.zip[start..end]
                    };
                    match out.machine.next_chunk(raw) {
                        Ok(frame) => StreamStep::Send {
                            device: out.device,
                            frame,
                            done: out.machine.done_bytes(),
                            total: out.machine.total_bytes(),
                            last: out.machine.last_chunk_built(),
                        },
                        // The machine says the transfer is not streaming (ended meanwhile).
                        Err(_) => return,
                    }
                }
            };
            let (device, frame, done, total, last) = match step {
                StreamStep::Wait(signal, deadline) => {
                    tokio::select! {
                        _ = signal.notified() => {}
                        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                            if self.ack_overdue(id) {
                                return;
                            }
                        }
                    }
                    continue;
                }
                StreamStep::Send {
                    device,
                    frame,
                    done,
                    total,
                    last,
                } => (device, frame, done, total, last),
            };
            if self.deps.network.send_to(&device, frame).await.is_err() {
                self.fail_out(id, Outcome::Failed);
                return;
            }
            {
                // The entry may have left while the frame was in flight. Then nothing more is said.
                // The event goes out under the lock on purpose: `Finished` is emitted after the
                // entry is removed, so this keeps `Progress` before it. The publisher must only
                // enqueue and never call back into the service.
                let inner = self.lock();
                if !inner.outgoing.contains_key(id) {
                    return;
                }
                self.emit(TransferEvent::Progress {
                    transfer_id: id.as_str().to_string(),
                    direction: Direction::Send,
                    done_bytes: done,
                    total_bytes: total,
                });
            }
            if last {
                // No result timer yet: the receiver's final `ChunkAck` starts it. Until then
                // the idle limit guards the missing ack and a stall ends `Failed`.
                self.spawn_final_ack_timer(id.clone());
                return;
            }
        }
    }

    /// The idle deadline of a full window passed. True when the send ended here (no ack came
    /// meanwhile): it is `Failed` and the peer is told `failed`.
    fn ack_overdue(&self, id: &TransferId) -> bool {
        let now = Self::now();
        let limit = self.timing.idle_limit;
        let Some(out) = self.take_out_if(id, |o| {
            !o.machine.window_open() && !o.machine.awaiting_result() && o.last_ack + limit <= now
        }) else {
            // An ack came first, or the send already ended: the loop looks again.
            return !self.lock().outgoing.contains_key(id);
        };
        let mut out = out;
        out.machine.fail_with(Outcome::Failed);
        let device = out.device;
        let frame = out.machine.abort_message(AbortReason::Failed);
        self.finish_out(id, out);
        self.tell_later(device, frame);
        true
    }
}
