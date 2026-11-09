//! The sync service: keeps linked collections in step with one paired node.
//!
//! Same shape as the transfer service: one queue, one worker that only runs short sync
//! handlers, spawned tasks for the slow work, one `std::sync::Mutex` never held across an
//! `.await`, and whoever removes a session emits its single `Finished`. The payloads ride the
//! `rocket-network` transfer machines (`sync_payload.rs`); the pure rules are `sync_checks.rs`.
//!
//! This file holds the links (create, list, unlink, removal with the pairing), the router's
//! lookup (`claims`) and the frame queue. The sessions build on it.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::{
    DeviceId, LinkId, LinkRecord, LinkRepository, LinkState, PeerMessage, SyncEvent, SyncId,
    TransferId, TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::EventPublisher;
use tokio::sync::mpsc;

use super::sync_ports::{base_from_reader, SyncCollectionPort};
use super::transfer::{PeerSender, Timing};

/// Items that may wait for the worker. A full queue drops a frame (S4).
const SYNC_FRAME_QUEUE: usize = 256;
/// The name a link shows when its peer is no longer paired.
const UNKNOWN_NODE: &str = "Unknown node";
/// The gap between the two "is it still paired" reads before links are deleted (S16).
const UNPAIR_RECHECK: Duration = Duration::from_millis(50);

const LINK_EXISTS: &str = "That link already exists.";
const NO_SUCH_LINK: &str = "That link does not exist.";
const CANNOT_LINK: &str = "The sent collection cannot be linked.";

/// Everything the service needs from outside. All of it is a port.
pub struct SyncDeps {
    pub network: Arc<dyn PeerSender>,
    pub links: Arc<dyn LinkRepository>,
    pub collections: Arc<dyn SyncCollectionPort>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
}

/// One link as the frontend lists it. `state` is derived, see `SyncService::links`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkView {
    pub link_id: String,
    pub device_id: String,
    pub display_name: String,
    pub short_id: String,
    pub collection_name: String,
    pub state: LinkState,
    pub updated_at: String,
}

/// The two texts of a conflict diff, read as lossy UTF-8. `None` for a side without the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFiles {
    pub mine: Option<String>,
    pub theirs: Option<String>,
}

/// What the worker handles, in arrival order.
enum Item {
    Frame(DeviceId, PeerMessage),
    Gone(DeviceId),
}

/// The state behind the one lock.
#[derive(Default)]
struct SyncInner {
    /// Every open sync payload: its peer and its session. The router reads it.
    transfers: HashMap<TransferId, (DeviceId, SyncId)>,
}

pub struct SyncService {
    me: Weak<SyncService>,
    deps: SyncDeps,
    /// The waits of the sessions. Read once the sessions run.
    #[allow(dead_code)]
    timing: Timing,
    inner: Mutex<SyncInner>,
    frames: mpsc::Sender<Item>,
    worker: Mutex<Option<mpsc::Receiver<Item>>>,
    #[cfg(test)]
    frames_seen: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    gone_seen: std::sync::atomic::AtomicUsize,
}

impl SyncService {
    /// Builds the service. It does not register as the frame sink: `FrameRouter` is the sink
    /// and hands it the sync frames.
    pub fn new(deps: SyncDeps) -> Arc<SyncService> {
        Self::new_with_timing(deps, Timing::default())
    }

    /// Like `new`, with other waits. Tests use it.
    #[doc(hidden)]
    pub fn new_with_timing(deps: SyncDeps, timing: Timing) -> Arc<SyncService> {
        let (frames, queue) = mpsc::channel(SYNC_FRAME_QUEUE);
        Arc::new_cyclic(|me| SyncService {
            me: me.clone(),
            deps,
            timing,
            inner: Mutex::new(SyncInner::default()),
            frames,
            worker: Mutex::new(Some(queue)),
            #[cfg(test)]
            frames_seen: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            gone_seen: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, SyncInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn arc(&self) -> Option<Arc<SyncService>> {
        self.me.upgrade()
    }

    fn emit(&self, event: SyncEvent) {
        self.deps.events.publish(&event);
    }

    fn link_changed(&self, link_id: &LinkId) {
        self.emit(SyncEvent::LinkChanged {
            link_id: link_id.as_str().to_string(),
        });
    }

    fn audit_link_removed(&self, record: &LinkRecord) {
        self.deps.audit.publish(
            AuditActor::User.into(),
            None,
            AuditEventKind::NetworkLinkRemoved {
                device_id: record.peer_device_id.short(),
                collection: record.collection_name.clone(),
            },
        );
    }

    /// True when the router must hand this frame to the sync service: a `Sync*` frame, or a
    /// transfer frame whose id is a sync payload of this very peer. Short lock, one lookup.
    pub(crate) fn claims(&self, from: &DeviceId, message: &PeerMessage) -> bool {
        let id = match message {
            PeerMessage::SyncOffer { .. }
            | PeerMessage::SyncAbort { .. }
            | PeerMessage::SyncResult { .. } => return true,
            PeerMessage::OfferAnswer { transfer_id, .. }
            | PeerMessage::Chunk { transfer_id, .. }
            | PeerMessage::ChunkAck { transfer_id, .. }
            | PeerMessage::TransferAbort { transfer_id, .. }
            | PeerMessage::TransferResult { transfer_id, .. } => transfer_id,
            _ => return false,
        };
        let Ok(id) = TransferId::parse(id) else {
            return false;
        };
        self.lock()
            .transfers
            .get(&id)
            .is_some_and(|(device, _)| device == from)
    }

    // ----- links -----

    /// Stores a new link. An existing link id is `AlreadyExists("That link already exists.")`.
    /// Publishes `LinkChanged`. Blocks (a repository call).
    pub fn create_link(
        &self,
        record: LinkRecord,
        base_files: Vec<(String, Vec<u8>)>,
    ) -> DomainResult<()> {
        if self.deps.links.get(&record.link_id)?.is_some() {
            return Err(DomainError::AlreadyExists(LINK_EXISTS.to_string()));
        }
        self.deps.links.save(&record, &base_files)?;
        self.link_changed(&record.link_id);
        Ok(())
    }

    /// The link of the first linked send, on either end (S18). The ZIP passes the strict
    /// validator, then its files (root folder stripped) are the base, so both ends build the
    /// same base from the same bytes. The record takes the local root and uid, state `in_sync`
    /// and `updated_at` now. Blocks.
    pub(crate) fn link_first_transfer(
        &self,
        link_id: LinkId,
        peer: DeviceId,
        collection: &str,
        zip: &[u8],
    ) -> DomainResult<()> {
        let cannot = || DomainError::InvalidInput(CANNOT_LINK.to_string());
        rocket_import::transfer_zip::validate_bytes(zip).map_err(|_| cannot())?;
        let base = base_from_reader(Cursor::new(zip)).map_err(|_| cannot())?;
        let collections = &self.deps.collections;
        let record = LinkRecord {
            link_id,
            peer_device_id: peer,
            collection_name: collection.to_string(),
            collection_root: collections.collection_root(collection)?,
            collection_uid: collections.collection_uid(collection)?,
            base: base.manifest,
            state: LinkState::InSync,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        self.create_link(record, base.files)
    }

    /// Every link with its derived state (S15), sorted by collection name then link id. Nothing
    /// is saved: `broken` when the collection is gone or its uid or root differ from the
    /// record, `changes_waiting` when an `in_sync` link's fresh manifest differs from its base.
    /// A collection that cannot be read shows its stored state. Blocks (it runs the export
    /// once per link).
    pub fn links(&self) -> DomainResult<Vec<LinkView>> {
        let mut records = self.deps.links.list()?;
        records.sort_by(|a, b| {
            a.collection_name
                .cmp(&b.collection_name)
                .then_with(|| a.link_id.as_str().cmp(b.link_id.as_str()))
        });
        Ok(records
            .into_iter()
            .map(|record| {
                let display_name = self
                    .deps
                    .network
                    .peer_label(&record.peer_device_id)
                    .map_or_else(|| UNKNOWN_NODE.to_string(), |(name, _)| name);
                LinkView {
                    link_id: record.link_id.as_str().to_string(),
                    device_id: record.peer_device_id.to_base32(),
                    display_name,
                    short_id: record.peer_device_id.short(),
                    collection_name: record.collection_name.clone(),
                    state: self.derived_state(&record),
                    updated_at: record.updated_at.clone(),
                }
            })
            .collect())
    }

    /// The state `links` shows for one record. Read errors keep the stored state.
    fn derived_state(&self, record: &LinkRecord) -> LinkState {
        if record.state == LinkState::Broken {
            return LinkState::Broken;
        }
        match self.folder_matches(record) {
            Ok(true) => {}
            Ok(false) => return LinkState::Broken,
            Err(e) => {
                tracing::debug!(code = "network_link_state_unread", error = %e, "stored state shown");
                return record.state;
            }
        }
        if record.state != LinkState::InSync {
            return record.state;
        }
        match self.deps.collections.manifest(&record.collection_name) {
            Ok(fresh) if fresh.base_hash() != record.base_hash() => LinkState::ChangesWaiting,
            Ok(_) => LinkState::InSync,
            Err(e) => {
                tracing::debug!(code = "network_link_state_unread", error = %e, "stored state shown");
                record.state
            }
        }
    }

    /// True when the collection still exists with the record's root and uid.
    fn folder_matches(&self, record: &LinkRecord) -> DomainResult<bool> {
        let collections = &self.deps.collections;
        let name = &record.collection_name;
        if !collections.exists(name)? {
            return Ok(false);
        }
        Ok(collections.collection_root(name)? == record.collection_root
            && collections.collection_uid(name)? == record.collection_uid)
    }

    /// Deletes a link and its base. Audits `NetworkLinkRemoved` and publishes `LinkChanged`.
    /// The peer is not told; it learns at its next sync. An unknown link is
    /// `NotFound("That link does not exist.")`.
    pub async fn unlink(&self, link_id: &LinkId) -> DomainResult<()> {
        let links = Arc::clone(&self.deps.links);
        let id = link_id.clone();
        let removed = tokio::task::spawn_blocking(move || -> DomainResult<Option<LinkRecord>> {
            let Some(record) = links.get(&id)? else {
                return Ok(None);
            };
            links.delete(&id)?;
            Ok(Some(record))
        })
        .await
        .map_err(|_| DomainError::Internal("The unlink task failed.".to_string()))??;
        let Some(record) = removed else {
            return Err(DomainError::NotFound(NO_SUCH_LINK.to_string()));
        };
        self.audit_link_removed(&record);
        self.link_changed(&record.link_id);
        Ok(())
    }

    // ----- the queue -----

    /// Starts the worker on first use. Every caller runs inside the tokio runtime.
    fn ensure_worker(&self) {
        let queue = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(queue) = queue {
            drop(tokio::spawn(Self::run_worker(self.me.clone(), queue)));
        }
    }

    async fn run_worker(me: Weak<SyncService>, mut queue: mpsc::Receiver<Item>) {
        while let Some(item) = queue.recv().await {
            let Some(svc) = me.upgrade() else {
                break;
            };
            match item {
                Item::Frame(from, message) => svc.handle_frame(from, message),
                Item::Gone(device) => svc.handle_gone(device),
            }
        }
    }

    /// Puts an item into the queue. A full queue drops a frame, but a peer-gone notice must
    /// not be lost, so it runs right here.
    fn enqueue(&self, item: Item) {
        match self.frames.try_send(item) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(Item::Gone(device)))
            | Err(mpsc::error::TrySendError::Closed(Item::Gone(device))) => {
                self.handle_gone(device);
            }
            Err(_) => {
                tracing::debug!(
                    code = "network_sync_frame_dropped",
                    "sync frame queue is full"
                );
            }
        }
    }

    /// No session runs yet, so every sync frame is dropped here.
    fn handle_frame(&self, _from: DeviceId, _message: PeerMessage) {
        tracing::debug!(code = "network_sync_frame_ignored", "no sync session");
    }

    /// The device's connection ended or its pairing was removed. Only a removed pairing
    /// deletes links (S16): a blocking task asks `peer_label` twice, `UNPAIR_RECHECK` apart,
    /// because a read error also reads as "not paired", and deletes the device's links only
    /// when both say so.
    fn handle_gone(&self, device: DeviceId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::task::spawn_blocking(move || {
            if me.deps.network.peer_label(&device).is_some() {
                return;
            }
            std::thread::sleep(UNPAIR_RECHECK);
            if me.deps.network.peer_label(&device).is_some() {
                return;
            }
            me.remove_links_of(&device);
        }));
    }

    /// Deletes every link with `device`, each with its audit row and `LinkChanged`. Blocks.
    fn remove_links_of(&self, device: &DeviceId) {
        let records = match self.deps.links.list() {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!(code = "network_link_list_failed", "links not removed");
                tracing::debug!(error = %e, "links not removed");
                return;
            }
        };
        for record in records.iter().filter(|r| r.peer_device_id == *device) {
            if let Err(e) = self.deps.links.delete(&record.link_id) {
                tracing::warn!(code = "network_link_delete_failed", "link not removed");
                tracing::debug!(error = %e, "link not removed");
                continue;
            }
            self.audit_link_removed(record);
            self.link_changed(&record.link_id);
        }
    }

    // ----- test hooks -----

    /// Test hook: registers a sync payload id for `device`, as an outgoing payload would.
    #[cfg(test)]
    pub(crate) fn claim_transfer_for_test(&self, device: DeviceId) -> String {
        let id = TransferId::random();
        let text = id.as_str().to_string();
        self.lock().transfers.insert(id, (device, SyncId::random()));
        text
    }

    /// Test hook: how many frames reached the service.
    #[cfg(test)]
    pub(crate) fn frames_seen(&self) -> usize {
        self.frames_seen.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Test hook: how many peer-gone notices reached the service.
    #[cfg(test)]
    pub(crate) fn gone_seen(&self) -> usize {
        self.gone_seen.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl TransferSink for SyncService {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        #[cfg(test)]
        self.frames_seen
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.ensure_worker();
        self.enqueue(Item::Frame(from, message));
    }

    fn on_peer_gone(&self, device: DeviceId) {
        #[cfg(test)]
        self.gone_seen
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.ensure_worker();
        self.enqueue(Item::Gone(device));
    }
}
