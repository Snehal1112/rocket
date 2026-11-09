//! Pure rules of the sync: which paths are allowed, what a received plan must satisfy, the
//! starter's new base, the planner's split of actions, summaries, marks and fixed texts.
//!
//! Every text a user or a peer can see is a fixed sentence from this file. None of them ever
//! carries a peer text, a file name, a path or file content (S14).

use std::collections::{BTreeMap, BTreeSet};

use rocket_network::limits::{MAX_MANIFEST_ENTRIES, MAX_TRANSFER_BYTES};
use rocket_network::{
    is_sha256_hex, ConflictKind, Manifest, SyncAbortReason, SyncAction, SyncFinish, SyncPlan,
    SyncPlanPayload, SyncSummary,
};

use super::sync_payload::sha_hex;
use super::sync_ports::is_sync_entry;

const MIB: u64 = 1024 * 1024;
/// The limit in `PEER_TOO_BIG`. The assert below keeps the text and the cap together.
const LIMIT_MIB: u64 = MAX_TRANSFER_BYTES / MIB;
const _: () = assert!(LIMIT_MIB == 10, "PEER_TOO_BIG names the transfer cap");

pub(super) const WENT_OFFLINE: &str = "The other node went offline.";
pub(super) const DAMAGED: &str = "The changes arrived damaged. Nothing was changed.";
pub(super) const UNSAFE: &str =
    "The changes were refused because a file is not safe. Nothing was changed.";
pub(super) const BAD_PLAN: &str =
    "The other node sent a sync plan that cannot be used. Nothing was changed.";
pub(super) const BASE_MISMATCH: &str =
    "The two copies disagree about the last agreed version. Unlink and send the collection again.";
pub(super) const LINK_MISSING: &str =
    "The link is missing on the other side. Unlink and send the collection again.";
pub(super) const FOLDER_CHANGED: &str =
    "The collection folder or uid changed. Unlink and send the collection again.";
/// A local check before the apply saw a change (an export hash that differs from the plan).
pub(super) const CHANGED: &str = "The collection changed during the sync. Sync again.";
/// Any `Conflict` from the collection port's apply. The port uses it for a changed manifest,
/// a busy collection and an earlier apply that is not finished, so one text covers them all.
pub(super) const APPLY_CONFLICT: &str = "The collection changed or is busy, or an earlier sync is not finished. Sync again. If this keeps happening, restart Rocket.";
/// The two manifests hold paths that differ only by letter case. This also covers a rename
/// that changes only letter case, which the port cannot apply.
pub(super) const CASE_CLASH: &str =
    "The two copies have file names that differ only by letter case.";
pub(super) const APPLY_FAILED: &str = "The changes could not be applied. Nothing was changed.";
pub(super) const PEER_APPLY_FAILED: &str = "The other node could not apply the changes.";
pub(super) const PEER_FAILED: &str = "The sync failed on the other node.";
pub(super) const PEER_TOO_BIG: &str = "The changes are too big to sync (limit 10 MiB).";
pub(super) const BUSY: &str = "The other node is busy with another sync. Try again.";
pub(super) const STALLED: &str = "The other node stopped answering.";
pub(super) const UNCONFIRMED: &str = "Applied here, but the other node did not confirm.";
pub(super) const RESULT_NOT_SENT: &str =
    "Applied here, but the other node did not get the result. Sync again.";
pub(super) const ALREADY_IN_SYNC: &str = "Already in sync.";
pub(super) const INTERNAL: &str = "The sync could not be planned.";

/// Why a session stops. Each maps to one outcome, one fixed text and the frame to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    Damaged,
    Unsafe,
    BadPlan,
    /// A local check saw a change before the apply.
    Changed,
    /// The port's apply answered `Conflict`.
    ApplyConflict,
    CaseClash,
    /// The files ZIP is this many bytes, above `MAX_TRANSFER_BYTES`.
    TooBig(u64),
    ApplyFailed,
    PeerApplyFailed,
    WentOffline,
    Stalled,
    Internal,
    /// This node's user declined.
    Declined,
    /// This node's dialog timer passed.
    Expired,
    /// This node's user cancelled.
    Cancelled,
    /// The peer's `SyncAbort`.
    Peer(SyncAbortReason),
}

impl Stop {
    /// The outcome of the session's `Finished`.
    pub(super) fn finish(self) -> SyncFinish {
        match self {
            Stop::Declined => SyncFinish::Declined,
            Stop::Expired => SyncFinish::Expired,
            Stop::Cancelled => SyncFinish::Cancelled,
            Stop::Peer(reason) => match reason {
                SyncAbortReason::Declined => SyncFinish::Declined,
                SyncAbortReason::Expired => SyncFinish::Expired,
                SyncAbortReason::Cancelled => SyncFinish::Cancelled,
                SyncAbortReason::BaseMismatch => SyncFinish::BaseMismatch,
                SyncAbortReason::LinkMissing => SyncFinish::LinkMissing,
                SyncAbortReason::Busy | SyncAbortReason::TooBig | SyncAbortReason::Failed => {
                    SyncFinish::Failed
                }
            },
            _ => SyncFinish::Failed,
        }
    }

    /// The fixed text of the session's `Finished`. A user's own decline, expiry or cancel has
    /// none.
    pub(super) fn message(self) -> Option<String> {
        let text = match self {
            Stop::Damaged => DAMAGED,
            Stop::Unsafe => UNSAFE,
            Stop::BadPlan => BAD_PLAN,
            Stop::Changed => CHANGED,
            Stop::ApplyConflict => APPLY_CONFLICT,
            Stop::CaseClash => CASE_CLASH,
            Stop::TooBig(len) => return Some(too_big_text(len)),
            Stop::ApplyFailed => APPLY_FAILED,
            Stop::PeerApplyFailed => PEER_APPLY_FAILED,
            Stop::WentOffline => WENT_OFFLINE,
            Stop::Stalled => STALLED,
            Stop::Internal => INTERNAL,
            Stop::Declined | Stop::Expired | Stop::Cancelled => return None,
            Stop::Peer(reason) => match reason {
                SyncAbortReason::Declined
                | SyncAbortReason::Expired
                | SyncAbortReason::Cancelled => return None,
                SyncAbortReason::BaseMismatch => BASE_MISMATCH,
                SyncAbortReason::LinkMissing => LINK_MISSING,
                SyncAbortReason::Busy => BUSY,
                SyncAbortReason::TooBig => PEER_TOO_BIG,
                SyncAbortReason::Failed => PEER_FAILED,
            },
        };
        Some(text.to_string())
    }

    /// The abort to tell the peer. Nothing goes back for a peer's own abort, a gone peer or a
    /// peer that failed its apply.
    pub(super) fn abort(self) -> Option<SyncAbortReason> {
        match self {
            Stop::TooBig(_) => Some(SyncAbortReason::TooBig),
            Stop::Declined => Some(SyncAbortReason::Declined),
            Stop::Expired => Some(SyncAbortReason::Expired),
            Stop::Cancelled => Some(SyncAbortReason::Cancelled),
            Stop::WentOffline | Stop::PeerApplyFailed | Stop::Peer(_) => None,
            _ => Some(SyncAbortReason::Failed),
        }
    }
}

/// "The changes are too big to sync (X.Y MiB, limit 10).", rounded up to one decimal like the
/// transfer's own text.
fn too_big_text(len: u64) -> String {
    let tenths = len.saturating_mul(10).div_ceil(MIB);
    format!(
        "The changes are too big to sync ({}.{} MiB, limit {}).",
        tenths / 10,
        tenths % 10,
        LIMIT_MIB
    )
}

/// The wire name of a finish, for the audit `outcome` field. A test pins it to serde.
pub(super) fn finish_name(finish: SyncFinish) -> &'static str {
    match finish {
        SyncFinish::Applied => "applied",
        SyncFinish::Declined => "declined",
        SyncFinish::Expired => "expired",
        SyncFinish::Cancelled => "cancelled",
        SyncFinish::Failed => "failed",
        SyncFinish::BaseMismatch => "base_mismatch",
        SyncFinish::LinkMissing => "link_missing",
        SyncFinish::Unconfirmed => "unconfirmed",
    }
}

// ----- paths -----

/// A hash for path probes; `Manifest::insert` only checks its shape.
const PROBE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Whether a path a peer names may be used here: the manifest path rule, the sync path rule
/// (`is_sync_entry`, which also refuses a nested `opencollection.yml`) and no `bruno.json`
/// below the root.
pub(super) fn path_allowed(path: &str) -> bool {
    let mut probe = Manifest::new();
    if probe.insert(path, PROBE_HASH).is_err() || !is_sync_entry(path) {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let nested = path.contains('/');
    let collection_file =
        name.eq_ignore_ascii_case("opencollection.yml") || name.eq_ignore_ascii_case("bruno.json");
    !(nested && collection_file)
}

/// Keep both writes the peer's copy as received, so it is only for request files, never for
/// the collection or a folder settings file (S11).
pub(super) fn keep_both_allowed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !path.eq_ignore_ascii_case("opencollection.yml") && !name.eq_ignore_ascii_case("folder.yml")
}

/// True for the root collection file. Its edits sync; a delete or a rename never does.
pub(super) fn is_root_file(path: &str) -> bool {
    path.eq_ignore_ascii_case("opencollection.yml")
}

// ----- received plans -----

/// Decodes a received plan. A plan the decoder refuses is `Unsafe` when one of its paths is
/// not allowed and `BadPlan` otherwise, so a bad path reads the same in every payload (S10).
pub(super) fn decode_plan(bytes: &[u8]) -> Result<SyncPlanPayload, Stop> {
    if let Ok(plan) = SyncPlanPayload::from_json(bytes) {
        return Ok(plan);
    }
    // The size was capped by the payload machine, so a plain parse is bounded too.
    let raw: Option<SyncPlanPayload> = serde_json::from_slice(bytes).ok();
    let bad_path = raw.is_some_and(|plan| {
        plan.wanted
            .iter()
            .chain(&plan.deletes)
            .chain(plan.renames.iter().flat_map(|r| [&r.from, &r.to]))
            .chain(plan.decisions.iter().map(|d| &d.path))
            .chain(plan.sends.iter().map(|f| &f.path))
            .any(|p| !path_allowed(p))
    });
    Err(if bad_path {
        Stop::Unsafe
    } else {
        Stop::BadPlan
    })
}

/// A request plan names only paths this node offered, each once, and nothing else.
pub(super) fn check_request(
    plan: &SyncPlanPayload,
    offered: &Manifest,
) -> Result<BTreeSet<String>, Stop> {
    let only_wanted = plan.deletes.is_empty()
        && plan.renames.is_empty()
        && plan.decisions.is_empty()
        && plan.sends.is_empty()
        && plan.new_base_hash.is_none();
    if !only_wanted || plan.wanted.is_empty() {
        return Err(Stop::BadPlan);
    }
    let mut wanted = BTreeSet::new();
    for path in &plan.wanted {
        if !path_allowed(path) || offered.get(path).is_none() {
            return Err(Stop::Unsafe);
        }
        if !wanted.insert(path.clone()) {
            return Err(Stop::BadPlan);
        }
    }
    Ok(wanted)
}

/// Checks a final plan against the manifest this node offered and returns the new base it
/// gives. The base is computed here; only its hash came from the peer.
pub(super) fn check_final(plan: &SyncPlanPayload, offered: &Manifest) -> Result<Manifest, Stop> {
    let Some(expected) = plan.new_base_hash.as_deref() else {
        return Err(Stop::BadPlan);
    };
    if !plan.wanted.is_empty() {
        return Err(Stop::BadPlan);
    }
    let paths = plan
        .deletes
        .iter()
        .chain(plan.renames.iter().flat_map(|r| [&r.from, &r.to]))
        .chain(plan.sends.iter().map(|s| &s.path))
        .chain(plan.decisions.iter().map(|d| &d.path));
    for path in paths {
        if !path_allowed(path) {
            return Err(Stop::Unsafe);
        }
    }
    let moves_root = plan
        .deletes
        .iter()
        .chain(plan.renames.iter().flat_map(|r| [&r.from, &r.to]))
        .any(|p| is_root_file(p));
    if moves_root {
        return Err(Stop::Unsafe);
    }
    if plan
        .sends
        .iter()
        .any(|s| !is_sha256_hex(&s.sha256) || s.sha256.len() != 64)
    {
        return Err(Stop::BadPlan);
    }
    let unique = |items: Vec<&String>| {
        let n = items.len();
        items.into_iter().collect::<BTreeSet<_>>().len() == n
    };
    let deletes: BTreeSet<&String> = plan.deletes.iter().collect();
    if !unique(plan.deletes.iter().collect())
        || !unique(plan.renames.iter().map(|r| &r.from).collect())
        || !unique(plan.renames.iter().map(|r| &r.to).collect())
        || !unique(plan.sends.iter().map(|s| &s.path).collect())
        || plan.sends.iter().any(|s| deletes.contains(&s.path))
    {
        return Err(Stop::Unsafe);
    }
    let new_base = starter_new_base(offered, plan)?;
    if new_base.base_hash() != expected {
        return Err(Stop::BadPlan);
    }
    Ok(new_base)
}

/// The starter's manifest after a final plan, in the port's order: renames one after the
/// other, then deletes, then sends.
pub(super) fn starter_new_base(
    offered: &Manifest,
    plan: &SyncPlanPayload,
) -> Result<Manifest, Stop> {
    let mut map: BTreeMap<String, String> = offered
        .iter()
        .map(|(p, h)| (p.to_string(), h.to_string()))
        .collect();
    for rename in &plan.renames {
        let hash = map.remove(&rename.from).ok_or(Stop::Unsafe)?;
        if map.insert(rename.to.clone(), hash).is_some() {
            return Err(Stop::Unsafe);
        }
    }
    for path in &plan.deletes {
        map.remove(path).ok_or(Stop::Unsafe)?;
    }
    for send in &plan.sends {
        map.insert(send.path.clone(), send.sha256.clone());
    }
    manifest_from_map(&map, Stop::Unsafe)
}

fn manifest_from_map(map: &BTreeMap<String, String>, refused: Stop) -> Result<Manifest, Stop> {
    if map.len() > MAX_MANIFEST_ENTRIES {
        return Err(Stop::BadPlan);
    }
    let mut manifest = Manifest::new();
    for (path, hash) in map {
        manifest.insert(path, hash).map_err(|_| refused)?;
    }
    Ok(manifest)
}

// ----- the planner's side -----

/// The planner's actions split by side. Every list is sorted and has no repeats.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Split {
    /// Local deletes.
    pub deletes: Vec<String>,
    /// Local renames `(from, to)`.
    pub renames: Vec<(String, String)>,
    /// Local writes of the peer's file at the same path.
    pub puts: Vec<String>,
    /// Local writes of the peer's file at `path` under the new path: `(peer path, local path)`.
    pub put_as: Vec<(String, String)>,
    pub peer_deletes: Vec<String>,
    pub peer_renames: Vec<(String, String)>,
    /// Local files the peer gets.
    pub sends: BTreeSet<String>,
}

/// Splits resolved actions into what this node does and what it tells the peer.
pub(super) fn split_actions(actions: &[SyncAction], mine: &Manifest) -> Split {
    let mut deletes = BTreeSet::new();
    let mut renames = BTreeSet::new();
    let mut puts = BTreeSet::new();
    let mut put_as = BTreeSet::new();
    let mut peer_deletes = BTreeSet::new();
    let mut peer_renames = BTreeSet::new();
    let mut sends = BTreeSet::new();
    for action in actions {
        match action {
            SyncAction::Pull { path } => {
                puts.insert(path.clone());
            }
            SyncAction::PullAs { path, as_path } => {
                put_as.insert((path.clone(), as_path.clone()));
                // Keep both moves the peer's copy aside and sends it mine for the path.
                peer_renames.insert((path.clone(), as_path.clone()));
                if mine.get(path).is_some() {
                    sends.insert(path.clone());
                }
            }
            SyncAction::Push { path } => {
                sends.insert(path.clone());
            }
            SyncAction::DeleteMine { path } => {
                deletes.insert(path.clone());
            }
            SyncAction::DeleteTheirs { path } => {
                peer_deletes.insert(path.clone());
            }
            SyncAction::RenameMine { from, to } => {
                renames.insert((from.clone(), to.clone()));
            }
            SyncAction::RenameTheirs { from, to } => {
                peer_renames.insert((from.clone(), to.clone()));
            }
        }
    }
    Split {
        deletes: deletes.into_iter().collect(),
        renames: renames.into_iter().collect(),
        puts: puts.into_iter().collect(),
        put_as: put_as.into_iter().collect(),
        peer_deletes: peer_deletes.into_iter().collect(),
        peer_renames: peer_renames.into_iter().collect(),
        sends,
    }
}

/// The rename marker of a `RenameVsEdit` conflict at `path`: `(theirs renamed, to)`.
fn rename_marker(plan: &SyncPlan, path: &str) -> Option<(bool, String)> {
    plan.actions.iter().find_map(|a| match a {
        SyncAction::RenameMine { from, to } if from == path => Some((true, to.clone())),
        SyncAction::RenameTheirs { from, to } if from == path => Some((false, to.clone())),
        _ => None,
    })
}

/// Where each side holds the file of a conflict: `(mine, theirs)`. A rename conflict has the
/// file at the rename target on the side that renamed it.
pub(super) fn conflict_paths(plan: &SyncPlan, path: &str, kind: ConflictKind) -> (String, String) {
    if kind == ConflictKind::RenameVsEdit {
        match rename_marker(plan, path) {
            Some((true, to)) => return (path.to_string(), to),
            Some((false, to)) => return (to, path.to_string()),
            None => {}
        }
    }
    (path.to_string(), path.to_string())
}

/// True for a rename that only marks a `RenameVsEdit` conflict; `resolve` replaces it.
fn is_marker(plan: &SyncPlan, action: &SyncAction) -> bool {
    let from = match action {
        SyncAction::RenameMine { from, .. } | SyncAction::RenameTheirs { from, .. } => from,
        _ => return false,
    };
    plan.conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::RenameVsEdit && &c.path == from)
}

/// The starter's files the planner needs before its review: every pulled file, and the
/// starter's copy of every conflict it still has (for the diff and the marks).
pub(super) fn wanted_paths(plan: &SyncPlan) -> BTreeSet<String> {
    let mut wanted = BTreeSet::new();
    for action in &plan.actions {
        match action {
            SyncAction::Pull { path } | SyncAction::PullAs { path, .. } => {
                wanted.insert(path.clone());
            }
            _ => {}
        }
    }
    for c in plan.conflicts.iter().filter(|c| c.theirs.is_some()) {
        wanted.insert(conflict_paths(plan, &c.path, c.kind).1);
    }
    wanted
}

/// The planner's manifest after its own changes, in the port's order: renames, deletes,
/// puts, then put-as. A missing source is a planning error.
pub(super) fn apply_to_manifest(
    mine: &Manifest,
    split: &Split,
    theirs: &Manifest,
) -> Result<Manifest, Stop> {
    let mut map: BTreeMap<String, String> = mine
        .iter()
        .map(|(p, h)| (p.to_string(), h.to_string()))
        .collect();
    for (from, to) in &split.renames {
        let hash = map.remove(from).ok_or(Stop::Internal)?;
        map.insert(to.clone(), hash);
    }
    for path in &split.deletes {
        map.remove(path).ok_or(Stop::Internal)?;
    }
    for path in &split.puts {
        let hash = theirs.get(path).ok_or(Stop::Internal)?;
        map.insert(path.clone(), hash.to_string());
    }
    for (peer_path, local) in &split.put_as {
        let hash = theirs.get(peer_path).ok_or(Stop::Internal)?;
        map.insert(local.clone(), hash.to_string());
    }
    manifest_from_map(&map, Stop::Internal)
}

/// The entries of a received ZIP must be exactly the expected set (`Unsafe` otherwise), each
/// with its expected hash (`Damaged` otherwise).
pub(super) fn entries_match(
    files: &BTreeMap<String, Vec<u8>>,
    expected: &BTreeMap<String, String>,
) -> Result<(), Stop> {
    if !files.keys().eq(expected.keys()) {
        return Err(Stop::Unsafe);
    }
    for (path, bytes) in files {
        if expected.get(path).map(String::as_str) != Some(sha_hex(bytes).as_str()) {
            return Err(Stop::Damaged);
        }
    }
    Ok(())
}

// ----- marks and summaries -----

/// Whether a file holds scripts or auth (spec section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Marks {
    pub script: bool,
    pub auth: bool,
}

/// The marks of a YAML file. A file that does not parse counts as both.
pub(super) fn file_marks(bytes: &[u8]) -> Marks {
    match serde_yaml::from_slice::<serde_yaml::Value>(bytes) {
        Ok(value) => {
            let mut marks = Marks {
                script: false,
                auth: false,
            };
            walk_marks(&value, &mut marks);
            marks
        }
        Err(_) => Marks {
            script: true,
            auth: true,
        },
    }
}

/// The marks of one file by its path: a `.js` file is a script, a `.proto` file has neither,
/// anything else is read as YAML.
fn path_marks(path: &str, bytes: &[u8]) -> Marks {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".js") {
        return Marks {
            script: true,
            auth: false,
        };
    }
    if lower.ends_with(".proto") {
        return Marks {
            script: false,
            auth: false,
        };
    }
    file_marks(bytes)
}

fn is_empty_value(value: &serde_yaml::Value) -> bool {
    use serde_yaml::Value;
    match value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Sequence(s) => s.is_empty(),
        Value::Mapping(m) => m.is_empty(),
        _ => false,
    }
}

fn is_no_auth(value: &serde_yaml::Value) -> bool {
    use serde_yaml::Value;
    let off = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case("inherit") || s.eq_ignore_ascii_case("none"))
    };
    match value {
        Value::Null | Value::Bool(false) => true,
        Value::String(_) => off(Some(value)),
        Value::Mapping(m) => off(m.get("type")) || off(m.get("mode")),
        _ => false,
    }
}

fn walk_marks(value: &serde_yaml::Value, marks: &mut Marks) {
    use serde_yaml::Value;
    match value {
        Value::Mapping(map) => {
            for (key, inner) in map {
                match key.as_str() {
                    Some("script" | "scripts" | "tests") if !is_empty_value(inner) => {
                        marks.script = true;
                    }
                    Some("auth") if !is_no_auth(inner) => marks.auth = true,
                    _ => {}
                }
                walk_marks(inner, marks);
            }
        }
        Value::Sequence(items) => items.iter().for_each(|v| walk_marks(v, marks)),
        Value::Tagged(tagged) => walk_marks(&tagged.value, marks),
        _ => {}
    }
}

/// The sorted script and auth lists of the received files.
fn marked(received: &BTreeMap<String, Vec<u8>>) -> (Vec<String>, Vec<String>) {
    let mut scripts = Vec::new();
    let mut auth = Vec::new();
    for (path, bytes) in received {
        let m = path_marks(path, bytes);
        if m.script {
            scripts.push(path.clone());
        }
        if m.auth {
            auth.push(path.clone());
        }
    }
    (scripts, auth)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The planner's review summary. Rename markers of conflicts are not counted (R12).
pub(super) fn planner_summary(
    plan: &SyncPlan,
    mine: &Manifest,
    theirs: &Manifest,
    received: &BTreeMap<String, Vec<u8>>,
) -> SyncSummary {
    let mut s = SyncSummary::default();
    for action in plan.actions.iter().filter(|a| !is_marker(plan, a)) {
        match action {
            SyncAction::Pull { path } if mine.get(path).is_some() => s.changed_here += 1,
            SyncAction::Pull { .. } | SyncAction::PullAs { .. } => s.added_here += 1,
            SyncAction::DeleteMine { .. } => s.deleted_here += 1,
            SyncAction::RenameMine { .. } => s.changed_here += 1,
            SyncAction::Push { path } if theirs.get(path).is_some() => s.sent_changed += 1,
            SyncAction::Push { .. } => s.sent_added += 1,
            SyncAction::DeleteTheirs { .. } => s.sent_deleted += 1,
            SyncAction::RenameTheirs { .. } => s.sent_changed += 1,
        }
    }
    (s.scripts_changed, s.auth_changed) = marked(received);
    s
}

/// The starter's confirm summary: what the final plan changes here, and how many files it
/// sent (`wanted`).
pub(super) fn starter_summary(
    plan: &SyncPlanPayload,
    offered: &Manifest,
    wanted: usize,
    received: &BTreeMap<String, Vec<u8>>,
) -> SyncSummary {
    let mut s = SyncSummary::default();
    for send in &plan.sends {
        if offered.get(&send.path).is_some() {
            s.changed_here += 1;
        } else {
            s.added_here += 1;
        }
    }
    s.deleted_here = count(plan.deletes.len());
    s.changed_here += count(plan.renames.len());
    s.sent_changed = count(wanted);
    (s.scripts_changed, s.auth_changed) = marked(received);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_names_match_the_wire_names() {
        for f in [
            SyncFinish::Applied,
            SyncFinish::Declined,
            SyncFinish::Expired,
            SyncFinish::Cancelled,
            SyncFinish::Failed,
            SyncFinish::BaseMismatch,
            SyncFinish::LinkMissing,
            SyncFinish::Unconfirmed,
        ] {
            let wire = serde_json::to_value(f).expect("json");
            assert_eq!(wire.as_str(), Some(finish_name(f)));
        }
    }

    #[test]
    fn each_peer_abort_maps_to_its_outcome_and_text() {
        use SyncAbortReason as R;
        let rows = [
            (R::Declined, SyncFinish::Declined, None),
            (R::Expired, SyncFinish::Expired, None),
            (R::Cancelled, SyncFinish::Cancelled, None),
            (
                R::BaseMismatch,
                SyncFinish::BaseMismatch,
                Some(BASE_MISMATCH),
            ),
            (R::LinkMissing, SyncFinish::LinkMissing, Some(LINK_MISSING)),
            (R::Busy, SyncFinish::Failed, Some(BUSY)),
            (R::TooBig, SyncFinish::Failed, Some(PEER_TOO_BIG)),
            (R::Failed, SyncFinish::Failed, Some(PEER_FAILED)),
        ];
        for (reason, finish, text) in rows {
            let stop = Stop::Peer(reason);
            assert_eq!(stop.finish(), finish, "{reason:?}");
            assert_eq!(stop.message().as_deref(), text, "{reason:?}");
            assert_eq!(stop.abort(), None, "nothing is sent back for a peer abort");
        }
    }

    #[test]
    fn too_big_names_the_size_and_tells_the_peer_too_big() {
        let stop = Stop::TooBig(10 * 1024 * 1024 + 1);
        assert_eq!(stop.finish(), SyncFinish::Failed);
        assert_eq!(
            stop.message().as_deref(),
            Some("The changes are too big to sync (10.1 MiB, limit 10).")
        );
        assert_eq!(stop.abort(), Some(SyncAbortReason::TooBig));
        assert_eq!(Stop::WentOffline.abort(), None);
        assert_eq!(Stop::Damaged.abort(), Some(SyncAbortReason::Failed));
    }

    #[test]
    fn each_local_stop_has_its_outcome_text_and_frame() {
        use SyncAbortReason as R;
        let failed = Some(R::Failed);
        let rows = [
            (Stop::Damaged, SyncFinish::Failed, Some(DAMAGED), failed),
            (Stop::Unsafe, SyncFinish::Failed, Some(UNSAFE), failed),
            (Stop::BadPlan, SyncFinish::Failed, Some(BAD_PLAN), failed),
            (Stop::Changed, SyncFinish::Failed, Some(CHANGED), failed),
            (
                Stop::ApplyConflict,
                SyncFinish::Failed,
                Some(APPLY_CONFLICT),
                failed,
            ),
            (
                Stop::CaseClash,
                SyncFinish::Failed,
                Some(CASE_CLASH),
                failed,
            ),
            (
                Stop::ApplyFailed,
                SyncFinish::Failed,
                Some(APPLY_FAILED),
                failed,
            ),
            (
                Stop::PeerApplyFailed,
                SyncFinish::Failed,
                Some(PEER_APPLY_FAILED),
                None,
            ),
            (
                Stop::WentOffline,
                SyncFinish::Failed,
                Some(WENT_OFFLINE),
                None,
            ),
            (Stop::Stalled, SyncFinish::Failed, Some(STALLED), failed),
            (Stop::Internal, SyncFinish::Failed, Some(INTERNAL), failed),
            (
                Stop::Declined,
                SyncFinish::Declined,
                None,
                Some(R::Declined),
            ),
            (Stop::Expired, SyncFinish::Expired, None, Some(R::Expired)),
            (
                Stop::Cancelled,
                SyncFinish::Cancelled,
                None,
                Some(R::Cancelled),
            ),
        ];
        for (stop, finish, text, abort) in rows {
            assert_eq!(stop.finish(), finish, "{stop:?}");
            assert_eq!(stop.message().as_deref(), text, "{stop:?}");
            assert_eq!(stop.abort(), abort, "{stop:?}");
        }
    }
}

#[cfg(test)]
mod rule_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use rocket_network::{Manifest, PlanFile, PlanRename, SyncAction, SyncPlan, SyncPlanPayload};

    use super::*;
    use crate::network::sync_payload::sha_hex;

    fn h(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn m(list: &[(&str, char)]) -> Manifest {
        let mut x = Manifest::new();
        for (path, c) in list {
            x.insert(path, &h(*c)).expect("a valid test path");
        }
        x
    }

    fn payload() -> SyncPlanPayload {
        SyncPlanPayload {
            wanted: vec![],
            deletes: vec![],
            renames: vec![],
            decisions: vec![],
            sends: vec![],
            new_base_hash: None,
        }
    }

    fn rename(from: &str, to: &str) -> PlanRename {
        PlanRename {
            from: from.into(),
            to: to.into(),
        }
    }

    fn send(path: &str, c: char) -> PlanFile {
        PlanFile {
            path: path.into(),
            sha256: h(c),
        }
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn path_allowed_follows_the_manifest_rule_and_refuses_nested_collection_files() {
        for ok in [
            "a.yml",
            "folder/a.yml",
            "opencollection.yml",
            "f/folder.yml",
            "a (from peer).yml",
            "s.js",
        ] {
            assert!(path_allowed(ok), "{ok}");
        }
        for bad in [
            "",
            "../a.yml",
            "/a.yml",
            "a//b.yml",
            ".rocket/x.yml",
            "f/opencollection.yml",
            "f/Bruno.json",
            "CON.yml",
            "bruno.json",
            "f/OpenCollection.yml",
            "environments/a.yml",
            "a.txt",
        ] {
            assert!(!path_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn keep_both_is_only_for_request_files() {
        assert!(keep_both_allowed("a.yml"));
        assert!(keep_both_allowed("f/a.yml"));
        assert!(!keep_both_allowed("opencollection.yml"));
        assert!(!keep_both_allowed("folder.yml"));
        assert!(!keep_both_allowed("f/g/folder.yml"));
    }

    #[test]
    fn a_request_names_only_offered_paths_and_nothing_else() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let mut p = payload();
        p.wanted = vec!["a.yml".into()];
        assert_eq!(
            check_request(&p, &offered),
            Ok(BTreeSet::from(["a.yml".to_string()]))
        );

        p.wanted = vec!["c.yml".into()];
        assert_eq!(
            check_request(&p, &offered),
            Err(Stop::Unsafe),
            "not offered"
        );
        p.wanted = vec!["../a.yml".into()];
        assert_eq!(check_request(&p, &offered), Err(Stop::Unsafe), "unsafe");
        p.wanted = vec!["a.yml".into(), "a.yml".into()];
        assert_eq!(check_request(&p, &offered), Err(Stop::BadPlan), "twice");
        p.wanted = vec![];
        assert_eq!(
            check_request(&p, &offered),
            Err(Stop::BadPlan),
            "an empty request is never sent"
        );

        let mut q = payload();
        q.wanted = vec!["a.yml".into()];
        q.deletes = vec!["b.yml".into()];
        assert_eq!(
            check_request(&q, &offered),
            Err(Stop::BadPlan),
            "a request carries nothing else"
        );
    }

    #[test]
    fn the_starter_new_base_applies_renames_then_deletes_then_sends() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b'), ("c.yml", 'c')]);
        let mut p = payload();
        p.deletes = vec!["c.yml".into()];
        p.renames = vec![rename("b.yml", "d.yml")];
        p.sends = vec![send("a.yml", 'd'), send("n.yml", 'e')];

        assert_eq!(
            starter_new_base(&offered, &p),
            Ok(m(&[("a.yml", 'd'), ("d.yml", 'b'), ("n.yml", 'e')]))
        );

        // The port runs renames before deletes, so a rename onto a file the plan deletes
        // finds the target taken.
        let mut onto = payload();
        onto.deletes = vec!["c.yml".into()];
        onto.renames = vec![rename("b.yml", "c.yml")];
        assert_eq!(starter_new_base(&offered, &onto), Err(Stop::Unsafe));
    }

    #[test]
    fn a_valid_final_plan_returns_the_new_base() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let mut p = payload();
        p.deletes = vec!["a.yml".into()];
        p.sends = vec![send("b.yml", 'c')];
        let expected = m(&[("b.yml", 'c')]);
        p.new_base_hash = Some(expected.base_hash());

        assert_eq!(check_final(&p, &offered), Ok(expected));
    }

    /// One broken rule: its name, the change to a valid plan, and the stop it must give.
    type Case = (&'static str, Box<dyn Fn(&mut SyncPlanPayload)>, Stop);

    #[test]
    fn a_final_plan_is_refused_for_each_broken_rule() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let any = Some(h('0'));
        let cases: Vec<Case> = vec![
            (
                "unsafe delete",
                Box::new(|p| p.deletes = vec!["../a.yml".into()]),
                Stop::Unsafe,
            ),
            (
                "unknown delete",
                Box::new(|p| p.deletes = vec!["x.yml".into()]),
                Stop::Unsafe,
            ),
            (
                "delete twice",
                Box::new(|p| p.deletes = vec!["a.yml".into(), "a.yml".into()]),
                Stop::Unsafe,
            ),
            (
                "rename of an unknown file",
                Box::new(|p| p.renames = vec![rename("x.yml", "y.yml")]),
                Stop::Unsafe,
            ),
            (
                "rename over a file",
                Box::new(|p| p.renames = vec![rename("a.yml", "b.yml")]),
                Stop::Unsafe,
            ),
            (
                "rename to an unsafe path",
                Box::new(|p| p.renames = vec![rename("a.yml", "../y.yml")]),
                Stop::Unsafe,
            ),
            (
                "rename to a nested collection",
                Box::new(|p| p.renames = vec![rename("a.yml", "f/opencollection.yml")]),
                Stop::Unsafe,
            ),
            (
                "send to an unsafe path",
                Box::new(|p| p.sends = vec![send(".rocket/x.yml", 'c')]),
                Stop::Unsafe,
            ),
            (
                "send twice",
                Box::new(|p| p.sends = vec![send("n.yml", 'c'), send("n.yml", 'd')]),
                Stop::Unsafe,
            ),
            (
                "send a deleted path",
                Box::new(|p| {
                    p.deletes = vec!["a.yml".into()];
                    p.sends = vec![send("a.yml", 'c')];
                }),
                Stop::Unsafe,
            ),
            (
                "send with a bad hash",
                Box::new(|p| {
                    p.sends = vec![PlanFile {
                        path: "n.yml".into(),
                        sha256: "XYZ".into(),
                    }]
                }),
                Stop::BadPlan,
            ),
            (
                "decision on an unsafe path",
                Box::new(|p| {
                    p.decisions = vec![rocket_network::Decision {
                        path: "../a.yml".into(),
                        resolution: rocket_network::Resolution::KeepMine,
                    }]
                }),
                Stop::Unsafe,
            ),
            (
                "a final plan that also wants files",
                Box::new(|p| p.wanted = vec!["a.yml".into()]),
                Stop::BadPlan,
            ),
            (
                "rename onto a deleted file",
                Box::new(|p| {
                    p.deletes = vec!["b.yml".into()];
                    p.renames = vec![rename("a.yml", "b.yml")];
                }),
                Stop::Unsafe,
            ),
            (
                "rename a deleted file",
                Box::new(|p| {
                    p.deletes = vec!["a.yml".into()];
                    p.renames = vec![rename("a.yml", "y.yml")];
                }),
                Stop::Unsafe,
            ),
        ];
        for (what, edit, stop) in cases {
            let mut p = payload();
            p.new_base_hash = any.clone();
            edit(&mut p);
            assert_eq!(check_final(&p, &offered), Err(stop), "{what}");
        }

        let mut no_hash = payload();
        no_hash.deletes = vec!["a.yml".into()];
        assert_eq!(
            check_final(&no_hash, &offered),
            Err(Stop::BadPlan),
            "no new base hash"
        );
        no_hash.new_base_hash = Some(m(&[("a.yml", 'a')]).base_hash());
        assert_eq!(
            check_final(&no_hash, &offered),
            Err(Stop::BadPlan),
            "a hash the starter does not reach"
        );
    }

    #[test]
    fn split_puts_each_action_on_its_side() {
        let mine = m(&[
            ("k.yml", 'a'),
            ("s.yml", 'b'),
            ("r.yml", 'c'),
            ("d.yml", 'd'),
        ]);
        let actions = vec![
            SyncAction::Pull {
                path: "p.yml".into(),
            },
            SyncAction::PullAs {
                path: "k.yml".into(),
                as_path: "k (from peer).yml".into(),
            },
            SyncAction::Push {
                path: "s.yml".into(),
            },
            SyncAction::DeleteMine {
                path: "d.yml".into(),
            },
            SyncAction::DeleteTheirs {
                path: "t.yml".into(),
            },
            SyncAction::RenameMine {
                from: "r.yml".into(),
                to: "r2.yml".into(),
            },
            SyncAction::RenameTheirs {
                from: "q.yml".into(),
                to: "q2.yml".into(),
            },
        ];

        let s = split_actions(&actions, &mine);

        assert_eq!(s.puts, vec!["p.yml"]);
        assert_eq!(s.put_as, owned(&[("k.yml", "k (from peer).yml")]));
        assert_eq!(s.deletes, vec!["d.yml"]);
        assert_eq!(s.renames, owned(&[("r.yml", "r2.yml")]));
        assert_eq!(s.peer_deletes, vec!["t.yml"]);
        assert_eq!(
            s.peer_renames,
            owned(&[("k.yml", "k (from peer).yml"), ("q.yml", "q2.yml")]),
            "keep both moves the peer's copy aside"
        );
        assert_eq!(
            s.sends,
            BTreeSet::from(["k.yml".to_string(), "s.yml".to_string()]),
            "and sends it mine for the original path"
        );
    }

    #[test]
    fn keep_both_moves_already_listed_by_resolve_are_not_doubled() {
        let mine = m(&[("k.yml", 'a')]);
        let actions = vec![
            SyncAction::PullAs {
                path: "k.yml".into(),
                as_path: "k (from peer).yml".into(),
            },
            SyncAction::RenameTheirs {
                from: "k.yml".into(),
                to: "k (from peer).yml".into(),
            },
            SyncAction::Push {
                path: "k.yml".into(),
            },
        ];
        let s = split_actions(&actions, &mine);
        assert_eq!(s.peer_renames.len(), 1);
        assert_eq!(s.sends.len(), 1);
    }

    #[test]
    fn keep_both_without_a_local_file_sends_nothing_for_the_path() {
        let s = split_actions(
            &[SyncAction::PullAs {
                path: "k.yml".into(),
                as_path: "k (from peer).yml".into(),
            }],
            &Manifest::new(),
        );
        assert!(s.sends.is_empty());
        assert_eq!(s.peer_renames, owned(&[("k.yml", "k (from peer).yml")]));
    }

    #[test]
    fn the_planner_manifest_after_its_changes() {
        let mine = m(&[
            ("d.yml", '1'),
            ("r.yml", '2'),
            ("p.yml", '3'),
            ("k.yml", '4'),
        ]);
        let theirs = m(&[("p.yml", '5'), ("k.yml", '6')]);
        let s = Split {
            deletes: vec!["d.yml".into()],
            renames: owned(&[("r.yml", "r2.yml")]),
            puts: vec!["p.yml".into()],
            put_as: owned(&[("k.yml", "k (from peer).yml")]),
            ..Split::default()
        };

        assert_eq!(
            apply_to_manifest(&mine, &s, &theirs),
            Ok(m(&[
                ("r2.yml", '2'),
                ("p.yml", '5'),
                ("k.yml", '4'),
                ("k (from peer).yml", '6')
            ]))
        );
        let missing = Split {
            puts: vec!["x.yml".into()],
            ..Split::default()
        };
        assert_eq!(
            apply_to_manifest(&mine, &missing, &theirs),
            Err(Stop::Internal)
        );
    }

    #[test]
    fn wanted_paths_are_pulls_and_conflicts_the_peer_still_has() {
        let plan = SyncPlan {
            actions: vec![
                SyncAction::Pull {
                    path: "p.yml".into(),
                },
                SyncAction::Push {
                    path: "s.yml".into(),
                },
                SyncAction::RenameMine {
                    from: "r.yml".into(),
                    to: "r2.yml".into(),
                },
            ],
            conflicts: vec![
                rocket_network::Conflict {
                    path: "c.yml".into(),
                    kind: rocket_network::ConflictKind::BothChanged,
                    mine: Some(h('1')),
                    theirs: Some(h('2')),
                },
                rocket_network::Conflict {
                    path: "e.yml".into(),
                    kind: rocket_network::ConflictKind::DeleteVsEdit,
                    mine: Some(h('1')),
                    theirs: None,
                },
            ],
        };
        assert_eq!(
            wanted_paths(&plan),
            BTreeSet::from(["c.yml".to_string(), "p.yml".to_string()])
        );
    }

    #[test]
    fn received_entries_must_be_exactly_the_expected_set_with_their_hashes() {
        let files: BTreeMap<String, Vec<u8>> = [("a.yml".to_string(), b"x".to_vec())].into();
        let good: BTreeMap<String, String> = [("a.yml".to_string(), sha_hex(b"x"))].into();
        assert_eq!(entries_match(&files, &good), Ok(()));

        let mut more = good.clone();
        more.insert("b.yml".into(), sha_hex(b"y"));
        assert_eq!(
            entries_match(&files, &more),
            Err(Stop::Unsafe),
            "an entry is missing"
        );
        assert_eq!(
            entries_match(&files, &BTreeMap::new()),
            Err(Stop::Unsafe),
            "an entry not in the plan"
        );
        let wrong: BTreeMap<String, String> = [("a.yml".to_string(), sha_hex(b"z"))].into();
        assert_eq!(entries_match(&files, &wrong), Err(Stop::Damaged));
    }

    #[test]
    fn marks_find_scripts_and_auth() {
        let none = Marks {
            script: false,
            auth: false,
        };
        assert_eq!(file_marks(b"http:\n  url: x\n"), none);
        assert!(
            file_marks(b"runtime:\n  scripts:\n    - type: before-request\n      code: x\n").script
        );
        assert!(file_marks(b"tests: \"expect(1)\"\n").script);
        assert_eq!(
            file_marks(b"scripts: []\n"),
            none,
            "an empty list is no script"
        );
        assert_eq!(file_marks(b"http:\n  auth: inherit\n"), none);
        assert_eq!(file_marks(b"http:\n  auth:\n    type: none\n"), none);
        assert!(file_marks(b"http:\n  auth:\n    type: bearer\n    token: ''\n").auth);
        assert_eq!(
            file_marks(b": not yaml: ["),
            Marks {
                script: true,
                auth: true
            },
            "an unreadable file counts as both"
        );
    }

    #[test]
    fn the_planner_summary_counts_each_side_and_marks_incoming_files() {
        let mine = m(&[
            ("c.yml", '1'),
            ("d.yml", '2'),
            ("r.yml", '3'),
            ("s.yml", '4'),
            ("x.yml", '5'),
        ]);
        let theirs = m(&[
            ("c.yml", '6'),
            ("n.yml", '7'),
            ("s.yml", '8'),
            ("t.yml", '9'),
        ]);
        let plan = SyncPlan {
            actions: vec![
                SyncAction::Pull {
                    path: "c.yml".into(),
                },
                SyncAction::Pull {
                    path: "n.yml".into(),
                },
                SyncAction::DeleteMine {
                    path: "d.yml".into(),
                },
                SyncAction::RenameMine {
                    from: "r.yml".into(),
                    to: "r2.yml".into(),
                },
                SyncAction::Push {
                    path: "s.yml".into(),
                },
                SyncAction::Push {
                    path: "x.yml".into(),
                },
                SyncAction::DeleteTheirs {
                    path: "t.yml".into(),
                },
            ],
            conflicts: vec![],
        };
        let received: BTreeMap<String, Vec<u8>> = [
            ("c.yml".to_string(), b"scripts:\n  - code: x\n".to_vec()),
            ("n.yml".to_string(), b"auth:\n  type: basic\n".to_vec()),
        ]
        .into();

        let s = planner_summary(&plan, &mine, &theirs, &received);

        assert_eq!(
            (
                s.added_here,
                s.changed_here,
                s.deleted_here,
                s.sent_added,
                s.sent_changed,
                s.sent_deleted
            ),
            (1, 2, 1, 1, 1, 1)
        );
        assert_eq!(s.scripts_changed, vec!["c.yml"]);
        assert_eq!(s.auth_changed, vec!["n.yml"]);
    }

    #[test]
    fn the_starter_summary_counts_its_own_changes() {
        let offered = m(&[("a.yml", '1'), ("b.yml", '2'), ("r.yml", '3')]);
        let mut p = payload();
        p.deletes = vec!["b.yml".into()];
        p.renames = vec![rename("r.yml", "r2.yml")];
        p.sends = vec![send("a.yml", '4'), send("n.yml", '5')];
        let received: BTreeMap<String, Vec<u8>> = [
            ("a.yml".to_string(), b"url: a\n".to_vec()),
            ("n.yml".to_string(), b"tests: x\n".to_vec()),
        ]
        .into();

        let s = starter_summary(&p, &offered, 2, &received);

        assert_eq!(
            (
                s.added_here,
                s.changed_here,
                s.deleted_here,
                s.sent_added,
                s.sent_changed,
                s.sent_deleted
            ),
            (1, 2, 1, 0, 2, 0)
        );
        assert_eq!(s.scripts_changed, vec!["n.yml"]);
        assert!(s.auth_changed.is_empty());
    }

    #[test]
    fn a_rename_conflict_wants_the_file_where_the_starter_holds_it() {
        let conflict = rocket_network::Conflict {
            path: "r.yml".into(),
            kind: rocket_network::ConflictKind::RenameVsEdit,
            mine: Some(h('1')),
            theirs: Some(h('2')),
        };
        // The starter renamed r.yml to r2.yml: it no longer has r.yml.
        let theirs_renamed = SyncPlan {
            actions: vec![SyncAction::RenameMine {
                from: "r.yml".into(),
                to: "r2.yml".into(),
            }],
            conflicts: vec![conflict.clone()],
        };
        assert_eq!(
            wanted_paths(&theirs_renamed),
            BTreeSet::from(["r2.yml".to_string()])
        );
        // The planner renamed it: the starter still has r.yml.
        let mine_renamed = SyncPlan {
            actions: vec![SyncAction::RenameTheirs {
                from: "r.yml".into(),
                to: "r2.yml".into(),
            }],
            conflicts: vec![conflict],
        };
        assert_eq!(
            wanted_paths(&mine_renamed),
            BTreeSet::from(["r.yml".to_string()])
        );
        assert_eq!(
            conflict_paths(
                &mine_renamed,
                "r.yml",
                rocket_network::ConflictKind::RenameVsEdit
            ),
            ("r2.yml".to_string(), "r.yml".to_string())
        );
        let s = planner_summary(
            &mine_renamed,
            &Manifest::new(),
            &Manifest::new(),
            &BTreeMap::new(),
        );
        assert_eq!(s.sent_changed, 0, "a rename marker is not counted");
    }

    #[test]
    fn a_script_file_is_marked_by_its_path() {
        let received: BTreeMap<String, Vec<u8>> = [
            ("lib/a.js".to_string(), b"export const a = 1;".to_vec()),
            ("p.proto".to_string(), b"syntax = \"proto3\";".to_vec()),
        ]
        .into();
        let s = starter_summary(&payload(), &Manifest::new(), 0, &received);
        assert_eq!(s.scripts_changed, vec!["lib/a.js"]);
        assert!(s.auth_changed.is_empty());
    }
}
