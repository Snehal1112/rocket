//! Three-way merge of a synced request: the baseline, the user's copy and the new spec.

pub(crate) mod json_merge;
pub(crate) mod merge;
pub(crate) mod plan;
pub(crate) mod rename;

pub use merge::{
    merge_request, request_is_edited, Choice, FieldChange, FieldOutcome, MergeOutcome,
};
pub use plan::{
    build_sync_plan, fingerprint, CurrentRequest, SyncInputs, SyncItem, SyncPlan, VariableChange,
    VariableKind,
};
