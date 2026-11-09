pub mod collection;
pub mod contract;
pub mod discovery;
pub mod duplicate;
mod events;
pub mod file_tx;
pub mod folder;
pub mod folder_settings;
pub mod graphql_request;
pub mod grpc_request;
pub mod names;
pub mod openapi_backup;
pub mod openapi_baseline;
pub mod openapi_consent;
pub mod openapi_local;
pub mod openapi_schedule;
pub mod openapi_source;
pub mod rel_path;
pub mod repository;
pub mod request;
pub mod request_kind;
pub mod request_renderer;
pub mod request_summary;
mod script_file;
pub mod settings;
pub mod summary;
pub mod template;
pub mod template_repository;
pub mod trust;
pub(crate) mod uid;
pub mod websocket;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use duplicate::{
    duplicate_candidate, duplicate_of, DuplicateCandidate, MAX_DUPLICATE_ATTEMPTS,
};
pub use events::{CollectionEvent, FileChangeKind};
pub use file_tx::{validate_rel_path, CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome};
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use folder_settings::{
    chain_scripts, effective_auth, inherited_headers, resolve_folder_auth, FolderSettings,
    ScriptFlow, ScriptPhase,
};
pub use graphql_request::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
pub use grpc_request::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript};
pub use names::CollectionName;
pub use openapi_backup::{
    BackupEntry, OpenApiBackupRepository, OpenApiBackups, BACKUP_FILE_VERSION,
};
pub use openapi_baseline::{
    BaselineOperation, OpenApiBaseline, OpenApiBaselineRepository, BASELINE_FILE_VERSION,
};
pub use openapi_consent::{
    consent_root_key, location_key, ConsentState, InMemoryConsentStore, OpenApiConsent,
    OpenApiConsentStore,
};
pub use openapi_local::{OpenApiLocalRepository, OpenApiLocalState, LOCAL_STATE_FILE_VERSION};
pub use openapi_schedule::{
    backoff_secs, clamp_retry_after, jitter_secs, next_check_at, SchedulePlan,
};
pub use rel_path::RelPath;
pub use repository::{CollectionRepository, RequestScriptPhase};
pub use request::{
    candidate_filename, fold_name, request_filename_for, Request, MAX_FILENAME_COLLISION_RETRIES,
};
pub use request_kind::RequestKind;
pub use request_renderer::RequestRenderer;
pub use request_summary::RequestSummary;
pub use script_file::{normalize_script_name, ScriptFileItem, SCRIPT_TEMPLATE};
pub use settings::{CollectionSettings, CollectionVariable};
pub use summary::CollectionSummary;
pub use template::Template;
pub use template_repository::TemplateRepository;
pub use trust::{
    normalize_root, request_fingerprint, resolve_effective, CollectionGrant, CollectionIdentity,
    CollectionTrustStore, DenyAllTrustStore, EffectiveCapabilities, GrantSource, LegacyCollection,
    MigrationNoticeEntry, RequestedElevation,
};
pub use uid::generate_uid;
pub use websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
