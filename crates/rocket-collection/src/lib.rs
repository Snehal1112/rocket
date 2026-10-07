pub mod collection;
pub mod contract;
pub mod folder;
pub mod folder_settings;
pub mod graphql_request;
pub mod grpc_request;
pub mod repository;
pub mod request;
pub mod request_kind;
pub mod request_summary;
mod script_file;
pub mod settings;
pub mod summary;
pub mod websocket;
pub(crate) mod uid;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use folder_settings::{
    chain_scripts, inherited_headers, resolve_folder_auth, FolderSettings, ScriptFlow,
    ScriptPhase,
};
pub use graphql_request::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
pub use grpc_request::{
    GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript,
};
pub use repository::CollectionRepository;
pub use request_kind::RequestKind;
pub use request::{
    candidate_filename, request_filename_for, Request, MAX_FILENAME_COLLISION_RETRIES,
};
pub use request_summary::RequestSummary;
pub use script_file::{normalize_script_name, ScriptFileItem, SCRIPT_TEMPLATE};
pub use settings::{CollectionSettings, CollectionVariable};
pub use summary::CollectionSummary;
pub use websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
pub use uid::generate_uid;
