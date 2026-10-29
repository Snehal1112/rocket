use rocket_collection::CollectionRepository;
use rocket_environment::EnvironmentRepository;
use rocket_import::{baseline_hash, EnvironmentRepositoryFactory, GroupBy, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use std::collections::BTreeSet;
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

fn repo(workspace_path: &Path) -> FsCollectionRepo {
    FsCollectionRepo::new_standalone(workspace_path.join("collections"))
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/openapi")
        .join(name);
    std::fs::read_to_string(path).expect("fixture should be readable")
}

fn dir_names(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .expect("directory should exist")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

fn small_spec(tags_per_path: &[(&str, Option<&str>)]) -> String {
    let mut text = String::from("openapi: 3.0.3\ninfo: { title: Tags, version: '1' }\npaths:\n");
    for (i, (path, tag)) in tags_per_path.iter().enumerate() {
        text.push_str(&format!("  {path}:\n    get:\n      operationId: op{i}\n"));
        if let Some(tag) = tag {
            text.push_str(&format!("      tags: ['{tag}']\n"));
        }
        text.push_str("      responses: { '200': { description: ok } }\n");
    }
    text
}

#[test]
fn tags_layout_makes_one_folder_per_first_tag() {
    let ws = TempDir::new().expect("tempdir");
    let out = make_service(ws.path())
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("should import");
    assert_eq!(out.collection, "Orders");
    assert_eq!(out.report.imported, 3);
    let folder = ws.path().join("collections/Orders/Order-Management");
    assert!(folder.is_dir());
    let requests = dir_names(&folder)
        .into_iter()
        .filter(|n| n.ends_with(".yml") && n != "folder.yml")
        .count();
    assert_eq!(requests, 3);
}

#[test]
fn paths_layout_nests_literal_segments() {
    let ws = TempDir::new().expect("tempdir");
    let out = make_service(ws.path())
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Paths,
            "default",
        )
        .expect("should import");
    assert_eq!(out.operations.len(), 3);
    assert!(out
        .operations
        .iter()
        .all(|o| o.request_path.starts_with("orders/")));
    assert!(ws.path().join("collections/Orders/orders").is_dir());
}

#[test]
fn untagged_operations_go_to_untagged_folder() {
    let ws = TempDir::new().expect("tempdir");
    let out = make_service(ws.path())
        .import_openapi_text(
            &small_spec(&[("/a", None)]),
            "t.yaml",
            GroupBy::Tags,
            "default",
        )
        .expect("should import");
    assert!(out.operations[0].request_path.starts_with("untagged/"));
}

#[test]
fn collection_variables_hold_base_url_and_empty_secrets() {
    let ws = TempDir::new().expect("tempdir");
    make_service(ws.path())
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("should import");
    let settings = repo(ws.path()).get_settings("Orders").expect("settings");
    let base = settings
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl variable");
    assert_eq!(base.value, "https://api.example.com/v1");
    let token = settings
        .variables
        .iter()
        .find(|v| v.key == "bearerAuth_token")
        .expect("bearer variable");
    assert!(token.secret);
    assert!(token.value.is_empty());
}

#[test]
fn outcome_lists_every_written_request() {
    let ws = TempDir::new().expect("tempdir");
    let out = make_service(ws.path())
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("should import");
    assert_eq!(out.operations.len(), out.report.imported);
    let repo = repo(ws.path());
    for op in &out.operations {
        let saved = repo
            .get_request(&out.collection, &op.request_path)
            .expect("request should be on disk");
        assert_eq!(saved.uid, op.uid, "{}", op.key);
        assert_eq!(baseline_hash(&saved), op.baseline_hash, "{}", op.key);
    }
    let keys: BTreeSet<&str> = out.operations.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(
        keys,
        BTreeSet::from(["createOrder", "getOrder", "listOrders"])
    );
}

#[test]
fn cap_and_empty_spec_are_reported_not_fatal() {
    let ws = TempDir::new().expect("tempdir");
    let mut big = String::from("openapi: 3.0.3\ninfo: { title: Big, version: '1' }\npaths:\n");
    for i in 0..5_001 {
        big.push_str(&format!(
            "  /p{i}:\n    get: {{ operationId: op{i}, responses: {{ '200': {{ description: ok }} }} }}\n"
        ));
    }
    let out = make_service(ws.path())
        .import_openapi_text(&big, "big.yaml", GroupBy::Tags, "default")
        .expect("should import");
    assert_eq!(out.report.imported, 5_000);
    assert_eq!(out.report.skipped.len(), 1);
    assert!(format!("{:?}", out.report.skipped[0].reason).contains("1 operations not imported"));

    let empty = make_service(ws.path())
        .import_openapi_text(
            "openapi: 3.0.3\ninfo: { title: Empty, version: '1' }\npaths: {}\n",
            "empty.yaml",
            GroupBy::Tags,
            "default",
        )
        .expect("should import");
    assert_eq!(empty.report.imported, 0);
    assert_eq!(empty.report.skipped.len(), 1);
    assert!(ws.path().join("collections/Empty").is_dir());
}

#[test]
fn name_collision_gets_suffix() {
    let ws = TempDir::new().expect("tempdir");
    let service = make_service(ws.path());
    let first = service
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("first import");
    let second = service
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("second import");
    assert_eq!(first.collection, "Orders");
    assert_eq!(second.collection, "Orders-1");
}

#[test]
fn collection_name_avoids_device_names_and_case_only_clashes() {
    let ws = TempDir::new().expect("tempdir");
    let service = make_service(ws.path());
    let device = service
        .import_openapi_text(
            "openapi: 3.0.3\ninfo: { title: CON, version: '1' }\npaths: {}\n",
            "t.yaml",
            GroupBy::Tags,
            "default",
        )
        .expect("device-named title should import");
    assert_eq!(device.collection, "CON-1");

    service
        .import_openapi_text(
            &fixture("orders-3.0.json"),
            "orders.json",
            GroupBy::Tags,
            "default",
        )
        .expect("first import");
    let lower = service
        .import_openapi_text(
            "openapi: 3.0.3\ninfo: { title: orders, version: '1' }\npaths: {}\n",
            "t.yaml",
            GroupBy::Tags,
            "default",
        )
        .expect("case-only clash should import");
    assert_eq!(lower.collection, "orders-1");
}

#[test]
fn reserved_and_hostile_names_are_safe() {
    let ws = TempDir::new().expect("tempdir");
    let spec = small_spec(&[
        ("/a", Some("environments")),
        ("/b", Some("a/b")),
        ("/c", Some("..")),
        ("/d", Some("CON")),
    ]);
    let out = make_service(ws.path())
        .import_openapi_text(&spec, "t.yaml", GroupBy::Tags, "default")
        .expect("should import");
    let names = dir_names(&ws.path().join("collections").join(&out.collection));
    let folders: BTreeSet<String> = names
        .into_iter()
        .filter(|n| !n.ends_with(".yml") && n != "environments")
        .collect();
    // The `environments` tag is reserved, so that request sits at the collection root.
    assert_eq!(
        folders,
        BTreeSet::from(["--".to_string(), "CON-2".to_string(), "a-b".to_string()])
    );
    assert_eq!(out.report.imported, 4);
}
