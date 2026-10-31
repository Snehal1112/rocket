pub(crate) mod bru;
pub(crate) mod converter;
pub mod error;
mod importer;
pub(crate) mod openapi;
pub(crate) mod postman;
pub mod report;
pub(crate) mod wsdl;

pub use converter::openapi::{baseline_hash, CONVERTER_VERSION};
pub use error::{ImportError, ImportResult};
pub use importer::{
    EnvironmentRepositoryFactory, ImportService, ImportedOperation, OpenApiImportOutcome,
};
pub use openapi::{
    build_apply, build_sync_plan, bundle_spec, check_spec_text, fetch_failure_message,
    fetch_target, fetch_target_fresh, find_remote_refs, fingerprint, inline_refs, item_digest,
    lost_refs_message, merge_item, merge_request, omitted_schema, parse_spec_target,
    parse_spec_url, plan_with_links, ref_failure_message, refs_unchanged, refs_unchanged_labeled,
    request_is_edited, reset_spec_owned_fields, restore_spec_owned_fields, source_record,
    variable_digest, whole_bundle_failed_message, AppliedItem, AppliedKind, ApplyInputs,
    ApplyOutput, ApplyReport, ApprovedItem, BodyView, Bundle, Choice, CurrentRequest, FetchTarget,
    FieldChange, FieldOutcome, FileChange, FileChangeKind, GroupBy, MergeOutcome,
    OpenApiImportResult, ParsedSpec, RemoteRefState, RequestRenderer, SchemaView, Selection,
    SkipRequest, SpecOperationView, SpecParamView, SpecResponseView, SyncInputs, SyncItem,
    SyncPlan, VariableApproval, VariableChange, VariableKind,
};
pub use openapi::{fetch_spec_bundle, fetch_spec_bundle_from, FetchedSpec, SpecSources};
pub use openapi::{
    match_requests, normalise_path, AmbiguousGroup, MatchPair, MatchRequest, MatchResult,
};
pub use report::{ImportReport, SkipReason, SkippedItem};
