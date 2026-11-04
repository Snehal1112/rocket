//! Filesystem implementation of the contract attachment store.

use std::path::{Path, PathBuf};

use rocket_collection::contract::repository::{ContractError, ContractResult};
use rocket_collection::contract::AttachmentStore;
use ulid::Ulid;

use crate::fs_contract_repo::FsContractRepo;

pub struct FsAttachmentStore;

impl AttachmentStore for FsAttachmentStore {
    fn file_size(&self, src: &Path) -> ContractResult<u64> {
        Ok(std::fs::metadata(src)?.len())
    }

    fn store(
        &self,
        collection_root: &Path,
        contract_id: Ulid,
        src: &Path,
    ) -> ContractResult<PathBuf> {
        let file_name = src
            .file_name()
            .ok_or_else(|| ContractError::Internal("Attachment has no filename.".into()))?;
        let dest_dir = FsContractRepo::attachments_dir(collection_root, contract_id);
        std::fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(file_name);
        std::fs::copy(src, &dest)?;
        // The relative path stays valid when the collection moves to another machine.
        let rel = dest.strip_prefix(collection_root).map_err(|_| {
            ContractError::Internal("Attachment dest is outside collection root.".into())
        })?;
        Ok(rel.to_path_buf())
    }

    fn retain_only(
        &self,
        collection_root: &Path,
        contract_id: Ulid,
        kept: &[PathBuf],
    ) -> ContractResult<()> {
        let dir = FsContractRepo::attachments_dir(collection_root, contract_id);
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let rel = path
                .strip_prefix(collection_root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.clone());
            if !kept.contains(&rel) {
                // A file that cannot be removed is left in place.
                let _ = std::fs::remove_file(&path);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn source(tmp: &TempDir, name: &str) -> PathBuf {
        let path = tmp.path().join("picked").join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"hello").expect("write");
        path
    }

    #[test]
    fn store_copies_and_returns_the_relative_path() {
        let tmp = TempDir::new().expect("tmp");
        let root = tmp.path().join("demo");
        let id = Ulid::new();
        let src = source(&tmp, "spec.pdf");

        let rel = FsAttachmentStore.store(&root, id, &src).expect("store");

        assert!(rel.is_relative());
        assert_eq!(std::fs::read(root.join(&rel)).expect("read"), b"hello");
    }

    #[test]
    fn file_size_reports_bytes_and_errors_for_a_missing_file() {
        let tmp = TempDir::new().expect("tmp");
        let src = source(&tmp, "a.txt");

        assert_eq!(FsAttachmentStore.file_size(&src).expect("size"), 5);
        assert!(FsAttachmentStore
            .file_size(&tmp.path().join("missing.txt"))
            .is_err());
    }

    #[test]
    fn store_of_a_missing_source_errors() {
        let tmp = TempDir::new().expect("tmp");
        let root = tmp.path().join("demo");

        let result = FsAttachmentStore.store(&root, Ulid::new(), &tmp.path().join("nope.pdf"));

        assert!(result.is_err());
    }

    #[test]
    fn retain_only_removes_unlisted_files() {
        let tmp = TempDir::new().expect("tmp");
        let root = tmp.path().join("demo");
        let id = Ulid::new();
        let keep = FsAttachmentStore
            .store(&root, id, &source(&tmp, "keep.txt"))
            .expect("store keep");
        let drop = FsAttachmentStore
            .store(&root, id, &source(&tmp, "drop.txt"))
            .expect("store drop");

        FsAttachmentStore
            .retain_only(&root, id, std::slice::from_ref(&keep))
            .expect("retain");

        assert!(root.join(keep).exists());
        assert!(!root.join(drop).exists());
    }

    #[test]
    fn retain_only_with_an_empty_kept_list_removes_every_file() {
        let tmp = TempDir::new().expect("tmp");
        let root = tmp.path().join("demo");
        let id = Ulid::new();
        let first = FsAttachmentStore
            .store(&root, id, &source(&tmp, "one.txt"))
            .expect("store one");
        let second = FsAttachmentStore
            .store(&root, id, &source(&tmp, "two.txt"))
            .expect("store two");

        FsAttachmentStore
            .retain_only(&root, id, &[])
            .expect("retain");

        assert!(!root.join(first).exists());
        assert!(!root.join(second).exists());
    }

    #[test]
    fn retain_only_with_no_directory_is_ok() {
        let tmp = TempDir::new().expect("tmp");

        let result = FsAttachmentStore.retain_only(tmp.path(), Ulid::new(), &[]);

        assert!(result.is_ok());
    }
}
