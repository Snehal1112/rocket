//! Read-only views of a pending sync, for a screen that shows what an apply would do. They
//! carry no serde; the IPC layer maps them to its own DTOs.

use rocket_import::SyncPlan;

/// An operation the pending spec leaves as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnchangedView {
    pub key: String,
    pub name: String,
    pub request_path: Option<String>,
}

/// What an apply would do with the pending spec. Without a pending copy `pending` is false and
/// everything else is empty.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanView {
    pub pending: bool,
    pub plan: SyncPlan,
    /// `item_digest` of each plan item, in plan order.
    pub item_digests: Vec<String>,
    /// `variable_digest` of each variable change, in plan order.
    pub variable_digests: Vec<String>,
    pub unchanged: Vec<UnchangedView>,
}

impl PlanView {
    pub(crate) fn empty() -> Self {
        Self {
            pending: false,
            plan: SyncPlan::default(),
            item_digests: Vec::new(),
            variable_digests: Vec::new(),
            unchanged: Vec::new(),
        }
    }
}

/// One request that differs from what the spec produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRow {
    /// The operation key, or `None` for a request the user added.
    pub key: Option<String>,
    pub uid: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub request_path: Option<String>,
    /// Names of the spec-owned fields the user changed. Empty when the baseline is not trusted
    /// and for deleted and added rows.
    pub fields: Vec<String>,
}

/// What the user changed since the last import or sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesView {
    /// False without a baseline of the current converter version. Then `modified` comes from
    /// hashes and carries no field names.
    pub baseline_trusted: bool,
    pub modified: Vec<ChangeRow>,
    pub added: Vec<ChangeRow>,
    pub deleted: Vec<ChangeRow>,
}

/// The counts and facts of the Overview tab. `in_sync` and `changed_by_you` are `None` when
/// the baseline cannot be trusted, so the screen shows them as unknown and not as zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewView {
    /// Operations in the stored source.
    pub endpoints: u32,
    pub in_sync: Option<u32>,
    pub changed_by_you: Option<u32>,
    /// Items and variable changes that wait for a decision.
    pub updates_waiting: u32,
    /// `info.version` of the pending copy. `None` without one.
    pub spec_version: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_synced_updated: u32,
}

/// A collection with updates waiting, for the sidebar badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeView {
    pub collection: String,
    pub waiting: u32,
}
