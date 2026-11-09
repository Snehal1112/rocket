//! The starter side of a sync: offer the manifest, answer a request with the wanted files,
//! check the final plan and the files, ask the user, apply first, report, store the base.

use std::collections::BTreeMap;

use rocket_network::limits::MAX_TRANSFER_BYTES;
use rocket_network::{
    LinkId, LinkRecord, LinkState, Manifest, SyncEvent, SyncFinish, SyncId, SyncOutcome,
    SyncPayloadKind, SyncPlanPayload, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};

use super::sync::{LinkUpdate, Phase, Role, Session, SyncService, NO_SUCH_LINK};
use super::sync_checks::{
    check_final, check_request, entries_match, starter_summary, Stop, FOLDER_CHANGED,
    RESULT_NOT_SENT,
};
use super::sync_payload::{build_out, pack_zip, sha_hex, sync_offer, unpack_zip};
use super::sync_ports::FileChange;

const BROKEN_LINK: &str = "This link is broken. Unlink and send the collection again.";
const NOT_PAIRED: &str = "That node is not paired.";
const OFFLINE: &str = "That node is offline.";
const ALREADY_RUNNING: &str = "A sync with that node is already running.";

fn invalid(text: &str) -> DomainError {
    DomainError::InvalidInput(text.to_string())
}

/// What the start's blocking step found.
enum Built {
    FolderChanged,
    Ready(Manifest, super::sync_payload::PayloadOut),
}

impl SyncService {
    /// Starts a sync of one link. Checks, in order: the link exists ("That link does not
    /// exist."), is not broken ("This link is broken. Unlink and send the collection again."),
    /// the peer is paired ("That node is not paired.") and online ("That node is offline."),
    /// no session, start or settle runs for the peer ("A sync with that node is already
    /// running."). Then, in a blocking task: the collection still exists with the link's uid
    /// and root (else the link is saved `broken` and FOLDER_CHANGED is returned), its manifest
    /// is built and offered. Returns the sync id once the offer frame was sent; a failed send
    /// removes the session without an event.
    pub async fn start(&self, link_id: &LinkId) -> DomainResult<SyncId> {
        self.ensure_worker();
        let links = std::sync::Arc::clone(&self.deps.links);
        let id = link_id.clone();
        let record = tokio::task::spawn_blocking(move || links.get(&id))
            .await
            .map_err(|_| DomainError::Internal("The sync could not start.".to_string()))??
            .ok_or_else(|| invalid(NO_SUCH_LINK))?;
        if record.state == LinkState::Broken {
            return Err(invalid(BROKEN_LINK));
        }
        let peer = record.peer_device_id;
        let Some((peer_name, _)) = self.deps.network.peer_label(&peer) else {
            return Err(invalid(NOT_PAIRED));
        };
        if !self.deps.network.is_online(&peer) {
            return Err(invalid(OFFLINE));
        }
        {
            let mut inner = self.lock();
            if inner.peer_busy(&peer) {
                return Err(invalid(ALREADY_RUNNING));
            }
            inner.starting.insert(peer);
        }
        let started = self.start_claimed(record, peer_name).await;
        self.lock().starting.remove(&peer);
        started
    }

    /// The part of `start` after the peer is claimed in `starting`.
    async fn start_claimed(&self, record: LinkRecord, peer_name: String) -> DomainResult<SyncId> {
        let Some(me) = self.arc() else {
            return Err(DomainError::Internal(
                "The sync could not start.".to_string(),
            ));
        };
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        let rec = record.clone();
        let built = tokio::task::spawn_blocking(move || -> DomainResult<Built> {
            if !me.folder_matches(&rec)? {
                me.mark_broken(&rec);
                return Ok(Built::FolderChanged);
            }
            let mine = me.deps.collections.manifest(&rec.collection_name)?;
            let bytes = mine.to_json()?;
            let out = build_out(SyncPayloadKind::Manifest, bytes, now, ttl).map_err(|_| {
                DomainError::Internal("The manifest could not be offered.".to_string())
            })?;
            Ok(Built::Ready(mine, out))
        })
        .await
        .map_err(|_| DomainError::Internal("The sync could not start.".to_string()))??;
        let (mine, out) = match built {
            Built::FolderChanged => return Err(invalid(FOLDER_CHANGED)),
            Built::Ready(mine, out) => (mine, out),
        };

        let sid = SyncId::random();
        let peer = record.peer_device_id;
        let frame = {
            let mut inner = self.lock();
            let key = TransferId::random();
            let payload_id = out.machine.transfer_id().clone();
            if inner.transfers.contains_key(&payload_id)
                || !inner.limits.begin_outgoing(&peer, &key)
            {
                return Err(invalid(ALREADY_RUNNING));
            }
            let frame = sync_offer(&out, &sid, &record.link_id, &record.base_hash());
            let mut session = Session::new(
                Role::Starter,
                peer,
                peer_name,
                record.link_id.clone(),
                record.base_hash(),
                key,
                Phase::SendManifest,
                Self::now(),
            );
            session.mine = mine;
            session.link = Some(record);
            session.out = Some(out);
            inner.transfers.insert(payload_id, (peer, sid.clone()));
            self.insert_session(&mut inner, sid.clone(), session);
            frame
        };
        if let Err(e) = self.deps.network.send_to(&peer, frame).await {
            self.remove_quietly(&sid);
            return Err(e);
        }
        Ok(sid)
    }

    /// The starter user's answer to `ConfirmNeeded`. `false` ends the sync `Declined`.
    /// `InvalidInput("The sync is no longer waiting for an answer.")` when not in AwaitConfirm.
    pub async fn confirm(&self, sync_id: &SyncId, accept: bool) -> DomainResult<()> {
        self.ensure_worker();
        {
            let mut inner = self.lock();
            let s = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.role == Role::Starter && s.phase == Phase::AwaitConfirm)
                .ok_or_else(Self::not_waiting)?;
            if accept {
                s.answered = true;
                s.enter(Phase::Applying, Self::now());
            }
        }
        if accept {
            self.spawn_starter_apply(sync_id.clone());
        } else {
            self.stop(sync_id, Stop::Declined, LinkUpdate::Keep);
        }
        Ok(())
    }

    /// The final ack of one of the starter's payloads: wait for the planner's next plan.
    pub(super) fn starter_payload_sent(&self, sync_id: &SyncId, _kind: SyncPayloadKind) {
        self.enter_wait(sync_id, Phase::AwaitPlan);
    }

    /// A whole payload from the planner, checked for size and hash. Runs in a blocking task.
    pub(super) fn starter_payload_received(
        &self,
        sync_id: &SyncId,
        kind: SyncPayloadKind,
        bytes: Vec<u8>,
    ) {
        let result = match kind {
            SyncPayloadKind::Plan => self.starter_plan(sync_id, &bytes),
            SyncPayloadKind::Files => self.starter_files(sync_id, &bytes),
            SyncPayloadKind::Manifest => Err(Stop::BadPlan),
        };
        if let Err(stop) = result {
            self.stop(sync_id, stop, LinkUpdate::Keep);
        }
    }

    /// A request plan (answered with the wanted files) or the final plan.
    fn starter_plan(&self, sync_id: &SyncId, bytes: &[u8]) -> Result<(), Stop> {
        let plan = SyncPlanPayload::from_json(bytes).map_err(|_| Stop::BadPlan)?;
        let (mine, collection, asked_before) = {
            let inner = self.lock();
            let Some(s) = inner
                .sessions
                .get(sync_id)
                .filter(|s| s.phase == Phase::RecvPlan)
            else {
                return Ok(());
            };
            (s.mine.clone(), s.collection(), s.request_done)
        };
        if plan.new_base_hash.is_none() {
            if asked_before {
                return Err(Stop::BadPlan);
            }
            return self.answer_request(sync_id, &plan, &mine, &collection);
        }
        let new_base = check_final(&plan, &mine)?;
        enum Next {
            Files,
            Apply,
            Confirm(SyncEvent),
        }
        let next = {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::RecvPlan)
            else {
                return Ok(());
            };
            let changes_here = !plan.deletes.is_empty() || !plan.renames.is_empty();
            let with_files = !plan.sends.is_empty();
            if !with_files && s.stash.is_some() {
                return Err(Stop::BadPlan);
            }
            s.final_plan = Some(plan.clone());
            s.new_base = Some(new_base);
            if with_files {
                Next::Files
            } else if !changes_here {
                // Nothing changes here: no confirm, but the apply still checks the manifest (S12).
                s.answered = true;
                s.enter(Phase::Applying, Self::now());
                Next::Apply
            } else {
                let summary = starter_summary(&plan, &s.mine, s.sent_files.len(), &s.received);
                s.enter(Phase::AwaitConfirm, Self::now());
                Next::Confirm(confirm_event(sync_id, s, summary))
            }
        };
        match next {
            Next::Files => self.enter_wait(sync_id, Phase::AwaitFiles),
            Next::Apply => self.spawn_starter_apply(sync_id.clone()),
            Next::Confirm(event) => self.emit(event),
        }
        Ok(())
    }

    /// Exports the wanted files, checks them against the offered manifest and offers them.
    fn answer_request(
        &self,
        sync_id: &SyncId,
        plan: &SyncPlanPayload,
        mine: &Manifest,
        collection: &str,
    ) -> Result<(), Stop> {
        let wanted: Vec<String> = check_request(plan, mine)?.into_iter().collect();
        {
            let mut inner = self.lock();
            let Some(s) = inner.sessions.get_mut(sync_id) else {
                return Ok(());
            };
            s.request_done = true;
            s.enter(Phase::Exporting, Self::now());
        }
        let files = self
            .deps
            .collections
            .export_files(collection, &wanted)
            .map_err(|_| Stop::Changed)?;
        if files.len() != wanted.len()
            || files
                .iter()
                .any(|f| mine.get(&f.path) != Some(sha_hex(&f.bytes).as_str()))
        {
            return Err(Stop::Changed);
        }
        let zip = pack_zip(&files).map_err(|_| Stop::Internal)?;
        if zip.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(Stop::TooBig(zip.len() as u64));
        }
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::Exporting)
            else {
                return Ok(());
            };
            s.sent_files = files.into_iter().map(|f| (f.path, f.bytes)).collect();
        }
        self.offer_payload(sync_id, SyncPayloadKind::Files, zip, Phase::SendFiles)
    }

    /// The planner's files: exactly the `sends` of the final plan, each with its hash.
    fn starter_files(&self, sync_id: &SyncId, bytes: &[u8]) -> Result<(), Stop> {
        let files = unpack_zip(bytes)?;
        let event = {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::RecvFiles)
            else {
                return Ok(());
            };
            let Some(plan) = s.final_plan.clone() else {
                return Err(Stop::BadPlan);
            };
            let expected: BTreeMap<String, String> = plan
                .sends
                .iter()
                .map(|f| (f.path.clone(), f.sha256.clone()))
                .collect();
            entries_match(&files, &expected)?;
            s.received = files;
            let summary = starter_summary(&plan, &s.mine, s.sent_files.len(), &s.received);
            s.enter(Phase::AwaitConfirm, Self::now());
            confirm_event(sync_id, s, summary)
        };
        self.emit(event);
        Ok(())
    }

    fn spawn_starter_apply(&self, sync_id: SyncId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(
            async move { me.starter_apply(&sync_id).await },
        ));
    }

    /// Applies the final plan here first (S7): deletes, renames and the received files, all
    /// through `apply_if_unchanged` against the offered manifest, then sends `applied`.
    async fn starter_apply(&self, sync_id: &SyncId) {
        let prepared = {
            let inner = self.lock();
            let Some(s) = inner
                .sessions
                .get(sync_id)
                .filter(|s| s.phase == Phase::Applying)
            else {
                return;
            };
            starter_changes(s).map(|changes| (s.collection(), s.mine.clone(), changes, s.peer))
        };
        let (collection, mine, changes, peer) = match prepared {
            Ok(found) => found,
            Err(stop) => return self.apply_failed(sync_id, stop),
        };
        let collections = std::sync::Arc::clone(&self.deps.collections);
        let applied = tokio::task::spawn_blocking(move || {
            collections.apply_if_unchanged(&collection, &mine, &changes)
        })
        .await;
        match applied {
            Ok(Ok(())) => {}
            Ok(Err(DomainError::Conflict(_))) => {
                return self.apply_failed(sync_id, Stop::ApplyConflict)
            }
            Ok(Err(e)) => {
                tracing::warn!(code = "network_sync_apply_failed", "sync apply failed");
                tracing::debug!(error = %e, "sync apply failed");
                return self.apply_failed(sync_id, Stop::ApplyFailed);
            }
            Err(_) => return self.apply_failed(sync_id, Stop::ApplyFailed),
        }
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::Applying)
            else {
                return;
            };
            // The result wait starts now; a peer that goes while the frame is written is
            // decided below, once it is known whether the frame left.
            s.result_pending = true;
            s.enter(Phase::AwaitResult, Self::now());
        }
        let sent = self
            .deps
            .network
            .send_to(&peer, Self::result_frame(sync_id, SyncOutcome::Applied))
            .await;
        if sent.is_err() {
            // The planner surely did not apply: the old base stays.
            return self.end_session(
                sync_id,
                SyncFinish::Unconfirmed,
                Some(RESULT_NOT_SENT.to_string()),
                None,
                LinkUpdate::State(LinkState::OutOfDate),
            );
        }
        let gone = {
            let mut inner = self.lock();
            match inner.sessions.get_mut(sync_id) {
                Some(s) => {
                    s.result_pending = false;
                    s.peer_gone
                }
                None => false,
            }
        };
        if gone {
            self.end_unconfirmed(sync_id);
        }
    }

    /// This node's apply failed or did not run: the peer is told `failed`, the old base stays
    /// and the link goes `out_of_date` (S17, S18).
    pub(super) fn apply_failed(&self, sync_id: &SyncId, stop: Stop) {
        let peer = self.lock().sessions.get(sync_id).map(|s| s.peer);
        let Some(peer) = peer else {
            return;
        };
        self.end_session(
            sync_id,
            stop.finish(),
            stop.message(),
            None,
            LinkUpdate::State(LinkState::OutOfDate),
        );
        self.tell_later(peer, Self::result_frame(sync_id, SyncOutcome::Failed));
    }

    /// The planner's `SyncResult`. Only while this node waits for it; any other is a bad plan.
    pub(super) fn starter_on_result(
        &self,
        from: &rocket_network::DeviceId,
        sync_id: &SyncId,
        outcome: SyncOutcome,
    ) {
        let update = {
            let inner = self.lock();
            let Some(s) = inner.sessions.get(sync_id).filter(|s| s.peer == *from) else {
                return;
            };
            if s.phase != Phase::AwaitResult {
                None
            } else {
                s.new_base.clone().map(|base| (base, s.known_files()))
            }
        };
        match (update, outcome) {
            (None, _) => self.stop(sync_id, Stop::BadPlan, LinkUpdate::Keep),
            (Some((base, known)), SyncOutcome::Applied) => self.end_session(
                sync_id,
                SyncFinish::Applied,
                None,
                None,
                LinkUpdate::NewBase {
                    base,
                    known,
                    state: None,
                },
            ),
            (Some(_), SyncOutcome::Failed) => self.stop(
                sync_id,
                Stop::PeerApplyFailed,
                LinkUpdate::State(LinkState::OutOfDate),
            ),
        }
    }
}

/// The starter's changes in apply order: deletes, renames, then a `Put` per received file.
fn starter_changes(s: &Session) -> Result<Vec<FileChange>, Stop> {
    let plan = s.final_plan.as_ref().ok_or(Stop::Internal)?;
    let mut changes: Vec<FileChange> = plan
        .deletes
        .iter()
        .map(|path| FileChange::Delete { path: path.clone() })
        .collect();
    changes.extend(plan.renames.iter().map(|r| FileChange::Rename {
        from: r.from.clone(),
        to: r.to.clone(),
    }));
    for send in &plan.sends {
        let bytes = s.received.get(&send.path).ok_or(Stop::Internal)?;
        changes.push(FileChange::Put {
            path: send.path.clone(),
            bytes: bytes.clone(),
        });
    }
    Ok(changes)
}

fn confirm_event(sync_id: &SyncId, s: &Session, summary: rocket_network::SyncSummary) -> SyncEvent {
    SyncEvent::ConfirmNeeded {
        sync_id: sync_id.as_str().to_string(),
        link_id: s.link_id.as_str().to_string(),
        device_id: s.peer.to_base32(),
        short_id: s.peer.short(),
        display_name: s.peer_name.clone(),
        collection_name: s.collection(),
        summary,
    }
}
