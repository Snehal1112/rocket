//! The planner side of a sync: check the link and the base, plan, ask for the files it needs,
//! show the review, send the final plan and its files, apply after the starter, report.

use std::collections::BTreeMap;
use std::sync::Arc;

use rocket_network::limits::MAX_TRANSFER_BYTES;
use rocket_network::{
    new_base, plan_sync, resolve, Admission, ConflictView, Decision, DeviceId, LinkId, LinkState,
    Manifest, PeerMessage, PlanFile, PlanRename, Resolution, SyncAbortReason, SyncAction,
    SyncEvent, SyncFinish, SyncId, SyncOutcome, SyncPayloadKind, SyncPlanPayload, SyncRole,
    TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};

use super::sync::{LinkUpdate, Phase, Role, Session, SyncOfferFields, SyncService};
use super::sync_checks::{
    apply_to_manifest, conflict_paths, entries_match, is_root_file, keep_both_allowed,
    path_allowed, planner_summary, split_actions, starter_new_base, wanted_paths, Stop,
    ALREADY_IN_SYNC,
};
use super::sync_payload::{pack_zip, sha_hex, unpack_zip};
use super::sync_ports::FileChange;

const KEEP_BOTH_SETTINGS: &str = "Keep both is not possible for collection or folder settings.";
/// `resolve` refused the decisions (one missing, one twice). Its own text is not shown.
const DECISIONS_NEEDED: &str = "Choose one answer for every conflict.";

/// What the final step needs from the session, read under the lock.
struct Finalize {
    mine: Manifest,
    theirs: Manifest,
    base: Manifest,
    collection: String,
    received: BTreeMap<String, Vec<u8>>,
}

impl SyncService {
    /// A `SyncOffer { manifest }` that opens a new session (worker, no I/O). The peer must be
    /// paired and the ids must parse, else it is ignored. The sync limits decide next, then
    /// the one-session-per-peer rule (S5, S6), both answered `busy`. An admitted offer opens a
    /// `Checking` session; its check runs in a blocking task.
    pub(super) fn planner_on_manifest_offer(&self, from: DeviceId, offer: SyncOfferFields) {
        let Some((peer_name, _)) = self.deps.network.peer_label(&from) else {
            return;
        };
        let (Ok(sid), Ok(link_id), Ok(tid)) = (
            SyncId::parse(&offer.sync_id),
            LinkId::parse(&offer.link_id),
            TransferId::parse(&offer.transfer_id),
        ) else {
            return;
        };
        let busy = {
            let mut inner = self.lock();
            match inner.limits.admit_offer(&from, &tid, Self::now()) {
                Admission::Duplicate => return,
                Admission::Refuse(_) => true,
                Admission::Admit
                    if inner.peer_busy(&from)
                        || inner.sessions.contains_key(&sid)
                        || inner.transfers.contains_key(&tid) =>
                {
                    inner.limits.release(&tid);
                    true
                }
                Admission::Admit => {
                    let session = Session::new(
                        Role::Planner,
                        from,
                        peer_name,
                        link_id,
                        offer.base_hash.clone(),
                        tid.clone(),
                        Phase::Checking,
                        Self::now(),
                    );
                    // Registered now, so no collection offer can take the id while the link
                    // is checked (the transfer service refuses an id a sync payload holds).
                    inner.transfers.insert(tid.clone(), (from, sid.clone()));
                    self.insert_session(&mut inner, sid.clone(), session);
                    false
                }
            }
        };
        if busy {
            self.tell_later(
                from,
                PeerMessage::SyncAbort {
                    sync_id: sid.as_str().to_string(),
                    reason: SyncAbortReason::Busy,
                },
            );
            return;
        }
        let check = sid.clone();
        self.spawn_owned(&sid, move |me| me.planner_check(&check, from, offer));
    }

    /// The `Checking` step (P3 to P6): the link exists for this peer and is not broken, the
    /// folder is the linked one, the base hashes agree. Then the manifest is taken. Blocks.
    fn planner_check(&self, sid: &SyncId, from: DeviceId, offer: SyncOfferFields) {
        let link_id = match LinkId::parse(&offer.link_id) {
            Ok(id) => id,
            Err(_) => return self.drop_quietly(sid, SyncAbortReason::LinkMissing),
        };
        let record = match self.deps.links.get(&link_id) {
            Ok(Some(r)) if r.peer_device_id == from && r.state != LinkState::Broken => r,
            Ok(_) => return self.drop_quietly(sid, SyncAbortReason::LinkMissing),
            Err(_) => return self.drop_quietly(sid, SyncAbortReason::Failed),
        };
        match self.folder_matches(&record) {
            Ok(true) => {}
            Ok(false) => {
                self.mark_broken(&record);
                return self.drop_quietly(sid, SyncAbortReason::LinkMissing);
            }
            Err(_) => return self.drop_quietly(sid, SyncAbortReason::Failed),
        }
        if record.base_hash() != offer.base_hash {
            return self.drop_quietly(sid, SyncAbortReason::BaseMismatch);
        }
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sid)
                .filter(|s| s.phase == Phase::Checking)
            else {
                return;
            };
            s.link = Some(record);
        }
        self.accept_payload(sid, offer, Phase::RecvManifest);
    }

    /// The final ack of one of the planner's payloads. After the request it waits for the
    /// files; after the final plan it sends its files, or waits for the starter.
    pub(super) fn planner_payload_sent(&self, sync_id: &SyncId, kind: SyncPayloadKind) {
        enum Next {
            WaitFiles,
            SendFiles(Vec<u8>),
            WaitStarter,
        }
        let next = {
            let mut inner = self.lock();
            let Some(s) = inner.sessions.get_mut(sync_id) else {
                return;
            };
            match (kind, s.final_plan.is_some()) {
                (SyncPayloadKind::Plan, false) => Next::WaitFiles,
                (SyncPayloadKind::Plan, true) => match s.files_zip.take() {
                    Some(zip) => Next::SendFiles(zip),
                    None => Next::WaitStarter,
                },
                _ => Next::WaitStarter,
            }
        };
        match next {
            Next::WaitFiles => self.enter_wait(sync_id, Phase::AwaitFiles),
            Next::WaitStarter => {
                self.set_phase(sync_id, Phase::AwaitStarter);
            }
            Next::SendFiles(zip) => {
                if let Err(stop) =
                    self.offer_payload(sync_id, SyncPayloadKind::Files, zip, Phase::SendFiles)
                {
                    self.stop(sync_id, stop, LinkUpdate::Keep);
                }
            }
        }
    }

    /// A whole payload from the starter, checked for size and hash. Runs in a blocking task.
    pub(super) fn planner_payload_received(
        &self,
        sync_id: &SyncId,
        kind: SyncPayloadKind,
        bytes: Vec<u8>,
    ) {
        let result = match kind {
            SyncPayloadKind::Manifest => self.planner_manifest(sync_id, &bytes),
            SyncPayloadKind::Files => self.planner_files(sync_id, &bytes),
            SyncPayloadKind::Plan => Err(Stop::BadPlan),
        };
        if let Err(stop) = result {
            self.stop(sync_id, stop, LinkUpdate::Keep);
        }
    }

    /// Plans the sync from the starter's manifest (P7 to P11).
    fn planner_manifest(&self, sync_id: &SyncId, bytes: &[u8]) -> Result<(), Stop> {
        let theirs = Manifest::from_json(bytes).map_err(|_| Stop::Unsafe)?;
        if !theirs.paths().all(path_allowed) {
            return Err(Stop::Unsafe);
        }
        let record = {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::RecvManifest)
            else {
                return Ok(());
            };
            s.enter(Phase::Planning, Self::now());
            s.link.clone().ok_or(Stop::Internal)?
        };
        let collection = record.collection_name.clone();
        let mine = self
            .deps
            .collections
            .manifest(&collection)
            .map_err(|_| Stop::Internal)?;
        // Paths that differ only by letter case would be planned as two files (Plan 1 rule).
        // This also covers a rename that changes only letter case.
        if mine.case_clashes_with(&theirs) {
            return Err(Stop::CaseClash);
        }
        let plan = plan_sync(&record.base, &mine, &theirs);
        let mut own: Vec<String> = plan
            .conflicts
            .iter()
            .map(|c| conflict_paths(&plan, &c.path, c.kind).0)
            .filter(|p| mine.get(p).is_some())
            .collect();
        own.sort();
        own.dedup();
        let mine_conflicts = self.export_checked(&collection, &own, &mine)?;
        let mut wanted = BTreeMap::new();
        for path in wanted_paths(&plan) {
            let hash = theirs.get(&path).ok_or(Stop::Internal)?;
            wanted.insert(path, hash.to_string());
        }
        let empty = plan.actions.is_empty() && plan.conflicts.is_empty();
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::Planning)
            else {
                return Ok(());
            };
            s.mine = mine;
            s.theirs = Some(theirs);
            s.plan = Some(plan.clone());
            s.wanted = wanted.clone();
            s.mine_conflicts = mine_conflicts;
            if empty {
                // Nothing to review (S12): the empty final plan goes out at once.
                s.answered = true;
                s.enter(Phase::Finalizing, Self::now());
                let key = s.key.clone();
                inner.limits.mark_active(&key);
            }
        }
        if empty {
            return self.finalize(sync_id, Vec::new(), Vec::new());
        }
        if wanted.is_empty() {
            self.open_review(sync_id);
            return Ok(());
        }
        let request = SyncPlanPayload {
            wanted: wanted.keys().cloned().collect(),
            ..SyncPlanPayload::default()
        };
        let bytes = request.to_json().map_err(|_| Stop::Internal)?;
        self.offer_payload(sync_id, SyncPayloadKind::Plan, bytes, Phase::SendRequest)
    }

    /// The starter's files: exactly the wanted set, each with the hash of its manifest.
    fn planner_files(&self, sync_id: &SyncId, bytes: &[u8]) -> Result<(), Stop> {
        let files = unpack_zip(bytes)?;
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::RecvFiles)
            else {
                return Ok(());
            };
            entries_match(&files, &s.wanted)?;
            s.received = files;
        }
        self.open_review(sync_id);
        Ok(())
    }

    /// Exports `paths` and checks each against `mine` (`Changed` when a file moved on).
    fn export_checked(
        &self,
        collection: &str,
        paths: &[String],
        mine: &Manifest,
    ) -> Result<BTreeMap<String, Vec<u8>>, Stop> {
        if paths.is_empty() {
            return Ok(BTreeMap::new());
        }
        let files = self
            .deps
            .collections
            .export_files(collection, paths)
            .map_err(|_| Stop::Changed)?;
        if files.len() != paths.len()
            || files
                .iter()
                .any(|f| mine.get(&f.path) != Some(sha_hex(&f.bytes).as_str()))
        {
            return Err(Stop::Changed);
        }
        Ok(files.into_iter().map(|f| (f.path, f.bytes)).collect())
    }

    /// Moves to `AwaitReview` and publishes `ReviewReady`.
    fn open_review(&self, sync_id: &SyncId) {
        let mut inner = self.lock();
        let Some(s) = inner.sessions.get_mut(sync_id) else {
            return;
        };
        let (Some(plan), Some(theirs)) = (s.plan.as_ref(), s.theirs.as_ref()) else {
            return;
        };
        let summary = planner_summary(plan, &s.mine, theirs, &s.received);
        let conflicts = plan
            .conflicts
            .iter()
            .map(|c| ConflictView {
                path: c.path.clone(),
                kind: c.kind,
                mine_hash: c.mine.clone(),
                theirs_hash: c.theirs.clone(),
            })
            .collect();
        let event = SyncEvent::ReviewReady {
            sync_id: sync_id.as_str().to_string(),
            link_id: s.link_id.as_str().to_string(),
            device_id: s.peer.to_base32(),
            short_id: s.peer.short(),
            display_name: s.peer_name.clone(),
            collection_name: s.collection(),
            summary,
            conflicts,
            role: SyncRole::Planner,
        };
        s.enter(Phase::AwaitReview, Self::now());
        // Under the lock, so an expiry cannot publish `Finished` before it.
        self.emit(event);
    }

    /// The planner user's answer to `ReviewReady`. `false` ends the sync `Declined`. With
    /// `true`, every conflict needs one decision (`resolve` decides; its refusal is returned
    /// and the review stays open), a decision on a path that is not a conflict is
    /// `InvalidInput("That file is not a conflict.")`, and `keep_both` on a settings file is
    /// `InvalidInput("Keep both is not possible for collection or folder settings.")`.
    /// Returns `Ok(())` once the final plan is offered or once the session ended for a reason
    /// of its own, which reaches the user through `Finished`.
    pub async fn review(
        &self,
        sync_id: &SyncId,
        accept: bool,
        decisions: Vec<Decision>,
    ) -> DomainResult<()> {
        self.ensure_worker();
        let (plan, taken) = {
            let mut inner = self.lock();
            let s = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| {
                    s.role == Role::Planner
                        && s.phase == Phase::AwaitReview
                        && !s.answered
                        && !s.reviewing
                })
                .ok_or_else(Self::not_waiting)?;
            if !accept {
                drop(inner);
                self.stop(sync_id, Stop::Declined, LinkUpdate::Keep);
                return Ok(());
            }
            let (Some(plan), Some(theirs)) = (s.plan.clone(), s.theirs.as_ref()) else {
                return Err(Self::not_waiting());
            };
            for d in &decisions {
                if !plan.conflicts.iter().any(|c| c.path == d.path) {
                    return Err(Self::not_a_conflict());
                }
                if d.resolution == Resolution::KeepBoth && !keep_both_allowed(&d.path) {
                    return Err(DomainError::InvalidInput(KEEP_BOTH_SETTINGS.to_string()));
                }
            }
            s.reviewing = true;
            (plan, (s.mine.clone(), theirs.clone()))
        };
        // A `review` future dropped while it waits clears the flag, so the review stays open.
        let _reviewing = ReviewingFlag { svc: self, sync_id };
        #[cfg(test)]
        let gate = self
            .review_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let given = decisions.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            if let Some(gate) = gate {
                let _ = gate.recv();
            }
            let mut all = taken.0.clone();
            for (path, hash) in taken.1.iter() {
                if all.get(path).is_none() && all.insert(path, hash).is_err() {
                    return Err(Stop::Internal);
                }
            }
            resolve(&plan, &given, &all).map_err(|_| Stop::BadPlan)
        })
        .await;
        let actions = match resolved {
            Ok(Ok(actions)) => actions,
            Ok(Err(Stop::Internal)) | Err(_) => {
                self.stop(sync_id, Stop::Internal, LinkUpdate::Keep);
                return Ok(());
            }
            Ok(Err(_)) => {
                if let Some(s) = self.lock().sessions.get_mut(sync_id) {
                    s.reviewing = false;
                }
                return Err(DomainError::InvalidInput(DECISIONS_NEEDED.to_string()));
            }
        };
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::AwaitReview)
            else {
                return Ok(());
            };
            s.reviewing = false;
            s.answered = true;
            s.enter(Phase::Finalizing, Self::now());
            let key = s.key.clone();
            inner.limits.mark_active(&key);
        }
        let Some(me) = self.arc() else {
            return Ok(());
        };
        let sid = sync_id.clone();
        let done = tokio::task::spawn_blocking(move || {
            if let Err(stop) = me.finalize(&sid, actions, decisions) {
                me.stop(&sid, stop, LinkUpdate::Keep);
            }
        })
        .await;
        if done.is_err() {
            self.stop(sync_id, Stop::Internal, LinkUpdate::Keep);
        }
        Ok(())
    }

    /// The final step (P16): the split, the new base and its two checks (S8), the exported
    /// sends with their hashes, the files ZIP, the local changes, then the final plan is
    /// offered. Nothing is sent when a check fails. Blocks.
    fn finalize(
        &self,
        sync_id: &SyncId,
        actions: Vec<SyncAction>,
        decisions: Vec<Decision>,
    ) -> Result<(), Stop> {
        let view = {
            let inner = self.lock();
            let Some(s) = inner
                .sessions
                .get(sync_id)
                .filter(|s| s.phase == Phase::Finalizing)
            else {
                return Ok(());
            };
            let link = s.link.as_ref().ok_or(Stop::Internal)?;
            Finalize {
                mine: s.mine.clone(),
                theirs: s.theirs.clone().ok_or(Stop::Internal)?,
                base: link.base.clone(),
                collection: link.collection_name.clone(),
                received: s.received.clone(),
            }
        };
        let split = split_actions(&actions, &view.mine);
        if touches_root_file(&split) {
            return Err(Stop::Unsafe);
        }
        let agreed = new_base(&view.base, &view.mine, &view.theirs, &actions, &decisions);
        if apply_to_manifest(&view.mine, &split, &view.theirs)? != agreed {
            return Err(Stop::Internal);
        }
        let mut sends = Vec::new();
        for path in &split.sends {
            let hash = view.mine.get(path).ok_or(Stop::Internal)?;
            sends.push(PlanFile {
                path: path.clone(),
                sha256: hash.to_string(),
            });
        }
        let final_plan = SyncPlanPayload {
            wanted: Vec::new(),
            deletes: split.peer_deletes.clone(),
            renames: split
                .peer_renames
                .iter()
                .map(|(from, to)| PlanRename {
                    from: from.clone(),
                    to: to.clone(),
                })
                .collect(),
            decisions,
            sends,
            new_base_hash: Some(agreed.base_hash()),
        };
        if starter_new_base(&view.theirs, &final_plan).map_err(|_| Stop::Internal)? != agreed {
            return Err(Stop::Internal);
        }
        let send_paths: Vec<String> = split.sends.iter().cloned().collect();
        let sent_files = self.export_checked(&view.collection, &send_paths, &view.mine)?;
        let files_zip = if sent_files.is_empty() {
            None
        } else {
            let list: Vec<super::sync_ports::SyncFile> = sent_files
                .iter()
                .map(|(path, bytes)| super::sync_ports::SyncFile {
                    path: path.clone(),
                    bytes: bytes.clone(),
                })
                .collect();
            let zip = pack_zip(&list).map_err(|_| Stop::Internal)?;
            if zip.len() as u64 > MAX_TRANSFER_BYTES {
                return Err(Stop::TooBig(zip.len() as u64));
            }
            Some(zip)
        };
        let changes = planner_changes(&split, &view.received)?;
        let bytes = final_plan.to_json().map_err(|_| Stop::Internal)?;
        {
            let mut inner = self.lock();
            let Some(s) = inner
                .sessions
                .get_mut(sync_id)
                .filter(|s| s.phase == Phase::Finalizing)
            else {
                return Ok(());
            };
            s.final_plan = Some(final_plan);
            s.new_base = Some(agreed);
            s.changes = changes;
            s.sent_files = sent_files;
            s.files_zip = files_zip;
        }
        self.offer_payload(sync_id, SyncPayloadKind::Plan, bytes, Phase::SendFinal)
    }

    /// The starter's `SyncResult`. `applied` starts this node's apply; `failed` keeps the old
    /// base and marks the link `out_of_date`. A result in another phase is a bad plan. A
    /// result that overtakes the final ack of the last payload (the final plan with no files,
    /// or the files) counts as that ack.
    pub(super) fn planner_on_result(
        &self,
        from: &DeviceId,
        sync_id: &SyncId,
        outcome: SyncOutcome,
    ) {
        let waiting = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let Some(s) = inner.sessions.get_mut(sync_id).filter(|s| s.peer == *from) else {
                return;
            };
            // The last payload is out but its final ack has not come: the starter acks the
            // last chunk before it acts, so its result can overtake that ack.
            let last_payload = (s.phase == Phase::SendFinal && s.files_zip.is_none())
                || s.phase == Phase::SendFiles;
            let early =
                last_payload && s.out.as_ref().is_some_and(|o| o.machine.last_chunk_built());
            if early {
                if let Some(out) = s.out.take() {
                    inner.transfers.remove(out.machine.transfer_id());
                }
            }
            let waiting = s.phase == Phase::AwaitStarter || early;
            if waiting && outcome == SyncOutcome::Applied {
                s.enter(Phase::Applying, Self::now());
            }
            waiting
        };
        match (waiting, outcome) {
            (false, _) => self.stop(sync_id, Stop::BadPlan, LinkUpdate::Keep),
            (true, SyncOutcome::Applied) => self.spawn_planner_apply(sync_id.clone()),
            (true, SyncOutcome::Failed) => self.stop(
                sync_id,
                Stop::PeerApplyFailed,
                LinkUpdate::State(LinkState::OutOfDate),
            ),
        }
    }

    fn spawn_planner_apply(&self, sync_id: SyncId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(
            async move { me.planner_apply(&sync_id).await },
        ));
    }

    /// Applies after the starter applied (P20 to P22). A peer that goes meanwhile does not
    /// stop it; the link is saved only while the peer is still paired.
    async fn planner_apply(&self, sync_id: &SyncId) {
        let prepared = {
            let inner = self.lock();
            let Some(s) = inner
                .sessions
                .get(sync_id)
                .filter(|s| s.phase == Phase::Applying)
            else {
                return;
            };
            let empty = s
                .plan
                .as_ref()
                .is_some_and(|p| p.actions.is_empty() && p.conflicts.is_empty());
            (
                s.collection(),
                s.mine.clone(),
                s.changes.clone(),
                s.peer,
                empty,
            )
        };
        let (collection, mine, changes, peer, empty) = prepared;
        let collections = Arc::clone(&self.deps.collections);
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
        // A failed send changes nothing: the starter already applied (S7).
        self.tell(&peer, Self::result_frame(sync_id, SyncOutcome::Applied))
            .await;
        let update = {
            let inner = self.lock();
            let Some(s) = inner.sessions.get(sync_id) else {
                return;
            };
            match &s.new_base {
                Some(base) => LinkUpdate::NewBase {
                    base: base.clone(),
                    known: s.known_files(),
                    state: None,
                },
                None => LinkUpdate::Keep,
            }
        };
        self.end_session(
            sync_id,
            SyncFinish::Applied,
            empty.then(|| ALREADY_IN_SYNC.to_string()),
            None,
            update,
        );
    }
}

/// True when a delete or rename of either side names the root collection file. A collection
/// without it is no collection, so a sync never takes it away.
fn touches_root_file(split: &super::sync_checks::Split) -> bool {
    let renames = split.renames.iter().chain(&split.peer_renames);
    split
        .deletes
        .iter()
        .chain(&split.peer_deletes)
        .chain(renames.flat_map(|(from, to)| [from, to]))
        .any(|p| is_root_file(p))
}

/// Clears `reviewing` when a `review` call ends, also when its future is dropped.
struct ReviewingFlag<'a> {
    svc: &'a SyncService,
    sync_id: &'a SyncId,
}

impl Drop for ReviewingFlag<'_> {
    fn drop(&mut self) {
        if let Some(s) = self.svc.lock().sessions.get_mut(self.sync_id) {
            s.reviewing = false;
        }
    }
}

/// The planner's changes in apply order: deletes, renames, the pulled files, then the
/// peer's copies kept under a new name.
fn planner_changes(
    split: &super::sync_checks::Split,
    received: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<FileChange>, Stop> {
    let mut changes: Vec<FileChange> = split
        .deletes
        .iter()
        .map(|path| FileChange::Delete { path: path.clone() })
        .collect();
    changes.extend(split.renames.iter().map(|(from, to)| FileChange::Rename {
        from: from.clone(),
        to: to.clone(),
    }));
    for path in &split.puts {
        let bytes = received.get(path).ok_or(Stop::Internal)?;
        changes.push(FileChange::Put {
            path: path.clone(),
            bytes: bytes.clone(),
        });
    }
    for (peer_path, local) in &split.put_as {
        let bytes = received.get(peer_path).ok_or(Stop::Internal)?;
        changes.push(FileChange::PutAs {
            path: local.clone(),
            bytes: bytes.clone(),
        });
    }
    Ok(changes)
}
