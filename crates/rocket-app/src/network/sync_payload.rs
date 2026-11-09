//! Sync payloads: the in-memory ZIP of files, and the payload machines with their stream and
//! receive (S3). Each payload is one transfer id with an `OutgoingTransfer` on the sender and
//! an `IncomingTransfer` on the receiver; the machine's `CollectionOffer` never leaves, the
//! service announces the payload with a `SyncOffer`. Nothing here touches a file.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_network::limits::{CHUNK_BYTES, MAX_UNZIPPED_BYTES};
use rocket_network::{
    ChunkStep, DeviceId, IncomingTransfer, LinkId, OfferError, Outcome, OutgoingState,
    OutgoingTransfer, PeerMessage, SyncEvent, SyncId, SyncPayloadKind, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Notify};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::sync::{LinkUpdate, Phase, Role, SyncOfferFields, SyncService};
use super::sync_checks::Stop;
use super::sync_ports::SyncFile;

/// The name the machines carry for a payload kind: `manifest`, `plan`, `files`.
pub(super) fn kind_name(kind: SyncPayloadKind) -> &'static str {
    kind.transfer_name()
}

/// One payload this node sends.
pub(super) struct PayloadOut {
    pub kind: SyncPayloadKind,
    pub machine: OutgoingTransfer,
    pub bytes: Vec<u8>,
    /// The offer, the answer or the last good ack. The idle limit counts from here.
    pub last_ack: Instant,
    /// Wakes the stream when an ack opens the window.
    pub ack_signal: Arc<Notify>,
    pub acked: u32,
}

/// One payload this node receives. The bytes stay in memory (S3).
pub(super) struct PayloadIn {
    pub kind: SyncPayloadKind,
    pub machine: IncomingTransfer,
    pub bytes: Vec<u8>,
    pub last_activity: Instant,
    /// The ack pump. Dropping it ends the pump once the queued acks are sent.
    pub acks: Option<mpsc::UnboundedSender<PeerMessage>>,
}

/// Hashes the bytes and builds the machine. `TooBig` and `Empty` come back as the machine's
/// `OfferError`; the caller turns `TooBig` into `Stop::TooBig(len)`.
pub(super) fn build_out(
    kind: SyncPayloadKind,
    bytes: Vec<u8>,
    now: Instant,
    ttl: Duration,
) -> Result<PayloadOut, OfferError> {
    let sha = sha_hex(&bytes);
    let machine = OutgoingTransfer::new(
        TransferId::random(),
        kind_name(kind),
        bytes.len() as u64,
        &sha,
        now,
    )?
    .with_offer_ttl(ttl);
    Ok(PayloadOut {
        kind,
        machine,
        bytes,
        last_ack: now,
        ack_signal: Arc::new(Notify::new()),
        acked: 0,
    })
}

/// The stop for a payload the machine refused to build.
pub(super) fn out_error(error: OfferError, len: usize) -> Stop {
    match error {
        OfferError::TooBig => Stop::TooBig(len as u64),
        _ => Stop::Internal,
    }
}

/// The `SyncOffer` for a payload, from the machine's own offer fields.
pub(super) fn sync_offer(
    out: &PayloadOut,
    sync_id: &SyncId,
    link_id: &LinkId,
    base_hash: &str,
) -> PeerMessage {
    match out.machine.offer_message() {
        PeerMessage::CollectionOffer {
            transfer_id,
            size_bytes,
            chunk_count,
            sha256,
            ..
        } => PeerMessage::SyncOffer {
            sync_id: sync_id.as_str().to_string(),
            link_id: link_id.as_str().to_string(),
            base_hash: base_hash.to_string(),
            kind: out.kind,
            transfer_id,
            size_bytes,
            chunk_count,
            sha256,
        },
        // `offer_message` only builds a CollectionOffer; this arm never runs.
        other => other,
    }
}

fn pack_error() -> DomainError {
    DomainError::Internal("The sync files could not be packed.".to_string())
}

/// A ZIP of `files` in path order, deflated, no folder entries and a fixed time per entry, so
/// the same files always give the same bytes.
pub(super) fn pack_zip(files: &[SyncFile]) -> DomainResult<Vec<u8>> {
    let mut sorted: Vec<&SyncFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for file in sorted {
        writer
            .start_file(file.path.as_str(), options)
            .map_err(|_| pack_error())?;
        writer.write_all(&file.bytes).map_err(|_| pack_error())?;
    }
    let out = writer.finish().map_err(|_| pack_error())?;
    Ok(out.into_inner())
}

/// The entries of a received files ZIP. The strict validator runs first; a refusal of it, or
/// any reader error, is `Stop::Unsafe`. Folder entries are skipped. The validator already caps
/// the unzipped size, and reading takes at most that much again as a second guard.
pub(super) fn unpack_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Stop> {
    rocket_import::transfer_zip::validate_bytes(bytes).map_err(|_| Stop::Unsafe)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| Stop::Unsafe)?;
    let mut left = MAX_UNZIPPED_BYTES;
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| Stop::Unsafe)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut data = Vec::new();
        // One byte more than is left shows an entry that goes past the cap.
        entry
            .take(left + 1)
            .read_to_end(&mut data)
            .map_err(|_| Stop::Unsafe)?;
        left = left.checked_sub(data.len() as u64).ok_or(Stop::Unsafe)?;
        if out.insert(name, data).is_some() {
            return Err(Stop::Unsafe);
        }
    }
    Ok(out)
}

/// Lowercase hex SHA-256 of a payload.
pub(super) fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// What the stream does next.
enum StreamStep {
    Send {
        device: DeviceId,
        frame: PeerMessage,
        done: u64,
        total: u64,
        last: bool,
        files: bool,
    },
    Wait(Arc<Notify>, Instant),
}

impl SyncService {
    /// Offers a payload: builds its machine (hashing the bytes; the caller runs in a blocking
    /// task), stores it on the session, registers its id, moves to `phase`, then sends the
    /// `SyncOffer` from its own task. A failed send ends the session "went offline". A
    /// session that is gone is left alone.
    pub(super) fn offer_payload(
        &self,
        sync_id: &SyncId,
        kind: SyncPayloadKind,
        bytes: Vec<u8>,
        phase: Phase,
    ) -> Result<(), Stop> {
        let len = bytes.len();
        let out = build_out(kind, bytes, Self::now(), self.timing.offer_ttl)
            .map_err(|e| out_error(e, len))?;
        let sent = {
            let mut inner = self.lock();
            let id = out.machine.transfer_id().clone();
            if inner.transfers.contains_key(&id) {
                return Err(Stop::Internal);
            }
            let Some(s) = inner.sessions.get_mut(sync_id) else {
                return Ok(());
            };
            let frame = sync_offer(&out, sync_id, &s.link_id, &s.base_hash);
            let peer = s.peer;
            s.out = Some(out);
            s.enter(phase, Self::now());
            inner.transfers.insert(id, (peer, sync_id.clone()));
            (peer, frame)
        };
        self.tell_or_end(sync_id, sent.0, sent.1);
        Ok(())
    }

    /// The peer's answer to one of our payload offers. A sync payload is never declined, so a
    /// decline is a bad plan; a late answer finds the offer expired.
    pub(super) fn on_payload_answer(&self, sync_id: &SyncId, id: &TransferId, accept: bool) {
        enum Next {
            Ignore,
            Stream,
            End(Stop),
        }
        let now = Self::now();
        let next = {
            let mut inner = self.lock();
            let out = inner
                .sessions
                .get_mut(sync_id)
                .and_then(|s| s.out.as_mut())
                .filter(|o| o.machine.transfer_id() == id);
            match out {
                None => Next::Ignore,
                Some(out) => {
                    let was_offered = out.machine.state() == OutgoingState::Offered;
                    match out.machine.on_answer(accept, now) {
                        Some(Outcome::Expired) => Next::End(Stop::Stalled),
                        Some(_) => Next::End(Stop::BadPlan),
                        None if was_offered && out.machine.state() == OutgoingState::Accepted => {
                            out.last_ack = now;
                            Next::Stream
                        }
                        None => Next::Ignore,
                    }
                }
            }
        };
        match next {
            Next::Ignore => {}
            Next::Stream => self.spawn_payload_stream(sync_id.clone(), id.clone()),
            Next::End(stop) => self.stop(sync_id, stop, LinkUpdate::Keep),
        }
    }

    fn spawn_payload_stream(&self, sync_id: SyncId, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move {
            me.payload_stream(&sync_id, &id).await
        }));
    }

    /// Sends every chunk in order inside the ack window, like the transfer stream. The first
    /// chunk marks the session for the audit; `Progress` is published for `Files` only. After
    /// the last chunk the watchdog guards the wait for the final ack.
    async fn payload_stream(&self, sync_id: &SyncId, id: &TransferId) {
        loop {
            let step = {
                let mut inner = self.lock();
                let Some(s) = inner.sessions.get_mut(sync_id) else {
                    return;
                };
                let peer = s.peer;
                let Some(out) = s.out.as_mut().filter(|o| o.machine.transfer_id() == id) else {
                    return;
                };
                if !out.machine.is_active() {
                    return;
                }
                if !out.machine.window_open() {
                    StreamStep::Wait(
                        out.ack_signal.clone(),
                        out.last_ack + self.timing.idle_limit,
                    )
                } else {
                    let start = out.machine.done_bytes() as usize;
                    let end = (start + CHUNK_BYTES).min(out.bytes.len());
                    match out.machine.next_chunk(&out.bytes[start..end]) {
                        Ok(frame) => StreamStep::Send {
                            device: peer,
                            frame,
                            done: out.machine.done_bytes(),
                            total: out.machine.total_bytes(),
                            last: out.machine.last_chunk_built(),
                            files: out.kind == SyncPayloadKind::Files,
                        },
                        Err(_) => return,
                    }
                }
            };
            match step {
                StreamStep::Wait(signal, deadline) => {
                    tokio::select! {
                        _ = signal.notified() => {}
                        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                            if self.payload_overdue(sync_id, id) {
                                return;
                            }
                        }
                    }
                }
                StreamStep::Send {
                    device,
                    frame,
                    done,
                    total,
                    last,
                    files,
                } => {
                    if self.deps.network.send_to(&device, frame).await.is_err() {
                        self.stop(sync_id, Stop::WentOffline, LinkUpdate::Keep);
                        return;
                    }
                    {
                        let mut inner = self.lock();
                        let Some(s) = inner.sessions.get_mut(sync_id) else {
                            return;
                        };
                        s.chunk_out = true;
                        if files {
                            // Under the lock, so it stays before the session's `Finished`.
                            self.emit(SyncEvent::Progress {
                                sync_id: sync_id.as_str().to_string(),
                                done_bytes: s.progress_base + done,
                                total_bytes: s.progress_base + total,
                            });
                        }
                    }
                    if last {
                        return;
                    }
                }
            }
        }
    }

    /// The idle deadline of a full window passed. True when the session ended here.
    fn payload_overdue(&self, sync_id: &SyncId, id: &TransferId) -> bool {
        let now = Self::now();
        let overdue = {
            let inner = self.lock();
            match inner.sessions.get(sync_id).and_then(|s| s.out.as_ref()) {
                Some(out) if out.machine.transfer_id() == id => {
                    !out.machine.window_open() && out.last_ack + self.timing.idle_limit <= now
                }
                _ => return true,
            }
        };
        if overdue {
            self.stop(sync_id, Stop::Stalled, LinkUpdate::Keep);
        }
        overdue
    }

    /// The peer acked `next_seq` chunks of our payload. A bad ack ends the session damaged;
    /// the final ack ends the payload and runs the next step of the session's role.
    pub(super) fn on_payload_ack(&self, sync_id: &SyncId, id: &TransferId, next_seq: u32) {
        enum Next {
            Ignore,
            Sent(Role, SyncPayloadKind),
        }
        let now = Self::now();
        let next = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            match inner.sessions.get_mut(sync_id) {
                None => Next::Ignore,
                Some(s) => match s.out.as_mut().filter(|o| o.machine.transfer_id() == id) {
                    None => Next::Ignore,
                    Some(out) => {
                        if out.machine.on_ack(next_seq).is_some() {
                            drop(guard);
                            return self.stop(sync_id, Stop::Damaged, LinkUpdate::Keep);
                        }
                        if next_seq > out.acked {
                            out.acked = next_seq;
                            out.last_ack = now;
                            out.ack_signal.notify_one();
                        }
                        if out.machine.awaiting_result() {
                            let kind = out.kind;
                            if kind == SyncPayloadKind::Files {
                                s.progress_base += out.machine.total_bytes();
                            }
                            s.out = None;
                            inner.transfers.remove(id);
                            Next::Sent(s.role, kind)
                        } else {
                            Next::Ignore
                        }
                    }
                },
            }
        };
        match next {
            Next::Ignore => {}
            Next::Sent(Role::Starter, kind) => self.starter_payload_sent(sync_id, kind),
            Next::Sent(Role::Planner, kind) => self.planner_payload_sent(sync_id, kind),
        }
    }

    /// Takes a payload the peer offers: builds the receiving machine, registers its id, starts
    /// its ack pump, moves to `phase` and answers `OfferAnswer { accept: true }` itself. A
    /// refused offer ends the session (`TooBig` for the size, else a bad plan).
    pub(super) fn accept_payload(&self, sync_id: &SyncId, offer: SyncOfferFields, phase: Phase) {
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        let answer = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let Ok(id) = TransferId::parse(&offer.transfer_id) else {
                drop(guard);
                return self.stop(sync_id, Stop::BadPlan, LinkUpdate::Keep);
            };
            if inner.transfers.contains_key(&id) {
                drop(guard);
                return self.stop(sync_id, Stop::BadPlan, LinkUpdate::Keep);
            }
            let Some(s) = inner.sessions.get_mut(sync_id) else {
                return;
            };
            let machine = IncomingTransfer::from_offer(
                &offer.transfer_id,
                kind_name(offer.kind),
                offer.size_bytes,
                offer.chunk_count,
                &offer.sha256,
                now,
            )
            .map(|m| m.with_offer_ttl(ttl));
            match machine {
                Err(OfferError::TooBig) => Err(Stop::TooBig(offer.size_bytes)),
                Err(_) => Err(Stop::BadPlan),
                Ok(mut machine) => match machine.respond(true, now) {
                    None => Err(Stop::BadPlan),
                    Some(frame) => {
                        let peer = s.peer;
                        s.inc = Some(PayloadIn {
                            kind: offer.kind,
                            machine,
                            bytes: Vec::new(),
                            last_activity: now,
                            acks: self.spawn_ack_pump(peer),
                        });
                        s.enter(phase, now);
                        inner.transfers.insert(id, (peer, sync_id.clone()));
                        Ok((peer, frame))
                    }
                },
            }
        };
        match answer {
            Ok((peer, frame)) => self.tell_or_end(sync_id, peer, frame),
            Err(stop) => self.stop(sync_id, stop, LinkUpdate::Keep),
        }
    }

    /// One task per received payload that sends its acks in order. It ends once the payload
    /// drops its sender and the queued acks are out.
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

    /// One chunk of a payload the peer sends. A broken chunk ends the session damaged. After
    /// the last chunk a blocking task checks the size and SHA-256 of the whole payload and
    /// hands the bytes to the session's role.
    pub(super) fn on_payload_chunk(&self, sync_id: &SyncId, id: &TransferId, seq: u32, data: &str) {
        enum Next {
            Ignore,
            Bad,
            Done(Role, SyncPayloadKind, Vec<u8>, u64, String),
        }
        let now = Self::now();
        let next = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            match inner.sessions.get_mut(sync_id) {
                None => Next::Ignore,
                Some(s) => {
                    let base = s.progress_base;
                    match s.inc.as_mut().filter(|i| i.machine.transfer_id() == id) {
                        None => Next::Ignore,
                        Some(inc) => match inc.machine.on_chunk(seq, data) {
                            ChunkStep::Ignored => Next::Ignore,
                            ChunkStep::Failed => Next::Bad,
                            ChunkStep::Data {
                                bytes,
                                done_bytes,
                                last,
                                ack,
                            } => {
                                inc.bytes.extend_from_slice(&bytes);
                                inc.last_activity = now;
                                if let (Some(ack), Some(pump)) = (ack, inc.acks.as_ref()) {
                                    let _ = pump.send(ack);
                                }
                                let total = inc.machine.total_bytes();
                                let files = inc.kind == SyncPayloadKind::Files;
                                if files {
                                    self.emit(SyncEvent::Progress {
                                        sync_id: sync_id.as_str().to_string(),
                                        done_bytes: base + done_bytes,
                                        total_bytes: base + total,
                                    });
                                }
                                if last {
                                    let sha = inc.machine.sha256().to_string();
                                    let kind = inc.kind;
                                    let bytes = std::mem::take(&mut inc.bytes);
                                    s.inc = None;
                                    if files {
                                        s.progress_base += total;
                                    }
                                    inner.transfers.remove(id);
                                    Next::Done(s.role, kind, bytes, total, sha)
                                } else {
                                    Next::Ignore
                                }
                            }
                        },
                    }
                }
            }
        };
        match next {
            Next::Ignore => {}
            Next::Bad => self.stop(sync_id, Stop::Damaged, LinkUpdate::Keep),
            Next::Done(role, kind, bytes, total, sha) => {
                let Some(me) = self.arc() else {
                    return;
                };
                let sid = sync_id.clone();
                drop(tokio::task::spawn_blocking(move || {
                    if bytes.len() as u64 != total || !sha.eq_ignore_ascii_case(&sha_hex(&bytes)) {
                        return me.stop(&sid, Stop::Damaged, LinkUpdate::Keep);
                    }
                    match role {
                        Role::Starter => me.starter_payload_received(&sid, kind, bytes),
                        Role::Planner => me.planner_payload_received(&sid, kind, bytes),
                    }
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SyncFile {
        SyncFile {
            path: path.to_string(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn pack_sorts_by_path_and_is_stable() {
        let a = pack_zip(&[file("b.yml", "b"), file("a.yml", "a")]).expect("zip");
        let b = pack_zip(&[file("a.yml", "a"), file("b.yml", "b")]).expect("zip");
        assert_eq!(a, b, "the input order does not matter");
        let names = super::super::transfer_fakes::zip_entry_names(&a);
        assert_eq!(names, vec!["a.yml", "b.yml"]);
    }

    #[test]
    fn unpack_refuses_what_the_strict_validator_refuses() {
        assert_eq!(unpack_zip(b"not a zip"), Err(Stop::Unsafe));
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("../a.yml", SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(b"x").expect("bytes");
        let bad = writer.finish().expect("zip").into_inner();
        assert_eq!(unpack_zip(&bad), Err(Stop::Unsafe));
    }

    #[test]
    fn sha_hex_is_lowercase_sha256() {
        assert_eq!(
            sha_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
