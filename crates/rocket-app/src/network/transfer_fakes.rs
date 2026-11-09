//! Fakes for the transfer service: peers, staging, importer, events and a rig that wires them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_network::limits::CHUNK_BYTES;
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, Outcome, PeerMessage, ResultOutcome,
    TransferEvent, TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use tokio::sync::Notify;

use super::transfer::{PeerSender, Timing, TransferDeps, TransferService};
use super::transfer_ports::{
    CollectionImportPort, ImportedCollection, StagedFile, StagedZip, TransferStaging,
    UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

// ----- payloads -----

/// Bytes that do not compress, so a ZIP of them is about as big as they are.
pub(crate) fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// A valid transfer id (32 lowercase hex characters) for `n`.
pub(crate) fn tid(n: u8) -> String {
    format!("{n:032x}")
}

pub(crate) fn answer(id: &str, accept: bool) -> PeerMessage {
    PeerMessage::OfferAnswer {
        transfer_id: id.to_string(),
        accept,
    }
}

pub(crate) fn abort(id: &str, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.to_string(),
        reason,
    }
}

pub(crate) fn result(id: &str, outcome: ResultOutcome) -> PeerMessage {
    PeerMessage::TransferResult {
        transfer_id: id.to_string(),
        outcome,
    }
}

pub(crate) fn ack(id: &str, next_seq: u32) -> PeerMessage {
    PeerMessage::ChunkAck {
        transfer_id: id.to_string(),
        next_seq,
    }
}

/// The entry names of a ZIP held in memory.
pub(crate) fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip");
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).expect("an entry");
            let mut sink = Vec::new();
            entry.read_to_end(&mut sink).expect("readable");
            entry.name().to_string()
        })
        .collect()
}

/// The bytes a sender would stream, with helpers that build the matching frames.
#[derive(Clone)]
pub(crate) struct Payload {
    pub bytes: Vec<u8>,
}

impl Payload {
    pub fn new(len: usize) -> Payload {
        Payload { bytes: noise(len) }
    }

    pub fn sha(&self) -> String {
        format!("{:x}", Sha256::digest(&self.bytes))
    }

    pub fn count(&self) -> u32 {
        chunk_count_for(self.bytes.len() as u64)
    }

    pub fn offer(&self, id: &str, name: &str) -> PeerMessage {
        PeerMessage::CollectionOffer {
            transfer_id: id.to_string(),
            name: name.to_string(),
            size_bytes: self.bytes.len() as u64,
            chunk_count: self.count(),
            sha256: self.sha(),
            keep_in_sync: false,
            link_id: None,
        }
    }

    pub fn chunk(&self, id: &str, seq: u32) -> PeerMessage {
        let start = seq as usize * CHUNK_BYTES;
        let end = (start + CHUNK_BYTES).min(self.bytes.len());
        PeerMessage::Chunk {
            transfer_id: id.to_string(),
            seq,
            data: STANDARD.encode(&self.bytes[start..end]),
        }
    }
}

/// Polls `check` until it is true. Fails the test after about 5 seconds.
pub(crate) async fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

// ----- peers -----

struct Hold {
    seq: u32,
    reached: Arc<Notify>,
    release: Arc<Notify>,
}

/// The network as the transfer service sees it: labels, online flags and a log of sent frames.
#[derive(Default)]
pub(crate) struct FakePeers {
    labels: Mutex<HashMap<DeviceId, String>>,
    online: Mutex<HashSet<DeviceId>>,
    sent: Mutex<Vec<(DeviceId, PeerMessage)>>,
    sink: Mutex<Option<Arc<dyn TransferSink>>>,
    hold: Mutex<Option<Hold>>,
    hold_offer: Mutex<Option<(Arc<Notify>, Arc<Notify>)>>,
}

impl FakePeers {
    /// A paired, online peer.
    pub fn add(&self, device: DeviceId, name: &str) {
        self.labels
            .lock()
            .expect("labels lock")
            .insert(device, name.to_string());
        self.online.lock().expect("online lock").insert(device);
    }

    pub fn set_online(&self, device: &DeviceId, online: bool) {
        let mut set = self.online.lock().expect("online lock");
        if online {
            set.insert(*device);
        } else {
            set.remove(device);
        }
    }

    /// The peer is no longer paired or is blocked.
    pub fn forget(&self, device: &DeviceId) {
        self.labels.lock().expect("labels lock").remove(device);
    }

    pub fn sent(&self) -> Vec<(DeviceId, PeerMessage)> {
        self.sent.lock().expect("sent lock").clone()
    }

    /// The frames sent from position `start` on, so a poll copies only what is new.
    pub fn sent_from(&self, start: usize) -> Vec<(DeviceId, PeerMessage)> {
        let sent = self.sent.lock().expect("sent lock");
        sent.get(start..).map(<[_]>::to_vec).unwrap_or_default()
    }

    pub fn sent_to(&self, device: &DeviceId) -> Vec<PeerMessage> {
        self.sent()
            .into_iter()
            .filter(|(to, _)| to == device)
            .map(|(_, message)| message)
            .collect()
    }

    pub fn sink(&self) -> Option<Arc<dyn TransferSink>> {
        self.sink.lock().expect("sink lock").clone()
    }

    /// Parks the send of the next offer until `release` is notified. `reached` fires when it parks.
    pub fn hold_offer(&self) -> (Arc<Notify>, Arc<Notify>) {
        let pair = (Arc::new(Notify::new()), Arc::new(Notify::new()));
        *self.hold_offer.lock().expect("hold offer lock") = Some(pair.clone());
        pair
    }

    /// Parks the send of chunk `seq` until `release` is notified. `reached` fires when it parks.
    pub fn hold_chunk(&self, seq: u32) -> (Arc<Notify>, Arc<Notify>) {
        let reached = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        *self.hold.lock().expect("hold lock") = Some(Hold {
            seq,
            reached: reached.clone(),
            release: release.clone(),
        });
        (reached, release)
    }
}

#[async_trait]
impl PeerSender for FakePeers {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let hold = {
            let guard = self.hold.lock().expect("hold lock");
            match (&*guard, &message) {
                (Some(h), PeerMessage::Chunk { seq, .. }) if h.seq == *seq => {
                    Some((h.reached.clone(), h.release.clone()))
                }
                _ => None,
            }
        };
        if matches!(message, PeerMessage::CollectionOffer { .. }) {
            let parked = self.hold_offer.lock().expect("hold offer lock").take();
            if let Some((reached, release)) = parked {
                reached.notify_one();
                release.notified().await;
            }
        }
        if let Some((reached, release)) = hold {
            reached.notify_one();
            release.notified().await;
        }
        if !self.online.lock().expect("online lock").contains(device) {
            return Err(DomainError::InvalidInput("That node is offline.".into()));
        }
        self.sent
            .lock()
            .expect("sent lock")
            .push((*device, message));
        Ok(())
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        self.labels
            .lock()
            .expect("labels lock")
            .get(device)
            .map(|name| (name.clone(), device.short()))
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        self.online.lock().expect("online lock").contains(device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        *self.sink.lock().expect("sink lock") = Some(sink);
    }
}

// ----- events -----

#[derive(Default)]
pub(crate) struct RecordingTransferEvents(Mutex<Vec<TransferEvent>>);

impl RecordingTransferEvents {
    pub fn all(&self) -> Vec<TransferEvent> {
        self.0.lock().expect("events lock").clone()
    }

    /// `(transfer id, display name, short id, collection, size)` of every `OfferIncoming`.
    pub fn incoming(&self) -> Vec<(String, String, String, String, u64)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::OfferIncoming {
                    transfer_id,
                    display_name,
                    short_id,
                    collection_name,
                    size_bytes,
                    ..
                } => Some((
                    transfer_id,
                    display_name,
                    short_id,
                    collection_name,
                    size_bytes,
                )),
                _ => None,
            })
            .collect()
    }

    pub fn progress(&self) -> Vec<(Direction, u64, u64)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Progress {
                    direction,
                    done_bytes,
                    total_bytes,
                    ..
                } => Some((direction, done_bytes, total_bytes)),
                _ => None,
            })
            .collect()
    }

    /// `(transfer id, direction, outcome)` of every `Finished`.
    pub fn finished(&self) -> Vec<(String, Direction, Outcome)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished {
                    transfer_id,
                    direction,
                    outcome,
                    ..
                } => Some((transfer_id, direction, outcome)),
                _ => None,
            })
            .collect()
    }

    /// The `message` of every `Finished`, in order.
    pub fn finished_messages(&self) -> Vec<Option<String>> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }
}

impl EventPublisher for RecordingTransferEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<TransferEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }
}

// ----- staging -----

#[derive(Default)]
pub(crate) struct StagingCounters {
    created: AtomicUsize,
    discarded: AtomicUsize,
    finished: AtomicUsize,
    released: AtomicUsize,
    purges: AtomicUsize,
}

impl StagingCounters {
    pub fn created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }
    pub fn discarded(&self) -> usize {
        self.discarded.load(Ordering::SeqCst)
    }
    pub fn finished(&self) -> usize {
        self.finished.load(Ordering::SeqCst)
    }
    pub fn released(&self) -> usize {
        self.released.load(Ordering::SeqCst)
    }
    pub fn purges(&self) -> usize {
        self.purges.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
pub(crate) struct FakeStaging {
    pub counters: Arc<StagingCounters>,
    pub fail_create: AtomicBool,
    pub fail_release: AtomicBool,
    /// The finished file reports one byte more than was appended, with the right hash.
    pub wrong_len: Arc<AtomicBool>,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeStaging {
    /// The next created file waits inside `finish` until the returned sender sends (or drops).
    /// Call it before the transfer is accepted. `finished()` counts the call before it waits.
    pub fn hold_finish(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *self.gate.lock().expect("gate lock") = Some(gate);
        release
    }

    /// Every file that was created was discarded or finished, every finished file was
    /// released, and the service never purged (the purge belongs to start-up).
    pub fn assert_clean(&self) {
        let c = &self.counters;
        assert_eq!(
            c.created(),
            c.discarded() + c.finished(),
            "every staged file is discarded or finished"
        );
        assert_eq!(
            c.released(),
            c.finished(),
            "every finished file is released"
        );
        assert_eq!(c.purges(), 0, "the service never purges");
    }
}

struct FakeStagedZip {
    id: String,
    bytes: Vec<u8>,
    counters: Arc<StagingCounters>,
    wrong_len: Arc<AtomicBool>,
    gate: Option<std::sync::mpsc::Receiver<()>>,
}

impl StagedZip for FakeStagedZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>) -> DomainResult<StagedFile> {
        self.counters.finished.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        let extra = u64::from(self.wrong_len.load(Ordering::SeqCst));
        Ok(StagedFile {
            path: PathBuf::from(format!("/staged/{}.zip", self.id)),
            len: self.bytes.len() as u64 + extra,
            sha256: format!("{:x}", Sha256::digest(&self.bytes)),
        })
    }

    fn discard(self: Box<Self>) {
        self.counters.discarded.fetch_add(1, Ordering::SeqCst);
    }
}

impl TransferStaging for FakeStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        if self.fail_create.load(Ordering::SeqCst) {
            return Err(DomainError::Io("disk full".into()));
        }
        self.counters.created.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeStagedZip {
            id: transfer_id.to_string(),
            bytes: Vec::new(),
            counters: self.counters.clone(),
            wrong_len: self.wrong_len.clone(),
            gate: self.gate.lock().expect("gate lock").take(),
        }))
    }

    fn release(&self, _file: &StagedFile) -> DomainResult<()> {
        self.counters.released.fetch_add(1, Ordering::SeqCst);
        if self.fail_release.load(Ordering::SeqCst) {
            return Err(DomainError::Io("cannot delete".into()));
        }
        Ok(())
    }

    fn purge(&self) -> DomainResult<()> {
        self.counters.purges.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ----- importer -----

#[derive(Clone, Copy, Default)]
pub(crate) enum ImportMode {
    #[default]
    Accept,
    /// The strict validator refused the ZIP: `InvalidInput` with `UNSAFE_ZIP_MESSAGE`.
    Reject,
    /// Any other failure.
    Fail,
    /// `InvalidInput` with another text. It must not count as a rejection.
    OtherInvalid,
}

#[derive(Default)]
pub(crate) struct FakeImporter {
    pub existing: Mutex<Vec<String>>,
    pub mode: Mutex<ImportMode>,
    calls: Mutex<Vec<(PathBuf, String)>>,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeImporter {
    /// The next import waits (after it was recorded in `calls`) until the sender sends or drops.
    pub fn hold_import(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *self.gate.lock().expect("gate lock") = Some(gate);
        release
    }

    pub fn set_mode(&self, mode: ImportMode) {
        *self.mode.lock().expect("mode lock") = mode;
    }

    pub fn calls(&self) -> Vec<(PathBuf, String)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl CollectionImportPort for FakeImporter {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.existing.lock().expect("existing lock").clone())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((zip_path.to_path_buf(), display_name.to_string()));
        let gate = self.gate.lock().expect("gate lock").take();
        if let Some(gate) = gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        let mode = *self.mode.lock().expect("mode lock");
        match mode {
            ImportMode::Accept => Ok(ImportedCollection {
                name: display_name.to_string(),
            }),
            ImportMode::Reject => Err(DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.into())),
            ImportMode::Fail => Err(DomainError::Internal("import failed".into())),
            ImportMode::OtherInvalid => Err(DomainError::InvalidInput("bad folder layout".into())),
        }
    }
}

// ----- the rig -----

pub(crate) struct FakeFiles {
    files: Vec<(String, Vec<u8>)>,
    calls: Arc<AtomicUsize>,
}

/// An export service over fixed files, for rigs that need a `TransferService`.
pub(crate) fn export_of(files: Vec<(String, Vec<u8>)>) -> Arc<CollectionExportService> {
    Arc::new(CollectionExportService::new(Box::new(FakeFiles {
        files,
        calls: Arc::new(AtomicUsize::new(0)),
    })))
}

impl CollectionFileSource for FakeFiles {
    fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .files
            .iter()
            .map(|(path, bytes)| SourceFile {
                rel_path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect())
    }
}
/// A small collection whose ZIP is about 100 KiB, so it needs four chunks.
pub(crate) fn default_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        ("scripts/data.js".to_string(), noise(100 * 1024)),
    ]
}

pub(crate) struct Rig {
    pub svc: Arc<TransferService>,
    pub peers: Arc<FakePeers>,
    pub events: Arc<RecordingTransferEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
    pub staging: Arc<FakeStaging>,
    pub importer: Arc<FakeImporter>,
    export_calls: Arc<AtomicUsize>,
}

impl Rig {
    pub fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    /// How often the export read the collection's files.
    pub fn exports(&self) -> usize {
        self.export_calls.load(Ordering::SeqCst)
    }
}

pub(crate) fn rig() -> Rig {
    rig_with(default_files())
}

pub(crate) fn rig_with(files: Vec<(String, Vec<u8>)>) -> Rig {
    rig_timed(files, Timing::default())
}

/// A rig whose service runs with the given waits (the test seam of index ruling 3).
pub(crate) fn rig_timed(files: Vec<(String, Vec<u8>)>, timing: Timing) -> Rig {
    let peers = Arc::new(FakePeers::default());
    let events = Arc::new(RecordingTransferEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let staging = Arc::new(FakeStaging::default());
    let importer = Arc::new(FakeImporter::default());
    let export_calls = Arc::new(AtomicUsize::new(0));
    let export = Arc::new(CollectionExportService::new(Box::new(FakeFiles {
        files,
        calls: export_calls.clone(),
    })));
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: peers.clone(),
            export,
            importer: importer.clone(),
            staging: staging.clone(),
            events: events.clone(),
            audit: audit.clone(),
        },
        timing,
    );
    Rig {
        svc,
        peers,
        events,
        audit,
        staging,
        importer,
        export_calls,
    }
}

// ----- two rigs joined -----

/// Two rigs joined by a relay: frames that `a` sends to `b_id` reach `b` as coming from
/// `a_id`, and back. `drop` decides which frames from `a` to `b` are lost on the way.
pub(crate) struct Link<'r> {
    pub a: &'r Rig,
    pub a_id: DeviceId,
    pub b: &'r Rig,
    pub b_id: DeviceId,
    a_seen: usize,
    b_seen: usize,
}

impl<'r> Link<'r> {
    /// `a` knows `b_id` as `b_name` and `b` knows `a_id` as `a_name`; both are paired and online.
    pub fn new(
        a: &'r Rig,
        a_id: DeviceId,
        a_name: &str,
        b: &'r Rig,
        b_id: DeviceId,
        b_name: &str,
    ) -> Link<'r> {
        a.peers.add(b_id, b_name);
        b.peers.add(a_id, a_name);
        Link {
            a,
            a_id,
            b,
            b_id,
            a_seen: 0,
            b_seen: 0,
        }
    }

    /// Moves every frame sent since the last call, both ways. Returns how many moved.
    pub fn pump(&mut self, drop: &dyn Fn(&PeerMessage) -> bool) -> usize {
        let from_a = self.a.peers.sent_to(&self.b_id);
        let from_b = self.b.peers.sent_to(&self.a_id);
        let mut moved = 0;
        for frame in from_a.iter().skip(self.a_seen) {
            if !drop(frame) {
                self.b.svc.on_frame(self.a_id, frame.clone());
            }
            moved += 1;
        }
        for frame in from_b.iter().skip(self.b_seen) {
            self.a.svc.on_frame(self.b_id, frame.clone());
            moved += 1;
        }
        self.a_seen = from_a.len();
        self.b_seen = from_b.len();
        moved
    }

    /// Pumps on the real clock until `done` holds. Fails the test after about 5 seconds.
    pub async fn run_until(
        &mut self,
        drop: &dyn Fn(&PeerMessage) -> bool,
        done: impl Fn() -> bool,
    ) {
        for _ in 0..500 {
            self.pump(drop);
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the linked transfer did not end");
    }
}
