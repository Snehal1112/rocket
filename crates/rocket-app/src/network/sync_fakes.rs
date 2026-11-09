//! In-memory fakes of the sync ports, for the `SyncService` tests of Plan 3: links,
//! collections held in memory, events, rigs and the scripted peer that plays the other node
//! frame by frame.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_network::limits::{ACK_EVERY_CHUNKS, CHUNK_BYTES};
use rocket_network::link::{LinkId, LinkRecord, LinkState, Manifest};
use rocket_network::ports::LinkRepository;
use rocket_network::{
    chunk_count_for, DeviceId, PeerMessage, SyncAbortReason, SyncEvent, SyncFinish, SyncOutcome,
    SyncPayloadKind,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};

use super::frame_router::FrameRouter;
use super::sync::{Phase, SyncDeps, SyncService};
use super::sync_collections::plan_changes;
use super::sync_payload::pack_zip;
use super::sync_ports::{is_sync_entry, sha256_hex, FileChange, SyncCollectionPort, SyncFile};
use super::test_support::identity;
use super::transfer::{PeerSender, Timing, TransferDeps, TransferService};
pub(crate) use super::transfer_fakes::wait_for;
use super::transfer_fakes::{
    default_files, export_of, FakeImporter, FakePeers, FakeStaging, RecordingTransferEvents,
};

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn missing() -> DomainError {
    DomainError::NotFound("The collection is not there.".into())
}

struct FakeCollection {
    uid: Option<String>,
    /// The exported (scrubbed) view of each file.
    files: BTreeMap<String, Vec<u8>>,
}

/// Collections kept in memory. `apply` plans with the same code as the real port (so the
/// order, the merge and the case rule match) and swaps all files at once, so it is atomic.
/// `fail_next_apply` makes the next apply fail before any change. The fake stores merged bytes
/// and serves them as its export view, so a secret kept by a merge shows in `manifest` here
/// while the real port hashes the scrubbed export. Keep secrets out of fake fixtures.
#[derive(Default)]
pub(crate) struct FakeSyncCollections {
    state: Mutex<HashMap<String, FakeCollection>>,
    fail_apply: AtomicBool,
    applied: Mutex<Vec<(String, Vec<FileChange>)>>,
}

impl FakeSyncCollections {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&self, name: &str, uid: Option<&str>, files: &[(&str, &str)]) {
        let files = files
            .iter()
            .map(|(path, text)| (path.to_string(), text.as_bytes().to_vec()))
            .collect();
        locked(&self.state).insert(
            name.to_string(),
            FakeCollection {
                uid: uid.map(str::to_string),
                files,
            },
        );
    }

    pub(crate) fn set_file(&self, name: &str, path: &str, text: &str) {
        if let Some(c) = locked(&self.state).get_mut(name) {
            c.files.insert(path.to_string(), text.as_bytes().to_vec());
        }
    }

    pub(crate) fn remove_file(&self, name: &str, path: &str) {
        if let Some(c) = locked(&self.state).get_mut(name) {
            c.files.remove(path);
        }
    }

    pub(crate) fn file(&self, name: &str, path: &str) -> Option<Vec<u8>> {
        locked(&self.state)
            .get(name)
            .and_then(|c| c.files.get(path).cloned())
    }

    pub(crate) fn paths(&self, name: &str) -> Vec<String> {
        locked(&self.state)
            .get(name)
            .map(|c| c.files.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn fail_next_apply(&self) {
        self.fail_apply.store(true, Ordering::SeqCst);
    }

    pub(crate) fn applied(&self) -> Vec<(String, Vec<FileChange>)> {
        locked(&self.applied).clone()
    }

    fn manifest_of(files: &BTreeMap<String, Vec<u8>>) -> DomainResult<Manifest> {
        let mut manifest = Manifest::new();
        for (path, bytes) in files {
            manifest.insert(path, &sha256_hex(bytes))?;
        }
        Ok(manifest)
    }

    fn apply_inner(
        &self,
        collection: &str,
        expected: Option<&Manifest>,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        locked(&self.applied).push((collection.to_string(), changes.to_vec()));
        if self.fail_apply.swap(false, Ordering::SeqCst) {
            return Err(DomainError::Internal(
                "The fake apply was told to fail.".into(),
            ));
        }
        let mut state = locked(&self.state);
        let c = state.get_mut(collection).ok_or_else(missing)?;
        if let Some(expected) = expected {
            if &Self::manifest_of(&c.files)? != expected {
                return Err(DomainError::Conflict(
                    "The collection changed while the sync was running.".into(),
                ));
            }
        }
        // The fake has no other files on disk, so nothing else can hold a path.
        let overlay = plan_changes(&c.files, changes, &|_| Ok(false))?;
        for (path, entry) in overlay {
            match entry {
                Some(bytes) => {
                    c.files.insert(path, bytes);
                }
                None => {
                    c.files.remove(&path);
                }
            }
        }
        Ok(())
    }
}

impl SyncCollectionPort for FakeSyncCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(locked(&self.state).contains_key(collection))
    }

    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>> {
        let state = locked(&self.state);
        Ok(state.get(collection).ok_or_else(missing)?.uid.clone())
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        let state = locked(&self.state);
        state.get(collection).ok_or_else(missing)?;
        Ok(format!("/fake/{collection}"))
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        let state = locked(&self.state);
        Self::manifest_of(&state.get(collection).ok_or_else(missing)?.files)
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let state = locked(&self.state);
        let c = state.get(collection).ok_or_else(missing)?;
        paths
            .iter()
            .map(|path| match c.files.get(path) {
                Some(bytes) => Ok(SyncFile {
                    path: path.clone(),
                    bytes: bytes.clone(),
                }),
                None => Err(DomainError::NotFound(
                    "A requested file is not in the collection.".into(),
                )),
            })
            .collect()
    }

    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()> {
        self.apply_inner(collection, None, changes)
    }

    fn apply_if_unchanged(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        self.apply_inner(collection, Some(expected), changes)
    }
}

/// Links kept in memory with the delta rule of `LinkRepository::save`.
#[derive(Default)]
pub(crate) struct FakeLinkRepo {
    links: Mutex<HashMap<String, (LinkRecord, HashMap<String, Vec<u8>>)>>,
}

impl FakeLinkRepo {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl LinkRepository for FakeLinkRepo {
    fn list(&self) -> DomainResult<Vec<LinkRecord>> {
        let links = locked(&self.links);
        let mut records: Vec<LinkRecord> = links.values().map(|(r, _)| r.clone()).collect();
        records.sort_by(|a, b| a.link_id.as_str().cmp(b.link_id.as_str()));
        Ok(records)
    }

    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        Ok(locked(&self.links).get(id.as_str()).map(|(r, _)| r.clone()))
    }

    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
        // Every copy is checked before anything is written.
        for (path, bytes) in base_files {
            let expected = record.base.get(path).ok_or_else(|| {
                DomainError::InvalidInput("A base copy is not in the base.".into())
            })?;
            if sha256_hex(bytes) != expected {
                return Err(DomainError::InvalidInput(
                    "A base copy does not match its hash.".into(),
                ));
            }
        }
        let mut links = locked(&self.links);
        let mut kept = links
            .remove(record.link_id.as_str())
            .map(|(_, files)| files)
            .unwrap_or_default();
        kept.retain(|path, _| record.base.get(path).is_some());
        for (path, bytes) in base_files {
            kept.insert(path.clone(), bytes.clone());
        }
        links.insert(record.link_id.as_str().to_string(), (record.clone(), kept));
        Ok(())
    }

    fn delete(&self, id: &LinkId) -> DomainResult<()> {
        locked(&self.links).remove(id.as_str());
        Ok(())
    }

    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
        if !rocket_network::link::path_is_valid(path) {
            return Err(DomainError::InvalidInput(
                "The base path is not valid.".into(),
            ));
        }
        let links = locked(&self.links);
        let Some((record, files)) = links.get(id.as_str()) else {
            return Ok(None);
        };
        // A copy that no longer matches the base hash reads as none.
        Ok(files
            .get(path)
            .filter(|bytes| record.base.get(path) == Some(sha256_hex(bytes).as_str()))
            .cloned())
    }
}

// ----- the Plan 3 service fakes -----

/// The node under test, as its peer knows it.
pub(crate) const BO: u8 = 1;
/// The peer.
pub(crate) const ANA: u8 = 2;
/// A second peer.
pub(crate) const BEN: u8 = 3;
pub(crate) const ORDERS: &str = "orders";
pub(crate) const ROOT: &str = "/ws/collections/orders";
pub(crate) const UID: &str = "uid-orders";

pub(crate) type Files = BTreeMap<String, Vec<u8>>;

pub(crate) fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

pub(crate) fn lid(n: u8) -> LinkId {
    LinkId::parse(&format!("{n:032x}")).expect("a link id")
}

/// A payload transfer id the scripted peer uses: 32 lowercase hex characters.
pub(crate) fn ptid(n: u8) -> String {
    format!("f{n:031x}")
}

pub(crate) fn sha(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

pub(crate) fn files(list: &[(&str, &str)]) -> Files {
    list.iter()
        .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
        .collect()
}

pub(crate) fn manifest_of(files: &Files) -> Manifest {
    let mut m = Manifest::new();
    for (path, bytes) in files {
        m.insert(path, &sha(bytes)).expect("a valid test path");
    }
    m
}

pub(crate) fn zip_of(files: &Files) -> Vec<u8> {
    let list: Vec<SyncFile> = files
        .iter()
        .map(|(path, bytes)| SyncFile {
            path: path.clone(),
            bytes: bytes.clone(),
        })
        .collect();
    pack_zip(&list).expect("a zip")
}

/// Short waits, so the expiry and stall tests run in well under a second each.
pub(crate) fn fast() -> Timing {
    Timing {
        offer_ttl: Duration::from_millis(300),
        idle_limit: Duration::from_millis(300),
        result_timeout: Duration::from_millis(300),
    }
}

/// Long clocks for tests that move megabytes through the fakes in a debug build.
pub(crate) fn slow() -> Timing {
    Timing {
        offer_ttl: Duration::from_secs(10),
        idle_limit: Duration::from_secs(10),
        result_timeout: Duration::from_secs(10),
    }
}

/// Polls `check` until it is true. Fails the test after about 30 seconds.
pub(crate) async fn wait_long(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..3000 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

pub(crate) fn record(link: &LinkId, peer: u8, base: &Files) -> LinkRecord {
    LinkRecord {
        link_id: link.clone(),
        peer_device_id: dev(peer),
        collection_name: ORDERS.to_string(),
        collection_root: ROOT.to_string(),
        collection_uid: Some(UID.to_string()),
        base: manifest_of(base),
        state: LinkState::InSync,
        updated_at: "2026-11-08T10:00:00+00:00".to_string(),
    }
}

// ----- links -----

/// `FakeLinkRepo` (the R9 rules: delta, prune, hash and path checks) plus a save counter and a
/// switch that makes every save fail.
#[derive(Default)]
pub(crate) struct FakeLinks {
    repo: FakeLinkRepo,
    pub fail_save: AtomicBool,
    saves: AtomicUsize,
    get_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeLinks {
    pub fn record(&self, id: &LinkId) -> Option<LinkRecord> {
        self.repo.get(id).expect("get")
    }

    /// The stored base copies in path order.
    pub fn base_files(&self, id: &LinkId) -> Vec<(String, Vec<u8>)> {
        let links = locked(&self.repo.links);
        let mut out: Vec<(String, Vec<u8>)> = links
            .get(id.as_str())
            .map(|(_, f)| f.iter().map(|(p, b)| (p.clone(), b.clone())).collect())
            .unwrap_or_default();
        out.sort();
        out
    }

    pub fn count(&self) -> usize {
        locked(&self.repo.links).len()
    }

    pub fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }

    pub fn list_for_test(&self) -> Vec<LinkRecord> {
        self.list().expect("list")
    }

    /// Holds the next `get` until the returned sender sends or is dropped.
    pub fn hold_get(&self) -> std::sync::mpsc::Sender<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        *locked(&self.get_gate) = Some(rx);
        tx
    }
}

impl LinkRepository for FakeLinks {
    fn list(&self) -> DomainResult<Vec<LinkRecord>> {
        self.repo.list()
    }

    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        let gate = locked(&self.get_gate).take();
        if let Some(gate) = gate {
            // Callers run in a blocking task, so waiting here does not stall the runtime.
            let _ = gate.recv();
        }
        self.repo.get(id)
    }

    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(DomainError::Io("The link was not saved.".into()));
        }
        self.repo.save(record, base_files)?;
        self.saves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn delete(&self, id: &LinkId) -> DomainResult<()> {
        self.repo.delete(id)
    }

    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
        self.repo.read_base_file(id, path)
    }
}

// ----- collections -----

pub(crate) struct FakeColl {
    pub uid: Option<String>,
    pub root: String,
    pub files: Files,
}

/// Collections in memory. "Export" is the identity: a file's wire bytes are its bytes. Apply
/// plans with the real port's `plan_changes`, so the order (renames, deletes, puts), the YAML
/// merge of a `Put` and the case rule match the port. Only `apply_if_unchanged` applies; a
/// plain `apply` is counted and refused, because the service must never call it.
#[derive(Default)]
pub(crate) struct FakeCollections {
    colls: Mutex<HashMap<String, FakeColl>>,
    applied: Mutex<Vec<(String, Vec<FileChange>)>>,
    /// Every apply that passed the manifest check fails while this is set.
    pub fail_apply: AtomicBool,
    plain_applies: AtomicUsize,
    manifests: AtomicUsize,
    edit_after: Mutex<Option<(usize, String, Vec<u8>)>>,
    edit_before_apply: Mutex<Option<(String, Vec<u8>)>>,
    apply_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    manifest_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeCollections {
    pub fn add(&self, name: &str, files: Files) {
        let root = if name == ORDERS {
            ROOT.to_string()
        } else {
            format!("/ws/collections/{name}")
        };
        locked(&self.colls).insert(
            name.to_string(),
            FakeColl {
                uid: Some(UID.to_string()),
                root,
                files,
            },
        );
    }

    pub fn set_uid(&self, name: &str, uid: Option<&str>) {
        if let Some(c) = locked(&self.colls).get_mut(name) {
            c.uid = uid.map(str::to_string);
        }
    }

    pub fn set_root(&self, name: &str, root: &str) {
        if let Some(c) = locked(&self.colls).get_mut(name) {
            c.root = root.to_string();
        }
    }

    pub fn remove(&self, name: &str) {
        locked(&self.colls).remove(name);
    }

    pub fn files(&self, name: &str) -> Files {
        locked(&self.colls)
            .get(name)
            .map(|c| c.files.clone())
            .unwrap_or_default()
    }

    pub fn put(&self, name: &str, path: &str, bytes: &[u8]) {
        if let Some(c) = locked(&self.colls).get_mut(name) {
            c.files.insert(path.to_string(), bytes.to_vec());
        }
    }

    /// Every non-empty apply that passed its manifest check, in order.
    pub fn applied(&self) -> Vec<(String, Vec<FileChange>)> {
        locked(&self.applied).clone()
    }

    /// How often the plain `apply` was called. The service must leave it at zero.
    pub fn plain_applies(&self) -> usize {
        self.plain_applies.load(Ordering::SeqCst)
    }

    pub fn manifest_calls(&self) -> usize {
        self.manifests.load(Ordering::SeqCst)
    }

    /// The `n + 1`-th `manifest` call and every later read see `path` written: a local save
    /// made while the sync runs.
    pub fn edit_after_manifests(&self, n: usize, path: &str, bytes: &[u8]) {
        *locked(&self.edit_after) = Some((n, path.to_string(), bytes.to_vec()));
    }

    /// The next `apply_if_unchanged` sees `path` written just before its check: a local save
    /// that lands between the user's answer and the apply.
    pub fn edit_before_apply(&self, path: &str, bytes: &[u8]) {
        *locked(&self.edit_before_apply) = Some((path.to_string(), bytes.to_vec()));
    }

    /// Holds the next `manifest` until the returned sender sends or is dropped.
    pub fn hold_manifest(&self) -> std::sync::mpsc::Sender<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        *locked(&self.manifest_gate) = Some(rx);
        tx
    }

    /// Holds the next apply until the returned sender sends or is dropped.
    pub fn hold_apply(&self) -> std::sync::mpsc::Sender<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        *locked(&self.apply_gate) = Some(rx);
        tx
    }

    fn checked_apply(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        let gate = locked(&self.apply_gate).take();
        if let Some(gate) = gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        let edit = locked(&self.edit_before_apply).take();
        if let Some((path, bytes)) = edit {
            self.put(collection, &path, &bytes);
        }
        let mut colls = locked(&self.colls);
        let c = colls.get_mut(collection).ok_or_else(missing)?;
        if &synced_manifest(&c.files)? != expected {
            return Err(DomainError::Conflict(
                "The collection changed while the sync was running.".into(),
            ));
        }
        if changes.is_empty() {
            return Ok(());
        }
        locked(&self.applied).push((collection.to_string(), changes.to_vec()));
        if self.fail_apply.load(Ordering::SeqCst) {
            return Err(DomainError::Io("The fake apply was told to fail.".into()));
        }
        // The fake has no other files on disk, so nothing else can hold a path.
        let overlay = plan_changes(&c.files, changes, &|_| Ok(false))?;
        for (path, entry) in overlay {
            match entry {
                Some(bytes) => {
                    c.files.insert(path, bytes);
                }
                None => {
                    c.files.remove(&path);
                }
            }
        }
        Ok(())
    }
}

/// The manifest of the files the sync carries, with the real port's filter (`is_sync_entry`).
fn synced_manifest(files: &Files) -> DomainResult<Manifest> {
    let mut manifest = Manifest::new();
    for (path, bytes) in files.iter().filter(|(p, _)| is_sync_entry(p)) {
        manifest.insert(path, &sha256_hex(bytes))?;
    }
    Ok(manifest)
}

impl SyncCollectionPort for FakeCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(locked(&self.colls).contains_key(collection))
    }

    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>> {
        Ok(locked(&self.colls)
            .get(collection)
            .ok_or_else(missing)?
            .uid
            .clone())
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        Ok(locked(&self.colls)
            .get(collection)
            .ok_or_else(missing)?
            .root
            .clone())
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        let gate = locked(&self.manifest_gate).take();
        if let Some(gate) = gate {
            let _ = gate.recv();
        }
        let n = self.manifests.fetch_add(1, Ordering::SeqCst);
        let edit = {
            let mut slot = locked(&self.edit_after);
            match slot.as_ref() {
                Some((after, _, _)) if n >= *after => slot.take(),
                _ => None,
            }
        };
        if let Some((_, path, bytes)) = edit {
            self.put(collection, &path, &bytes);
        }
        let colls = locked(&self.colls);
        synced_manifest(&colls.get(collection).ok_or_else(missing)?.files)
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let colls = locked(&self.colls);
        let c = colls.get(collection).ok_or_else(missing)?;
        paths
            .iter()
            .map(|p| {
                c.files
                    .get(p)
                    .map(|b| SyncFile {
                        path: p.clone(),
                        bytes: b.clone(),
                    })
                    .ok_or_else(|| {
                        DomainError::NotFound("A requested file is not in the collection.".into())
                    })
            })
            .collect()
    }

    fn apply(&self, _collection: &str, _changes: &[FileChange]) -> DomainResult<()> {
        self.plain_applies.fetch_add(1, Ordering::SeqCst);
        Err(DomainError::Internal(
            "The sync service must not call a plain apply.".into(),
        ))
    }

    fn apply_if_unchanged(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        self.checked_apply(collection, expected, changes)
    }
}

// ----- events -----

#[derive(Default)]
pub(crate) struct RecordingSyncEvents(Mutex<Vec<SyncEvent>>);

impl RecordingSyncEvents {
    pub fn all(&self) -> Vec<SyncEvent> {
        locked(&self.0).clone()
    }

    pub fn reviews(&self) -> Vec<SyncEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, SyncEvent::ReviewReady { .. }))
            .collect()
    }

    pub fn confirms(&self) -> Vec<SyncEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, SyncEvent::ConfirmNeeded { .. }))
            .collect()
    }

    /// `(sync_id, outcome, message)` of every `Finished`.
    pub fn finished(&self) -> Vec<(String, SyncFinish, Option<String>)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                SyncEvent::Finished {
                    sync_id,
                    outcome,
                    message,
                    ..
                } => Some((sync_id, outcome, message)),
                _ => None,
            })
            .collect()
    }

    pub fn link_changes(&self) -> usize {
        self.all()
            .iter()
            .filter(|e| matches!(e, SyncEvent::LinkChanged { .. }))
            .count()
    }
}

impl EventPublisher for RecordingSyncEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<SyncEvent>() {
            locked(&self.0).push(event.clone());
        }
    }
}

// ----- the rig -----

pub(crate) struct SyncRig {
    pub sync: Arc<SyncService>,
    pub transfer: Arc<TransferService>,
    pub router: Arc<FrameRouter>,
    pub peers: Arc<FakePeers>,
    pub events: Arc<RecordingSyncEvents>,
    pub transfer_events: Arc<RecordingTransferEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
    pub links: Arc<FakeLinks>,
    pub colls: Arc<FakeCollections>,
    pub importer: Arc<FakeImporter>,
    /// The transfer service's staging. The sync service has none (index R3).
    pub staging: Arc<FakeStaging>,
}

pub(crate) fn sync_rig() -> SyncRig {
    sync_rig_timed(fast())
}

/// A transfer service and a sync service over the same fake peers, wired as Plan 4 wires
/// them: the transfer service knows the sync service and the router is the sink.
pub(crate) fn sync_rig_timed(timing: Timing) -> SyncRig {
    let peers = Arc::new(FakePeers::default());
    let events = Arc::new(RecordingSyncEvents::default());
    let transfer_events = Arc::new(RecordingTransferEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let links = Arc::new(FakeLinks::default());
    let colls = Arc::new(FakeCollections::default());
    let importer = Arc::new(FakeImporter::default());
    let staging = Arc::new(FakeStaging::default());
    let transfer = TransferService::new_with_timing(
        TransferDeps {
            network: peers.clone(),
            export: export_of(default_files()),
            importer: importer.clone(),
            staging: staging.clone(),
            events: transfer_events.clone(),
            audit: audit.clone(),
        },
        timing,
    );
    let sync = SyncService::new_with_timing(
        SyncDeps {
            network: peers.clone(),
            links: links.clone(),
            collections: colls.clone(),
            events: events.clone(),
            audit: audit.clone(),
        },
        timing,
    );
    transfer.set_sync(sync.clone());
    let router = FrameRouter::new(transfer.clone(), sync.clone());
    peers.set_transfer_sink(router.clone());
    SyncRig {
        sync,
        transfer,
        router,
        peers,
        events,
        transfer_events,
        audit,
        links,
        colls,
        importer,
        staging,
    }
}

impl SyncRig {
    pub fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    /// A frame from `seed` arrives at this node.
    pub fn push(&self, seed: u8, message: PeerMessage) {
        use rocket_network::TransferSink as _;
        self.router.on_frame(dev(seed), message);
    }

    pub fn sent(&self, seed: u8) -> Vec<PeerMessage> {
        self.peers.sent_to(&dev(seed))
    }

    pub fn aborts(&self, seed: u8) -> Vec<SyncAbortReason> {
        self.sent(seed)
            .into_iter()
            .filter_map(|m| match m {
                PeerMessage::SyncAbort { reason, .. } => Some(reason),
                _ => None,
            })
            .collect()
    }

    pub fn results(&self, seed: u8) -> Vec<SyncOutcome> {
        self.sent(seed)
            .into_iter()
            .filter_map(|m| match m {
                PeerMessage::SyncResult { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect()
    }

    /// Ana is paired and online, `orders` holds `mine`, and link `link` with Ana has base `base`.
    pub fn linked(&self, link: &LinkId, base: &Files, mine: &Files) {
        self.peers.add(dev(ANA), "Ana");
        self.colls.add(ORDERS, mine.clone());
        let base_files: Vec<(String, Vec<u8>)> =
            base.iter().map(|(p, b)| (p.clone(), b.clone())).collect();
        self.links
            .save(&record(link, ANA, base), &base_files)
            .expect("link saved");
    }
}

// ----- the scripted peer -----

/// The fields of one `SyncOffer` frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Offered {
    pub sync_id: String,
    pub link_id: String,
    pub base_hash: String,
    pub kind: SyncPayloadKind,
    pub transfer_id: String,
    pub size_bytes: u64,
    pub chunk_count: u32,
    pub sha256: String,
}

pub(crate) fn as_offer(m: &PeerMessage) -> Option<Offered> {
    match m {
        PeerMessage::SyncOffer {
            sync_id,
            link_id,
            base_hash,
            kind,
            transfer_id,
            size_bytes,
            chunk_count,
            sha256,
        } => Some(Offered {
            sync_id: sync_id.clone(),
            link_id: link_id.clone(),
            base_hash: base_hash.clone(),
            kind: *kind,
            transfer_id: transfer_id.clone(),
            size_bytes: *size_bytes,
            chunk_count: *chunk_count,
            sha256: sha256.clone(),
        }),
        _ => None,
    }
}

pub(crate) fn offer_frame(o: &Offered) -> PeerMessage {
    PeerMessage::SyncOffer {
        sync_id: o.sync_id.clone(),
        link_id: o.link_id.clone(),
        base_hash: o.base_hash.clone(),
        kind: o.kind,
        transfer_id: o.transfer_id.clone(),
        size_bytes: o.size_bytes,
        chunk_count: o.chunk_count,
        sha256: o.sha256.clone(),
    }
}

/// The offer the scripted peer makes for `bytes`.
pub(crate) fn offered(
    sync_id: &str,
    link: &LinkId,
    base_hash: &str,
    kind: SyncPayloadKind,
    transfer_id: &str,
    bytes: &[u8],
) -> Offered {
    Offered {
        sync_id: sync_id.to_string(),
        link_id: link.as_str().to_string(),
        base_hash: base_hash.to_string(),
        kind,
        transfer_id: transfer_id.to_string(),
        size_bytes: bytes.len() as u64,
        chunk_count: chunk_count_for(bytes.len() as u64),
        sha256: sha(bytes),
    }
}

pub(crate) fn chunk_frames(transfer_id: &str, bytes: &[u8]) -> Vec<PeerMessage> {
    bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(seq, part)| PeerMessage::Chunk {
            transfer_id: transfer_id.to_string(),
            seq: seq as u32,
            data: STANDARD.encode(part),
        })
        .collect()
}

/// The `n`-th (from 0) `SyncOffer` of `kind` this node sent to `seed`.
pub(crate) async fn nth_offer(rig: &SyncRig, seed: u8, kind: SyncPayloadKind, n: usize) -> Offered {
    let mut found = None;
    wait_for("a sync offer", || {
        found = rig
            .sent(seed)
            .iter()
            .filter_map(as_offer)
            .filter(|o| o.kind == kind)
            .nth(n);
        found.is_some()
    })
    .await;
    found.expect("an offer")
}

/// Plays the receiving peer of one payload this node offers: answers, takes every chunk,
/// acks as the real receiver does and returns the bytes.
pub(crate) async fn take_payload(
    rig: &SyncRig,
    seed: u8,
    kind: SyncPayloadKind,
    n: usize,
) -> (Offered, Vec<u8>) {
    let offer = nth_offer(rig, seed, kind, n).await;
    rig.push(
        seed,
        PeerMessage::OfferAnswer {
            transfer_id: offer.transfer_id.clone(),
            accept: true,
        },
    );
    let mut acked = 0u32;
    let mut seen = 0usize;
    let mut chunks: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        // Only the frames sent since the last poll are copied.
        let fresh = rig.peers.sent_from(seen);
        seen += fresh.len();
        chunks.extend(fresh.into_iter().filter_map(|(to, m)| match m {
            PeerMessage::Chunk {
                transfer_id, data, ..
            } if to == dev(seed) && transfer_id == offer.transfer_id => Some(data),
            _ => None,
        }));
        let got = chunks.len() as u32;
        let ack_to = if got == offer.chunk_count {
            got
        } else {
            got - got % ACK_EVERY_CHUNKS
        };
        if ack_to > acked {
            rig.push(
                seed,
                PeerMessage::ChunkAck {
                    transfer_id: offer.transfer_id.clone(),
                    next_seq: ack_to,
                },
            );
            acked = ack_to;
        }
        if got == offer.chunk_count {
            let mut bytes = Vec::new();
            for data in chunks {
                bytes.extend(STANDARD.decode(data).expect("base64"));
            }
            return (offer, bytes);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the payload did not arrive: {:?}", rig.events.finished());
}

/// Plays the sending peer of one payload: offers, waits for the answer, sends every chunk.
pub(crate) async fn give_payload(rig: &SyncRig, seed: u8, offer: &Offered, bytes: &[u8]) {
    rig.push(seed, offer_frame(offer));
    wait_for("the answer to the payload", || {
        rig.sent(seed).iter().any(|m| {
            matches!(m, PeerMessage::OfferAnswer { transfer_id, accept: true }
                if *transfer_id == offer.transfer_id)
        })
    })
    .await;
    for chunk in chunk_frames(&offer.transfer_id, bytes) {
        rig.push(seed, chunk);
    }
}

// ----- two nodes -----

/// Two rigs joined by a relay: Bo (`a`) and Ana (`b`). Frames `a` sends to Ana reach `b`
/// from Bo, and back. `drop` decides which frames from `a` to `b` are lost.
pub(crate) struct SyncPair {
    pub a: SyncRig,
    pub b: SyncRig,
    a_seen: usize,
    b_seen: usize,
}

impl SyncPair {
    pub fn new() -> SyncPair {
        let a = sync_rig();
        let b = sync_rig();
        a.peers.add(dev(ANA), "Ana");
        b.peers.add(dev(BO), "Bo");
        SyncPair {
            a,
            b,
            a_seen: 0,
            b_seen: 0,
        }
    }

    /// Both hold `orders`, linked by `link` with the same base; Bo has `bo`, Ana has `ana`.
    pub fn linked(link: &LinkId, base: &Files, bo: &Files, ana: &Files) -> SyncPair {
        let pair = SyncPair::new();
        let base_files: Vec<(String, Vec<u8>)> =
            base.iter().map(|(p, b)| (p.clone(), b.clone())).collect();
        pair.a.colls.add(ORDERS, bo.clone());
        pair.b.colls.add(ORDERS, ana.clone());
        pair.a
            .links
            .save(&record(link, ANA, base), &base_files)
            .expect("a link");
        pair.b
            .links
            .save(&record(link, BO, base), &base_files)
            .expect("b link");
        pair
    }

    pub fn pump(&mut self, drop: &dyn Fn(&PeerMessage) -> bool) -> usize {
        let from_a = self.a.sent(ANA);
        let from_b = self.b.sent(BO);
        let mut moved = 0;
        for frame in from_a.iter().skip(self.a_seen) {
            if !drop(frame) {
                self.b.push(BO, frame.clone());
            }
            moved += 1;
        }
        for frame in from_b.iter().skip(self.b_seen) {
            self.a.push(ANA, frame.clone());
            moved += 1;
        }
        self.a_seen = from_a.len();
        self.b_seen = from_b.len();
        moved
    }

    /// Pumps on the real clock until `done` holds. Fails the test after about 5 seconds.
    pub async fn run_until(&mut self, done: impl Fn(&SyncPair) -> bool) {
        for _ in 0..500 {
            self.pump(&|_| false);
            if done(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the linked sync did not reach the expected state");
    }
}

// ----- flow helpers shared by the Task 2 and Task 3 tests -----

/// The link every flow test uses.
pub(crate) const L: u8 = 1;

pub(crate) fn base() -> Files {
    files(&[
        ("opencollection.yml", "name: orders\n"),
        ("a.yml", "url: a\n"),
        ("b.yml", "url: b\n"),
    ])
}

pub(crate) fn with(f: &Files, path: &str, text: &str) -> Files {
    let mut f = f.clone();
    f.insert(path.to_string(), text.as_bytes().to_vec());
    f
}

pub(crate) fn without(f: &Files, path: &str) -> Files {
    let mut f = f.clone();
    f.remove(path);
    f
}

pub(crate) fn bh() -> String {
    manifest_of(&base()).base_hash()
}

pub(crate) fn empty_plan() -> rocket_network::SyncPlanPayload {
    rocket_network::SyncPlanPayload {
        wanted: vec![],
        deletes: vec![],
        renames: vec![],
        decisions: vec![],
        sends: vec![],
        new_base_hash: None,
    }
}

pub(crate) fn request(wanted: &[&str]) -> rocket_network::SyncPlanPayload {
    let mut p = empty_plan();
    p.wanted = wanted.iter().map(|s| s.to_string()).collect();
    p
}

/// A final plan for a starter that offered `offered`, and the files the starter has after it.
/// The new base hash is the one the starter must reach.
pub(crate) fn final_plan(
    offered: &Files,
    deletes: &[&str],
    renames: &[(&str, &str)],
    sends: &[(&str, &str)],
) -> (rocket_network::SyncPlanPayload, Files) {
    let mut p = empty_plan();
    p.deletes = deletes.iter().map(|s| s.to_string()).collect();
    p.renames = renames
        .iter()
        .map(|(from, to)| rocket_network::PlanRename {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect();
    p.sends = sends
        .iter()
        .map(|(path, text)| rocket_network::PlanFile {
            path: path.to_string(),
            sha256: sha(text.as_bytes()),
        })
        .collect();
    let mut after = offered.clone();
    for d in deletes {
        after.remove(*d);
    }
    for (from, to) in renames {
        if let Some(bytes) = after.remove(*from) {
            after.insert(to.to_string(), bytes);
        }
    }
    for (path, text) in sends {
        after.insert(path.to_string(), text.as_bytes().to_vec());
    }
    p.new_base_hash = Some(manifest_of(&after).base_hash());
    (p, after)
}

/// Bo starts the sync of link `L`; the scripted Ana takes the manifest.
pub(crate) async fn bo_starts(rig: &SyncRig) -> (rocket_network::SyncId, Offered, Vec<u8>) {
    let sid = rig.sync.start(&lid(L)).await.expect("started");
    let (offer, bytes) = take_payload(rig, ANA, SyncPayloadKind::Manifest, 0).await;
    (sid, offer, bytes)
}

pub(crate) async fn ana_sends_plan(
    rig: &SyncRig,
    sid: &str,
    n: u8,
    plan: &rocket_network::SyncPlanPayload,
) {
    let bytes = plan.to_json().expect("json");
    let offer = offered(sid, &lid(L), &bh(), SyncPayloadKind::Plan, &ptid(n), &bytes);
    give_payload(rig, ANA, &offer, &bytes).await;
}

pub(crate) async fn ana_sends_files(rig: &SyncRig, sid: &str, n: u8, zip: &[u8]) {
    let offer = offered(sid, &lid(L), &bh(), SyncPayloadKind::Files, &ptid(n), zip);
    give_payload(rig, ANA, &offer, zip).await;
}

/// The scripted Ana starts a sync with Bo as the planner. Returns the sync id.
pub(crate) async fn ana_starts(rig: &SyncRig, theirs: &Files) -> rocket_network::SyncId {
    let sid = rocket_network::SyncId::random();
    let bytes = manifest_of(theirs).to_json().expect("json");
    let offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &bytes,
    );
    give_payload(rig, ANA, &offer, &bytes).await;
    sid
}

pub(crate) async fn wait_finished(rig: &SyncRig) -> (SyncFinish, Option<String>) {
    wait_for("the finished event", || !rig.events.finished().is_empty()).await;
    let (_, finish, message) = rig.events.finished().remove(0);
    (finish, message)
}

pub(crate) async fn wait_phase(
    rig: &SyncRig,
    sid: &rocket_network::SyncId,
    phase: super::sync::Phase,
) {
    wait_for("the phase", || rig.sync.phase_of(sid) == Some(phase)).await;
}

pub(crate) async fn wait_link(rig: &SyncRig, check: impl Fn(&LinkRecord) -> bool) -> LinkRecord {
    wait_for("the link update", || {
        rig.links.record(&lid(L)).is_some_and(|r| check(&r))
    })
    .await;
    rig.links.record(&lid(L)).expect("the link")
}

/// Bo starts, Ana answers with a final plan that sends b2 and n, and Bo confirms.
pub(crate) async fn starter_confirmed(rig: &SyncRig) -> (rocket_network::SyncId, Files) {
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(rig).await;
    let sends = [("b.yml", "url: b2\n"), ("n.yml", "url: n\n")];
    let (plan, after) = final_plan(&base(), &[], &[], &sends);
    ana_sends_plan(rig, sid.as_str(), 2, &plan).await;
    ana_sends_files(rig, sid.as_str(), 3, &zip_of(&files(&sends))).await;
    wait_phase(rig, &sid, Phase::AwaitConfirm).await;
    rig.sync.confirm(&sid, true).await.expect("confirmed");
    (sid, after)
}

/// Bo (planner) renamed b.yml to b2.yml and added n.yml; Ana deleted a.yml. Runs to the
/// state where Bo waits for the starter. Returns the sync id and Bo's files after the sync.
pub(crate) async fn planner_waiting(rig: &SyncRig) -> (rocket_network::SyncId, Files) {
    let mut mine = without(&base(), "b.yml");
    mine.insert("b2.yml".into(), b"url: b\n".to_vec());
    let mine = with(&mine, "n.yml", "url: n\n");
    rig.linked(&lid(L), &base(), &mine);
    let sid = ana_starts(rig, &without(&base(), "a.yml")).await;
    wait_phase(rig, &sid, Phase::AwaitReview).await;
    rig.sync.review(&sid, true, vec![]).await.expect("accepted");
    (sid, without(&mine, "a.yml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_network::link::{LinkId, LinkRecord, LinkState, Manifest};
    use rocket_network::ports::LinkRepository;
    use rocket_network::DeviceId;
    use rocket_shared::error::DomainError;

    use crate::network::sync_ports::{sha256_hex, FileChange, SyncCollectionPort};

    #[test]
    fn the_fake_apply_merges_like_the_real_one_and_is_atomic() {
        let fake = FakeSyncCollections::new();
        fake.add(
            "c",
            Some("u"),
            &[
                ("a.yml", "http:\n  url: https://a\nauth:\n  token: s\n"),
                ("b.yml", "x: 1\n"),
            ],
        );
        let received = b"http:\n  url: https://b\nauth:\n  token: ''\n".to_vec();
        fake.apply(
            "c",
            &[FileChange::Put {
                path: "a.yml".into(),
                bytes: received,
            }],
        )
        .expect("applies");
        let text = String::from_utf8(fake.file("c", "a.yml").expect("file")).expect("utf8");
        assert!(text.contains("https://b") && text.contains("token: s"));

        let before = fake.file("c", "b.yml");
        let failing = [
            FileChange::Delete {
                path: "b.yml".into(),
            },
            FileChange::Rename {
                from: "gone.yml".into(),
                to: "z.yml".into(),
            },
        ];
        assert!(fake.apply("c", &failing).is_err());
        assert_eq!(fake.file("c", "b.yml"), before);

        fake.fail_next_apply();
        assert!(fake
            .apply(
                "c",
                &[FileChange::Delete {
                    path: "b.yml".into()
                }]
            )
            .is_err());
        assert!(fake.file("c", "b.yml").is_some());
        fake.apply(
            "c",
            &[FileChange::Delete {
                path: "b.yml".into(),
            }],
        )
        .expect("applies");
        assert!(fake.file("c", "b.yml").is_none());
        assert_eq!(fake.applied().len(), 4);
    }

    #[test]
    fn the_fake_apply_follows_the_real_order_and_case_rule() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("Users/a.yml", "a: 1\n"), ("b.yml", "b: 1\n")]);
        let err = fake
            .apply(
                "c",
                &[FileChange::Put {
                    path: "users/a.yml".into(),
                    bytes: b"a: 2\n".to_vec(),
                }],
            )
            .expect_err("refused");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
        fake.apply(
            "c",
            &[
                FileChange::Delete {
                    path: "b.yml".into(),
                },
                FileChange::Rename {
                    from: "b.yml".into(),
                    to: "moved/b.yml".into(),
                },
            ],
        )
        .expect("renames run before deletes");
        assert_eq!(fake.paths("c"), vec!["Users/a.yml", "moved/b.yml"]);
    }

    #[test]
    fn the_fake_apply_if_unchanged_refuses_a_changed_manifest() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("a.yml", "a: 1\n")]);
        let expected = fake.manifest("c").expect("manifest");
        fake.set_file("c", "a.yml", "a: 2\n");
        let err = fake
            .apply_if_unchanged(
                "c",
                &expected,
                &[FileChange::Delete {
                    path: "a.yml".into(),
                }],
            )
            .expect_err("conflict");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert!(fake.file("c", "a.yml").is_some());
        let fresh = fake.manifest("c").expect("manifest");
        fake.apply_if_unchanged(
            "c",
            &fresh,
            &[FileChange::Delete {
                path: "a.yml".into(),
            }],
        )
        .expect("applies");
        assert!(fake.file("c", "a.yml").is_none());
    }

    #[test]
    fn the_fake_manifest_hashes_the_stored_bytes() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("a.yml", "a: 1\n")]);
        let manifest = fake.manifest("c").expect("manifest");
        assert_eq!(manifest.get("a.yml"), Some(sha256_hex(b"a: 1\n").as_str()));
        assert!(!fake.exists("nope").expect("exists"));
        assert!(matches!(
            fake.manifest("nope"),
            Err(DomainError::NotFound(_))
        ));
    }

    fn record(id: &LinkId, files: &[(&str, &[u8])]) -> LinkRecord {
        let mut base = Manifest::new();
        for (path, bytes) in files {
            base.insert(path, &sha256_hex(bytes)).expect("valid entry");
        }
        LinkRecord {
            link_id: id.clone(),
            peer_device_id: DeviceId::from_bytes([7u8; 32]),
            collection_name: "c".into(),
            collection_root: "/c".into(),
            collection_uid: None,
            base,
            state: LinkState::InSync,
            updated_at: "2026-11-08T10:00:00Z".into(),
        }
    }

    #[test]
    fn the_fake_link_repo_keeps_a_delta_and_prunes() {
        let repo = FakeLinkRepo::new();
        let id = LinkId::random();
        let with_a = record(&id, &[("a.yml", b"one"), ("b.yml", b"two")]);
        repo.save(&with_a, &[("a.yml".to_string(), b"one".to_vec())])
            .expect("saves");
        // A state-only save keeps the copy.
        repo.save(&with_a, &[]).expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"one".to_vec())
        );
        assert_eq!(repo.read_base_file(&id, "b.yml").expect("reads"), None);
        // A record without the path prunes the copy.
        let without_a = record(&id, &[("b.yml", b"two")]);
        repo.save(&without_a, &[]).expect("saves");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
        assert_eq!(repo.list().expect("lists"), vec![without_a.clone()]);
        assert_eq!(repo.get(&id).expect("gets"), Some(without_a));
        repo.delete(&id).expect("deletes");
        assert_eq!(repo.get(&id).expect("gets"), None);
    }

    #[test]
    fn the_fake_link_repo_validates_before_it_writes() {
        let repo = FakeLinkRepo::new();
        let id = LinkId::random();
        let rec = record(&id, &[("a.yml", b"one"), ("b.yml", b"two")]);
        // The second copy does not match its hash, so the first is not written either.
        let err = repo
            .save(
                &rec,
                &[
                    ("a.yml".to_string(), b"one".to_vec()),
                    ("b.yml".to_string(), b"wrong".to_vec()),
                ],
            )
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!err.to_string().contains("b.yml"));
        assert_eq!(repo.get(&id).expect("gets"), None);
        // A path outside the base is refused.
        let err = repo
            .save(&rec, &[("c.yml".to_string(), b"one".to_vec())])
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // A path that is not valid is refused, like the real repository.
        let err = repo.read_base_file(&id, "../a.yml").expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // A copy whose hash no longer matches reads as none.
        repo.save(&rec, &[("a.yml".to_string(), b"one".to_vec())])
            .expect("saves");
        let changed = record(&id, &[("a.yml", b"changed")]);
        repo.save(&changed, &[]).expect("saves");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }
}
