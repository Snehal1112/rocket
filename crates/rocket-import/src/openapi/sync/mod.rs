//! Three-way merge of a synced request: the baseline, the user's copy and the new spec.

pub(crate) mod apply;
pub(crate) mod apply_paths;
pub(crate) mod apply_types;
pub(crate) mod json_merge;
pub(crate) mod merge;
pub(crate) mod plan;
pub(crate) mod rename;
pub(crate) mod spec;

pub use apply::build_apply;
pub use apply_types::{
    AppliedItem, AppliedKind, ApplyInputs, ApplyOutput, ApplyReport, ApprovedItem, FileChange,
    FileChangeKind, RequestRenderer, Selection, SkipRequest,
};
pub use merge::{
    merge_request, request_is_edited, Choice, FieldChange, FieldOutcome, MergeOutcome,
};
pub use plan::{
    build_sync_plan, fingerprint, item_digest, merge_item, CurrentRequest, SyncInputs, SyncItem,
    SyncPlan, VariableChange, VariableKind,
};
pub use spec::ParsedSpec;
