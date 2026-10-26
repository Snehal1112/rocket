pub mod collection;
pub mod contract;
pub mod folder;
pub mod folder_settings;
pub mod graphql_request;
pub mod file_tx;
pub mod openapi_baseline;
pub mod openapi_local;
pub mod openapi_source;
pub mod grpc_request;
pub mod repository;
pub mod request;
pub mod request_kind;
pub mod request_summary;
mod script_file;
pub mod settings;
pub mod summary;
pub mod trust;
pub(crate) mod uid;
pub mod websocket;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use folder_settings::{
    chain_scripts, inherited_headers, resolve_folder_auth, FolderSettings, ScriptFlow, ScriptPhase,
};
pub use graphql_request::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
pub use grpc_request::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript};
pub use repository::{CollectionRepository, RequestScriptPhase};
pub use request::{
    candidate_filename, fold_name, request_filename_for, Request, MAX_FILENAME_COLLISION_RETRIES,
};
pub use openapi_baseline::{
    BaselineOperation, OpenApiBaseline, OpenApiBaselineRepository, BASELINE_FILE_VERSION,
};
pub use file_tx::{
    validate_rel_path, CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome,
};
pub use openapi_local::{OpenApiLocalRepository, OpenApiLocalState, LOCAL_STATE_FILE_VERSION};
pub use request_kind::RequestKind;
pub use request_summary::RequestSummary;
pub use script_file::{normalize_script_name, ScriptFileItem, SCRIPT_TEMPLATE};
pub use settings::{CollectionSettings, CollectionVariable};
pub use summary::CollectionSummary;
pub use trust::{
    normalize_root, request_fingerprint, resolve_effective, CollectionGrant, CollectionIdentity,
    CollectionTrustStore, DenyAllTrustStore, EffectiveCapabilities, GrantSource, LegacyCollection,
    MigrationNoticeEntry, RequestedElevation,
};
pub use uid::generate_uid;
pub use websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
