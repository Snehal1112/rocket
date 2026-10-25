//! Builds the record a collection keeps about its OpenAPI import.

use crate::importer::OpenApiImportOutcome;
use crate::openapi::GroupBy;
use rocket_collection::openapi_source::{
    OpenApiSource, SourceOperation, SpecLocation, SOURCE_FILE_VERSION,
};
use sha2::{Digest, Sha256};

/// The source record for an import: where the spec came from, the layout that was chosen, a
/// hash of the spec text and its server validators, and one entry per imported request.
pub fn source_record(
    location: &SpecLocation,
    group_by: GroupBy,
    spec_body: &[u8],
    etag: Option<String>,
    last_modified: Option<String>,
    outcome: &OpenApiImportOutcome,
) -> OpenApiSource {
    OpenApiSource {
        version: SOURCE_FILE_VERSION,
        location: location.clone(),
        group_by: group_by.as_str().to_string(),
        spec_sha256: format!("{:x}", Sha256::digest(spec_body)),
        etag,
        last_modified,
        operations: outcome
            .operations
            .iter()
            .map(|op| SourceOperation {
                key: op.key.clone(),
                uid: op.uid.clone(),
                request_path: op.request_path.clone(),
                baseline_hash: op.baseline_hash.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::ImportService;
    use tempfile::TempDir;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/openapi/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).expect("fixture should be readable")
    }

    #[test]
    fn record_mirrors_the_import_outcome() {
        let ws = TempDir::new().expect("tempdir");
        let outcome = ImportService::new_with_workspace_path(ws.path())
            .import_openapi_text(
                &fixture("orders-3.0.json"),
                "orders.json",
                GroupBy::Tags,
                "d",
            )
            .expect("import");
        let location = SpecLocation::Https {
            url: "https://example.com/orders.json".to_string(),
        };
        let record = source_record(
            &location,
            GroupBy::Tags,
            b"abc",
            Some("\"e\"".to_string()),
            None,
            &outcome,
        );
        assert_eq!(record.version, SOURCE_FILE_VERSION);
        assert_eq!(record.location, location);
        assert_eq!(record.group_by, "tags");
        assert_eq!(
            record.spec_sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(record.etag.as_deref(), Some("\"e\""));
        assert_eq!(record.last_modified, None);
        assert_eq!(record.operations.len(), 3);
        for (op, imported) in record.operations.iter().zip(&outcome.operations) {
            assert_eq!(op.key, imported.key);
            assert_eq!(op.uid, imported.uid);
            assert_eq!(op.request_path, imported.request_path);
            assert_eq!(op.baseline_hash, imported.baseline_hash);
        }
    }

    #[test]
    fn paths_layout_is_recorded_as_paths() {
        let ws = TempDir::new().expect("tempdir");
        let outcome = ImportService::new_with_workspace_path(ws.path())
            .import_openapi_text(
                &fixture("orders-3.0.json"),
                "orders.json",
                GroupBy::Paths,
                "d",
            )
            .expect("import");
        let location = SpecLocation::Https {
            url: "https://example.com/orders.json".to_string(),
        };
        let record = source_record(&location, GroupBy::Paths, b"x", None, None, &outcome);
        assert_eq!(record.group_by, "paths");
    }
}
