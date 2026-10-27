pub(crate) mod bundle;
pub(crate) mod entry;
pub(crate) mod layout;
pub(crate) mod matching;
pub(crate) mod model;
pub(crate) mod reader;
pub(crate) mod source_record;
pub(crate) mod source_url;
pub(crate) mod yaml_budget;

pub use layout::GroupBy;
pub(crate) use model::{
    OpenApiDoc, Operation, ParamLocation, Parameter, SecurityScheme, SecuritySchemeKind,
};
pub(crate) use reader::parse_openapi;
pub use source_record::source_record;
pub use bundle::{
    bundle_spec, find_remote_refs, inline_refs, lost_refs_message, ref_failure_message, whole_bundle_failed_message, refs_unchanged,
    refs_unchanged_labeled, Bundle, RemoteRefState,
};
pub use source_url::{fetch_target, parse_spec_target, parse_spec_url, FetchTarget};
pub use entry::{
    check_spec_text, fetch_failure_message, fetch_spec_bundle, FetchedSpec, OpenApiImportResult,
};
pub(crate) mod sync;
pub use sync::{
    build_apply, item_digest, plan_with_links, AppliedItem, AppliedKind, ApplyInputs, ApplyOutput, ApplyReport,
    ApprovedItem, FileChange, FileChangeKind, ParsedSpec, RequestRenderer, Selection, SkipRequest, SpecOperationView, SpecParamView, SpecResponseView, reset_spec_owned_fields, RESET_FIELD_NAMES,
    VariableApproval, variable_digest,
    build_sync_plan, fingerprint, merge_item, merge_request, request_is_edited, Choice,
    CurrentRequest, FieldChange, FieldOutcome, MergeOutcome, SyncInputs, SyncItem, SyncPlan,
    VariableChange, VariableKind,
};
pub use matching::{
    match_requests, normalise_path, AmbiguousGroup, MatchPair, MatchRequest, MatchResult,
};
