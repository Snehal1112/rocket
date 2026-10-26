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
