//! Ports for the temporary file that holds a received collection ZIP.

use std::path::PathBuf;

use rocket_shared::error::DomainResult;

/// A finished staged file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub path: PathBuf,
    pub len: u64,
    /// Lowercase hex SHA-256 of the whole file.
    pub sha256: String,
}

/// One file being written chunk by chunk. Dropping it without `finish` removes the file.
pub trait StagedZip: Send {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()>;
    /// Closes the file and returns its path, length and hash.
    fn finish(self: Box<Self>) -> DomainResult<StagedFile>;
    /// Deletes the file.
    fn discard(self: Box<Self>);
}

/// Where received ZIPs are staged. Implemented by `rocket-infra`.
pub trait TransferStaging: Send + Sync {
    /// Starts a staged file for a transfer id. The id must be 32 lowercase hex characters.
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>>;
    /// Removes everything in the staging folder. A missing folder is fine.
    fn purge(&self) -> DomainResult<()>;
    /// Deletes the staged `<transfer_id>.zip` after the import or an abort. A missing file is
    /// fine. A path outside the staging folder is refused with `InvalidInput`.
    fn release(&self, file: &StagedFile) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_traits_are_object_safe() {
        fn takes(_: Option<&dyn TransferStaging>, _: Option<Box<dyn StagedZip>>) {}
        takes(None, None);
    }
}
