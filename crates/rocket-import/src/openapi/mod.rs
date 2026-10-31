pub(crate) mod bundle;
pub(crate) mod entry;
pub(crate) mod layout;
pub(crate) mod matching;
pub(crate) mod model;
pub(crate) mod reader;
pub(crate) mod source_record;
pub(crate) mod source_url;
pub(crate) mod yaml_budget;

pub use bundle::{
    bundle_spec, find_remote_refs, inline_refs, lost_refs_message, ref_failure_message,
    refs_unchanged, refs_unchanged_labeled, whole_bundle_failed_message, Bundle, RemoteRefState,
};
pub use entry::{
    check_spec_text, fetch_failure_message, fetch_spec_bundle, fetch_spec_bundle_from, FetchedSpec,
    OpenApiImportResult, SpecSources,
};
pub use layout::GroupBy;
pub(crate) use model::{
    OpenApiDoc, Operation, ParamLocation, Parameter, SecurityScheme, SecuritySchemeKind,
};
pub(crate) use reader::parse_openapi;
pub use source_record::source_record;
pub use source_url::{
    fetch_target, fetch_target_fresh, parse_spec_target, parse_spec_url, FetchTarget,
};
pub(crate) mod sync;
pub use matching::{
    match_requests, normalise_path, AmbiguousGroup, MatchPair, MatchRequest, MatchResult,
};
pub use sync::{
    build_apply, build_sync_plan, fingerprint, item_digest, merge_item, merge_request,
    omitted_schema, plan_with_links, request_is_edited, reset_spec_owned_fields,
    restore_spec_owned_fields, variable_digest, AppliedItem, AppliedKind, ApplyInputs, ApplyOutput,
    ApplyReport, ApprovedItem, BodyView, Choice, CurrentRequest, FieldChange, FieldOutcome,
    FileChange, FileChangeKind, MergeOutcome, ParsedSpec, RequestRenderer, SchemaView, Selection,
    SkipRequest, SpecOperationView, SpecParamView, SpecResponseView, SyncInputs, SyncItem,
    SyncPlan, VariableApproval, VariableChange, VariableKind,
};
