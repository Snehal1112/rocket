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
        Box::new(FsCollectionRepo::new(path.join("collections"))),
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
