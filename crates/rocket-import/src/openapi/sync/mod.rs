//! Three-way merge of a synced request: the baseline, the user's copy and the new spec.

pub(crate) mod apply;
pub(crate) mod apply_paths;
pub(crate) mod apply_types;
pub(crate) mod json_merge;
pub(crate) mod merge;
pub(crate) mod plan;
pub(crate) mod rename;
pub(crate) mod reset;
pub(crate) mod schema_view;
pub(crate) mod spec;

pub use apply::{build_apply, plan_with_links};
pub use apply_types::{
    AppliedItem, AppliedKind, ApplyInputs, ApplyOutput, ApplyReport, ApprovedItem, FileChange,
    FileChangeKind, RequestRenderer, Selection, SkipRequest, VariableApproval,
};
pub use merge::{
    merge_request, request_is_edited, Choice, FieldChange, FieldOutcome, MergeOutcome,
};
pub use plan::{
    build_sync_plan, fingerprint, item_digest, merge_item, variable_digest, CurrentRequest, SyncInputs, SyncItem,
    SyncPlan, VariableChange, VariableKind,
};
pub use reset::{reset_spec_owned_fields, restore_spec_owned_fields};
pub use schema_view::{omitted as omitted_schema, BodyView, SchemaView};
pub use spec::{ParsedSpec, SpecOperationView, SpecParamView, SpecResponseView};
