//! Pure rules of the sync: which paths are allowed, what a received plan must satisfy, the
//! starter's new base, the planner's split of actions, summaries, marks and fixed texts.
//!
//! Every text a user or a peer can see is a fixed sentence from this file. None of them ever
//! carries a peer text, a file name, a path or file content (S14).

use rocket_network::limits::MAX_TRANSFER_BYTES;
use rocket_network::{SyncAbortReason, SyncFinish};

const MIB: u64 = 1024 * 1024;

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
    let tenths = (len * 10).div_ceil(MIB);
    format!(
        "The changes are too big to sync ({}.{} MiB, limit {}).",
        tenths / 10,
        tenths % 10,
        MAX_TRANSFER_BYTES / MIB
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
