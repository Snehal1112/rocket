pub(crate) mod bru;
pub(crate) mod converter;
pub mod error;
mod importer;
pub(crate) mod openapi;
pub(crate) mod postman;
pub mod report;
pub(crate) mod wsdl;

pub use error::{ImportError, ImportResult};
pub use importer::{
    EnvironmentRepositoryFactory, ImportService, ImportedOperation, OpenApiImportOutcome,
};
pub use converter::openapi::{baseline_hash, CONVERTER_VERSION};
pub use openapi::{fetch_spec_bundle, fetch_spec_bundle_from, FetchedSpec, SpecSources};
pub use openapi::{
    match_requests, normalise_path, AmbiguousGroup, MatchPair, MatchRequest, MatchResult,
};
pub use openapi::{
    bundle_spec, find_remote_refs, inline_refs, lost_refs_message, ref_failure_message, whole_bundle_failed_message, refs_unchanged,
    refs_unchanged_labeled, Bundle, RemoteRefState,
    build_apply, item_digest, plan_with_links, AppliedItem, AppliedKind, ApplyInputs, ApplyOutput, ApplyReport,
    ApprovedItem, FileChange, FileChangeKind, ParsedSpec, RequestRenderer, Selection, SkipRequest, SpecOperationView, SpecParamView, SpecResponseView, BodyView, SchemaView, omitted_schema, reset_spec_owned_fields, restore_spec_owned_fields,
    VariableApproval, variable_digest,
    build_sync_plan, check_spec_text, fetch_failure_message, fetch_target, fetch_target_fresh, fingerprint,
    merge_item, merge_request, parse_spec_target, parse_spec_url, request_is_edited, source_record, Choice,
    CurrentRequest, FetchTarget, FieldChange, FieldOutcome, GroupBy, MergeOutcome, OpenApiImportResult,
    SyncInputs, SyncItem, SyncPlan, VariableChange, VariableKind,
};
pub use report::{ImportReport, SkipReason, SkippedItem};
