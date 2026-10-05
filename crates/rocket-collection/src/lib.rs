pub mod collection;
pub mod contract;
pub mod folder;
pub mod graphql_request;
pub mod repository;
pub mod request;
pub mod request_kind;
pub mod request_summary;
pub mod settings;
pub mod summary;
pub(crate) mod uid;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use graphql_request::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
pub use repository::CollectionRepository;
pub use request_kind::RequestKind;
pub use request::{
    candidate_filename, request_filename_for, Request, MAX_FILENAME_COLLISION_RETRIES,
};
pub use request_summary::RequestSummary;
pub use settings::{CollectionSettings, CollectionVariable};
pub use summary::CollectionSummary;
pub use uid::generate_uid;
