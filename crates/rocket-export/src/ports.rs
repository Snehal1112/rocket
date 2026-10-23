//! Ports implemented by the infrastructure layer.

use rocket_shared::error::DomainResult;

/// One file of a collection, with a path relative to the collection root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub rel_path: String,
    pub bytes: Vec<u8>,
}

/// Reads the raw files of a collection.
pub trait CollectionFileSource: Send + Sync {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>>;
}
