//! Stores a collection's OpenAPI source at `<collection>/.rocket/openapi-source.yml`.
//! A dot folder is hidden from the collection loader, so the file is never shown as a request.

use std::path::{Path, PathBuf};

use rocket_collection::openapi_source::{
    OpenApiSource, OpenApiSourceRepository, SOURCE_FILE_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// The filesystem implementation. It holds no state; every call is given the collection root.
pub struct FsOpenApiSourceRepo;

const SOURCE_DIR: &str = ".rocket";
const SOURCE_FILE: &str = "openapi-source.yml";

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
        if version.is_some_and(|v| v > u64::from(SOURCE_FILE_VERSION)) {
            return Err(DomainError::Internal(format!(
                "{SOURCE_FILE} was written by a newer version of Rocket (it reads up to version {SOURCE_FILE_VERSION})"
            )));
        }
        let source: OpenApiSource = serde_yaml::from_value(value)
            .map_err(|e| DomainError::Internal(format!("{SOURCE_FILE} cannot be parsed: {e}")))?;
        Ok(Some(source))
    }

    fn save(&self, collection_root: &Path, source: &OpenApiSource) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(source).map_err(|e| {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::openapi_source::{SourceOperation, SpecLocation, SOURCE_FILE_VERSION};
    use rocket_collection::CollectionRepository;
    use tempfile::tempdir;

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
}
