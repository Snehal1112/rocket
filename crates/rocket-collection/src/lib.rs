pub mod collection;
pub mod contract;
pub mod folder;
pub mod repository;
pub mod request;
pub mod settings;
pub mod summary;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use repository::CollectionRepository;
pub use request::Request;
pub use settings::{CollectionSettings, CollectionVariable};
pub use summary::CollectionSummary;
