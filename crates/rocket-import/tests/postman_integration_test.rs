use rocket_environment::EnvironmentRepository;
use rocket_import::{EnvironmentRepositoryFactory, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
        ))
    }
}

fn make_service(workspace_path: &Path) -> ImportService {
    let path = workspace_path.to_path_buf();
    ImportService::new(
        path.clone(),
        Box::new(FsCollectionRepo::new_standalone(path.join("collections"))),
        Box::new(FsEnvFactory(path)),
    )
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/postman")
        .join(name)
}

#[test]
fn imports_minimal_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("minimal-collection.json"), "default")
        .expect("should import");
    assert_eq!(report.imported, 2);
    assert!(report
        .created_collections
        .iter()
        .any(|n| n.starts_with("Minimal")));
}

#[test]
fn imports_full_collection_with_folders() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("full-collection.json"), "default")
        .expect("should import");
    assert!(report.imported >= 4, "got {}", report.imported);
    assert!(report
        .skipped
        .iter()
        .any(|s| s.path.contains("Upload") || s.path.contains("file")));
}

#[test]
fn imports_v2_0_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("v2.0-collection.json"), "default")
        .expect("should import v2.0");
    assert_eq!(report.imported, 1);
}

#[test]
fn auto_renames_on_conflict() {
    let ws = TempDir::new().unwrap();
    let svc = make_service(ws.path());
    svc.import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();
    let r2 = svc
        .import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();
    assert!(
        r2.created_collections.iter().any(|n| n.contains("-1")),
        "expected -1 suffix, got {:?}",
        r2.created_collections
    );
}

fn postman_request(name: &str, url: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "request": { "method": "GET", "url": url }
    })
}

/// Imports a collection named `Dup` with the given items and returns its folder.
fn import_items(ws: &Path, items: serde_json::Value) -> PathBuf {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("dup.json");
    let doc = serde_json::json!({
        "info": {
            "name": "Dup",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": items
    });
    std::fs::write(&path, doc.to_string()).expect("write json");
    make_service(ws)
        .import_postman_collection(&path, "default")
        .expect("should import");
    ws.join("collections").join("Dup")
}

fn request_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".yml") && n != "opencollection.yml" && n != "folder.yml")
        .collect();
    names.sort();
    names
}

#[test]
fn postman_import_keeps_both_items_with_one_sanitized_name() {
    let ws = TempDir::new().expect("tempdir");
    let col = import_items(
        ws.path(),
        serde_json::json!([
            postman_request("List users", "https://example.com/a"),
            postman_request("List-users", "https://example.com/b"),
        ]),
    );
    let files = request_files(&col);
    assert_eq!(files.len(), 2, "got {files:?}");
    assert!(files.iter().any(|f| f.ends_with(" 2.yml")), "got {files:?}");
    let all: String = files
        .iter()
        .map(|f| std::fs::read_to_string(col.join(f)).expect("read"))
        .collect();
    assert!(all.contains("example.com/a") && all.contains("example.com/b"));
}

#[test]
fn postman_import_allows_the_same_name_in_two_folders() {
    let ws = TempDir::new().expect("tempdir");
    let col = import_items(
        ws.path(),
        serde_json::json!([
            { "name": "One", "item": [postman_request("Get", "https://example.com/1")] },
            { "name": "Two", "item": [postman_request("Get", "https://example.com/2")] },
        ]),
    );
    for folder in ["One", "Two"] {
        assert_eq!(request_files(&col.join(folder)), vec!["Get.yml"]);
    }
}

#[test]
fn rejects_non_postman_json() {
    let ws = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("other.json");
    std::fs::write(&path, r#"{"foo": "bar"}"#).unwrap();
    let result = make_service(ws.path()).import_postman_collection(&path, "default");
    assert!(result.is_err());
}

#[test]
fn imports_embedded_environments_from_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("full-collection.json"), "default")
        .expect("should import");

    let col_name = report.created_collections[0].clone();
    let env_dir = ws
        .path()
        .join("collections")
        .join(&col_name)
        .join("environments");

    assert!(
        env_dir.join("Local.yml").exists(),
        "Local.yml not found in {:?}",
        env_dir
    );
    assert!(
        env_dir.join("Staging.yml").exists(),
        "Staging.yml not found in {:?}",
        env_dir
    );
}

#[test]
fn imports_environment_into_existing_collection() {
    let ws = TempDir::new().unwrap();
    let svc = make_service(ws.path());
    svc.import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();

    let col_name = std::fs::read_dir(ws.path().join("collections"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();

    let report = svc
        .import_postman_environment(&fixture("environment.json"), &col_name, "default")
        .expect("should import env");

    assert_eq!(report.imported, 3);
    assert!(ws
        .path()
        .join("collections")
        .join(&col_name)
        .join("environments/Local.yml")
        .exists());
}

#[test]
fn imports_environment_with_separator_in_name() {
    let ws = TempDir::new().expect("temp dir");
    let json = ws.path().join("env.postman_environment.json");
    std::fs::write(
        &json,
        r#"{"name":"Staging / EU","values":[{"key":"host","value":"eu.example.com","enabled":true}]}"#,
    )
    .expect("write env json");
    let svc = make_service(ws.path());
    svc.import_postman_environment(&json, "default", "ws")
        .expect("import should sanitize the name");
    // A second import of the same name must not overwrite the first.
    svc.import_postman_environment(&json, "default", "ws")
        .expect("second import");
    let repo = FsEnvironmentRepo::new(ws.path().join("collections/default/environments"));
    assert_eq!(
        repo.get("Staging - EU").expect("first").name,
        "Staging - EU"
    );
    assert_eq!(
        repo.get("Staging - EU (2)").expect("second").name,
        "Staging - EU (2)"
    );
}
