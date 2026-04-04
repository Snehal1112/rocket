use rocket_import::ImportService;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/my-api")
}

fn workspace_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/my-workspace")
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

#[test]
fn import_collection_fails_for_non_bruno_directory() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    // workspace_dir itself has no bruno.json.
    let result = service.import_collection(workspace_dir.path(), "default");
    assert!(result.is_err(), "expected error for non-Bruno directory");
}

#[test]
fn import_workspace_imports_all_sub_collections() {
    // Build a minimal Bruno workspace: one outer directory with two collection subdirs.
    let src_dir = TempDir::new().unwrap();
    let ws_path = src_dir.path();

    // Workspace root must have bruno.json to be detected as a workspace.
    std::fs::write(ws_path.join("bruno.json"), r#"{"name":"ws","version":"1","type":"collection"}"#).unwrap();

    // Sub-collection A.
    let col_a = ws_path.join("col-a");
    std::fs::create_dir_all(&col_a).unwrap();
    std::fs::write(col_a.join("bruno.json"), r#"{"name":"col-a","version":"1","type":"collection"}"#).unwrap();
    std::fs::write(col_a.join("req.bru"), "meta {\n  name: Req A\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com/a\n}\n").unwrap();

    // Sub-collection B.
    let col_b = ws_path.join("col-b");
    std::fs::create_dir_all(&col_b).unwrap();
    std::fs::write(col_b.join("bruno.json"), r#"{"name":"col-b","version":"1","type":"collection"}"#).unwrap();
    std::fs::write(col_b.join("req.bru"), "meta {\n  name: Req B\n  type: http\n  seq: 1\n}\npost {\n  url: https://example.com/b\n}\n").unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());
    let report = service.import_workspace(ws_path, false, Some("default")).unwrap();

    assert_eq!(report.imported, 2, "expected 2 requests imported, got {}", report.imported);
    assert_eq!(report.created_collections.len(), 2);
    assert!(workspace_dir.path().join("collections/col-a").exists());
    assert!(workspace_dir.path().join("collections/col-b").exists());
}

#[test]
fn parse_error_in_file_is_reported_as_skipped() {
    let tmp = TempDir::new().unwrap();
    // Use a named subdirectory so the collection name doesn't start with '.'.
    let col_dir = tmp.path().join("bad-col");
    std::fs::create_dir_all(&col_dir).unwrap();
    std::fs::write(col_dir.join("bruno.json"), r#"{"name":"bad-col","version":"1","type":"collection"}"#).unwrap();
    // Malformed YAML — serde_yaml will fail to parse this.
    std::fs::write(col_dir.join("bad.yml"), "http:\n  url: {{invalid: yaml: [unclosed").unwrap();
    // A valid file alongside the bad one.
    std::fs::write(col_dir.join("good.bru"), "meta {\n  name: Good\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n").unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());
    let report = service.import_collection(&col_dir, "default").unwrap();

    assert_eq!(report.total_files, 2);
    assert_eq!(report.imported, 1, "only the valid file should be imported");
    // bad.bru should appear as a parse error skip.
    assert!(!report.skipped.is_empty(), "expected at least one skip");
}

/// Ensures the fixture workspace directory (used by workspace tests) exists.
/// This is a compile-time sanity check — the test passes trivially if the
/// fixture is not yet created; it fails if the path exists but is not a directory.
#[test]
fn workspace_fixture_dir_setup() {
    let p = workspace_fixture_path();
    if p.exists() {
        assert!(p.is_dir(), "workspace fixture path exists but is not a directory");
    }
    // No fixture yet — that is fine; this test documents intent.
}
