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

// ──────────────────────────────────────────────────────────
// Bruno Import v2 — modern format and ZIP tests
// ──────────────────────────────────────────────────────────

fn make_modern_collection_dir(col_dir: &std::path::Path, name: &str, req_count: usize) {
    std::fs::create_dir_all(col_dir).unwrap();
    std::fs::write(
        col_dir.join("opencollection.yml"),
        format!("opencollection: \"1.0.0\"\ninfo:\n  name: {name}\n"),
    )
    .unwrap();
    for i in 0..req_count {
        std::fs::write(
            col_dir.join(format!("req-{i}.yml")),
            format!("name: Req {i}\nmethod: GET\nurl: https://api.example.com/{i}\n"),
        )
        .unwrap();
    }
    let env_dir = col_dir.join("environments");
    std::fs::create_dir_all(&env_dir).unwrap();
    std::fs::write(env_dir.join("local.yml"), "name: local\nvars: []\n").unwrap();
}

#[test]
fn import_auto_modern_collection_directory() {
    let src = TempDir::new().unwrap();
    let col_src = src.path().join("my-col");
    make_modern_collection_dir(&col_src, "my-col", 3);

    let ws = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws.path());
    let report = service.import_auto(&col_src, "default", false).unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 3);
    assert!(report.created_collections.contains(&"my-col".to_string()));
    assert!(ws.path().join("collections/my-col/opencollection.yml").exists());
    assert!(ws.path().join("collections/my-col/req-0.yml").exists());
    assert!(ws.path().join("collections/my-col/environments/local.yml").exists());
}

#[test]
fn import_auto_modern_workspace_directory() {
    let src = TempDir::new().unwrap();
    let ws_src = src.path().join("my-workspace");
    std::fs::create_dir_all(&ws_src).unwrap();
    std::fs::write(ws_src.join("workspace.yml"), "name: my-workspace\n").unwrap();

    let col_a = ws_src.join("col-a");
    make_modern_collection_dir(&col_a, "col-a", 2);
    let col_b = ws_src.join("col-b");
    make_modern_collection_dir(&col_b, "col-b", 1);

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_auto(&ws_src, "default", false).unwrap();

    assert_eq!(report.detected_type, "workspace");
    assert_eq!(report.imported, 3, "2 from col-a + 1 from col-b");
    assert_eq!(report.created_collections.len(), 2);
    assert!(ws_dir.path().join("collections/col-a").exists());
    assert!(ws_dir.path().join("collections/col-b").exists());
}

#[test]
fn import_auto_legacy_collection_still_works() {
    let workspace_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(workspace_dir.path());

    let report = service
        .import_auto(&fixture_path(), "default", false)
        .expect("legacy collection import via import_auto should succeed");

    assert_eq!(report.detected_type, "collection");
    assert!(report.imported >= 3);
    assert!(report.created_collections.contains(&"my-api".to_string()));
}

#[test]
fn import_auto_returns_error_for_non_bruno_dir() {
    let dir = TempDir::new().unwrap();
    let ws = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws.path());
    let result = service.import_auto(dir.path(), "default", false);
    assert!(result.is_err(), "expected error for non-Bruno directory");
}

#[test]
fn import_auto_from_zip_modern_collection() {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let src = TempDir::new().unwrap();
    let zip_path = src.path().join("my-col.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut w = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    w.add_directory("my-col/", opts).unwrap();
    w.start_file("my-col/opencollection.yml", opts).unwrap();
    w.write_all(b"opencollection: \"1.0.0\"\ninfo:\n  name: my-col\n").unwrap();
    w.start_file("my-col/get-users.yml", opts).unwrap();
    w.write_all(b"name: Get Users\nmethod: GET\nurl: https://api.example.com/users\n").unwrap();
    w.finish().unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_auto_from_zip(&zip_path, "default", false).unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 1);
    assert!(ws_dir.path().join("collections/my-col/get-users.yml").exists());
}

#[test]
fn import_workspace_mixed_modern_and_legacy_collections() {
    let src = TempDir::new().unwrap();
    let ws_src = src.path();

    // Workspace root with workspace.yml (modern marker).
    std::fs::write(ws_src.join("workspace.yml"), "name: mixed-ws\n").unwrap();

    // Modern sub-collection.
    let modern_col = ws_src.join("modern-col");
    make_modern_collection_dir(&modern_col, "modern-col", 2);

    // Legacy sub-collection.
    let legacy_col = ws_src.join("legacy-col");
    std::fs::create_dir_all(&legacy_col).unwrap();
    std::fs::write(legacy_col.join("bruno.json"), r#"{"name":"legacy-col","version":"1","type":"collection"}"#).unwrap();
    std::fs::write(
        legacy_col.join("req.bru"),
        "meta {\n  name: Req\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n",
    )
    .unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = ImportService::new_with_workspace_path(ws_dir.path());
    let report = service.import_workspace(ws_src, false, Some("default")).unwrap();

    assert_eq!(report.detected_type, "workspace");
    assert_eq!(report.imported, 3, "2 modern + 1 legacy");
    assert_eq!(report.created_collections.len(), 2);
    assert!(ws_dir.path().join("collections/modern-col").exists());
    assert!(ws_dir.path().join("collections/legacy-col").exists());
}
