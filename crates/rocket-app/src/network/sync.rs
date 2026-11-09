//! The sync service: keeps linked collections in step with one paired node.
//!
//! Same shape as the transfer service: one queue, one worker that only runs short sync
//! handlers, spawned tasks for the slow work, one `std::sync::Mutex` never held across an
//! `.await`, and whoever removes a session emits its single `Finished`. The payloads ride the
//! `rocket-network` transfer machines (`sync_payload.rs`); the pure rules are `sync_checks.rs`.
//!
//! This file holds the links (create, list, unlink, removal with the pairing), the router's
//! lookup (`claims`), the frame queue, and the session core: `Session`, `Phase`, the
//! watchdog and `end_session`, the one place a session ends. The starter half is
//! `sync_starter.rs`, the planner half `sync_planner.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::{
    DeviceId, LinkId, LinkRecord, LinkRepository, LinkState, Manifest, PeerMessage,
    SyncAbortReason, SyncEvent, SyncFinish, SyncId, SyncOutcome, SyncPayloadKind, SyncPlan,
    SyncPlanPayload, TransferId, TransferLimits, TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::EventPublisher;
use tokio::sync::mpsc;

use super::sync_checks::{conflict_paths, finish_name, Stop, UNCONFIRMED};
use super::sync_payload::{sha_hex, PayloadIn, PayloadOut};
use super::sync_ports::{base_from_reader, FileChange, SyncCollectionPort};
use super::transfer::{PeerSender, Timing};

/// Items that may wait for the worker. A full queue drops a frame (S4).
const SYNC_FRAME_QUEUE: usize = 256;
/// The name a link shows when its peer is no longer paired.
const UNKNOWN_NODE: &str = "Unknown node";
/// The gap between the two "is it still paired" reads before links are deleted (S16).
const UNPAIR_RECHECK: Duration = Duration::from_millis(50);

const LINK_EXISTS: &str = "That link already exists.";
pub(super) const NO_SUCH_LINK: &str = "That link does not exist.";
const CANNOT_LINK: &str = "The sent collection cannot be linked.";
pub(super) const NOT_WAITING: &str = "The sync is no longer waiting for an answer.";
const ALREADY_APPLYING: &str = "The sync is already being applied.";
const REVIEW_CHECKING: &str = "The answer to the review is being checked. Try again in a moment.";
const NOT_A_CONFLICT: &str = "That file is not a conflict.";

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

/// Where a session stands. The tables of the plan (S1 to S29, P1 to P31) name these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    // Starter.
    SendManifest,
    AwaitPlan,
    RecvPlan,
    Exporting,
    AwaitConfirm,
    AwaitResult,
    // Planner.
    Checking,
    RecvManifest,
    Planning,
    SendRequest,
    AwaitReview,
    Finalizing,
    SendFinal,
    AwaitStarter,
    // Both.
    SendFiles,
    AwaitFiles,
    RecvFiles,
    Applying,
}

/// Which side of the sync this node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Starter,
    Planner,
}

/// What `end_session` does to the link once the session is gone.
pub(super) enum LinkUpdate {
    Keep,
    /// Stores this state; the base stays.
    State(LinkState),
    /// Stores the agreed base. `known` holds bytes this node already has for some of its
    /// paths. `state: None` is `in_sync` when the fresh manifest equals the base, else
    /// `changes_waiting`.
    NewBase {
        base: Manifest,
        known: BTreeMap<String, Vec<u8>>,
        state: Option<LinkState>,
    },
}

/// The fields of one `SyncOffer` frame.
#[derive(Debug, Clone)]
pub(super) struct SyncOfferFields {
    pub sync_id: String,
    pub link_id: String,
    pub base_hash: String,
    pub kind: SyncPayloadKind,
    pub transfer_id: String,
    pub size_bytes: u64,
    pub chunk_count: u32,
    pub sha256: String,
}

/// One sync with one peer, in either role.
pub(super) struct Session {
    pub role: Role,
    pub peer: DeviceId,
    pub peer_name: String,
    pub link_id: LinkId,
    /// The base hash both sides agreed on at the start. Every follow-up offer carries it.
    pub base_hash: String,
    /// The link record. The planner has it once its check passed.
    pub link: Option<LinkRecord>,
    pub phase: Phase,
    /// Bumped by every phase change, so a timer can tell its phase is over.
    pub gen: u64,
    pub phase_since: Instant,
    /// The session's entry in `sync_limits`.
    pub key: TransferId,
    pub out: Option<PayloadOut>,
    pub inc: Option<PayloadIn>,
    /// A follow-up offer that came while this node still finished its own step.
    pub stash: Option<SyncOfferFields>,
    /// A chunk of a payload left this node (S13).
    pub chunk_out: bool,
    /// This node's user answered the dialog (or the sync needed none).
    pub answered: bool,
    /// A `review` call is checking its decisions.
    pub reviewing: bool,
    /// The peer went away while this node was applying.
    pub peer_gone: bool,
    /// The starter is sending its `SyncResult`.
    pub result_pending: bool,
    /// The starter answered a request plan already.
    pub request_done: bool,
    /// The bytes of earlier `Files` payloads, so the progress bar never restarts (R16).
    pub progress_base: u64,
    /// The manifest this node planned or offered with.
    pub mine: Manifest,
    /// Planner: the starter's manifest.
    pub theirs: Option<Manifest>,
    pub plan: Option<SyncPlan>,
    /// Planner: the starter's files it asked for, with their expected hashes.
    pub wanted: BTreeMap<String, String>,
    /// The final plan, received (starter) or sent (planner).
    pub final_plan: Option<SyncPlanPayload>,
    pub received: BTreeMap<String, Vec<u8>>,
    pub sent_files: BTreeMap<String, Vec<u8>>,
    /// Planner: its own copies of the conflict files, for the diff.
    pub mine_conflicts: BTreeMap<String, Vec<u8>>,
    pub new_base: Option<Manifest>,
    pub changes: Vec<FileChange>,
    /// Planner: the files ZIP that follows the final plan.
    pub files_zip: Option<Vec<u8>>,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        role: Role,
        peer: DeviceId,
        peer_name: String,
        link_id: LinkId,
        base_hash: String,
        key: TransferId,
        phase: Phase,
        now: Instant,
    ) -> Session {
        Session {
            role,
            peer,
            peer_name,
            link_id,
            base_hash,
            link: None,
            phase,
            gen: 0,
            phase_since: now,
            key,
            out: None,
            inc: None,
            stash: None,
            chunk_out: false,
            answered: false,
            reviewing: false,
            peer_gone: false,
            result_pending: false,
            request_done: false,
            progress_base: 0,
            mine: Manifest::new(),
            theirs: None,
            plan: None,
            wanted: BTreeMap::new(),
            final_plan: None,
            received: BTreeMap::new(),
            sent_files: BTreeMap::new(),
            mine_conflicts: BTreeMap::new(),
            new_base: None,
            changes: Vec::new(),
            files_zip: None,
        }
    }

    pub(super) fn collection(&self) -> String {
        self.link
            .as_ref()
            .map(|l| l.collection_name.clone())
            .unwrap_or_default()
    }

    /// Moves to `phase` and restarts its clock.
    pub(super) fn enter(&mut self, phase: Phase, now: Instant) {
        self.phase = phase;
        self.gen += 1;
        self.phase_since = now;
    }

    /// The files whose bytes this node holds for the new base: received and sent.
    pub(super) fn known_files(&self) -> BTreeMap<String, Vec<u8>> {
        let mut known = self.sent_files.clone();
        known.extend(self.received.clone());
        known
    }
}

/// The state behind the one lock.
#[derive(Default)]
pub(super) struct SyncInner {
    /// Every open sync payload: its peer and its session. The router reads it.
    pub transfers: HashMap<TransferId, (DeviceId, SyncId)>,
    pub sessions: HashMap<SyncId, Session>,
    /// Peers whose `start` is between its checks and its session.
    pub starting: HashSet<DeviceId>,
    /// Peers whose ended session still saves its link.
    pub settling: HashSet<DeviceId>,
    /// The sync's own offer limits (S5). Collection transfers never reach them.
    pub limits: TransferLimits,
}

impl SyncInner {
    /// True when the peer has a session, a start or a link save running (S6).
    pub(super) fn peer_busy(&self, peer: &DeviceId) -> bool {
        self.starting.contains(peer)
            || self.settling.contains(peer)
            || self.sessions.values().any(|s| s.peer == *peer)
    }

    /// Removes a session and frees its payload ids and its limits entry.
    pub(super) fn take_session(&mut self, sync_id: &SyncId) -> Option<Session> {
        let session = self.sessions.remove(sync_id)?;
        if let Some(out) = &session.out {
            self.transfers.remove(out.machine.transfer_id());
        }
        if let Some(inc) = &session.inc {
            self.transfers.remove(inc.machine.transfer_id());
        }
        // The planner registers its manifest id under its limits key while it checks the link.
        if self
            .transfers
            .get(&session.key)
            .is_some_and(|(_, sid)| sid == sync_id)
        {
            self.transfers.remove(&session.key);
        }
        self.limits.release(&session.key);
        Some(session)
    }
}

pub struct SyncService {
    me: Weak<SyncService>,
    pub(super) deps: SyncDeps,
    /// The waits of the sessions.
    pub(super) timing: Timing,
    inner: Mutex<SyncInner>,
    frames: mpsc::Sender<Item>,
    worker: Mutex<Option<mpsc::Receiver<Item>>>,
    /// Serialises `create_link`, so two creates of one id cannot both pass the check.
    create_guard: Mutex<()>,
    /// Test hook: holds the next `review` check until the sender sends or is dropped.
    #[cfg(test)]
    pub(super) review_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    /// Test hook: how long the step after a payload's last chunk waits before it runs.
    #[cfg(test)]
    pub(super) processing_delay: Mutex<Duration>,
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
            create_guard: Mutex::new(()),
            #[cfg(test)]
            review_gate: Mutex::new(None),
            #[cfg(test)]
            processing_delay: Mutex::new(Duration::ZERO),
            #[cfg(test)]
            frames_seen: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            gone_seen: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, SyncInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn arc(&self) -> Option<Arc<SyncService>> {
        self.me.upgrade()
    }

    /// The time the machines are given. It follows the tokio clock, like the transfer service.
    pub(super) fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    pub(super) fn emit(&self, event: SyncEvent) {
        self.deps.events.publish(&event);
    }

    pub(super) fn link_changed(&self, link_id: &LinkId) {
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

    /// True when a sync payload uses this transfer id. `TransferService` refuses a collection
    /// offer that reuses one.
    pub(crate) fn holds_transfer(&self, id: &TransferId) -> bool {
        self.lock().transfers.contains_key(id)
    }

    // ----- links -----

    /// Stores a new link. An existing link id is `AlreadyExists("That link already exists.")`.
    /// Publishes `LinkChanged`. Blocks (a repository call).
    pub fn create_link(
        &self,
        record: LinkRecord,
        base_files: Vec<(String, Vec<u8>)>,
    ) -> DomainResult<()> {
        {
            let _one_at_a_time = self.create_guard.lock().unwrap_or_else(|e| e.into_inner());
            if self.deps.links.get(&record.link_id)?.is_some() {
                return Err(DomainError::AlreadyExists(LINK_EXISTS.to_string()));
            }
            self.deps.links.save(&record, &base_files)?;
        }
        self.link_changed(&record.link_id);
        Ok(())
    }

    /// The link of the first linked send, on either end (S18). The ZIP passes the strict
    /// validator, then its files (root folder stripped) are the base, so both ends build the
    /// same base from the same bytes. The record takes the local root and uid, state `in_sync`
    /// and `updated_at` now. A peer that is no longer paired gets no link. Blocks.
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
        if self.deps.network.peer_label(&peer).is_none() {
            return Err(cannot());
        }
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
    pub(super) fn folder_matches(&self, record: &LinkRecord) -> DomainResult<bool> {
        let collections = &self.deps.collections;
        let name = &record.collection_name;
        if !collections.exists(name)? {
            return Ok(false);
        }
        Ok(collections.collection_root(name)? == record.collection_root
            && collections.collection_uid(name)? == record.collection_uid)
    }

    /// Deletes a link and its base. Audits `NetworkLinkRemoved` and publishes `LinkChanged`.
    /// The peer is not told; it learns at its next sync. A sync of the link that this node's
    /// user has not answered yet ends `cancelled` first (S17); one that is being applied, or
    /// still saves the link, refuses the unlink with "The sync is already being applied.", so
    /// its link save cannot bring the link back. An unknown link is
    /// `NotFound("That link does not exist.")`.
    pub async fn unlink(&self, link_id: &LinkId) -> DomainResult<()> {
        self.ensure_worker();
        let (running, settling) = {
            let inner = self.lock();
            let session = inner
                .sessions
                .iter()
                .find(|(_, s)| s.link_id == *link_id)
                .map(|(sid, s)| (sid.clone(), s.answered || s.reviewing));
            let settling: HashSet<DeviceId> = inner.settling.iter().copied().collect();
            match session {
                Some((_, true)) => return Err(Self::already_applying()),
                Some((sid, false)) => (Some(sid), settling),
                None => (None, settling),
            }
        };
        if let Some(sid) = running {
            self.stop(&sid, Stop::Cancelled, LinkUpdate::Keep);
        }
        let links = Arc::clone(&self.deps.links);
        let id = link_id.clone();
        let removed = tokio::task::spawn_blocking(move || -> DomainResult<Option<LinkRecord>> {
            let Some(record) = links.get(&id)? else {
                return Ok(None);
            };
            if settling.contains(&record.peer_device_id) {
                return Err(SyncService::already_applying());
            }
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
    pub(super) fn ensure_worker(&self) {
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

    /// One frame from the router. Only short sync work: lock, decide, spawn.
    fn handle_frame(&self, from: DeviceId, message: PeerMessage) {
        match message {
            PeerMessage::SyncOffer {
                sync_id,
                link_id,
                base_hash,
                kind,
                transfer_id,
                size_bytes,
                chunk_count,
                sha256,
            } => {
                let offer = SyncOfferFields {
                    sync_id,
                    link_id,
                    base_hash,
                    kind,
                    transfer_id,
                    size_bytes,
                    chunk_count,
                    sha256,
                };
                let known = SyncId::parse(&offer.sync_id)
                    .is_ok_and(|id| self.lock().sessions.contains_key(&id));
                if kind == SyncPayloadKind::Manifest && !known {
                    self.planner_on_manifest_offer(from, offer);
                } else {
                    self.on_follow_up_offer(from, offer);
                }
            }
            PeerMessage::SyncAbort { sync_id, reason } => {
                if let Ok(id) = SyncId::parse(&sync_id) {
                    self.on_sync_abort(&from, &id, reason);
                }
            }
            PeerMessage::SyncResult { sync_id, outcome } => {
                let Ok(id) = SyncId::parse(&sync_id) else {
                    return;
                };
                let role = self.lock().sessions.get(&id).map(|s| s.role);
                match role {
                    Some(Role::Starter) => self.starter_on_result(&from, &id, outcome),
                    Some(Role::Planner) => self.planner_on_result(&from, &id, outcome),
                    None => {}
                }
            }
            PeerMessage::OfferAnswer {
                transfer_id,
                accept,
            } => {
                if let Some((sid, id)) = self.payload_of(&from, &transfer_id) {
                    self.on_payload_answer(&sid, &id, accept);
                }
            }
            PeerMessage::Chunk {
                transfer_id,
                seq,
                data,
            } => {
                if let Some((sid, id)) = self.payload_of(&from, &transfer_id) {
                    self.on_payload_chunk(&sid, &id, seq, &data);
                }
            }
            PeerMessage::ChunkAck {
                transfer_id,
                next_seq,
            } => {
                if let Some((sid, id)) = self.payload_of(&from, &transfer_id) {
                    self.on_payload_ack(&sid, &id, next_seq);
                }
            }
            PeerMessage::TransferAbort { transfer_id, .. } => {
                if let Some((sid, _)) = self.payload_of(&from, &transfer_id) {
                    self.stop(&sid, Stop::Peer(SyncAbortReason::Failed), LinkUpdate::Keep);
                }
            }
            // A sync payload never gets a `TransferResult`; one that arrives is ignored.
            _ => {}
        }
    }

    /// The session of a sync payload id, only for that payload's own peer.
    fn payload_of(&self, from: &DeviceId, transfer_id: &str) -> Option<(SyncId, TransferId)> {
        let id = TransferId::parse(transfer_id).ok()?;
        let inner = self.lock();
        let (device, sid) = inner.transfers.get(&id)?;
        (device == from).then(|| (sid.clone(), id))
    }

    /// A follow-up `SyncOffer` (a plan or files, or a second manifest). It is taken only for
    /// the session's own id, from its peer, with its link and base hash, in the phase that
    /// expects it (S5, S29). One that comes while this node still finishes its own step is
    /// kept until that step is done. An unknown `sync_id` is ignored.
    fn on_follow_up_offer(&self, from: DeviceId, offer: SyncOfferFields) {
        enum Next {
            Ignore,
            Accept(Phase),
            Kept,
            Bad,
        }
        let Ok(sid) = SyncId::parse(&offer.sync_id) else {
            return;
        };
        let next = {
            let mut inner = self.lock();
            match inner.sessions.get_mut(&sid) {
                None => Next::Ignore,
                Some(s) if s.peer != from => Next::Ignore,
                Some(s)
                    if offer.link_id != s.link_id.as_str() || offer.base_hash != s.base_hash =>
                {
                    Next::Bad
                }
                Some(s) => {
                    use Phase as P;
                    use SyncPayloadKind as K;
                    let free = s.stash.is_none();
                    match (s.role, s.phase, offer.kind) {
                        (Role::Starter, P::AwaitPlan, K::Plan) => Next::Accept(P::RecvPlan),
                        (_, P::AwaitFiles, K::Files) => Next::Accept(P::RecvFiles),
                        (Role::Starter, P::SendManifest | P::SendFiles, K::Plan)
                        | (Role::Planner, P::SendRequest, K::Files)
                            if free =>
                        {
                            s.stash = Some(offer.clone());
                            Next::Kept
                        }
                        (Role::Starter, P::RecvPlan, K::Files) if free && s.inc.is_none() => {
                            s.stash = Some(offer.clone());
                            Next::Kept
                        }
                        _ => Next::Bad,
                    }
                }
            }
        };
        match next {
            Next::Ignore | Next::Kept => {}
            Next::Accept(phase) => self.accept_payload(&sid, offer, phase),
            Next::Bad => self.stop(&sid, Stop::BadPlan, LinkUpdate::Keep),
        }
    }

    /// The peer's `SyncAbort`. A planner that is applying finishes its apply (S7). A starter
    /// that applied, or is applying, keeps its old base and goes `out_of_date` (S14).
    fn on_sync_abort(&self, from: &DeviceId, sid: &SyncId, reason: SyncAbortReason) {
        let found = {
            let inner = self.lock();
            inner
                .sessions
                .get(sid)
                .filter(|s| s.peer == *from)
                .map(|s| (s.role, s.phase))
        };
        let Some((role, phase)) = found else {
            return;
        };
        match (role, phase) {
            (Role::Planner, Phase::Applying) => {}
            (Role::Starter, Phase::Applying | Phase::AwaitResult) => self.stop(
                sid,
                Stop::PeerApplyFailed,
                LinkUpdate::State(LinkState::OutOfDate),
            ),
            _ => {
                let link = if reason == SyncAbortReason::LinkMissing {
                    LinkUpdate::State(LinkState::Broken)
                } else {
                    LinkUpdate::Keep
                };
                self.stop(sid, Stop::Peer(reason), link);
            }
        }
    }

    /// The device's connection ended or its pairing was removed. A session that is applying
    /// finishes; a starter waiting for the result ends `unconfirmed` (S15); every other
    /// session ends "went offline". Then only a removed pairing deletes links (S16): a
    /// blocking task asks `peer_label` twice, `UNPAIR_RECHECK` apart, because a read error
    /// also reads as "not paired", and deletes the device's links only when both say so.
    fn handle_gone(&self, device: DeviceId) {
        let mut unconfirmed = Vec::new();
        let mut offline = Vec::new();
        {
            let mut inner = self.lock();
            for (sid, s) in inner.sessions.iter_mut().filter(|(_, s)| s.peer == device) {
                match s.phase {
                    Phase::Applying => s.peer_gone = true,
                    Phase::AwaitResult if s.result_pending => s.peer_gone = true,
                    Phase::AwaitResult => unconfirmed.push(sid.clone()),
                    _ => offline.push(sid.clone()),
                }
            }
        }
        for sid in unconfirmed {
            self.end_unconfirmed(&sid);
        }
        for sid in offline {
            self.stop(&sid, Stop::WentOffline, LinkUpdate::Keep);
        }
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

    // ----- the session core -----

    /// Runs a step that owns the session's phase (a phase with no deadline) in a blocking task.
    /// A step that panics ends the session, so such a phase cannot hang.
    pub(super) fn spawn_owned(
        &self,
        sync_id: &SyncId,
        step: impl FnOnce(&SyncService) + Send + 'static,
    ) {
        let Some(me) = self.arc() else {
            return;
        };
        let sid = sync_id.clone();
        drop(tokio::spawn(async move {
            let task = Arc::clone(&me);
            if tokio::task::spawn_blocking(move || step(&task))
                .await
                .is_err()
            {
                me.stop(&sid, Stop::Internal, LinkUpdate::Keep);
            }
        }));
    }

    /// Sends a frame and logs a failure.
    pub(super) async fn tell(&self, device: &DeviceId, message: PeerMessage) {
        if let Err(e) = self.deps.network.send_to(device, message).await {
            tracing::debug!(code = "network_sync_send_failed", error = %e, "frame not sent");
        }
    }

    /// Sends a frame from its own task.
    pub(super) fn tell_later(&self, device: DeviceId, message: PeerMessage) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move { me.tell(&device, message).await }));
    }

    /// Sends a frame from its own task and ends the session "went offline" when it fails.
    pub(super) fn tell_or_end(&self, sync_id: &SyncId, device: DeviceId, message: PeerMessage) {
        let Some(me) = self.arc() else {
            return;
        };
        let sid = sync_id.clone();
        drop(tokio::spawn(async move {
            if me.deps.network.send_to(&device, message).await.is_err() {
                me.stop(&sid, Stop::WentOffline, LinkUpdate::Keep);
            }
        }));
    }

    /// The one place a session ends: removes it, frees its ids and limits entry, publishes
    /// `Finished`, audits (S13), tells the peer `abort` from its own task, and runs `link` in a
    /// blocking task that publishes `LinkChanged` when it saved. The peer stays in `settling`
    /// until that task is done. A session that is already gone is left alone.
    pub(super) fn end_session(
        &self,
        sync_id: &SyncId,
        finish: SyncFinish,
        message: Option<String>,
        abort: Option<SyncAbortReason>,
        link: LinkUpdate,
    ) {
        let session = {
            let mut inner = self.lock();
            let Some(session) = inner.take_session(sync_id) else {
                return;
            };
            let saves = !matches!(link, LinkUpdate::Keep);
            if saves {
                inner.settling.insert(session.peer);
            }
            // Under the lock, so a `Progress` of this session stays before its `Finished`.
            self.emit(SyncEvent::Finished {
                sync_id: sync_id.as_str().to_string(),
                link_id: session.link_id.as_str().to_string(),
                display_name: session.peer_name.clone(),
                collection_name: session.collection(),
                outcome: finish,
                message,
            });
            session
        };
        if session.chunk_out {
            let device_id = session.peer.short();
            let collection = session.collection();
            let outcome = finish_name(finish).to_string();
            let kind = match session.role {
                Role::Starter => AuditEventKind::NetworkSyncSent {
                    device_id,
                    collection,
                    outcome,
                },
                Role::Planner => AuditEventKind::NetworkSyncReceived {
                    device_id,
                    collection,
                    outcome,
                },
            };
            self.deps.audit.publish(AuditActor::User.into(), None, kind);
        }
        if let Some(reason) = abort {
            self.tell_later(
                session.peer,
                PeerMessage::SyncAbort {
                    sync_id: sync_id.as_str().to_string(),
                    reason,
                },
            );
        }
        if matches!(link, LinkUpdate::Keep) {
            return;
        }
        let Some(me) = self.arc() else {
            return;
        };
        let peer = session.peer;
        drop(tokio::spawn(async move {
            let task = Arc::clone(&me);
            let link_id = session.link_id.clone();
            let saved = tokio::task::spawn_blocking(move || task.save_link_update(&session, link))
                .await
                .unwrap_or(false);
            me.lock().settling.remove(&peer);
            if saved {
                me.link_changed(&link_id);
            }
        }));
    }

    /// `end_session` with the outcome, text and frame of a `Stop`.
    pub(super) fn stop(&self, sync_id: &SyncId, stop: Stop, link: LinkUpdate) {
        self.end_session(sync_id, stop.finish(), stop.message(), stop.abort(), link);
    }

    /// A starter that sent its result and then lost the peer or the wait: the new base, the
    /// link `out_of_date` (S7, S15).
    pub(super) fn end_unconfirmed(&self, sync_id: &SyncId) {
        let update = {
            let inner = self.lock();
            let Some(s) = inner.sessions.get(sync_id) else {
                return;
            };
            match &s.new_base {
                Some(base) => LinkUpdate::NewBase {
                    base: base.clone(),
                    known: s.known_files(),
                    state: Some(LinkState::OutOfDate),
                },
                None => LinkUpdate::State(LinkState::OutOfDate),
            }
        };
        self.end_session(
            sync_id,
            SyncFinish::Unconfirmed,
            Some(UNCONFIRMED.to_string()),
            None,
            update,
        );
    }

    /// Ends a session the user never saw (planner `Checking` refusals): no event, no audit.
    pub(super) fn drop_quietly(&self, sync_id: &SyncId, abort: SyncAbortReason) {
        let Some(session) = self.lock().take_session(sync_id) else {
            return;
        };
        self.tell_later(
            session.peer,
            PeerMessage::SyncAbort {
                sync_id: sync_id.as_str().to_string(),
                reason: abort,
            },
        );
    }

    /// Removes a session with no event and no frame (a start whose offer was not sent).
    pub(super) fn remove_quietly(&self, sync_id: &SyncId) {
        self.lock().take_session(sync_id);
    }

    /// Moves a session to `phase`, bumps `gen`, resets `phase_since`. Returns the new gen.
    pub(super) fn set_phase(&self, sync_id: &SyncId, phase: Phase) -> Option<u64> {
        let mut inner = self.lock();
        let s = inner.sessions.get_mut(sync_id)?;
        s.enter(phase, Self::now());
        Some(s.gen)
    }

    /// Moves to a phase that waits for the peer's next payload, and takes a kept offer of the
    /// kind it waits for.
    pub(super) fn enter_wait(&self, sync_id: &SyncId, phase: Phase) {
        let kept = {
            let mut inner = self.lock();
            let Some(s) = inner.sessions.get_mut(sync_id) else {
                return;
            };
            s.enter(phase, Self::now());
            let wanted = match phase {
                Phase::AwaitPlan => Some(SyncPayloadKind::Plan),
                Phase::AwaitFiles => Some(SyncPayloadKind::Files),
                _ => None,
            };
            match s.stash.take() {
                Some(offer) if Some(offer.kind) == wanted => Some(Ok(offer)),
                Some(_) => Some(Err(())),
                None => None,
            }
        };
        match kept {
            None => {}
            Some(Ok(offer)) => {
                let recv = match offer.kind {
                    SyncPayloadKind::Plan => Phase::RecvPlan,
                    _ => Phase::RecvFiles,
                };
                self.accept_payload(sync_id, offer, recv);
            }
            Some(Err(())) => self.stop(sync_id, Stop::BadPlan, LinkUpdate::Keep),
        }
    }

    /// Inserts a new session under the caller's lock and starts its watchdog, which takes the
    /// lock only later, from its own task.
    pub(super) fn insert_session(&self, inner: &mut SyncInner, sync_id: SyncId, session: Session) {
        inner.sessions.insert(sync_id.clone(), session);
        self.spawn_watchdog(sync_id);
    }

    /// When the current phase gives up. `None`: a task owns the phase and ends it itself.
    fn deadline(&self, s: &Session) -> Option<Instant> {
        let t = self.timing;
        match s.phase {
            Phase::AwaitConfirm | Phase::AwaitReview => Some(s.phase_since + t.offer_ttl),
            Phase::AwaitResult => Some(s.phase_since + t.result_timeout),
            Phase::AwaitPlan | Phase::AwaitStarter => {
                Some(s.phase_since + t.offer_ttl + t.idle_limit)
            }
            Phase::AwaitFiles => Some(
                s.inc
                    .as_ref()
                    .map_or(s.phase_since, |i| i.last_activity.max(s.phase_since))
                    + t.idle_limit,
            ),
            // After the last chunk a step owns the phase and ends it itself (`spawn_owned`).
            Phase::RecvManifest | Phase::RecvPlan | Phase::RecvFiles => s
                .inc
                .as_ref()
                .map(|i| i.last_activity.max(s.phase_since) + t.idle_limit),
            Phase::SendManifest | Phase::SendRequest | Phase::SendFinal | Phase::SendFiles => {
                Some(s.out.as_ref().map_or(s.phase_since, |o| o.last_ack) + t.idle_limit)
            }
            Phase::Checking
            | Phase::Planning
            | Phase::Exporting
            | Phase::Finalizing
            | Phase::Applying => None,
        }
    }

    /// One task per session: ends it when the deadline of its phase passes with the phase
    /// unchanged. It returns when the session is gone.
    fn spawn_watchdog(&self, sync_id: SyncId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move {
            loop {
                let due = {
                    let inner = me.lock();
                    let Some(s) = inner.sessions.get(&sync_id) else {
                        return;
                    };
                    me.deadline(s).map(|d| (s.gen, d))
                };
                let Some((gen, deadline)) = due else {
                    tokio::time::sleep(me.timing.idle_limit).await;
                    continue;
                };
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                me.on_deadline(&sync_id, gen);
            }
        }));
    }

    /// The deadline of phase `gen` may have passed. The deadline is read again, because an
    /// ack or a chunk moves it without a phase change.
    fn on_deadline(&self, sync_id: &SyncId, gen: u64) {
        let now = Self::now();
        let phase = {
            let inner = self.lock();
            let Some(s) = inner.sessions.get(sync_id) else {
                return;
            };
            let passed = self.deadline(s).is_some_and(|d| d <= now);
            if s.gen != gen || !passed || s.result_pending {
                return;
            }
            s.phase
        };
        match phase {
            Phase::AwaitConfirm | Phase::AwaitReview => {
                self.stop(sync_id, Stop::Expired, LinkUpdate::Keep)
            }
            Phase::AwaitResult => self.end_unconfirmed(sync_id),
            _ => self.stop(sync_id, Stop::Stalled, LinkUpdate::Keep),
        }
    }

    /// Saves the link after a session ended. Only while the peer is still paired and the
    /// link still exists for it (S7, S16). Blocks. True when it saved.
    fn save_link_update(&self, session: &Session, update: LinkUpdate) -> bool {
        if self.deps.network.peer_label(&session.peer).is_none() {
            return false;
        }
        let record = match self.deps.links.get(&session.link_id) {
            Ok(Some(record)) if record.peer_device_id == session.peer => record,
            Ok(_) => return false,
            Err(e) => {
                tracing::warn!(code = "network_link_read_failed", "link not updated");
                tracing::debug!(error = %e, "link not updated");
                return false;
            }
        };
        let saved = match update {
            LinkUpdate::Keep => return false,
            LinkUpdate::State(state) => {
                let mut record = record;
                record.state = state;
                record.updated_at = chrono::Utc::now().to_rfc3339();
                self.deps.links.save(&record, &[])
            }
            LinkUpdate::NewBase { base, known, state } => {
                let files = self.new_base_files(&record, &base, &known);
                let state = state.unwrap_or_else(|| {
                    match self.deps.collections.manifest(&record.collection_name) {
                        Ok(fresh) if fresh == base => LinkState::InSync,
                        _ => LinkState::ChangesWaiting,
                    }
                });
                let mut record = record;
                record.base = base;
                record.state = state;
                record.updated_at = chrono::Utc::now().to_rfc3339();
                self.deps.links.save(&record, &files)
            }
        };
        match saved {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(code = "network_link_save_failed", "link not updated");
                tracing::debug!(error = %e, "link not updated");
                false
            }
        }
    }

    /// The base copies to store with a new base: only paths whose hash changed (the repository
    /// keeps the others). Each comes from bytes that crossed the wire or were exported for the
    /// plan, from an old base copy with the same hash, or from one export after the apply, and
    /// is kept only when its hash matches (S8).
    fn new_base_files(
        &self,
        record: &LinkRecord,
        base: &Manifest,
        known: &BTreeMap<String, Vec<u8>>,
    ) -> Vec<(String, Vec<u8>)> {
        let mut files = Vec::new();
        let mut missing = Vec::new();
        for (path, hash) in base.iter() {
            if record.base.get(path) == Some(hash) {
                continue;
            }
            if let Some(bytes) = known.get(path).filter(|b| sha_hex(b) == hash) {
                files.push((path.to_string(), bytes.clone()));
                continue;
            }
            let old = record
                .base
                .iter()
                .find(|(_, h)| *h == hash)
                .and_then(|(p, _)| self.deps.links.read_base_file(&record.link_id, p).ok())
                .flatten()
                .filter(|b| sha_hex(b) == hash);
            match old {
                Some(bytes) => files.push((path.to_string(), bytes)),
                None => missing.push(path.to_string()),
            }
        }
        if !missing.is_empty() {
            match self
                .deps
                .collections
                .export_files(&record.collection_name, &missing)
            {
                Ok(exported) => {
                    for file in exported {
                        if base.get(&file.path) == Some(sha_hex(&file.bytes).as_str()) {
                            files.push((file.path, file.bytes));
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(code = "network_link_base_export_failed", error = %e, "base copies left out");
                }
            }
        }
        files
    }

    /// Saves a link `broken` (the folder or uid changed) and announces it. Blocks.
    pub(super) fn mark_broken(&self, record: &LinkRecord) {
        if self
            .deps
            .network
            .peer_label(&record.peer_device_id)
            .is_none()
        {
            return;
        }
        let mut record = record.clone();
        record.state = LinkState::Broken;
        record.updated_at = chrono::Utc::now().to_rfc3339();
        match self.deps.links.save(&record, &[]) {
            Ok(()) => self.link_changed(&record.link_id),
            Err(e) => {
                tracing::warn!(code = "network_link_save_failed", "link not marked broken");
                tracing::debug!(error = %e, "link not marked broken");
            }
        }
    }

    /// Cancels a sync (S17). Before this node's dialog the sync ends `cancelled`; during it,
    /// it is a decline; after this node's user answered it is refused ("The sync is already
    /// being applied."). While a `review` answer is being checked it is refused with "The
    /// answer to the review is being checked. Try again in a moment.". An unknown or finished
    /// sync is not an error.
    pub async fn cancel(&self, sync_id: &SyncId) -> DomainResult<()> {
        let phase = {
            let inner = self.lock();
            match inner.sessions.get(sync_id) {
                None => return Ok(()),
                Some(s) if s.answered => return Err(Self::already_applying()),
                // The user's answer is being checked; it either opens the review again or
                // goes on to the apply, so the cancel cannot decide yet.
                Some(s) if s.reviewing => {
                    return Err(DomainError::InvalidInput(REVIEW_CHECKING.to_string()))
                }
                Some(s) => s.phase,
            }
        };
        match phase {
            Phase::AwaitReview => self.review(sync_id, false, Vec::new()).await,
            Phase::AwaitConfirm => self.confirm(sync_id, false).await,
            _ => {
                self.stop(sync_id, Stop::Cancelled, LinkUpdate::Keep);
                Ok(())
            }
        }
    }

    /// The two texts of a conflict, for the diff. Only while the planner's review is open.
    pub fn conflict_files(&self, sync_id: &SyncId, path: &str) -> DomainResult<ConflictFiles> {
        let inner = self.lock();
        let s = inner
            .sessions
            .get(sync_id)
            .filter(|s| s.role == Role::Planner && s.phase == Phase::AwaitReview)
            .ok_or_else(|| DomainError::InvalidInput(NOT_WAITING.to_string()))?;
        let plan = s
            .plan
            .as_ref()
            .ok_or_else(|| DomainError::InvalidInput(NOT_WAITING.to_string()))?;
        let conflict = plan
            .conflicts
            .iter()
            .find(|c| c.path == path)
            .ok_or_else(|| DomainError::InvalidInput(NOT_A_CONFLICT.to_string()))?;
        let (mine_path, theirs_path) = conflict_paths(plan, &conflict.path, conflict.kind);
        let text = |bytes: Option<&Vec<u8>>| bytes.map(|b| String::from_utf8_lossy(b).into_owned());
        Ok(ConflictFiles {
            mine: text(s.mine_conflicts.get(&mine_path)),
            theirs: text(s.received.get(&theirs_path)),
        })
    }

    /// The fixed answer once this node's user answered the sync.
    pub(super) fn already_applying() -> DomainError {
        DomainError::InvalidInput(ALREADY_APPLYING.to_string())
    }

    /// The fixed answer for a decision on a path that is not a conflict.
    pub(super) fn not_a_conflict() -> DomainError {
        DomainError::InvalidInput(NOT_A_CONFLICT.to_string())
    }

    /// The fixed answer when a sync no longer waits for this answer.
    pub(super) fn not_waiting() -> DomainError {
        DomainError::InvalidInput(NOT_WAITING.to_string())
    }

    /// Test hook: the phase of a session, `None` once it ended.
    #[cfg(test)]
    pub(crate) fn phase_of(&self, sync_id: &SyncId) -> Option<Phase> {
        self.lock().sessions.get(sync_id).map(|s| s.phase)
    }

    /// The result frame for the peer.
    pub(super) fn result_frame(sync_id: &SyncId, outcome: SyncOutcome) -> PeerMessage {
        PeerMessage::SyncResult {
            sync_id: sync_id.as_str().to_string(),
            outcome,
        }
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
