//! Stores a collection's OpenAPI source at `<collection>/.rocket/openapi-source.yml`.
//! A dot folder is hidden from the collection loader, so the file is never shown as a request.

use std::path::{Path, PathBuf};

use rocket_collection::openapi_source::{
    OpenApiSource, OpenApiSourceRepository, SOURCE_FILE_MAX_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// The filesystem implementation. It holds no state; every call is given the collection root.
pub struct FsOpenApiSourceRepo;

const SOURCE_DIR: &str = ".rocket";
const SOURCE_FILE: &str = "openapi-source.yml";

const PENDING_FILE: &str = "openapi-pending.spec";
const APPLIED_FILE: &str = "openapi-applied.spec";

fn pending_path(collection_root: &Path) -> PathBuf {
    collection_root.join(SOURCE_DIR).join(PENDING_FILE)
}

fn applied_path(collection_root: &Path) -> PathBuf {
    collection_root.join(SOURCE_DIR).join(APPLIED_FILE)
}

fn source_path(collection_root: &Path) -> PathBuf {
    collection_root.join(SOURCE_DIR).join(SOURCE_FILE)
}

impl OpenApiSourceRepository for FsOpenApiSourceRepo {
    fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiSource>> {
        let text = match std::fs::read_to_string(source_path(collection_root)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(DomainError::Internal(format!(
                    "{SOURCE_FILE} could not be read: {e}"
                )))
            }
        };
        // Read the version first, so a file from a newer Rocket is reported as that even when
        // its other fields no longer fit this version's shape.
        let value: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("{SOURCE_FILE} cannot be parsed: {e}")))?;
        let version = value.get("version").and_then(serde_yaml::Value::as_u64);
        if version.is_some_and(|v| v > u64::from(SOURCE_FILE_MAX_VERSION)) {
            return Err(DomainError::Internal(format!(
                "{SOURCE_FILE} was written by a newer version of Rocket (it reads up to version {SOURCE_FILE_MAX_VERSION})"
            )));
        }
        let source: OpenApiSource = serde_yaml::from_value(value)
            .map_err(|e| DomainError::Internal(format!("{SOURCE_FILE} cannot be parsed: {e}")))?;
        Ok(Some(source))
    }

    fn save(&self, collection_root: &Path, source: &OpenApiSource) -> DomainResult<()> {
        // A local file source is written as version 2, so an older build refuses it.
        let mut source = source.clone();
        source.version = source.write_version();
        let yaml = serde_yaml::to_string(&source).map_err(|e| {
            DomainError::Internal(format!("{SOURCE_FILE} could not be written: {e}"))
        })?;
        atomic_write(&source_path(collection_root), yaml.as_bytes())?;
        Ok(())
    }

    fn delete(&self, collection_root: &Path) -> DomainResult<()> {
        match std::fs::remove_file(source_path(collection_root)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DomainError::Internal(format!(
                "{SOURCE_FILE} could not be removed: {e}"
            ))),
        }
    }

    fn save_pending(&self, collection_root: &Path, spec: &[u8]) -> DomainResult<()> {
        // Ignore the file first, so a failure never leaves an unignored copy on disk.
        crate::rocket_dir::ensure_gitignore(collection_root)?;
        atomic_write(&pending_path(collection_root), spec)?;
        Ok(())
    }

    fn load_pending(&self, collection_root: &Path) -> DomainResult<Option<Vec<u8>>> {
        match std::fs::read(pending_path(collection_root)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DomainError::Internal(format!(
                "{PENDING_FILE} could not be read: {e}"
            ))),
        }
    }

    fn save_applied(&self, collection_root: &Path, spec: &[u8]) -> DomainResult<()> {
        // Ignore the file first, so a failure never leaves an unignored copy on disk.
        crate::rocket_dir::ensure_gitignore(collection_root)?;
        atomic_write(&applied_path(collection_root), spec)?;
        Ok(())
    }

    fn load_applied(&self, collection_root: &Path) -> DomainResult<Option<Vec<u8>>> {
        match std::fs::read(applied_path(collection_root)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DomainError::Internal(format!(
                "{APPLIED_FILE} could not be read: {e}"
            ))),
        }
    }

    fn applied_matches(&self, collection_root: &Path, expected_sha256: &str) -> DomainResult<bool> {
        use sha2::{Digest, Sha256};
        let unreadable = |e: std::io::Error| {
            DomainError::Internal(format!("{APPLIED_FILE} could not be read: {e}"))
        };
        let mut file = match std::fs::File::open(applied_path(collection_root)) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(unreadable(e)),
        };
        let mut hasher = Sha256::new();
        // A directory opens on Linux and fails on the first read, so it reads as damaged.
        std::io::copy(&mut file, &mut hasher).map_err(unreadable)?;
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(actual == expected_sha256)
    }

    fn clear_pending(&self, collection_root: &Path) -> DomainResult<()> {
        match std::fs::remove_file(pending_path(collection_root)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DomainError::Internal(format!(
                "{PENDING_FILE} could not be removed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::openapi_source::{
        SourceOperation, SpecLocation, SyncInterval, SOURCE_FILE_VERSION,
    };
    use rocket_collection::CollectionRepository;
    use tempfile::tempdir;

    #[test]
    fn applied_copy_round_trips_and_a_missing_one_is_none() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(
            FsOpenApiSourceRepo.load_applied(dir.path()).expect("load"),
            None
        );
        FsOpenApiSourceRepo
            .save_applied(dir.path(), b"openapi: 3.0.3")
            .expect("save");
        assert_eq!(
            FsOpenApiSourceRepo.load_applied(dir.path()).expect("load"),
            Some(b"openapi: 3.0.3".to_vec())
        );
    }

    #[test]
    fn applied_matches_only_a_file_with_the_expected_hash() {
        use sha2::{Digest, Sha256};
        let dir = tempdir().expect("tempdir");
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let spec = b"openapi: 3.0.3";
        assert!(!FsOpenApiSourceRepo
            .applied_matches(dir.path(), &hash(spec))
            .expect("matches"));
        FsOpenApiSourceRepo
            .save_applied(dir.path(), spec)
            .expect("save");
        assert!(FsOpenApiSourceRepo
            .applied_matches(dir.path(), &hash(spec))
            .expect("matches"));
        assert!(!FsOpenApiSourceRepo
            .applied_matches(dir.path(), &hash(b"another spec"))
            .expect("matches"));
    }

    #[test]
    fn applied_matches_compares_the_hash_exactly_like_the_other_checks() {
        use sha2::{Digest, Sha256};
        let dir = tempdir().expect("tempdir");
        let spec = b"openapi: 3.0.3";
        FsOpenApiSourceRepo
            .save_applied(dir.path(), spec)
            .expect("save");
        let lower: String = Sha256::digest(spec)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(FsOpenApiSourceRepo
            .applied_matches(dir.path(), &lower)
            .expect("matches"));
        // The other three comparisons are exact, so an upper-case hash must not match here.
        assert!(!FsOpenApiSourceRepo
            .applied_matches(dir.path(), &lower.to_uppercase())
            .expect("matches"));
    }

    #[test]
    fn a_failing_ignore_step_leaves_no_pending_file_behind() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket/.gitignore")).expect("mkdir");
        assert!(FsOpenApiSourceRepo
            .save_pending(dir.path(), b"spec")
            .is_err());
        assert!(!dir.path().join(".rocket/openapi-pending.spec").exists());
    }

    fn sample(location: SpecLocation) -> OpenApiSource {
        OpenApiSource {
            version: SOURCE_FILE_VERSION,
            location,
            group_by: "tags".to_string(),
            spec_sha256: "ab".repeat(32),
            etag: Some("\"abc\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
            operations: vec![SourceOperation {
                key: "listOrders".to_string(),
                uid: "uid-1".to_string(),
                request_path: "Orders/List-orders".to_string(),
                baseline_hash: "cd".repeat(32),
            }],
            source_id: "id-1".to_string(),
            sync_interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            pending_sha256: Some("ef".repeat(32)),
            remote_refs: Vec::new(),
            remote_refs_incomplete: false,
        }
    }

    fn github() -> SpecLocation {
        SpecLocation::Github {
            owner: "acme".to_string(),
            repo: "orders".to_string(),
            git_ref: "main".to_string(),
            path: "docs/openapi.yaml".to_string(),
        }
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(FsOpenApiSourceRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn save_then_load_round_trips_both_location_kinds() {
        for location in [
            github(),
            SpecLocation::Https {
                url: "https://example.com/openapi.json".to_string(),
            },
        ] {
            let dir = tempdir().expect("tempdir");
            let source = sample(location);
            FsOpenApiSourceRepo.save(dir.path(), &source).expect("save");
            assert_eq!(
                FsOpenApiSourceRepo.load(dir.path()).expect("load"),
                Some(source)
            );
        }
    }

    #[test]
    fn file_has_snake_case_keys_and_no_secret_fields() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &sample(github()))
            .expect("save");
        let text = std::fs::read_to_string(dir.path().join(".rocket/openapi-source.yml"))
            .expect("file should exist");
        assert!(text.contains("spec_sha256:"), "{text}");
        assert!(text.contains("git_ref:"), "{text}");
        assert!(text.contains("request_path:"), "{text}");
        assert!(!text.to_lowercase().contains("token"), "{text}");
        assert!(!text.contains("specSha256"), "{text}");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("mkdir");
        std::fs::write(
            dir.path().join(".rocket/openapi-source.yml"),
            "version: [oops",
        )
        .expect("write");
        let err = FsOpenApiSourceRepo
            .load(dir.path())
            .expect_err("a corrupt file must not load");
        assert!(err.to_string().contains("openapi-source.yml"), "{err}");
    }

    #[test]
    fn newer_version_is_an_error() {
        let dir = tempdir().expect("tempdir");
        let mut source = sample(github());
        source.version = SOURCE_FILE_VERSION + 98;
        FsOpenApiSourceRepo.save(dir.path(), &source).expect("save");
        let err = FsOpenApiSourceRepo
            .load(dir.path())
            .expect_err("a newer file must not load");
        assert!(err.to_string().contains("newer"), "{err}");
    }

    fn written_version(root: &Path) -> u64 {
        let text = std::fs::read_to_string(root.join(".rocket/openapi-source.yml")).expect("read");
        let value: serde_yaml::Value = serde_yaml::from_str(&text).expect("yaml");
        value
            .get("version")
            .and_then(serde_yaml::Value::as_u64)
            .expect("version")
    }

    #[test]
    fn a_file_source_is_written_as_version_2() {
        let dir = tempdir().expect("tempdir");
        let location = SpecLocation::File {
            path: "specs/api.yaml".to_string(),
        };
        FsOpenApiSourceRepo
            .save(dir.path(), &sample(location.clone()))
            .expect("save");
        assert_eq!(written_version(dir.path()), 2);
        let loaded = FsOpenApiSourceRepo
            .load(dir.path())
            .expect("load")
            .expect("some");
        assert_eq!(loaded.location, location);
    }

    #[test]
    fn a_github_source_stays_version_1() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &sample(github()))
            .expect("save");
        assert_eq!(written_version(dir.path()), 1);
    }

    #[test]
    fn a_version_3_source_is_refused() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("mkdir");
        std::fs::write(
            dir.path().join(".rocket/openapi-source.yml"),
            "version: 3\nlocation:\n  kind: file\n  path: a.yaml\n",
        )
        .expect("write");
        let err = FsOpenApiSourceRepo.load(dir.path()).expect_err("too new");
        assert!(err.to_string().contains("newer"), "{err}");
    }

    #[test]
    fn delete_removes_the_file_and_ignores_a_missing_one() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiSourceRepo
            .delete(dir.path())
            .expect("missing is fine");
        FsOpenApiSourceRepo
            .save(dir.path(), &sample(github()))
            .expect("save");
        FsOpenApiSourceRepo.delete(dir.path()).expect("delete");
        assert!(!dir.path().join(".rocket/openapi-source.yml").exists());
        assert_eq!(FsOpenApiSourceRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn collection_with_source_file_loads_in_strict_mode() {
        let base = tempdir().expect("tempdir");
        let repo = crate::FsCollectionRepo::new_standalone(base.path().to_path_buf());
        repo.create("Orders").expect("create");
        let request = rocket_collection::Request::new(
            "List orders",
            rocket_shared::types::HttpMethod::Get,
            "{{baseUrl}}/orders",
        );
        repo.save_request("Orders", "list-orders", &request)
            .expect("save request");
        let root = repo.collection_root_path("Orders").expect("root");
        FsOpenApiSourceRepo
            .save(&root, &sample(github()))
            .expect("save source");

        let collection = repo
            .get_strict("Orders")
            .expect("strict load must not fail");
        assert_eq!(collection.request_count(), 1);
    }

    #[test]
    fn sync_fields_round_trip_with_snake_case_keys() {
        let dir = tempdir().expect("tempdir");
        let source = sample(github());
        FsOpenApiSourceRepo.save(dir.path(), &source).expect("save");
        let text = std::fs::read_to_string(dir.path().join(".rocket/openapi-source.yml"))
            .expect("file should exist");
        assert!(text.contains("source_id:"), "{text}");
        // The legacy timing fields are never written again.
        assert!(!text.contains("sync_interval:"), "{text}");
        assert!(!text.contains("last_checked_at:"), "{text}");
        assert!(!text.contains("last_status:"), "{text}");
        assert!(!text.to_lowercase().contains("token"), "{text}");
        assert_eq!(
            FsOpenApiSourceRepo.load(dir.path()).expect("load"),
            Some(source)
        );
    }

    #[test]
    fn pending_copy_round_trips_and_clears() {
        let dir = tempdir().expect("tempdir");
        let repo = FsOpenApiSourceRepo;
        assert_eq!(repo.load_pending(dir.path()).expect("load"), None);
        repo.save_pending(dir.path(), b"openapi: 3.0.0")
            .expect("save");
        assert_eq!(
            repo.load_pending(dir.path()).expect("load"),
            Some(b"openapi: 3.0.0".to_vec())
        );
        repo.clear_pending(dir.path()).expect("clear");
        assert_eq!(repo.load_pending(dir.path()).expect("load"), None);
        repo.clear_pending(dir.path()).expect("missing is fine");
    }

    #[test]
    fn pending_copy_does_not_break_the_collection_loader() {
        let base = tempdir().expect("tempdir");
        let repo = crate::FsCollectionRepo::new_standalone(base.path().to_path_buf());
        repo.create("Orders").expect("create");
        let request = rocket_collection::Request::new(
            "List orders",
            rocket_shared::types::HttpMethod::Get,
            "{{baseUrl}}/orders",
        );
        repo.save_request("Orders", "list-orders", &request)
            .expect("save request");
        let root = repo.collection_root_path("Orders").expect("root");
        FsOpenApiSourceRepo
            .save(&root, &sample(github()))
            .expect("save source");
        FsOpenApiSourceRepo
            .save_pending(&root, b"openapi: 3.0.0")
            .expect("save pending");

        let collection = repo
            .get_strict("Orders")
            .expect("strict load must not fail");
        assert_eq!(collection.request_count(), 1);
    }
}
