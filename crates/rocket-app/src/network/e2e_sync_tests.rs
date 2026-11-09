//! End-to-end sync tests: two real nodes over real TLS on 127.0.0.1, each with a real
//! `TransferService`, `SyncService`, `FrameRouter`, `FsLinkRepo` and `FsTransferStaging`.
//!
//! Test-only fakes: `MemCollections` (the collection port, over the real export and the real
//! file merge through `plan_changes`), `SyncImport` (the import of the first send) and
//! `WireTap` (a sender that can damage one payload kind). The first-send tests at the end run
//! the real disk stack instead: the real `ExportBackedSyncCollections` and the real Bruno
//! import, as the shipped import adapter runs it. The wiring order mirrors
//! `src-tauri/src/lib.rs`.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use rocket_audit::AuditEventKind;
use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_import::ImportService;
use rocket_infra::{
    FsCollectionFileSource, FsCollectionRepo, FsEnvironmentRepo, FsFileTxFactory, FsImportFiles,
    FsLinkRepo, FsTransferStaging, SharedPathCollectionRepo,
};
use rocket_network::{
    ConflictKind, ConflictView, Decision, DeviceId, Direction, LinkId, LinkRecord, LinkRepository,
    LinkState, Manifest, Outcome, PeerMessage, Resolution, SyncEvent, SyncFinish, SyncId,
    SyncPayloadKind, SyncRole, SyncSummary, TransferEvent, TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::e2e_tests::{online_pair, wait_until, E2eNode};
use super::e2e_transfer_tests::{count_files, file, normal};
use super::sync_collections::plan_changes;
use super::sync_ports::{base_from_reader, FileChange, SyncCollectionPort, SyncFile};
use super::test_support::RecordingEvents;
use super::{
    CollectionImportPort, ExportBackedSyncCollections, FrameRouter, ImportedCollection,
    NetworkHandle, NetworkService, PeerSender, SyncDeps, SyncService, TransferDeps,
    TransferService, TransferStaging, UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

// ----- fixtures -----

const O: &str = "Orders";
const GET: &str = "users/get.yml";
const LIST: &str = "orders/list.yml";

const URL0: &str = "https://api.example.test/users";
const URL_A: &str = "https://api.example.test/users-from-ann";
const URL_B: &str = "https://api.example.test/users-from-ben";
const LIST0: &str = "https://api.example.test/orders";
const LIST_B: &str = "https://api.example.test/orders-from-ben";

const A_TOKEN: &str = "A-TOKEN-91c4d2";
const B_TOKEN: &str = "B-TOKEN-5e21aa";
const A_UID: &str = "a-uid-12e0";
const B_UID: &str = "b-uid-77c3";
const B_EXAMPLE: &str = "b-example-body-3f9a";

/// Keys only A keeps: a uid.
const A_LOCAL: &str = "uid: a-uid-12e0\n";
/// Keys only B keeps: a uid, a Rocket-only extension and a saved example. The export removes
/// all three, so they never change a hash.
const B_LOCAL: &str = "uid: b-uid-77c3\nextensions:\n  rocketapi:\n    sandboxMode: developer\nexamples:\n  - name: ok\n    response:\n      body:\n        data: b-example-body-3f9a\n";

fn get_yaml(url: &str, token: &str, local: &str) -> String {
    format!(
        "info:\n  name: get\nhttp:\n  method: GET\n  url: {url}\n  headers:\n    - name: Authorization\n      value: Bearer {token}\n{local}"
    )
}

fn list_yaml(url: &str) -> String {
    format!("info:\n  name: list\nhttp:\n  method: GET\n  url: {url}\n")
}

/// The same four files on both sides. Only the secrets and the local-only keys differ.
fn collection(token: &str, local: &str) -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: \"Orders\"\n",
        ),
        file("users/folder.yml", "info:\n  name: users\n"),
        file(GET, &get_yaml(URL0, token, local)),
        file(LIST, &list_yaml(LIST0)),
    ]
}

// ----- fakes -----

/// Collections held in memory: name to (relative path to bytes). The bytes are the node's own
/// files, local secrets and keys included.
#[derive(Default)]
struct MemCollections {
    files: Mutex<BTreeMap<String, BTreeMap<String, Vec<u8>>>>,
}

/// A fixed file list that ignores the collection name, for one export run.
struct Fixed(Vec<SourceFile>);

impl CollectionFileSource for Fixed {
    fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
        Ok(self.0.clone())
    }
}

/// Reads the live files of a `MemCollections`, for the `TransferService` export.
struct ColsSource(Arc<MemCollections>);

impl CollectionFileSource for ColsSource {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.0.source_files(collection)
    }
}

impl MemCollections {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, BTreeMap<String, Vec<u8>>>> {
        self.files.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn seed(&self, name: &str, files: Vec<SourceFile>) {
        let map = files.into_iter().map(|f| (f.rel_path, f.bytes)).collect();
        self.lock().insert(name.to_string(), map);
    }

    fn names(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    fn source_files(&self, name: &str) -> DomainResult<Vec<SourceFile>> {
        let all = self.lock();
        let map = all
            .get(name)
            .ok_or_else(|| DomainError::NotFound(name.to_string()))?;
        Ok(map
            .iter()
            .map(|(path, bytes)| SourceFile {
                rel_path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect())
    }

    fn text(&self, name: &str, path: &str) -> Option<String> {
        self.lock()
            .get(name)
            .and_then(|m| m.get(path))
            .map(|b| String::from_utf8_lossy(b).to_string())
    }

    fn paths(&self, name: &str) -> Vec<String> {
        self.lock()
            .get(name)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn write(&self, name: &str, path: &str, text: &str) {
        self.lock()
            .entry(name.to_string())
            .or_default()
            .insert(path.to_string(), text.as_bytes().to_vec());
    }

    fn remove(&self, name: &str, path: &str) {
        if let Some(map) = self.lock().get_mut(name) {
            map.remove(path);
        }
    }

    fn rename(&self, name: &str, from: &str, to: &str) {
        if let Some(map) = self.lock().get_mut(name) {
            if let Some(bytes) = map.remove(from) {
                map.insert(to.to_string(), bytes);
            }
        }
    }

    /// The scrubbed export of the collection as ZIP bytes (the real export).
    fn export_zip(&self, name: &str) -> DomainResult<Vec<u8>> {
        let files = self.source_files(name)?;
        let service = CollectionExportService::new(Box::new(Fixed(files)));
        Ok(service.export_bruno(name, false)?.bytes)
    }

    /// The sync view of the export: the same filter and root rule the real port uses.
    fn synced(&self, name: &str) -> DomainResult<(Manifest, Vec<(String, Vec<u8>)>)> {
        let base = base_from_reader(Cursor::new(self.export_zip(name)?))?;
        Ok((base.manifest, base.files))
    }
}

impl SyncCollectionPort for MemCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(self.lock().contains_key(collection))
    }

    fn collection_uid(&self, _collection: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        if !self.lock().contains_key(collection) {
            return Err(DomainError::NotFound(collection.to_string()));
        }
        Ok(format!("/mem/{collection}"))
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        self.synced(collection).map(|(manifest, _)| manifest)
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let (_, files) = self.synced(collection)?;
        let by_path: BTreeMap<String, Vec<u8>> = files.into_iter().collect();
        paths
            .iter()
            .map(|path| match by_path.get(path) {
                Some(bytes) => Ok(SyncFile {
                    path: path.clone(),
                    bytes: bytes.clone(),
                }),
                None => Err(DomainError::NotFound("a wanted file".to_string())),
            })
            .collect()
    }

    fn apply(&self, _collection: &str, _changes: &[FileChange]) -> DomainResult<()> {
        Err(DomainError::Internal(
            "The sync must not call a plain apply.".to_string(),
        ))
    }

    fn apply_if_unchanged(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        // One lock for the check and the write, like the collection lock of the real port.
        let mut all = self.lock();
        let local = all
            .get(collection)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(collection.to_string()))?;
        let files: Vec<SourceFile> = local
            .iter()
            .map(|(path, bytes)| SourceFile {
                rel_path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect();
        let zip =
            CollectionExportService::new(Box::new(Fixed(files))).export_bruno(collection, false)?;
        if &base_from_reader(Cursor::new(zip.bytes))?.manifest != expected {
            return Err(DomainError::Conflict(
                "The collection changed while the sync was running.".to_string(),
            ));
        }
        // The real merge, order and case rules. Nothing else sits on a memory disk.
        let overlay = plan_changes(&local, changes, &|_| Ok(false))?;
        let mut working = local;
        for (path, entry) in overlay {
            match entry {
                Some(bytes) => working.insert(path, bytes),
                None => working.remove(&path),
            };
        }
        // All or nothing: the working copy replaces the collection only when every change ran.
        all.insert(collection.to_string(), working);
        Ok(())
    }
}

/// The import of the first send. It runs the real ZIP validator, then seeds the collection.
struct SyncImport {
    cols: Arc<MemCollections>,
}

impl CollectionImportPort for SyncImport {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.cols.names())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        rocket_import::transfer_zip::validate(&FsImportFiles, zip_path)
            .map_err(|_| DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.to_string()))?;
        let bytes = std::fs::read(zip_path).map_err(|e| DomainError::Internal(e.to_string()))?;
        let files = super::e2e_transfer_tests::zip_entries(&bytes)
            .into_iter()
            .filter_map(|(entry, data)| {
                entry.split_once('/').map(|(_, rest)| SourceFile {
                    rel_path: rest.to_string(),
                    bytes: data,
                })
            })
            .collect();
        self.cols.seed(display_name, files);
        Ok(ImportedCollection {
            name: display_name.to_string(),
        })
    }
}

/// A sender that can damage the first chunk of one payload kind. It sits between the
/// `SyncService` and the real `NetworkService`, so the receiver sees what a hostile hop would
/// deliver.
struct WireTap {
    inner: Arc<NetworkService>,
    kinds: Mutex<HashMap<String, SyncPayloadKind>>,
    tamper: Mutex<Option<SyncPayloadKind>>,
}

impl WireTap {
    fn new(inner: Arc<NetworkService>) -> Arc<WireTap> {
        Arc::new(WireTap {
            inner,
            kinds: Mutex::new(HashMap::new()),
            tamper: Mutex::new(None),
        })
    }

    /// Damages chunk 0 of every payload of this kind from now on.
    fn tamper(&self, kind: Option<SyncPayloadKind>) {
        *self.tamper.lock().unwrap_or_else(|e| e.into_inner()) = kind;
    }

    fn maybe_tamper(&self, message: PeerMessage) -> PeerMessage {
        if let PeerMessage::SyncOffer {
            transfer_id, kind, ..
        } = &message
        {
            self.kinds
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(transfer_id.clone(), *kind);
            return message;
        }
        if let PeerMessage::Chunk {
            transfer_id,
            seq,
            data,
        } = &message
        {
            let target = *self.tamper.lock().unwrap_or_else(|e| e.into_inner());
            let kind = self
                .kinds
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(transfer_id)
                .copied();
            if *seq == 0 && target.is_some() && kind == target {
                return PeerMessage::Chunk {
                    transfer_id: transfer_id.clone(),
                    seq: *seq,
                    data: flip_first_char(data),
                };
            }
        }
        message
    }
}

/// Changes the first base64 character, so the first decoded byte changes.
fn flip_first_char(data: &str) -> String {
    let mut chars: Vec<char> = data.chars().collect();
    if let Some(first) = chars.first_mut() {
        *first = if *first == 'A' { 'B' } else { 'A' };
    }
    chars.into_iter().collect()
}

#[async_trait]
impl PeerSender for WireTap {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let message = self.maybe_tamper(message);
        PeerSender::send_to(self.inner.as_ref(), device, message).await
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        PeerSender::peer_label(self.inner.as_ref(), device)
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        PeerSender::is_online(self.inner.as_ref(), device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        PeerSender::set_transfer_sink(self.inner.as_ref(), sink);
    }
}

// ----- the harness -----

/// The whole sync stack of one node on top of its real network service.
struct Stack {
    node: E2eNode,
    sync: Arc<SyncService>,
    transfer: Arc<TransferService>,
    links: Arc<FsLinkRepo>,
    wire: Arc<WireTap>,
    link_dir: TempDir,
    staging_dir: TempDir,
}

/// Builds the services in the order `lib.rs` uses: transfer service, sync service,
/// `set_sync`, router, then the router becomes the sink.
fn build_stack(
    node: E2eNode,
    export: Arc<CollectionExportService>,
    importer: Arc<dyn CollectionImportPort>,
    collections: Arc<dyn SyncCollectionPort>,
) -> Stack {
    let staging_dir = TempDir::new().expect("tempdir");
    let link_dir = TempDir::new().expect("tempdir");
    let staging = Arc::new(FsTransferStaging::new(staging_dir.path().to_path_buf()));
    staging.purge().expect("purge at start");
    let links = Arc::new(FsLinkRepo::new(link_dir.path().to_path_buf()));
    let wire = WireTap::new(Arc::clone(&node.svc));
    let transfer = TransferService::new_with_timing(
        TransferDeps {
            network: node.svc.clone(),
            export,
            importer,
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
        },
        normal(),
    );
    let sync = SyncService::new(SyncDeps {
        network: wire.clone(),
        links: links.clone(),
        collections,
        events: node.events.clone(),
        audit: node.audit.clone(),
    });
    transfer.set_sync(Arc::clone(&sync));
    let router = FrameRouter::new(Arc::clone(&transfer), Arc::clone(&sync));
    node.svc.set_transfer_sink(router);
    Stack {
        node,
        sync,
        transfer,
        links,
        wire,
        link_dir,
        staging_dir,
    }
}

impl Stack {
    fn id(&self) -> DeviceId {
        self.node.id()
    }

    fn events(&self) -> &RecordingEvents {
        &self.node.events
    }

    fn syncs(&self) -> Vec<SyncEvent> {
        self.events().syncs()
    }

    /// The review of this sync, if this node showed it.
    fn review_ready(&self, sid: &str) -> Option<(SyncSummary, Vec<ConflictView>, SyncRole)> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::ReviewReady {
                sync_id,
                summary,
                conflicts,
                role,
                ..
            } if sync_id == sid => Some((summary, conflicts, role)),
            _ => None,
        })
    }

    /// The confirm summary of this sync, if this node showed it.
    fn confirm_summary(&self, sid: &str) -> Option<SyncSummary> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::ConfirmNeeded {
                sync_id, summary, ..
            } if sync_id == sid => Some(summary),
            _ => None,
        })
    }

    /// How this sync ended on this node, if it ended.
    fn finish(&self, sid: &str) -> Option<(SyncFinish, Option<String>)> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::Finished {
                sync_id,
                outcome,
                message,
                ..
            } if sync_id == sid => Some((outcome, message)),
            _ => None,
        })
    }

    /// True once the link save that follows this sync's `Finished` is done: `Finished` is
    /// published first and the link is saved after it, then `LinkChanged` follows.
    fn settled(&self, sid: &str) -> bool {
        let events = self.syncs();
        let Some((at, link)) = events.iter().enumerate().find_map(|(i, e)| match e {
            SyncEvent::Finished {
                sync_id, link_id, ..
            } if sync_id == sid => Some((i, link_id.clone())),
            _ => None,
        }) else {
            return false;
        };
        events[at..]
            .iter()
            .any(|e| matches!(e, SyncEvent::LinkChanged { link_id } if *link_id == link))
    }

    fn link(&self, id: &LinkId) -> Option<LinkRecord> {
        self.links.get(id).expect("the link store reads")
    }

    fn link_count(&self) -> usize {
        self.links.list().map(|l| l.len()).unwrap_or(0)
    }

    /// The states `links()` shows. It blocks, so it runs off the test runtime.
    async fn link_states(&self) -> Vec<LinkState> {
        let sync = Arc::clone(&self.sync);
        tokio::task::spawn_blocking(move || sync.links())
            .await
            .expect("the links task")
            .expect("links")
            .into_iter()
            .map(|view| view.state)
            .collect()
    }

    /// Both sides of a linked send finished `Completed` on this node in `direction`.
    fn completed(&self, direction: Direction) -> bool {
        self.events()
            .finished_outcomes()
            .contains(&(direction, Outcome::Completed))
    }

    fn audited_sync(&self, sent: bool) -> bool {
        self.node.audited().iter().any(|k| match k {
            AuditEventKind::NetworkSyncSent { collection, .. } if sent => collection == O,
            AuditEventKind::NetworkSyncReceived { collection, .. } if !sent => collection == O,
            _ => false,
        })
    }
}

/// A node with the sync stack over in-memory collections.
struct Peer {
    stack: Stack,
    cols: Arc<MemCollections>,
}

impl Deref for Peer {
    type Target = Stack;

    fn deref(&self) -> &Stack {
        &self.stack
    }
}

fn attach_sync(node: E2eNode, collections: Vec<(&str, Vec<SourceFile>)>) -> Peer {
    let cols = Arc::new(MemCollections::default());
    for (name, files) in collections {
        cols.seed(name, files);
    }
    let export = Arc::new(CollectionExportService::new(Box::new(ColsSource(
        Arc::clone(&cols),
    ))));
    let importer = Arc::new(SyncImport { cols: cols.clone() });
    let stack = build_stack(node, export, importer, cols.clone());
    Peer { stack, cols }
}

impl Peer {
    fn manifest(&self, name: &str) -> Manifest {
        self.cols.manifest(name).expect("a manifest")
    }

    fn text(&self, path: &str) -> String {
        self.cols
            .text(O, path)
            .unwrap_or_else(|| panic!("{path} is missing"))
    }
}

/// A started pair with one linked collection. The handles keep the nodes running.
struct Rig {
    a: Peer,
    b: Peer,
    link: LinkId,
    _ha: NetworkHandle,
    _hb: NetworkHandle,
}

/// A pair with equal exports and a link on both sides, made with `create_link` from the same
/// exported bytes.
async fn seed_pair() -> Rig {
    let (a, b, ha, hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![(O, collection(B_TOKEN, B_LOCAL))]);
    let manifest = a.manifest(O);
    assert_eq!(
        manifest.base_hash(),
        b.manifest(O).base_hash(),
        "the fixtures export to the same files; a difference here means the export keeps a secret or a local key"
    );
    let link = LinkId::random();
    for (me, other) in [(&a, &b), (&b, &a)] {
        let paths: Vec<String> = manifest.paths().map(str::to_string).collect();
        let base_files = me
            .cols
            .export_files(O, &paths)
            .expect("base files")
            .into_iter()
            .map(|f| (f.path, f.bytes))
            .collect();
        me.sync
            .create_link(
                LinkRecord {
                    link_id: link.clone(),
                    peer_device_id: other.id(),
                    collection_name: O.to_string(),
                    collection_root: format!("/mem/{O}"),
                    collection_uid: None,
                    base: manifest.clone(),
                    state: LinkState::InSync,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
                base_files,
            )
            .expect("the link is created");
    }
    Rig {
        a,
        b,
        link,
        _ha: ha,
        _hb: hb,
    }
}

/// What the planner showed before it accepted.
struct Ran {
    sync_id: SyncId,
    sid: String,
    summary: SyncSummary,
    conflicts: Vec<ConflictView>,
}

/// Starts a sync and waits for the planner's review. Does not answer it.
async fn start_and_wait_review(starter: &Stack, planner: &Stack, link: &LinkId) -> Ran {
    let sync_id = starter.sync.start(link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the planner shows the review", || {
        planner.review_ready(&sid).is_some() || planner.finish(&sid).is_some()
    })
    .await;
    let (summary, conflicts, role) = planner
        .review_ready(&sid)
        .unwrap_or_else(|| panic!("no review, the planner ended: {:?}", planner.finish(&sid)));
    assert_eq!(role, SyncRole::Planner);
    Ran {
        sync_id,
        sid,
        summary,
        conflicts,
    }
}

/// The planner accepts with `decisions`, the starter confirms if it is asked, and the call
/// returns once both sides show an end.
async fn accept_and_finish(starter: &Stack, planner: &Stack, ran: &Ran, decisions: Vec<Decision>) {
    planner
        .sync
        .review(&ran.sync_id, true, decisions)
        .await
        .expect("the planner accepts");
    wait_until("the starter is asked or has ended", || {
        starter.confirm_summary(&ran.sid).is_some() || starter.finish(&ran.sid).is_some()
    })
    .await;
    if starter.finish(&ran.sid).is_none() {
        starter
            .sync
            .confirm(&ran.sync_id, true)
            .await
            .expect("the starter confirms");
    }
    wait_until("both sides finished", || {
        starter.finish(&ran.sid).is_some() && planner.finish(&ran.sid).is_some()
    })
    .await;
    // An applied sync saves the new base after `Finished`; tests read it after the save.
    wait_until("both applied sides saved their link", || {
        [starter, planner]
            .iter()
            .all(|side| !applied(side, &ran.sid) || side.settled(&ran.sid))
    })
    .await;
}

fn no_decisions(_: &[ConflictView]) -> Vec<Decision> {
    Vec::new()
}

/// One decision per conflict, all with the same resolution.
fn choose(resolution: Resolution) -> impl Fn(&[ConflictView]) -> Vec<Decision> {
    move |conflicts| {
        conflicts
            .iter()
            .map(|c| Decision {
                path: c.path.clone(),
                resolution,
            })
            .collect()
    }
}

/// A full sync: start, review, confirm, wait for both ends.
async fn run_sync(
    starter: &Stack,
    planner: &Stack,
    link: &LinkId,
    decide: impl Fn(&[ConflictView]) -> Vec<Decision>,
) -> Ran {
    let ran = start_and_wait_review(starter, planner, link).await;
    let decisions = decide(&ran.conflicts);
    accept_and_finish(starter, planner, &ran, decisions).await;
    ran
}

/// Both copies export the same files and both stored bases are that manifest, in sync.
fn assert_converged(rig: &Rig) {
    let hash_a = rig.a.manifest(O).base_hash();
    let hash_b = rig.b.manifest(O).base_hash();
    assert_eq!(hash_a, hash_b, "both copies export the same files");
    for peer in [&rig.a, &rig.b] {
        let record = peer.link(&rig.link).expect("the link is kept");
        assert_eq!(
            record.base_hash(),
            hash_a,
            "the stored base is the agreed manifest"
        );
        assert_eq!(record.state, LinkState::InSync);
    }
}

/// The stored base hash of a side, to prove it did not move.
fn base_hash_of(peer: &Peer, link: &LinkId) -> String {
    peer.link(link).expect("the link is kept").base_hash()
}

fn applied(peer: &Stack, sid: &str) -> bool {
    matches!(peer.finish(sid), Some((SyncFinish::Applied, _)))
}

/// Waits for the first incoming offer on `peer` and returns its transfer id.
async fn wait_offer_id(peer: &Stack) -> String {
    wait_until("the offer shows", || {
        peer.events()
            .transfers()
            .iter()
            .any(|e| matches!(e, TransferEvent::OfferIncoming { .. }))
    })
    .await;
    peer.events()
        .transfers()
        .into_iter()
        .find_map(|e| match e {
            TransferEvent::OfferIncoming { transfer_id, .. } => Some(transfer_id),
            _ => None,
        })
        .expect("an offer")
}

/// Sends `name` from `a` to `b` with or without keep in sync, accepts, and waits until both
/// sides completed.
async fn send_and_accept(a: &Stack, b: &Stack, name: &str, keep_in_sync: bool) {
    let id = a
        .transfer
        .send(&b.id(), name, false, keep_in_sync)
        .await
        .expect("the offer is sent")
        .transfer_id;
    assert_eq!(wait_offer_id(b).await, id);
    b.transfer.respond(&id, true).await.expect("accept");
    wait_until("the send completed on both sides", || {
        a.completed(Direction::Send) && b.completed(Direction::Receive)
    })
    .await;
}

// ----- the first send creates the link -----

#[tokio::test]
async fn the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![]);

    send_and_accept(&a, &b, O, true).await;
    wait_until("both links exist", || {
        a.link_count() == 1 && b.link_count() == 1
    })
    .await;

    let on_a = a.links.list().expect("list").remove(0);
    let on_b = b.links.list().expect("list").remove(0);
    assert_eq!(on_a.link_id.as_str(), on_b.link_id.as_str(), "one link id");
    assert_eq!(on_a.peer_device_id, b.id());
    assert_eq!(on_b.peer_device_id, a.id());
    assert_eq!(on_a.collection_name, O);
    assert_eq!(on_b.collection_name, O);
    assert_eq!(on_a.state, LinkState::InSync);
    assert_eq!(on_b.state, LinkState::InSync);
    assert_eq!(
        on_a.base_hash(),
        on_b.base_hash(),
        "both bases come from the same sent bytes"
    );
    assert_eq!(on_a.base_hash(), a.manifest(O).base_hash());
    assert_eq!(on_b.base_hash(), b.manifest(O).base_hash());

    // The link works: an edit on A reaches B through the first sync.
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let rig = Rig {
        a,
        b,
        link: on_a.link_id.clone(),
        _ha,
        _hb,
    };
    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;
    assert!(applied(&rig.b, &ran.sid));
    assert!(rig.b.text(GET).contains(URL_A));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_send_without_keep_in_sync_creates_no_link() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![]);

    send_and_accept(&a, &b, O, false).await;
    assert_eq!(a.link_count(), 0, "no link on the sender");
    assert_eq!(b.link_count(), 0, "no link on the receiver");
    assert!(
        b.cols.names().contains(&O.to_string()),
        "the collection did arrive"
    );
}

#[tokio::test]
async fn the_seeded_copies_start_from_one_base() {
    let rig = seed_pair().await;
    assert_converged(&rig);
    assert_ne!(
        rig.a.text(GET),
        rig.b.text(GET),
        "the raw files differ by design"
    );
    assert!(rig.b.text(GET).contains(B_TOKEN) && !rig.b.text(GET).contains(A_TOKEN));
}

// ----- a clean two-way sync -----

#[tokio::test]
async fn a_clean_two_way_sync_applies_both_edits_and_updates_both_bases() {
    let rig = seed_pair().await;
    let (a, b) = (&rig.a, &rig.b);
    // A edits request 1, B edits request 2.
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    b.cols.write(O, LIST, &list_yaml(LIST_B));
    assert_ne!(a.manifest(O).base_hash(), b.manifest(O).base_hash());

    let ran = run_sync(a, b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty(), "nothing to decide");
    assert_eq!(ran.summary.changed_here, 1, "B takes A's edit");
    assert_eq!(ran.summary.sent_changed, 1, "B sends its own edit");
    assert_eq!(ran.summary.added_here + ran.summary.deleted_here, 0);
    assert_eq!(ran.summary.sent_added + ran.summary.sent_deleted, 0);
    assert!(ran.summary.scripts_changed.is_empty() && ran.summary.auth_changed.is_empty());
    assert!(applied(a, &ran.sid) && applied(b, &ran.sid));

    assert!(b.text(GET).contains(URL_A), "B has A's edit");
    assert!(a.text(LIST).contains(LIST_B), "A has B's edit");
    assert_converged(&rig);

    // Audit: the starter sent, the planner received, with the short peer id.
    assert!(a.audited_sync(true) && b.audited_sync(false));
    assert!(a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkSyncSent { device_id, outcome, .. }
            if *device_id == b.id().short() && outcome == "applied"
    )));
}

#[tokio::test]
async fn b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs() {
    let rig = seed_pair().await;
    let (a, b) = (&rig.a, &rig.b);
    let a_before = a.text(GET);
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let a_edited = a.text(GET);

    let ran = run_sync(a, b, &rig.link, no_decisions).await;
    assert!(applied(b, &ran.sid));

    let b_now = b.text(GET);
    assert!(b_now.contains(URL_A), "B has the new url");
    // The new url starts with the old one, so the line end is part of the check.
    assert!(
        !b_now.contains(&format!("url: {URL0}\n")),
        "the old url is gone"
    );
    assert!(b_now.contains(B_TOKEN), "B still has its own token");
    assert!(b_now.contains(B_UID), "B still has its own uid");
    assert!(
        b_now.contains("sandboxMode: developer"),
        "B's rocketapi settings stay"
    );
    assert!(b_now.contains(B_EXAMPLE), "B's saved example stays");
    assert!(!b_now.contains(A_TOKEN), "A's token never crossed");
    assert!(!b_now.contains(A_UID), "A's uid never crossed");

    // A took nothing, and nothing of B's local data reached A.
    assert_eq!(a.text(GET), a_edited, "A's file is untouched by the sync");
    assert_ne!(a.text(GET), a_before);
    assert!(!a.text(GET).contains(B_TOKEN));
    assert!(!a.text(GET).contains(B_UID));
    assert!(!a.text(GET).contains(B_EXAMPLE));
    assert_converged(&rig);
}

// ----- conflicts -----

/// A and B both change request 1 and the planner (B) answers with `resolution`.
async fn conflict_run(resolution: Resolution) -> (Rig, Ran) {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    rig.b.cols.write(O, GET, &get_yaml(URL_B, B_TOKEN, B_LOCAL));
    let hash_a = rig.a.manifest(O).get(GET).map(str::to_string);
    let hash_b = rig.b.manifest(O).get(GET).map(str::to_string);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(resolution)).await;

    assert_eq!(ran.conflicts.len(), 1, "exactly one conflict");
    let conflict = &ran.conflicts[0];
    assert_eq!(conflict.path, GET);
    assert_eq!(conflict.kind, ConflictKind::BothChanged);
    assert_eq!(conflict.mine_hash, hash_b, "mine is the planner's hash");
    assert_eq!(conflict.theirs_hash, hash_a, "theirs is the starter's hash");
    assert!(applied(&rig.a, &ran.sid) && applied(&rig.b, &ran.sid));
    (rig, ran)
}

#[tokio::test]
async fn a_conflict_keep_mine_gives_the_planners_version_on_both_sides() {
    let (rig, _) = conflict_run(Resolution::KeepMine).await;
    assert!(rig.a.text(GET).contains(URL_B) && !rig.a.text(GET).contains(URL_A));
    assert!(rig.b.text(GET).contains(URL_B));
    assert!(rig.a.text(GET).contains(A_TOKEN), "A keeps its own token");
    assert!(rig.b.text(GET).contains(B_TOKEN), "B keeps its own token");
    assert!(!rig.a.text(GET).contains(B_TOKEN) && !rig.b.text(GET).contains(A_TOKEN));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_conflict_keep_theirs_gives_the_starters_version_on_both_sides() {
    let (rig, _) = conflict_run(Resolution::KeepTheirs).await;
    assert!(rig.a.text(GET).contains(URL_A) && !rig.a.text(GET).contains(URL_B));
    assert!(rig.b.text(GET).contains(URL_A) && !rig.b.text(GET).contains(URL_B));
    assert!(rig.a.text(GET).contains(A_TOKEN));
    assert!(rig.b.text(GET).contains(B_TOKEN));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_conflict_keep_both_keeps_the_original_path_and_adds_a_from_peer_copy() {
    let (rig, _) = conflict_run(Resolution::KeepBoth).await;
    let copy = "users/get (from peer).yml";
    for peer in [&rig.a, &rig.b] {
        assert!(
            peer.cols.paths(O).contains(&copy.to_string()),
            "the second copy exists: {:?}",
            peer.cols.paths(O)
        );
        assert_eq!(peer.cols.paths(O).len(), 5, "four files plus the copy");
        assert!(
            peer.text(GET).contains(URL_B),
            "the original path holds the planner's version"
        );
        assert!(
            peer.text(copy).contains(URL_A),
            "the copy holds the starter's version"
        );
    }
    assert!(
        !rig.b.text(copy).contains(A_TOKEN),
        "the copy B received carries no secret of A"
    );
    assert_converged(&rig);
}

// ----- deletes -----

#[tokio::test]
async fn a_delete_nobody_touched_is_taken() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert_eq!(ran.summary.deleted_here, 1, "B loses one file");
    assert!(rig.b.cols.text(O, LIST).is_none(), "the file is gone on B");
    assert!(rig.a.cols.text(O, LIST).is_none());
    assert_converged(&rig);
}

#[tokio::test]
async fn a_delete_against_an_edit_keep_mine_restores_the_file_on_both_sides() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);
    rig.b.cols.write(O, LIST, &list_yaml(LIST_B));

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(Resolution::KeepMine)).await;

    assert_eq!(ran.conflicts.len(), 1);
    assert_eq!(ran.conflicts[0].path, LIST);
    assert_eq!(ran.conflicts[0].kind, ConflictKind::DeleteVsEdit);
    assert!(
        ran.conflicts[0].mine_hash.is_some(),
        "the planner still has the file"
    );
    assert!(
        ran.conflicts[0].theirs_hash.is_none(),
        "the starter deleted it"
    );
    assert!(rig.a.text(LIST).contains(LIST_B), "A gets the file back");
    assert!(rig.b.text(LIST).contains(LIST_B));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_delete_against_an_edit_keep_theirs_deletes_it_on_both_sides() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);
    rig.b.cols.write(O, LIST, &list_yaml(LIST_B));

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(Resolution::KeepTheirs)).await;

    assert_eq!(ran.conflicts[0].kind, ConflictKind::DeleteVsEdit);
    assert!(rig.a.cols.text(O, LIST).is_none());
    assert!(
        rig.b.cols.text(O, LIST).is_none(),
        "B's edit is dropped by its own choice"
    );
    assert_converged(&rig);
}

// ----- renames -----

const FETCH: &str = "users/fetch.yml";

#[tokio::test]
async fn a_rename_by_the_starter_is_a_rename_on_the_planner() {
    let rig = seed_pair().await;
    rig.a.cols.rename(O, GET, FETCH);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert!(
        rig.b.cols.text(O, GET).is_none(),
        "the old path is gone on B"
    );
    let moved = rig.b.text(FETCH);
    // A delete plus an add would have written the blanked export and lost these.
    assert!(moved.contains(B_TOKEN), "the moved file keeps B's token");
    assert!(moved.contains(B_UID), "the moved file keeps B's uid");
    assert!(
        moved.contains(B_EXAMPLE),
        "the moved file keeps B's example"
    );
    assert_converged(&rig);
}

#[tokio::test]
async fn a_rename_by_the_planner_is_a_rename_on_the_starter() {
    let rig = seed_pair().await;
    rig.b.cols.rename(O, GET, FETCH);

    // A starts, B plans: B renamed, so the starter is told to move its file.
    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert!(
        rig.a.cols.text(O, GET).is_none(),
        "the old path is gone on A"
    );
    let moved = rig.a.text(FETCH);
    assert!(moved.contains(A_TOKEN), "the moved file keeps A's token");
    assert!(moved.contains(A_UID), "the moved file keeps A's uid");
    assert!(!moved.contains(B_TOKEN));
    assert_converged(&rig);
}

// ----- failures: nothing changes -----

/// The files of both sides, to compare before and after a failure.
fn files_snapshot(rig: &Rig) -> Vec<(String, String)> {
    [&rig.a, &rig.b]
        .iter()
        .flat_map(|peer| {
            peer.cols
                .paths(O)
                .into_iter()
                .map(|p| (format!("{}:{p}", peer.node.name), peer.text(&p)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The files and the stored bases of both sides, to compare before and after a failure.
fn snapshot(rig: &Rig) -> (Vec<(String, String)>, String, String) {
    (
        files_snapshot(rig),
        base_hash_of(&rig.a, &rig.link),
        base_hash_of(&rig.b, &rig.link),
    )
}

/// Waits until both sides show an end of this sync.
async fn wait_both_ended(rig: &Rig, sid: &str) {
    wait_until("both sides ended", || {
        rig.a.finish(sid).is_some() && rig.b.finish(sid).is_some()
    })
    .await;
}

#[tokio::test]
async fn a_tampered_payload_changes_nothing() {
    let rig = seed_pair().await;
    // B's edit is a push: B's files cross only after its user accepted the review.
    rig.b.cols.write(O, LIST, &list_yaml(LIST_B));
    let before = snapshot(&rig);
    // B's files ZIP reaches A with one damaged character.
    rig.b.wire.tamper(Some(SyncPayloadKind::Files));

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    assert_eq!(ran.summary.sent_changed, 1, "B sends one file");
    rig.b
        .sync
        .review(&ran.sync_id, true, Vec::new())
        .await
        .expect("the planner accepts");
    wait_both_ended(&rig, &ran.sid).await;

    assert!(
        matches!(rig.a.finish(&ran.sid), Some((SyncFinish::Failed, _))),
        "the starter got the damaged payload: {:?}",
        rig.a.finish(&ran.sid)
    );
    assert!(
        rig.a.confirm_summary(&ran.sid).is_none(),
        "A's user was never asked"
    );
    assert!(!applied(&rig.a, &ran.sid) && !applied(&rig.b, &ran.sid));
    assert!(
        !rig.a.text(LIST).contains(LIST_B),
        "A did not take the damaged file"
    );
    assert_eq!(snapshot(&rig), before, "no file and no base moved");
    wait_until("staging is empty on A", || {
        count_files(rig.a.staging_dir.path()) == 0
    })
    .await;
}

#[tokio::test]
async fn a_tampered_starter_payload_ends_the_sync_before_the_review() {
    let rig = seed_pair().await;
    // A's edit is a pull: the planner asks for A's files before it shows its review.
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);
    rig.a.wire.tamper(Some(SyncPayloadKind::Files));

    let sync_id = rig.a.sync.start(&rig.link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_both_ended(&rig, &sid).await;

    assert!(
        matches!(rig.b.finish(&sid), Some((SyncFinish::Failed, _))),
        "the planner got the damaged payload: {:?}",
        rig.b.finish(&sid)
    );
    assert!(
        rig.b.review_ready(&sid).is_none(),
        "B's user was never asked"
    );
    assert!(!applied(&rig.a, &sid) && !applied(&rig.b, &sid));
    assert!(
        !rig.b.text(GET).contains(URL_A),
        "B did not take the damaged file"
    );
    assert_eq!(snapshot(&rig), before, "no file and no base moved");
}

#[tokio::test]
async fn declining_the_review_changes_nothing() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.b
        .sync
        .review(&ran.sync_id, false, Vec::new())
        .await
        .expect("the planner declines");
    wait_both_ended(&rig, &ran.sid).await;

    assert!(matches!(
        rig.a.finish(&ran.sid),
        Some((SyncFinish::Declined, _))
    ));
    assert!(matches!(
        rig.b.finish(&ran.sid),
        Some((SyncFinish::Declined, _))
    ));
    assert_eq!(snapshot(&rig), before);
}

#[tokio::test]
async fn the_starter_cancels_while_the_planner_reviews() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.a
        .sync
        .cancel(&ran.sync_id)
        .await
        .expect("the starter cancels");
    wait_both_ended(&rig, &ran.sid).await;

    assert!(matches!(
        rig.a.finish(&ran.sid),
        Some((SyncFinish::Cancelled, _))
    ));
    assert!(matches!(
        rig.b.finish(&ran.sid),
        Some((SyncFinish::Cancelled, _))
    ));
    assert!(
        rig.b
            .sync
            .review(&ran.sync_id, true, Vec::new())
            .await
            .is_err(),
        "a late accept is refused"
    );
    assert_eq!(snapshot(&rig), before);
}

/// Removing the pairing ends the sync with no file changed, and the links go with the
/// pairing on both sides (spec section 5).
#[tokio::test]
async fn removing_the_pairing_during_a_sync_ends_it_with_no_change() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = files_snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.b
        .node
        .svc
        .remove_paired(&rig.a.id())
        .await
        .expect("B unpairs A");

    wait_both_ended(&rig, &ran.sid).await;
    assert!(!applied(&rig.a, &ran.sid) && !applied(&rig.b, &ran.sid));
    assert!(
        rig.b
            .sync
            .review(&ran.sync_id, true, Vec::new())
            .await
            .is_err(),
        "the session is gone"
    );
    assert_eq!(files_snapshot(&rig), before, "no file moved");
    // The links are deleted a moment after the unpair, on both sides.
    wait_until("both links are gone", || {
        rig.a.link_count() == 0 && rig.b.link_count() == 0
    })
    .await;
    assert!(rig.b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkLinkRemoved { device_id, collection }
            if *device_id == rig.a.id().short() && collection == O
    )));
    wait_until("staging is empty on both sides", || {
        count_files(rig.a.staging_dir.path()) == 0 && count_files(rig.b.staging_dir.path()) == 0
    })
    .await;
}

#[tokio::test]
async fn a_base_mismatch_ends_the_sync_before_any_review() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    // B's stored base drifts: an empty manifest has another base hash.
    let mut skewed = rig.b.link(&rig.link).expect("the link");
    skewed.base = Manifest::new();
    rig.b
        .links
        .save(&skewed, &[])
        .expect("save the skewed base");
    let before = snapshot(&rig);

    let sync_id = rig.a.sync.start(&rig.link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the starter ended", || rig.a.finish(&sid).is_some()).await;

    assert!(matches!(
        rig.a.finish(&sid),
        Some((SyncFinish::BaseMismatch, _))
    ));
    assert!(
        rig.b.review_ready(&sid).is_none(),
        "the planner never asked its user"
    );
    assert_eq!(snapshot(&rig), before);
}

#[tokio::test]
async fn an_unlink_on_one_side_ends_the_next_sync_with_link_missing() {
    let rig = seed_pair().await;
    rig.b.sync.unlink(&rig.link).await.expect("B unlinks");

    assert!(rig.b.link(&rig.link).is_none(), "B's link is gone");
    assert_eq!(
        count_files(&rig.b.link_dir.path().join(rig.link.as_str())),
        0,
        "B's base copies are gone"
    );
    assert!(
        rig.b.cols.text(O, GET).is_some(),
        "B's collection is untouched"
    );
    assert!(rig.b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkLinkRemoved { device_id, collection }
            if *device_id == rig.a.id().short() && collection == O
    )));

    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let a_files = rig.a.text(GET);
    let sync_id = rig.a.sync.start(&rig.link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the starter ended", || rig.a.finish(&sid).is_some()).await;

    assert!(matches!(
        rig.a.finish(&sid),
        Some((SyncFinish::LinkMissing, _))
    ));
    wait_until("A saved its link state", || rig.a.settled(&sid)).await;
    assert_eq!(
        rig.a.link(&rig.link).expect("A keeps the record").state,
        LinkState::Broken,
        "A marks its link broken"
    );
    assert_eq!(rig.a.text(GET), a_files, "A's collection is untouched");
    assert!(!rig.b.text(GET).contains(URL_A), "B got nothing");
}

// ----- the router -----

#[tokio::test]
async fn a_send_and_a_sync_can_run_at_the_same_time() {
    let rig = seed_pair().await;
    rig.a.cols.seed(
        "Extras",
        vec![file(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: \"Extras\"\n",
        )],
    );
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));

    // The sync is waiting for the planner's user when the send starts.
    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    send_and_accept(&rig.a, &rig.b, "Extras", false).await;
    assert!(rig.b.cols.names().contains(&"Extras".to_string()));

    // The sync is still open and finishes normally.
    assert!(rig.a.finish(&ran.sid).is_none());
    accept_and_finish(&rig.a, &rig.b, &ran, Vec::new()).await;
    assert!(applied(&rig.a, &ran.sid) && applied(&rig.b, &ran.sid));
    assert!(rig.b.text(GET).contains(URL_A));
    assert_converged(&rig);
}

// ----- the first send over the real disk stack -----

/// The environments of a disk workspace, as the import adapter builds them (no secret store).
struct Envs(PathBuf);

impl EnvironmentRepositoryFactory for Envs {
    fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection)
                .join("environments"),
        ))
    }
}

/// The import of the shipped adapter (`src-tauri` `ImporterAdapter`) without the trust store:
/// a copy named after the display name, the strict validator, the root check, then the real
/// Bruno import into the workspace.
struct DiskImport {
    ws: PathBuf,
}

impl DiskImport {
    fn repo(&self) -> FsCollectionRepo {
        FsCollectionRepo::new_standalone(self.ws.join("collections"))
    }
}

impl CollectionImportPort for DiskImport {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.repo().list()?.into_iter().map(|c| c.name).collect())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        let dir = TempDir::new().map_err(|e| DomainError::Internal(e.to_string()))?;
        let copy = dir.path().join(format!("{display_name}.zip"));
        std::fs::copy(zip_path, &copy).map_err(|e| DomainError::Io(e.to_string()))?;
        rocket_import::transfer_zip::validate(&FsImportFiles, &copy)
            .and_then(|()| {
                rocket_import::transfer_zip::require_collection_root(&FsImportFiles, &copy)
            })
            .map_err(|_| DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.to_string()))?;
        let service = ImportService::new(
            self.ws.clone(),
            Box::new(self.repo()),
            Box::new(Envs(self.ws.clone())),
            Arc::new(FsImportFiles),
        );
        let report = service
            .import_auto_from_zip(&copy, "default", false)
            .map_err(|e| {
                DomainError::Internal(format!("The collection could not be imported: {e}"))
            })?;
        let name = report
            .created_collections
            .into_iter()
            .next()
            .ok_or_else(|| {
                DomainError::Internal("The import created no collection.".to_string())
            })?;
        Ok(ImportedCollection { name })
    }
}

/// A node with the sync stack over a real workspace on disk, wired like `lib.rs`.
struct DiskPeer {
    stack: Stack,
    ws: TempDir,
}

impl Deref for DiskPeer {
    type Target = Stack;

    fn deref(&self) -> &Stack {
        &self.stack
    }
}

fn attach_disk(node: E2eNode) -> DiskPeer {
    let ws = TempDir::new().expect("tempdir");
    let shared = Arc::new(Mutex::new(ws.path().to_path_buf()));
    let export = Arc::new(
        CollectionExportService::new(Box::new(FsCollectionFileSource::new(shared.clone())))
            .with_collections(Box::new(SharedPathCollectionRepo::new(shared.clone()))),
    );
    let port = Arc::new(ExportBackedSyncCollections::new(
        Arc::clone(&export),
        Arc::new(SharedPathCollectionRepo::new(shared)),
        Arc::new(FsFileTxFactory::new()),
    ));
    let importer = Arc::new(DiskImport {
        ws: ws.path().to_path_buf(),
    });
    let stack = build_stack(node, export, importer, port);
    DiskPeer { stack, ws }
}

impl DiskPeer {
    fn repo(&self) -> FsCollectionRepo {
        FsCollectionRepo::new_standalone(self.ws.path().join("collections"))
    }

    fn path(&self, collection: &str, rel: &str) -> PathBuf {
        self.ws
            .path()
            .join("collections")
            .join(collection)
            .join(rel)
    }

    fn read(&self, collection: &str, rel: &str) -> String {
        std::fs::read_to_string(self.path(collection, rel))
            .unwrap_or_else(|_| panic!("{rel} is missing"))
    }

    fn write(&self, collection: &str, rel: &str, text: &str) {
        let path = self.path(collection, rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, text).expect("write");
    }

    fn root_yaml(&self, collection: &str) -> serde_yaml::Value {
        serde_yaml::from_str(&self.read(collection, "opencollection.yml")).expect("yaml")
    }
}

/// A root with docs, config and collection-level request settings. The importer rewrites
/// such a root (key order), which a fresh link must not read as a change.
const RICH_ROOT: &str = "docs: hello team\nconfig:\n  proxy:\n    disabled: true\nrequest:\n  auth:\n    type: bearer\n    token: SECRET\n  variables:\n    - name: base\n      value: https://x.test\n  scripts:\n    - type: before-request\n      code: console.log(1)\n";

/// The sender's collection on disk: the root as `create` writes it plus `RICH_ROOT`, a folder
/// and two requests.
fn seed_disk_sender(peer: &DiskPeer) {
    peer.repo().create(O).expect("created");
    let mut root = peer.read(O, "opencollection.yml");
    root.push_str(RICH_ROOT);
    peer.write(O, "opencollection.yml", &root);
    peer.write(O, "users/folder.yml", "info:\n  name: users\n");
    peer.write(O, GET, &get_yaml(URL0, A_TOKEN, A_LOCAL));
    peer.write(O, LIST, &list_yaml(LIST0));
}

/// The first linked send over the wire into a real import. `clash` gives the receiver its own
/// "Orders" first, so the import lands under a suffixed name. Returns the received name.
async fn first_send_on_disk(clash: bool) {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach_disk(a);
    let b = attach_disk(b);
    seed_disk_sender(&a);
    if clash {
        b.repo().create(O).expect("the receiver's own collection");
    }
    let expected = if clash {
        format!("{O} (from {})", a.node.name)
    } else {
        O.to_string()
    };
    let a_root = a.read(O, "opencollection.yml");

    send_and_accept(&a, &b, O, true).await;
    wait_until("both links exist", || {
        a.link_count() == 1 && b.link_count() == 1
    })
    .await;

    let on_a = a.links.list().expect("list").remove(0);
    let on_b = b.links.list().expect("list").remove(0);
    assert_eq!(on_a.link_id.as_str(), on_b.link_id.as_str(), "one link id");
    assert_eq!(
        on_b.collection_name, expected,
        "the link names the imported copy"
    );
    assert_eq!(on_a.base_hash(), on_b.base_hash(), "one base on both ends");
    assert_eq!(a.link_states().await, vec![LinkState::InSync], "sender");
    assert_eq!(
        b.link_states().await,
        vec![LinkState::InSync],
        "the receiver's import reads as the base"
    );
    // The receiver keeps its own name; the sent docs landed.
    let b_root = b.root_yaml(&expected);
    assert_eq!(b_root["info"]["name"].as_str(), Some(expected.as_str()));
    assert_eq!(b_root["docs"].as_str(), Some("hello team"));
    assert!(
        !b.read(&expected, GET).contains(A_TOKEN),
        "the sent request carries no token"
    );

    // The first sync carries A's edit and nothing of B's import back to A.
    a.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let ran = run_sync(&a, &b, &on_a.link_id, no_decisions).await;
    assert!(ran.conflicts.is_empty(), "nothing to decide");
    assert_eq!(ran.summary.changed_here, 1, "B takes A's edit");
    assert_eq!(
        ran.summary.sent_added + ran.summary.sent_changed + ran.summary.sent_deleted,
        0,
        "B has no change of its own to send"
    );
    assert!(applied(&a, &ran.sid) && applied(&b, &ran.sid));
    assert!(b.read(&expected, GET).contains(URL_A));
    assert_eq!(
        a.read(O, "opencollection.yml"),
        a_root,
        "A's root is untouched"
    );
    assert_eq!(a.link_states().await, vec![LinkState::InSync]);
    assert_eq!(b.link_states().await, vec![LinkState::InSync]);
    let (ra, rb) = (
        a.link(&on_a.link_id).expect("A's link"),
        b.link(&on_a.link_id).expect("B's link"),
    );
    assert_eq!(ra.base_hash(), rb.base_hash(), "both bases moved together");
}

#[tokio::test]
async fn a_linked_send_of_a_rich_root_reads_in_sync_on_both_ends_over_the_wire() {
    first_send_on_disk(false).await;
}

#[tokio::test]
async fn a_linked_send_into_a_suffixed_name_reads_in_sync_on_both_ends_over_the_wire() {
    first_send_on_disk(true).await;
}
