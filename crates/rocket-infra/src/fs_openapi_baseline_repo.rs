//! Stores a collection's OpenAPI baseline at `<collection>/.rocket/openapi-baseline.json`.
//! A dot folder is hidden from the collection loader, so the file is never shown as a request.

use std::path::{Path, PathBuf};

use rocket_collection::openapi_baseline::{
    OpenApiBaseline, OpenApiBaselineRepository, BASELINE_FILE_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// The filesystem implementation. It holds no state; every call is given the collection root.
pub struct FsOpenApiBaselineRepo;

const BASELINE_FILE: &str = "openapi-baseline.json";

fn baseline_path(collection_root: &Path) -> PathBuf {
    collection_root.join(".rocket").join(BASELINE_FILE)
}

impl OpenApiBaselineRepository for FsOpenApiBaselineRepo {
    fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiBaseline>> {
        let text = match std::fs::read_to_string(baseline_path(collection_root)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(DomainError::Internal(format!(
                    "{BASELINE_FILE} could not be read: {e}"
                )))
            }
        };
        // Read the version first, so a newer file is reported as that even when its other
        // fields no longer fit this version's shape.
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("{BASELINE_FILE} cannot be parsed: {e}")))?;
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        if version.is_some_and(|v| v > u64::from(BASELINE_FILE_VERSION)) {
            return Err(DomainError::Internal(format!(
                "{BASELINE_FILE} was written by a newer version of Rocket (it reads up to version {BASELINE_FILE_VERSION})"
            )));
        }
        let baseline: OpenApiBaseline = serde_json::from_value(value)
            .map_err(|e| DomainError::Internal(format!("{BASELINE_FILE} cannot be parsed: {e}")))?;
        Ok(Some(baseline))
    }

    fn save(&self, collection_root: &Path, baseline: &OpenApiBaseline) -> DomainResult<()> {
        let json = serde_json::to_vec_pretty(baseline).map_err(|e| {
            DomainError::Internal(format!("{BASELINE_FILE} could not be written: {e}"))
        })?;
        // Ignore the file first, so a failure never leaves an unignored copy on disk.
        crate::rocket_dir::ensure_gitignore(collection_root)?;
        atomic_write(&baseline_path(collection_root), &json)?;
        Ok(())
    }

    fn delete(&self, collection_root: &Path) -> DomainResult<()> {
        match std::fs::remove_file(baseline_path(collection_root)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DomainError::Internal(format!(
                "{BASELINE_FILE} could not be removed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::openapi_baseline::BaselineOperation;
    use rocket_collection::Request;
    use rocket_shared::types::HttpMethod;
    use tempfile::tempdir;

    fn sample() -> OpenApiBaseline {
        OpenApiBaseline {
            version: BASELINE_FILE_VERSION,
            converter_version: 1,
            base_url: "https://api.example.com".to_string(),
            operations: vec![BaselineOperation {
                key: "listOrders".to_string(),
                request: Request::new("List orders", HttpMethod::Get, "{{baseUrl}}/orders"),
            }],
        }
    }

    #[test]
    fn baseline_round_trips() {
        let dir = tempdir().expect("tempdir");
        let baseline = sample();
        FsOpenApiBaselineRepo
            .save(dir.path(), &baseline)
            .expect("save");
        assert_eq!(
            FsOpenApiBaselineRepo.load(dir.path()).expect("load"),
            Some(baseline)
        );
        let text = std::fs::read_to_string(dir.path().join(".rocket/openapi-baseline.json"))
            .expect("file");
        assert!(text.contains("converter_version"), "{text}");
        assert!(!text.to_lowercase().contains("token"), "{text}");
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(FsOpenApiBaselineRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn newer_version_is_an_error() {
        let dir = tempdir().expect("tempdir");
        let mut baseline = sample();
        baseline.version = BASELINE_FILE_VERSION + 98;
        FsOpenApiBaselineRepo
            .save(dir.path(), &baseline)
            .expect("save");
        let err = FsOpenApiBaselineRepo
            .load(dir.path())
            .expect_err("a newer file must not load");
        assert!(err.to_string().contains("newer"), "{err}");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("mkdir");
        std::fs::write(dir.path().join(".rocket/openapi-baseline.json"), "{oops").expect("write");
        let err = FsOpenApiBaselineRepo
            .load(dir.path())
            .expect_err("a corrupt file must not load");
        assert!(err.to_string().contains("openapi-baseline.json"), "{err}");
    }

    #[test]
    fn delete_ignores_a_missing_file() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiBaselineRepo
            .delete(dir.path())
            .expect("missing is fine");
        FsOpenApiBaselineRepo
            .save(dir.path(), &sample())
            .expect("save");
        FsOpenApiBaselineRepo.delete(dir.path()).expect("delete");
        assert_eq!(FsOpenApiBaselineRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn a_failing_ignore_step_leaves_no_baseline_file_behind() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket/.gitignore")).expect("mkdir");
        assert!(FsOpenApiBaselineRepo.save(dir.path(), &sample()).is_err());
        assert!(!dir.path().join(".rocket/openapi-baseline.json").exists());
    }

    #[test]
    fn gitignore_is_created_with_both_names() {
        let dir = tempdir().expect("tempdir");
        crate::ensure_gitignore(dir.path()).expect("ensure");
        let text = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("file");
        assert_eq!(text, "openapi-baseline.json\nopenapi-pending.spec\n");
    }

    #[test]
    fn gitignore_keeps_user_lines_and_adds_only_missing_ones() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("mkdir");
        std::fs::write(
            dir.path().join(".rocket/.gitignore"),
            "my-notes.txt\nopenapi-pending.spec",
        )
        .expect("write");
        crate::ensure_gitignore(dir.path()).expect("ensure");
        crate::ensure_gitignore(dir.path()).expect("ensure again");
        let text = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("file");
        assert_eq!(
            text,
            "my-notes.txt\nopenapi-pending.spec\nopenapi-baseline.json\n"
        );
    }

    #[test]
    fn saving_a_pending_copy_also_writes_the_gitignore() {
        use rocket_collection::openapi_source::OpenApiSourceRepository;
        let dir = tempdir().expect("tempdir");
        crate::FsOpenApiSourceRepo
            .save_pending(dir.path(), b"openapi: 3.0.0")
            .expect("save pending");
        let text = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("file");
        assert!(text.contains("openapi-pending.spec"), "{text}");
        assert!(text.contains("openapi-baseline.json"), "{text}");
    }

    #[test]
    fn a_collection_with_a_baseline_still_loads_in_strict_mode() {
        use rocket_collection::CollectionRepository;
        let base = tempdir().expect("tempdir");
        let repo = crate::FsCollectionRepo::new_standalone(base.path().to_path_buf());
        repo.create("Orders").expect("create");
        let request = Request::new("List orders", HttpMethod::Get, "{{baseUrl}}/orders");
        repo.save_request("Orders", "list-orders", &request)
            .expect("save request");
        let root = repo.collection_root_path("Orders").expect("root");
        FsOpenApiBaselineRepo.save(&root, &sample()).expect("save");
        let collection = repo
            .get_strict("Orders")
            .expect("strict load must not fail");
        assert_eq!(collection.request_count(), 1);
    }
}
