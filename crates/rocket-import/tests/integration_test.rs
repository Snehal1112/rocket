use rocket_import::ImportService;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/my-api")
}

#[test]
fn imports_fixture_collection_successfully() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    let report = service
        .import_collection(&fixture_path(), "default")
        .expect("import should succeed");

    assert!(report.imported >= 3, "expected at least 3 requests imported, got {}", report.imported);
    assert!(report.created_collections.contains(&"my-api".to_string()));

    // Collection structure.
    assert!(workspace_dir.path().join("collections/my-api/opencollection.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/get-users.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/create-user.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/auth/login.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/environments/local.yml").exists());
}

#[test]
fn import_report_counts_correctly() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    let report = service.import_collection(&fixture_path(), "default").unwrap();

    assert_eq!(report.total_files, 3); // get-users.bru, create-user.yml, auth/login.bru
    assert_eq!(report.imported, 3);
    assert!(report.skipped.is_empty(), "unexpected skips: {:?}", report.skipped);
}

#[test]
fn auto_renames_on_collection_name_conflict() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    // First import.
    service.import_collection(&fixture_path(), "default").unwrap();
    // Second import — should auto-rename.
    let report2 = service.import_collection(&fixture_path(), "default").unwrap();

    assert!(
        report2.created_collections.iter().any(|n| n == "my-api-1"),
        "expected 'my-api-1' in created_collections, got: {:?}",
        report2.created_collections
    );
    assert!(workspace_dir.path().join("collections/my-api-1").exists());
}
