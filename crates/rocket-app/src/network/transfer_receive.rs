//! The receiver side of a transfer, around one `IncomingTransfer` per offer.
//!
//! The machine decides what is allowed: the shape of an offer, expiry, the order and size of
//! chunks. `TransferLimits` decides how many offers are open. The service stages the bytes,
//! checks the hash, imports, publishes events and audit, sends the `TransferResult` and runs
//! the timers. An offer is pending until the user answers; nothing is written to the workspace
//! before Accept, and the import never overwrites. The handlers that run on the frame worker
//! are sync; the finalize step and the outbound frames run as their own tasks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_audit::AuditEventKind;
use rocket_network::{
    AbortReason, Admission, ChunkStep, DeviceId, Direction, IncomingTransfer, LinkId, Outcome,
    PeerMessage, ResultOutcome, TransferEvent, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::mpsc;

use super::sync::LinkEnd;
use super::transfer::{outcome_name, Inner, TransferService};
use super::transfer_ports::{
    receiver_display_name, CollectionOrigin, StagedFile, StagedZip, UNSAFE_ZIP_MESSAGE,
};

/// The text of a pending offer whose sender went away before the user answered.
const WENT_OFFLINE: &str = "The other node went offline.";

/// The text of a failed import. The importer's own error can hold receiver-local paths, so it
/// is only logged and never reaches the wire or an event.
const IMPORT_FAILED_MESSAGE: &str = "The collection could not be imported.";

/// The fields of a `CollectionOffer` frame.
pub(super) struct OfferFrame {
    pub transfer_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub chunk_count: u32,
    pub sha256: String,
    pub keep_in_sync: bool,
    pub link_id: Option<String>,
}

/// The link of a linked offer. `Err` when the two link fields disagree or the id does not
/// parse: `keep_in_sync` needs a valid `link_id`, and a `link_id` needs `keep_in_sync`.
fn offered_link(frame: &OfferFrame) -> Result<Option<LinkId>, ()> {
    match (frame.keep_in_sync, frame.link_id.as_deref()) {
        (false, None) => Ok(None),
        (true, Some(text)) => LinkId::parse(text).map(Some).map_err(|_| ()),
        _ => Err(()),
    }
}

/// One incoming transfer, from the offer to the end of the import.
pub(super) struct In {
    pub device: DeviceId,
    pub from_name: String,
    pub machine: IncomingTransfer,
    /// Exists from Accept until the last chunk or the end of the transfer.
    pub staged: Option<Box<dyn StagedZip>>,
    pub last_activity: Instant,
    /// True from the last chunk until the finalize task ends the transfer. Only that task
    /// removes the entry then; an abort or a gone peer just fails the machine.
    pub finalizing: bool,
    /// The ack pump (ruling 20), from Accept on. Dropping the entry ends the pump.
    pub acks: Option<mpsc::UnboundedSender<PeerMessage>>,
    /// A linked offer keeps its link id and a copy of every chunk in memory, so the link can be
    /// created from the received ZIP after the import.
    pub keep: Option<(LinkId, Vec<u8>)>,
}

enum Admit {
    Ignore,
    Refuse(AbortReason),
    Queued,
}

enum Answer {
    /// Unknown, answered already or not ours to answer.
    Gone,
    Expired(Box<In>),
    Declined(Box<In>, PeerMessage),
    Accepted(DeviceId, PeerMessage),
    StageFailed(Box<In>, DomainError),
}

enum Step {
    Ignore,
    Continue,
    Failed(Box<In>),
    Finalize(Box<dyn StagedZip>),
}

/// What the finalize task needs to know about the entry, read under the lock.
struct FinalView {
    size_bytes: u64,
    sha256: String,
    name: String,
    from_name: String,
    /// Set when the machine was failed meanwhile (an abort or a gone peer).
    interrupted: Option<Outcome>,
    device: DeviceId,
    /// The link id and the received bytes of a linked offer, taken out of the entry.
    keep: Option<(LinkId, Vec<u8>)>,
}

fn expired_error() -> DomainError {
    DomainError::InvalidInput("The request expired.".to_string())
}

/// The frame that refuses an offer for which no machine exists.
fn refusal(id: &TransferId, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.as_str().to_string(),
        reason,
    }
}

/// The verdict the sender is told. Other outcomes (declined, expired, cancelled) never get one.
fn result_for(outcome: Outcome) -> Option<ResultOutcome> {
    match outcome {
        Outcome::Completed => Some(ResultOutcome::Completed),
        Outcome::Damaged => Some(ResultOutcome::Damaged),
        Outcome::Rejected => Some(ResultOutcome::Rejected),
        Outcome::Failed => Some(ResultOutcome::Failed),
        _ => None,
    }
}

impl Inner {
    /// Removes an incoming transfer and frees its slot in the limits.
    fn take_in(&mut self, id: &TransferId) -> Option<In> {
        self.limits.release(id);
        self.incoming.remove(id)
    }
}

impl TransferService {
    /// The user's answer to a pending offer. Accepting stages the file and tells the sender
    /// to stream. An expired or unknown offer is `InvalidInput("The request expired.")`.
    pub async fn respond(&self, transfer_id: &str, accept: bool) -> DomainResult<()> {
        self.ensure_worker();
        let Ok(id) = TransferId::parse(transfer_id) else {
            return Err(expired_error());
        };
        match self.answer_offer(&id, accept) {
            Answer::Gone => Err(expired_error()),
            Answer::Expired(entry) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Expired);
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Err(expired_error())
            }
            Answer::Declined(entry, frame) => {
                let device = entry.device;
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Ok(())
            }
            Answer::StageFailed(entry, error) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Failed);
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Err(error)
            }
            Answer::Accepted(device, frame) => {
                if let Err(e) = self.deps.network.send_to(&device, frame).await {
                    let entry = self.lock().take_in(&id);
                    if let Some(mut entry) = entry {
                        entry.machine.fail_with(Outcome::Failed);
                        self.drop_in(&id, entry);
                    }
                    return Err(e);
                }
                self.spawn_idle_watch(id);
                Ok(())
            }
        }
    }

    /// Asks the machine and acts on its answer under the lock. The staged file is created and
    /// stored here, before the `OfferAnswer` frame is sent, so no chunk finds the entry without it.
    fn answer_offer(&self, id: &TransferId, accept: bool) -> Answer {
        let now = Self::now();
        let mut guard = self.lock();
        let inner = &mut *guard;
        let Some(entry) = inner.incoming.get_mut(id) else {
            return Answer::Gone;
        };
        let Some(frame) = entry.machine.respond(accept, now) else {
            // The machine refused the answer: expired, or answered before. Only an expired
            // offer leaves the map; an accepted entry must stay.
            if entry.machine.outcome() == Some(Outcome::Expired) {
                return match inner.take_in(id) {
                    Some(entry) => Answer::Expired(Box::new(entry)),
                    None => Answer::Gone,
                };
            }
            return Answer::Gone;
        };
        if !accept {
            return match inner.take_in(id) {
                Some(entry) => Answer::Declined(Box::new(entry), frame),
                None => Answer::Gone,
            };
        }
        match self.deps.staging.create(id.as_str()) {
            Ok(staged) => {
                entry.staged = Some(staged);
                entry.last_activity = now;
                let device = entry.device;
                entry.acks = self.spawn_ack_pump(device);
                inner.limits.mark_active(id);
                Answer::Accepted(device, frame)
            }
            Err(error) => {
                entry.machine.fail_with(Outcome::Failed);
                match inner.take_in(id) {
                    Some(entry) => Answer::StageFailed(Box::new(entry), error),
                    None => Answer::Gone,
                }
            }
        }
    }

    // ----- frames from the peer -----

    /// A `CollectionOffer` frame.
    pub(super) fn on_offer(&self, from: DeviceId, frame: OfferFrame) {
        let Some((from_name, short_id)) = self.deps.network.peer_label(&from) else {
            return;
        };
        let Ok(id) = TransferId::parse(&frame.transfer_id) else {
            return;
        };
        match self.admit(&from, &from_name, &id, &frame) {
            Admit::Ignore => {}
            Admit::Refuse(reason) => {
                self.tell_later(from, refusal(&id, reason));
            }
            Admit::Queued => {
                self.emit(TransferEvent::OfferIncoming {
                    transfer_id: frame.transfer_id,
                    device_id: from.to_base32(),
                    short_id,
                    display_name: from_name,
                    collection_name: frame.name,
                    size_bytes: frame.size_bytes,
                });
                self.spawn_pending_expiry(id);
            }
        }
    }

    /// The limits decide first, then the machine checks the offer. Both answers are the
    /// machine's own; the service only turns them into a queue entry or a refusal.
    fn admit(
        &self,
        from: &DeviceId,
        from_name: &str,
        id: &TransferId,
        frame: &OfferFrame,
    ) -> Admit {
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        // Read before the transfer lock, so the two service locks are never nested.
        let sync = self.sync_service();
        let id_taken = sync.as_ref().is_some_and(|s| s.holds_transfer(id));
        let mut guard = self.lock();
        let inner = &mut *guard;
        match inner.limits.admit_offer(from, id, now) {
            Admission::Duplicate => return Admit::Ignore,
            Admission::Refuse(reason) => return Admit::Refuse(reason),
            Admission::Admit => {}
        }
        // The rate window already counted the offer. A bad link refuses it like a bad offer,
        // and so do a link this node cannot make (no sync service) and an id a sync payload
        // already uses.
        let link = match offered_link(frame) {
            Ok(Some(_)) if sync.is_none() => Err(()),
            other => other,
        };
        let Ok(link) = link.and_then(|l| if id_taken { Err(()) } else { Ok(l) }) else {
            inner.limits.release(id);
            return Admit::Refuse(AbortReason::Failed);
        };
        let machine = IncomingTransfer::from_offer(
            &frame.transfer_id,
            &frame.name,
            frame.size_bytes,
            frame.chunk_count,
            &frame.sha256,
            now,
        )
        .map(|machine| machine.with_offer_ttl(ttl));
        match machine {
            Ok(machine) => {
                inner.incoming.insert(
                    id.clone(),
                    In {
                        device: *from,
                        from_name: from_name.to_string(),
                        machine,
                        staged: None,
                        last_activity: now,
                        finalizing: false,
                        acks: None,
                        keep: link.map(|link| (link, Vec::new())),
                    },
                );
                Admit::Queued
            }
            Err(error) => {
                inner.limits.release(id);
                Admit::Refuse(error.abort_reason())
            }
        }
    }

    /// A `Chunk` frame. The last chunk starts the finalize task and returns at once.
    pub(super) fn on_chunk(&self, from: DeviceId, id: &TransferId, seq: u32, data: &str) {
        match self.take_chunk_step(&from, id, seq, data) {
            Step::Ignore | Step::Continue => {}
            Step::Failed(entry) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Failed);
                self.drop_in(id, *entry);
                self.tell_later(device, frame);
            }
            Step::Finalize(staged) => self.spawn_finalize(id.clone(), staged),
        }
    }

    /// Feeds a chunk to the machine under the lock and acts on its answer. The append is a
    /// short sync write. `Progress` is published here so it stays ahead of a later `Finished`.
    fn take_chunk_step(&self, from: &DeviceId, id: &TransferId, seq: u32, data: &str) -> Step {
        let now = Self::now();
        let mut guard = self.lock();
        let inner = &mut *guard;
        let Some(entry) = inner.incoming.get_mut(id) else {
            return Step::Ignore;
        };
        if entry.device != *from {
            return Step::Ignore;
        }
        let last = match entry.machine.on_chunk(seq, data) {
            ChunkStep::Ignored => return Step::Ignore,
            ChunkStep::Failed => None,
            ChunkStep::Data {
                bytes,
                done_bytes,
                last,
                ack,
            } => match entry.staged.as_mut().map(|staged| staged.append(&bytes)) {
                Some(Ok(())) => {
                    if let Some((_, copy)) = entry.keep.as_mut() {
                        copy.extend_from_slice(&bytes);
                    }
                    entry.last_activity = now;
                    self.emit_receive_progress(id, done_bytes, entry.machine.total_bytes());
                    // Queued under the lock, so the pump sends the acks in chunk order.
                    if let (Some(ack), Some(pump)) = (ack, entry.acks.as_ref()) {
                        let _ = pump.send(ack);
                    }
                    Some(last)
                }
                // A write error or a missing file ends the transfer.
                _ => {
                    entry.machine.fail_with(Outcome::Failed);
                    None
                }
            },
        };
        match last {
            // The entry stays while chunks are coming.
            Some(false) => Step::Continue,
            // The entry stays, marked, while the finalize task works (ruling 16).
            Some(true) => {
                entry.finalizing = true;
                match entry.staged.take() {
                    Some(staged) => Step::Finalize(staged),
                    None => {
                        entry.machine.fail_with(Outcome::Failed);
                        match inner.take_in(id) {
                            Some(entry) => Step::Failed(Box::new(entry)),
                            None => Step::Ignore,
                        }
                    }
                }
            }
            None => match inner.take_in(id) {
                Some(entry) => Step::Failed(Box::new(entry)),
                None => Step::Ignore,
            },
        }
    }

    /// The peer aborted a transfer it offered or is streaming.
    pub(super) fn abort_incoming(&self, from: &DeviceId, id: &TransferId, reason: AbortReason) {
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let Some(e) = inner.incoming.get_mut(id).filter(|e| e.device == *from) else {
                return;
            };
            if e.finalizing {
                // The finalize task owns the entry. It sees the failed machine before it imports.
                let outcome = match reason {
                    AbortReason::Cancelled => Outcome::Cancelled,
                    _ => Outcome::Failed,
                };
                e.machine.fail_with(outcome);
                return;
            }
            if e.machine.on_abort(reason).is_some() {
                inner.take_in(id)
            } else {
                None
            }
        };
        if let Some(entry) = entry {
            self.drop_in(id, entry);
        }
    }

    /// The device's connection ended or its pairing was removed. Every transfer from it that
    /// is not importing yet is dropped now; one that is importing is failed and ends itself.
    /// A pending offer says the sender went offline (ruling 6).
    pub(super) fn incoming_gone(&self, device: &DeviceId) {
        let dropped: Vec<(TransferId, In, Option<String>)> = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let ids: Vec<TransferId> = inner
                .incoming
                .iter()
                .filter(|(_, e)| e.device == *device)
                .map(|(id, _)| id.clone())
                .collect();
            let mut dropped = Vec::new();
            for id in ids {
                let Some(e) = inner.incoming.get_mut(&id) else {
                    continue;
                };
                if e.finalizing {
                    e.machine.fail_with(Outcome::Failed);
                    continue;
                }
                let message = e.machine.is_pending().then(|| WENT_OFFLINE.to_string());
                if e.machine.connection_lost().is_some() {
                    if let Some(entry) = inner.take_in(&id) {
                        dropped.push((id, entry, message));
                    }
                }
            }
            dropped
        };
        for (id, entry, message) in dropped {
            self.drop_in_with(&id, entry, message);
        }
    }

    /// The user cancels a transfer that is not ours to send. A pending offer is declined. A
    /// transfer that is already importing is left alone.
    pub(super) async fn cancel_incoming(&self, id: &TransferId) -> bool {
        let (pending, finalizing) = self
            .lock()
            .incoming
            .get(id)
            .map(|e| (e.machine.is_pending(), e.finalizing))
            .unwrap_or((false, false));
        if finalizing {
            return false;
        }
        if pending {
            return self.respond(id.as_str(), false).await.is_ok();
        }
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let ended = inner
                .incoming
                .get_mut(id)
                .is_some_and(|e| e.machine.fail_with(Outcome::Cancelled).is_some());
            if ended {
                inner.take_in(id)
            } else {
                None
            }
        };
        let Some(entry) = entry else {
            return false;
        };
        let device = entry.device;
        let frame = entry.machine.abort_message(AbortReason::Cancelled);
        self.drop_in(id, entry);
        self.tell(&device, frame).await;
        true
    }

    // ----- finishing -----

    fn spawn_finalize(&self, id: TransferId, staged: Box<dyn StagedZip>) {
        let Some(me) = self.arc() else {
            staged.discard();
            return;
        };
        drop(tokio::spawn(async move { me.finalize(&id, staged).await }));
    }

    /// Checks the staged file and imports it. Every exit after `finish` releases the file.
    async fn finalize(&self, id: &TransferId, staged: Box<dyn StagedZip>) {
        let finished = tokio::task::spawn_blocking(move || staged.finish()).await;
        let Ok(Ok(file)) = finished else {
            self.conclude(id, Outcome::Failed, None, None, true);
            return;
        };
        let Some(view) = self.final_view(id) else {
            self.release_staged(&file);
            return;
        };
        // An abort or a gone peer during the finish ends the transfer without an import.
        if let Some(outcome) = view.interrupted {
            self.release_staged(&file);
            self.conclude(id, outcome, None, None, false);
            return;
        }
        // Both the size and the hash must fit (ruling 18).
        let intact = file.len == view.size_bytes && file.sha256.eq_ignore_ascii_case(&view.sha256);
        let result = if intact {
            self.import_staged(&file, &view.name, &view.from_name).await
        } else {
            Err((Outcome::Damaged, None))
        };
        self.release_staged(&file);
        match result {
            Ok(imported) => {
                self.record_origin(&imported, &view.from_name, view.device)
                    .await;
                // The link is made only after a successful import, under the imported name.
                if let Some((link, bytes)) = view.keep {
                    self.spawn_link(
                        link,
                        view.device,
                        imported.clone(),
                        bytes,
                        LinkEnd::Receiver,
                    );
                }
                self.conclude(id, Outcome::Completed, None, Some(imported), true)
            }
            Err((outcome, message)) => self.conclude(id, outcome, message, None, true),
        }
    }

    /// Remembers who sent the collection. It only matters for display, so a failure is logged
    /// and never changes the outcome of the receive.
    async fn record_origin(&self, imported: &str, from_name: &str, device: DeviceId) {
        let importer = Arc::clone(&self.deps.importer);
        let imported = imported.to_string();
        let origin = CollectionOrigin {
            sender_name: from_name.to_string(),
            sender_short_id: device.short(),
        };
        let done =
            tokio::task::spawn_blocking(move || importer.record_origin(&imported, &origin)).await;
        if !matches!(done, Ok(Ok(()))) {
            tracing::warn!(
                code = "network_origin_not_saved",
                "could not save the sender"
            );
        }
    }

    fn final_view(&self, id: &TransferId) -> Option<FinalView> {
        let mut inner = self.lock();
        let entry = inner.incoming.get_mut(id)?;
        Some(FinalView {
            size_bytes: entry.machine.total_bytes(),
            sha256: entry.machine.sha256().to_string(),
            name: entry.machine.name().to_string(),
            from_name: entry.from_name.clone(),
            interrupted: entry
                .machine
                .outcome()
                .filter(|outcome| *outcome != Outcome::Completed),
            device: entry.device,
            keep: entry.keep.take(),
        })
    }

    /// Imports the staged ZIP. The error is the outcome to report and a fixed text for the event.
    async fn import_staged(
        &self,
        file: &StagedFile,
        collection: &str,
        from_name: &str,
    ) -> Result<String, (Outcome, Option<String>)> {
        let importer = Arc::clone(&self.deps.importer);
        let path = file.path.clone();
        let collection = collection.to_string();
        let from_name = from_name.to_string();
        let imported = tokio::task::spawn_blocking(move || {
            let display = receiver_display_name(&collection, &from_name);
            importer.import_zip(&path, &display)
        })
        .await;
        match imported {
            Ok(Ok(done)) => Ok(done.name),
            // Only the strict validator's exact text is a rejection.
            Ok(Err(DomainError::InvalidInput(text))) if text == UNSAFE_ZIP_MESSAGE => {
                Err((Outcome::Rejected, Some(text)))
            }
            Ok(Err(error)) => {
                // The error can hold local paths and the peer's collection name, and warn
                // lines reach the webview, so only the code goes at warn.
                tracing::warn!(code = "network_transfer_import_failed", "import failed");
                tracing::debug!(error = %error, "import failed");
                Err((Outcome::Failed, Some(IMPORT_FAILED_MESSAGE.to_string())))
            }
            Err(_) => {
                tracing::warn!(
                    code = "network_transfer_import_panicked",
                    "import task ended"
                );
                Err((Outcome::Failed, Some(IMPORT_FAILED_MESSAGE.to_string())))
            }
        }
    }

    /// Ends a transfer that reached the finalize step: removes the entry, emits the single
    /// `Finished`, audits `Completed`, `Damaged` and `Rejected` (ruling 9) and tells the sender
    /// the verdict from its own task. `imported` is the name the collection got.
    fn conclude(
        &self,
        id: &TransferId,
        outcome: Outcome,
        message: Option<String>,
        imported: Option<String>,
        tell_sender: bool,
    ) {
        let Some(entry) = self.lock().take_in(id) else {
            return;
        };
        let In {
            device,
            from_name,
            machine,
            ..
        } = entry;
        let collection = imported.unwrap_or_else(|| machine.name().to_string());
        self.emit_finished(id, &from_name, &collection, outcome, message);
        if matches!(
            outcome,
            Outcome::Completed | Outcome::Damaged | Outcome::Rejected
        ) {
            self.audit(AuditEventKind::NetworkCollectionReceived {
                device_id: device.short(),
                collection,
                outcome: outcome_name(outcome).to_string(),
            });
        }
        if tell_sender {
            if let Some(verdict) = result_for(outcome) {
                self.tell_later(
                    device,
                    PeerMessage::TransferResult {
                        transfer_id: id.as_str().to_string(),
                        outcome: verdict,
                    },
                );
            }
        }
    }

    /// Deletes a finished staged file. A failure is logged; the start-up purge is the net.
    fn release_staged(&self, file: &StagedFile) {
        if let Err(e) = self.deps.staging.release(file) {
            tracing::warn!(code = "network_transfer_release_failed", error = %e, "staged file stays");
        }
    }

    /// Throws away an unfinished receive and reports the machine's outcome. The entry is
    /// already removed, so this is the one place its `Finished` is emitted.
    fn drop_in(&self, id: &TransferId, entry: In) {
        self.drop_in_with(id, entry, None);
    }

    /// `drop_in` with a `Finished.message`.
    fn drop_in_with(&self, id: &TransferId, entry: In, message: Option<String>) {
        let In {
            from_name,
            machine,
            staged,
            ..
        } = entry;
        if let Some(staged) = staged {
            staged.discard();
        }
        let outcome = machine.outcome().unwrap_or(Outcome::Failed);
        self.emit_finished(id, &from_name, machine.name(), outcome, message);
    }

    /// One task per accepted receive that sends its acks one after the other, so they never
    /// overtake each other (ruling 20). It ends when the entry, and so the sender half, goes.
    fn spawn_ack_pump(&self, device: DeviceId) -> Option<mpsc::UnboundedSender<PeerMessage>> {
        let me = self.arc()?;
        let (pump, mut queue) = mpsc::unbounded_channel::<PeerMessage>();
        drop(tokio::spawn(async move {
            while let Some(frame) = queue.recv().await {
                me.tell(&device, frame).await;
            }
        }));
        Some(pump)
    }

    fn emit_finished(
        &self,
        id: &TransferId,
        from_name: &str,
        collection: &str,
        outcome: Outcome,
        message: Option<String>,
    ) {
        self.emit(TransferEvent::Finished {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Receive,
            outcome,
            display_name: from_name.to_string(),
            collection_name: collection.to_string(),
            message,
        });
    }

    fn emit_receive_progress(&self, id: &TransferId, done: u64, total: u64) {
        self.emit(TransferEvent::Progress {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Receive,
            done_bytes: done,
            total_bytes: total,
        });
    }

    // ----- timers -----

    fn spawn_pending_expiry(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let ttl = self.timing.offer_ttl;
        drop(tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            me.expire_in(&id).await;
        }));
    }

    /// Ends an unanswered offer when the machine says it is due.
    async fn expire_in(&self, id: &TransferId) {
        let now = Self::now();
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let due = inner
                .incoming
                .get_mut(id)
                .is_some_and(|e| e.machine.expire_if_due(now));
            if due {
                inner.take_in(id)
            } else {
                None
            }
        };
        if let Some(entry) = entry {
            let device = entry.device;
            let frame = entry.machine.abort_message(AbortReason::Expired);
            self.drop_in(id, entry);
            self.tell(&device, frame).await;
        }
    }

    /// Drops a receive that gets no frame for `Timing::idle_limit`. This is the fallback for a
    /// half-open connection; the peer-gone hook ends a dead one at once. A transfer that is
    /// importing is not watched.
    fn spawn_idle_watch(&self, id: TransferId) {
        enum Idle {
            Gone,
            Wait(Duration),
            Dropped(Box<In>),
        }
        let Some(me) = self.arc() else {
            return;
        };
        let limit = self.timing.idle_limit;
        drop(tokio::spawn(async move {
            loop {
                let step = {
                    let now = TransferService::now();
                    let mut guard = me.lock();
                    let inner = &mut *guard;
                    let due = inner
                        .incoming
                        .get(&id)
                        .map(|e| (e.finalizing, e.last_activity + limit));
                    match due {
                        None | Some((true, _)) => Idle::Gone,
                        Some((false, due)) if due > now => {
                            Idle::Wait(due.saturating_duration_since(now))
                        }
                        Some(_) => {
                            let ended = inner
                                .incoming
                                .get_mut(&id)
                                .is_some_and(|e| e.machine.connection_lost().is_some());
                            match ended.then(|| inner.take_in(&id)).flatten() {
                                Some(entry) => Idle::Dropped(Box::new(entry)),
                                None => Idle::Gone,
                            }
                        }
                    }
                };
                match step {
                    Idle::Gone => return,
                    Idle::Wait(wait) => tokio::time::sleep(wait).await,
                    Idle::Dropped(entry) => {
                        let device = entry.device;
                        let frame = entry.machine.abort_message(AbortReason::Failed);
                        me.drop_in(&id, *entry);
                        me.tell(&device, frame).await;
                        return;
                    }
                }
            }
        }));
    }

    /// `(pending, accepted)` incoming transfers, for tests.
    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize) {
        let inner = self.lock();
        let pending = inner
            .incoming
            .values()
            .filter(|e| e.machine.is_pending())
            .count();
        (pending, inner.incoming.len() - pending)
    }
}
