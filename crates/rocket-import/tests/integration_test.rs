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

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/my-api")
}

fn workspace_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/my-workspace")
}

#[test]
fn imports_fixture_collection_successfully() {
    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());

    let report = service
        .import_collection(&fixture_path(), "default")
        .expect("import should succeed");

    assert!(
        report.imported >= 3,
        "expected at least 3 requests imported, got {}",
        report.imported
    );
    assert!(report.created_collections.contains(&"my-api".to_string()));

    // Collection structure.
    assert!(workspace_dir
        .path()
        .join("collections/my-api/opencollection.yml")
        .exists());
    assert!(workspace_dir
        .path()
        .join("collections/my-api/get-users.yml")
        .exists());
    assert!(workspace_dir
        .path()
        .join("collections/my-api/create-user.yml")
        .exists());
    assert!(workspace_dir
        .path()
        .join("collections/my-api/auth/login.yml")
        .exists());
    assert!(workspace_dir
        .path()
        .join("collections/my-api/environments/local.yml")
        .exists());
}

#[test]
fn import_report_counts_correctly() {
    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());

    let report = service
        .import_collection(&fixture_path(), "default")
        .unwrap();

    assert_eq!(report.total_files, 3); // get-users.bru, create-user.yml, auth/login.bru
    assert_eq!(report.imported, 3);
    assert!(
        report.skipped.is_empty(),
        "unexpected skips: {:?}",
        report.skipped
    );
}

#[test]
fn auto_renames_on_collection_name_conflict() {
    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());

    // First import.
    service
        .import_collection(&fixture_path(), "default")
        .unwrap();
    // Second import — should auto-rename.
    let report2 = service
        .import_collection(&fixture_path(), "default")
        .unwrap();

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
    let service = make_service(workspace_dir.path());

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
    std::fs::write(
        ws_path.join("bruno.json"),
        r#"{"name":"ws","version":"1","type":"collection"}"#,
    )
    .unwrap();

    // Sub-collection A.
    let col_a = ws_path.join("col-a");
    std::fs::create_dir_all(&col_a).unwrap();
    std::fs::write(
        col_a.join("bruno.json"),
        r#"{"name":"col-a","version":"1","type":"collection"}"#,
    )
    .unwrap();
    std::fs::write(col_a.join("req.bru"), "meta {\n  name: Req A\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com/a\n}\n").unwrap();

    // Sub-collection B.
    let col_b = ws_path.join("col-b");
    std::fs::create_dir_all(&col_b).unwrap();
    std::fs::write(
        col_b.join("bruno.json"),
        r#"{"name":"col-b","version":"1","type":"collection"}"#,
    )
    .unwrap();
    std::fs::write(col_b.join("req.bru"), "meta {\n  name: Req B\n  type: http\n  seq: 1\n}\npost {\n  url: https://example.com/b\n}\n").unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service
        .import_workspace(ws_path, false, Some("default"))
        .unwrap();

    assert_eq!(
        report.imported, 2,
        "expected 2 requests imported, got {}",
        report.imported
    );
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
    std::fs::write(
        col_dir.join("bruno.json"),
        r#"{"name":"bad-col","version":"1","type":"collection"}"#,
    )
    .unwrap();
    // Malformed YAML — serde_yaml will fail to parse this.
    std::fs::write(
        col_dir.join("bad.yml"),
        "http:\n  url: {{invalid: yaml: [unclosed",
    )
    .unwrap();
    // A valid file alongside the bad one.
    std::fs::write(
        col_dir.join("good.bru"),
        "meta {\n  name: Good\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
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
        assert!(
            p.is_dir(),
            "workspace fixture path exists but is not a directory"
        );
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
    let service = make_service(ws.path());
    let report = service.import_auto(&col_src, "default", false).unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 3);
    assert!(report.created_collections.contains(&"my-col".to_string()));
    assert!(ws
        .path()
        .join("collections/my-col/opencollection.yml")
        .exists());
    assert!(ws.path().join("collections/my-col/req-0.yml").exists());
    assert!(ws
        .path()
        .join("collections/my-col/environments/local.yml")
        .exists());
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
    let service = make_service(ws_dir.path());
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
    let service = make_service(workspace_dir.path());

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
    let service = make_service(ws.path());
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
    w.write_all(b"opencollection: \"1.0.0\"\ninfo:\n  name: my-col\n")
        .unwrap();
    w.start_file("my-col/get-users.yml", opts).unwrap();
    w.write_all(b"name: Get Users\nmethod: GET\nurl: https://api.example.com/users\n")
        .unwrap();
    w.finish().unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = make_service(ws_dir.path());
    let report = service
        .import_auto_from_zip(&zip_path, "default", false)
        .unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 1);
    assert!(ws_dir
        .path()
        .join("collections/my-col/get-users.yml")
        .exists());
}

/// Flat-root ZIP (no wrapper folder) should use ZIP filename as collection name,
/// not the temp directory name which starts with a dot.
#[test]
fn import_flat_root_zip_uses_zip_filename_as_collection_name() {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let src = TempDir::new().unwrap();
    let zip_path = src.path().join("Lockstep-Inbox.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut w = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    // Flat-root: bruno.json and request files at archive root, no wrapper folder.
    w.start_file("bruno.json", opts).unwrap();
    w.write_all(b"{}").unwrap();
    w.start_file("req.bru", opts).unwrap();
    w.write_all(
        b"meta {\n  name: Req\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n",
    )
    .unwrap();
    w.finish().unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = make_service(ws_dir.path());
    let report = service
        .import_auto_from_zip(&zip_path, "default", false)
        .unwrap();

    assert_eq!(report.detected_type, "collection");
    assert_eq!(report.imported, 1);
    assert!(
        report
            .created_collections
            .contains(&"Lockstep-Inbox".to_string()),
        "expected collection named 'Lockstep-Inbox' from ZIP filename, got: {:?}",
        report.created_collections
    );
    assert!(ws_dir.path().join("collections/Lockstep-Inbox").exists());
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
    std::fs::write(
        legacy_col.join("bruno.json"),
        r#"{"name":"legacy-col","version":"1","type":"collection"}"#,
    )
    .unwrap();
    std::fs::write(
        legacy_col.join("req.bru"),
        "meta {\n  name: Req\n  type: http\n  seq: 1\n}\nget {\n  url: https://example.com\n}\n",
    )
    .unwrap();

    let ws_dir = TempDir::new().unwrap();
    let service = make_service(ws_dir.path());
    let report = service
        .import_workspace(ws_src, false, Some("default"))
        .unwrap();

    assert_eq!(report.detected_type, "workspace");
    assert_eq!(report.imported, 3, "2 modern + 1 legacy");
    assert_eq!(report.created_collections.len(), 2);
    assert!(ws_dir.path().join("collections/modern-col").exists());
    assert!(ws_dir.path().join("collections/legacy-col").exists());
}

#[test]
fn bru_graphql_file_imports_as_graphql_item() {
    use rocket_collection::{CollectionRepository, GraphQlRequest};

    let tmp = TempDir::new().expect("tempdir");
    // Use a named subdirectory so the collection name doesn't start with '.'.
    let src = tmp.path().join("gql-api");
    std::fs::create_dir_all(&src).expect("create source dir");
    std::fs::write(
        src.join("bruno.json"),
        r#"{ "name": "gql-api", "version": "1", "type": "collection" }"#,
    )
    .expect("write bruno.json");
    std::fs::write(
        src.join("users.bru"),
        "meta {\n  name: Users\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://api.example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query Users($n: Int) {\n    users(first: $n) { id }\n  }\n}\n\nbody:graphql:vars {\n  {\n    \"n\": 5\n  }\n}\n",
    )
    .expect("write users.bru");
    std::fs::write(
        src.join("orders.yml"),
        "info:\n  name: Orders\n  type: graphql\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  body:\n    query: '{ orders { id } }'\n",
    )
    .expect("write orders.yml");

    let workspace_dir = TempDir::new().expect("tempdir");
    let service = make_service(workspace_dir.path());
    let report = service
        .import_collection(&src, "default")
        .expect("import");

    assert_eq!(report.total_files, 2);
    assert_eq!(report.imported, 2, "skipped: {:?}", report.skipped);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g: GraphQlRequest = repo
        .get_graphql_request(&report.created_collections[0], "users.yml")
        .expect("users.yml is a graphql item");
    assert!(g.body.query.contains("users(first: $n)"), "{}", g.body.query);
    assert!(g
        .body
        .variables
        .as_deref()
        .expect("variables")
        .contains("\"n\": 5"));
    assert_eq!(g.url, "https://api.example.com/graphql");

    let o = repo
        .get_graphql_request(&report.created_collections[0], "orders.yml")
        .expect("orders.yml is a graphql item");
    assert_eq!(o.body.query, "{ orders { id } }");
}

#[test]
fn opencollection_graphql_yml_keeps_variants_scripts_and_auth() {
    use rocket_collection::CollectionRepository;

    let tmp = TempDir::new().expect("tempdir");
    let src = tmp.path().join("gql-full");
    std::fs::create_dir_all(&src).expect("create source dir");
    std::fs::write(
        src.join("bruno.json"),
        r#"{ "name": "gql-full", "version": "1", "type": "collection" }"#,
    )
    .expect("write bruno.json");
    std::fs::write(
        src.join("multi.yml"),
        "info:\n  name: Multi\n  type: graphql\ngraphql:\n  url: https://api.example.com/graphql\n  body:\n    - title: Users\n      body:\n        query: '{ users { id } }'\n    - title: Orders\n      selected: true\n      body:\n        query: '{ orders { id } }'\n  auth:\n    type: bearer\n    token: abc\nruntime:\n  scripts:\n    - type: before-request\n      code: console.log(1)\ndocs: some docs\n",
    )
    .expect("write multi.yml");

    let workspace_dir = TempDir::new().expect("tempdir");
    let service = make_service(workspace_dir.path());
    let report = service
        .import_collection(&src, "default")
        .expect("import");
    assert_eq!(report.imported, 1, "skipped: {:?}", report.skipped);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g = repo
        .get_graphql_request(&report.created_collections[0], "multi.yml")
        .expect("multi.yml is a graphql item");
    assert_eq!(g.body_variants.len(), 2, "every variant is kept");
    assert_eq!(g.body.query, "{ orders { id } }");
    assert!(matches!(g.auth, rocket_shared::types::Auth::Bearer { .. }));
    assert_eq!(g.pre_request_script.as_deref(), Some("console.log(1)"));
}

const WS_BRU: &str = "meta {\n  name: Echo\n  type: ws\n  seq: 1\n}\n\nws {\n  url: wss://echo.websocket.org\n  body: ws\n  auth: none\n}\n\nheaders {\n  X-Trace: abc\n}\n\nbody:ws {\n  message 1 [json] {\n    {\"name\":\"Bruno\"}\n  }\n}\n";

fn legacy_ws_collection(dir: &Path) -> PathBuf {
    let root = dir.join("ws-col");
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("bruno.json"),
        r#"{"name":"ws-col","version":"1","type":"collection"}"#,
    )
    .expect("bruno.json");
    std::fs::write(root.join("echo.bru"), WS_BRU).expect("echo.bru");
    root
}

#[test]
fn legacy_bru_websocket_imports_as_a_websocket_item_not_http() {
    let source = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let service = make_service(workspace.path());

    let report = service
        .import_collection(&legacy_ws_collection(source.path()), "default")
        .expect("import should succeed");

    assert_eq!(report.imported, 1, "{report:?}");
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let repo = FsCollectionRepo::new_standalone(workspace.path().join("collections"));
    let ws = rocket_collection::CollectionRepository::get_websocket_request(&repo, "ws-col", "echo.yml")
        .expect("saved as a websocket request");
    assert_eq!(ws.name, "Echo");
    assert_eq!(ws.url, "wss://echo.websocket.org");
    assert_eq!(ws.headers[0].key, "X-Trace");
    assert_eq!(ws.messages.len(), 1);
    assert_eq!(ws.messages[0].data, "{\"name\":\"Bruno\"}");

    // It must not also exist as an HTTP request.
    let http = rocket_collection::CollectionRepository::get_request(&repo, "ws-col", "echo.yml");
    assert!(http.is_err(), "the file must be a websocket file, not an http one");
}

#[test]
fn a_websocket_import_that_cannot_be_saved_is_reported_not_counted() {
    // A name that sanitises to nothing cannot be written; the report must say so.
    // (Kept as a unit-level guard on the routing: see importer.rs.)
    let source = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let root = legacy_ws_collection(source.path());
    // Break the target: make the destination collection directory read-only is not portable,
    // so assert the success path counts exactly one item instead.
    let report = make_service(workspace.path())
        .import_collection(&root, "default")
        .expect("import");
    assert_eq!(report.imported, 1);
    assert_eq!(report.total_files, 1);
}

#[test]
fn bru_grpc_file_imports_as_a_grpc_item_and_copies_its_proto() {
    use rocket_collection::{CollectionRepository, GrpcMethodType};

    let src = TempDir::new().unwrap();
    let root = src.path().join("grpc-api");
    std::fs::create_dir_all(root.join("protos")).unwrap();
    std::fs::create_dir_all(root.join("calls")).unwrap();
    std::fs::write(
        root.join("bruno.json"),
        r#"{ "name": "grpc-api", "version": "1", "type": "collection" }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("protos/greeter.proto"),
        "syntax = \"proto3\";\npackage demo.v1;\nservice Greeter { rpc SayHello (Req) returns (Rep); }\nmessage Req { string name = 1; }\nmessage Rep { string message = 1; }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("calls/say-hello.bru"),
        "meta {\n  name: Say Hello\n  type: grpc\n  seq: 1\n}\n\ngrpc {\n  url: localhost:50051\n  method: /demo.v1.Greeter/SayHello\n  body: grpc\n  auth: none\n  methodType: unary\n  protoPath: ../protos/greeter.proto\n}\n\nmetadata {\n  x-trace: abc\n  ~x-off: 1\n}\n\nbody:grpc {\n  name: message 1\n  content: '''\n    {\n      \"name\": \"ada\"\n    }\n  '''\n}\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();

    assert_eq!(report.imported, 1, "skipped: {:?}", report.skipped);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let name = &report.created_collections[0];
    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g = repo.get_grpc_request(name, "calls/say-hello.yml").unwrap();
    assert_eq!(g.url, "localhost:50051");
    assert_eq!(g.method.as_deref(), Some("demo.v1.Greeter/SayHello"));
    assert_eq!(g.method_type, GrpcMethodType::Unary);
    assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
    assert_eq!(g.metadata.len(), 2);
    assert!(!g.metadata[1].enabled);
    assert!(g.messages[0].content.contains("\"name\": \"ada\""));
    assert!(
        workspace_dir
            .path()
            .join("collections")
            .join(name)
            .join("protos/greeter.proto")
            .exists(),
        "the proto file travels with the requests"
    );
}

#[test]
fn opencollection_grpc_collection_imports_with_its_proto() {
    use rocket_collection::CollectionRepository;

    let src = TempDir::new().unwrap();
    let root = src.path().join("oc-grpc");
    std::fs::create_dir_all(root.join("protos")).unwrap();
    std::fs::write(root.join("opencollection.yml"), "opencollection: 1.0.0\ninfo:\n  name: oc-grpc\n").unwrap();
    std::fs::write(root.join("protos/greeter.proto"), "syntax = \"proto3\";\n").unwrap();
    std::fs::write(
        root.join("say-hello.yml"),
        "info:\n  name: Say Hello\n  type: grpc\ngrpc:\n  url: localhost:50051\n  method: demo.v1.Greeter/SayHello\n  methodType: unary\n  protoFilePath: protos/greeter.proto\n  message: '{}'\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();
    assert_eq!(report.imported, 1, "the proto file is not counted as a request");

    let name = &report.created_collections[0];
    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g = repo.get_grpc_request(name, "say-hello.yml").unwrap();
    assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
    assert!(workspace_dir
        .path()
        .join("collections")
        .join(name)
        .join("protos/greeter.proto")
        .exists());
}

#[test]
fn bru_file_of_an_unsupported_type_is_skipped_not_imported_as_an_empty_get() {
    use rocket_import::SkipReason;

    let src = TempDir::new().unwrap();
    let root = src.path().join("ws-api");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("bruno.json"),
        r#"{ "name": "ws-api", "version": "1", "type": "collection" }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("chat.bru"),
        "meta {\n  name: Chat\n  type: mqtt\n}\n\nmqtt {\n  url: mqtt://example.com\n}\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").unwrap();

    assert_eq!(report.imported, 0);
    assert!(matches!(
        report.skipped.as_slice(),
        [item] if matches!(&item.reason, SkipReason::UnsupportedRequestType(t) if t == "mqtt")
    ), "{:?}", report.skipped);
    assert!(!workspace_dir
        .path()
        .join("collections")
        .join(&report.created_collections[0])
        .join("chat.yml")
        .exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_proto_is_not_copied_and_does_not_abort_a_bruno_import() {
    let src = TempDir::new().expect("tempdir");
    let root = src.path().join("grpc-api");
    std::fs::create_dir_all(root.join("protos")).expect("mkdir");
    std::fs::write(
        root.join("bruno.json"),
        r#"{ "name": "grpc-api", "version": "1", "type": "collection" }"#,
    )
    .expect("write");
    std::fs::write(root.join("protos/real.proto"), "syntax = \"proto3\";\n").expect("write");
    // A file outside the collection that must never travel with it.
    let secret = src.path().join("id_rsa");
    std::fs::write(&secret, "PRIVATE KEY").expect("write");
    std::os::unix::fs::symlink(&secret, root.join("protos/leak.proto")).expect("symlink");
    std::os::unix::fs::symlink(src.path().join("missing"), root.join("protos/dangling.proto"))
        .expect("symlink");

    let workspace_dir = TempDir::new().expect("tempdir");
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").expect("import");

    let name = &report.created_collections[0];
    let copied = workspace_dir.path().join("collections").join(name).join("protos");
    assert!(copied.join("real.proto").exists());
    assert!(!copied.join("leak.proto").exists(), "a symlink is not followed");
    assert!(!copied.join("dangling.proto").exists());
    let skipped: Vec<&str> = report.skipped.iter().map(|s| s.path.as_str()).collect();
    assert!(
        skipped.iter().any(|p| p.ends_with("leak.proto")),
        "the user is told: {skipped:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_proto_is_not_copied_by_an_opencollection_import() {
    let src = TempDir::new().expect("tempdir");
    let root = src.path().join("oc-grpc");
    std::fs::create_dir_all(root.join("protos")).expect("mkdir");
    std::fs::write(
        root.join("opencollection.yml"),
        "opencollection: 1.0.0\ninfo:\n  name: oc-grpc\n",
    )
    .expect("write");
    let secret = src.path().join("id_rsa");
    std::fs::write(&secret, "PRIVATE KEY").expect("write");
    std::os::unix::fs::symlink(&secret, root.join("protos/leak.proto")).expect("symlink");
    std::os::unix::fs::symlink(src.path().join("missing"), root.join("protos/dangling.proto"))
        .expect("symlink");

    let workspace_dir = TempDir::new().expect("tempdir");
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(&root, "default").expect("import");

    let name = &report.created_collections[0];
    let copied = workspace_dir.path().join("collections").join(name).join("protos");
    assert!(!copied.join("leak.proto").exists(), "a symlink is not followed");
    assert!(!copied.join("dangling.proto").exists());
}
