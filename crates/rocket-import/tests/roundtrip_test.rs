//! Data that leaves Rocket as a Bruno ZIP and comes back must still open, and imports of the
//! shipped fixtures must leave a collection whose files all open.
//!
//! `bruno_zip_round_trip_of_real_workspace` is ignored. Run it on your own data with
//! `ROCKET_ROUNDTRIP_WORKSPACE=<workspace dir> cargo test -j4 -p rocket-import --test roundtrip_test -- --ignored --nocapture`.
//! It only reads the workspace.

use rocket_collection::CollectionRepository;
use rocket_environment::EnvironmentRepository;
use rocket_export::{bruno_zip, CollectionFileSource};
use rocket_import::{EnvironmentRepositoryFactory, ImportReport, ImportService, SkipReason};
use rocket_infra::{FsCollectionFileSource, FsCollectionRepo, FsEnvironmentRepo};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

fn service(workspace: &Path) -> ImportService {
    let path = workspace.to_path_buf();
    ImportService::new(
        path.clone(),
        Box::new(FsCollectionRepo::new_standalone(path.join("collections"))),
        Box::new(FsEnvFactory(path)),
    )
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Report entries that say a copied file does not open.
fn unopenable(report: &ImportReport) -> Vec<String> {
    report
        .skipped
        .iter()
        .filter_map(|s| match &s.reason {
            SkipReason::ParseError(why) if why.contains("cannot open") => {
                Some(format!("{}: {why}", s.path))
            }
            _ => None,
        })
        .collect()
}

fn unzip(bytes: &[u8], dest: &Path) {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).expect("entry");
        let Some(rel) = file.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out).expect("mkdir");
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).expect("read entry");
        std::fs::write(out, buf).expect("write entry");
    }
}

/// Exports `collection` of `workspace` as a Bruno ZIP and imports it into a fresh workspace.
/// Returns the import report and the number of request files before and after.
fn bruno_round_trip(workspace: &Path, collection: &str) -> (ImportReport, usize, usize, Vec<String>) {
    let source = FsCollectionFileSource::new(Arc::new(Mutex::new(workspace.to_path_buf())));
    let files = source.list_files(collection).expect("list files");
    let source_paths: Vec<String> = files.iter().map(|f| f.rel_path.clone()).collect();
    let before = request_files(&source_paths.iter().map(String::as_str).collect::<Vec<_>>());
    let (bytes, _export_report) = bruno_zip(files, collection, true).expect("bruno zip");

    let extracted = TempDir::new().expect("extract dir");
    unzip(&bytes, extracted.path());
    let root = extracted.path().join(collection);
    assert!(root.is_dir(), "zip root folder {collection} is missing");

    for path in &source_paths {
        if !root.join(path).exists() {
            println!("  not in the export: {path} ({})", export_report_note(&_export_report, path));
        }
    }

    let target = TempDir::new().expect("target workspace");
    let report = service(target.path())
        .import_collection(&root, "default")
        .expect("import of the exported zip");
    let after = report.imported;
    let diffs = compare_requests(workspace, target.path(), collection, &source_paths);
    (report, before, after, diffs)
}

/// The parts of an HTTP request that must survive the round trip. Credential values are left
/// out on purpose, because the export blanks them.
fn request_shape(r: &rocket_collection::Request) -> serde_json::Value {
    let names = |v: Vec<String>| serde_json::json!(v);
    let body_is_form = r.body.as_ref().is_some_and(|b| {
        let mode = serde_json::to_value(&b.mode).unwrap_or_default();
        mode.as_str().is_some_and(|m| m.to_lowercase().contains("form"))
    });
    serde_json::json!({
        "name": r.name,
        "method": r.method,
        "url": r.url,
        "headers": names(r.headers.iter().map(|h| h.key.clone()).collect()),
        "queryParams": names(r.query_params.iter().map(|p| p.key.clone()).collect()),
        "body": if body_is_form { serde_json::Value::Null } else { serde_json::to_value(&r.body).unwrap_or_default() },
        "pre": r.pre_request_script,
        "post": r.post_response_script,
        "tests": r.tests,
        "assertions": r.assertions.len(),
        "seq": r.seq,
    })
}

/// Reads every HTTP request file of the source and of the round trip and lists the differences.
fn compare_requests(src_ws: &Path, dst_ws: &Path, collection: &str, paths: &[String]) -> Vec<String> {
    let src = FsCollectionRepo::new_standalone(src_ws.join("collections"));
    let dst = FsCollectionRepo::new_standalone(dst_ws.join("collections"));
    let mut diffs = Vec::new();
    for path in paths {
        let name = path.rsplit('/').next().unwrap_or(path);
        let skip = !path.ends_with(".yml")
            || path.starts_with("environments/")
            || path.starts_with("flows/")
            || path.starts_with(".rocket/")
            || name == "folder.yml"
            || name == "opencollection.yml";
        if skip {
            continue;
        }
        let kind = src.request_kind(collection, path);
        if !matches!(kind, Ok(rocket_collection::RequestKind::Http)) {
            continue;
        }
        let (a, b) = (src.get_request(collection, path), dst.get_request(collection, path));
        match (a, b) {
            (Ok(a), Ok(b)) => {
                let (a, b) = (request_shape(&a), request_shape(&b));
                if a != b {
                    diffs.push(format!("{path}: differs\n  before {a}\n  after  {b}"));
                }
            }
            (Err(e), _) => diffs.push(format!("{path}: source does not open: {e}")),
            (_, Err(e)) => diffs.push(format!("{path}: round trip does not open: {e}")),
        }
    }
    diffs
}

fn export_report_note(report: &rocket_export::ExportReport, path: &str) -> String {
    report
        .items
        .iter()
        .find(|n| n.path == path || path.starts_with(&format!("{}/", n.path)))
        .map(|n| format!("{}: {:?}", n.feature, n.action))
        .unwrap_or_else(|| "no report entry".to_string())
}

/// Counts the files the import counts: every YAML file outside `environments/` except the root
/// file, which includes `folder.yml`.
fn request_files(paths: &[&str]) -> usize {
    paths
        .iter()
        .filter(|p| {
            let name = p.rsplit('/').next().unwrap_or(p);
            (p.ends_with(".yml") || p.ends_with(".yaml"))
                && !p.starts_with("environments/")
                && name != "opencollection.yml"
                && !p.starts_with("flows/")
                && !p.starts_with(".rocket/")
        })
        .count()
}

#[test]
fn legacy_bruno_import_then_zip_export_then_import_opens_everything() {
    let ws = TempDir::new().expect("ws");
    let first = service(ws.path())
        .import_collection(&fixture("my-api"), "default")
        .expect("first import");
    assert!(unopenable(&first).is_empty(), "{:?}", unopenable(&first));

    let (second, before, after, diffs) = bruno_round_trip(ws.path(), "my-api");
    assert!(unopenable(&second).is_empty(), "{:?}", unopenable(&second));
    assert_eq!(before, after, "request files lost in the round trip");
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn postman_import_then_zip_export_then_import_opens_everything() {
    let ws = TempDir::new().expect("ws");
    let first = service(ws.path())
        .import_postman_collection(&fixture("postman/full-collection.json"), "default")
        .expect("postman import");
    assert!(unopenable(&first).is_empty(), "{:?}", unopenable(&first));
    let name = first.created_collections[0].clone();

    // The repo itself must read every request of the Postman import.
    let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
    repo.get_strict(&name).expect("strict read");

    let (second, before, after, diffs) = bruno_round_trip(ws.path(), &name);
    assert!(unopenable(&second).is_empty(), "{:?}", unopenable(&second));
    assert_eq!(before, after, "request files lost in the round trip");
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
#[ignore = "reads the workspace named by ROCKET_ROUNDTRIP_WORKSPACE"]
fn bruno_zip_round_trip_of_real_workspace() {
    let Ok(dir) = std::env::var("ROCKET_ROUNDTRIP_WORKSPACE") else {
        panic!("set ROCKET_ROUNDTRIP_WORKSPACE to a workspace directory");
    };
    let workspace = PathBuf::from(dir);
    let mut problems = Vec::new();
    let entries = std::fs::read_dir(workspace.join("collections")).expect("collections dir");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.path().is_dir() {
            continue;
        }
        let source_check = FsCollectionRepo::new_standalone(workspace.join("collections"));
        if let Err(e) = source_check.get_strict(&name) {
            problems.push(format!("{name}: source does not read strictly: {e}"));
        }
        let (report, before, after, diffs) = bruno_round_trip(&workspace, &name);
        println!("{name}: {before} request files -> {after} imported");
        if before != after {
            problems.push(format!("{name}: {before} request files became {after}"));
        }
        for line in unopenable(&report) {
            problems.push(format!("{name}: {line}"));
        }
        for line in diffs {
            problems.push(format!("{name}: {line}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
