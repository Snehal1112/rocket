use crate::contract::repository::ContractResult;
use std::path::{Path, PathBuf};
use ulid::Ulid;

/// Port for the files attached to a contract. The service validates type and size;
/// the store does the disk work.
pub trait AttachmentStore: Send + Sync {
    /// Size in bytes of a user-chosen source file.
    fn file_size(&self, src: &Path) -> ContractResult<u64>;

    /// Copies `src` into the contract's attachments directory (created on demand)
    /// and returns the stored path relative to `collection_root`.
    fn store(
        &self,
        collection_root: &Path,
        contract_id: Ulid,
        src: &Path,
    ) -> ContractResult<PathBuf>;

    /// Removes every stored file whose relative path is not in `kept`.
    /// A missing directory is not an error; a single failed removal is skipped.
    fn retain_only(
        &self,
        collection_root: &Path,
        contract_id: Ulid,
        kept: &[PathBuf],
    ) -> ContractResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn AttachmentStore>) {}
    }
}
