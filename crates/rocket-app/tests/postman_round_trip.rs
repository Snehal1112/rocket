//! A collection exported as a Postman file must import again and open.
//!
//! The fixture test starts from the shipped Postman fixture. `postman_round_trip_of_real_workspace`
//! is ignored and reads the workspace named by `ROCKET_ROUNDTRIP_WORKSPACE`.

use rocket_app::collection_export_service::{CollectionExportService, PostmanExportOptions};
use rocket_collection::CollectionRepository;
use rocket_environment::EnvironmentRepository;
use rocket_import::{
    EnvironmentRepositoryFactory, ImportReport, ImportService, SkipReason,
};
use rocket_infra::{FsCollectionFileSource, FsCollectionRepo, FsEnvironmentRepo};
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

fn importer(workspace: &Path) -> ImportService {
    let path = workspace.to_path_buf();
    ImportService::new(
        path.clone(),
        Box::new(FsCollectionRepo::new_standalone(path.join("collections"))),
        Box::new(FsEnvFactory(path)),
    )
}

fn exporter(workspace: &Path) -> CollectionExportService {
    CollectionExportService::new(Box::new(FsCollectionFileSource::new(Arc::new(Mutex::new(
        workspace.to_path_buf(),
    )))))
    .with_collections(Box::new(FsCollectionRepo::new_standalone(
        workspace.join("collections"),
    )))
}

fn count_requests(repo: &FsCollectionRepo, name: &str) -> usize {
    fn walk(items: &[rocket_collection::CollectionItem]) -> usize {
        items
            .iter()
            .map(|i| match i {
                rocket_collection::CollectionItem::Folder(f) => walk(&f.items),
                rocket_collection::CollectionItem::Summary(_)
                | rocket_collection::CollectionItem::Request(_)
                | rocket_collection::CollectionItem::GraphQl(_)
                | rocket_collection::CollectionItem::WebSocket(_)
                | rocket_collection::CollectionItem::Grpc(_) => 1,
                _ => 0,
            })
            .sum()
    }
    let loaded = repo.get_strict(name).expect("strict read");
    walk(&loaded.root.items)
}

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

/// Exports `collection` as Postman JSON and imports it into a new workspace.
/// Returns the request counts before and after and the files that do not open.
fn round_trip(workspace: &Path, collection: &str) -> (usize, usize, Vec<String>) {
    let source = FsCollectionRepo::new_standalone(workspace.join("collections"));
    let before = count_requests(&source, collection);
    let out = exporter(workspace)
        .export_postman(collection, &PostmanExportOptions::default())
        .expect("postman export");
    let dir = TempDir::new().expect("tmp");
    let file = dir.path().join(&out.file_name);
    std::fs::write(&file, &out.bytes).expect("write export");

    let target = TempDir::new().expect("target");
    let report = importer(target.path())
        .import_postman_collection(&file, "default")
        .expect("import of the exported file");
    let name = report.created_collections[0].clone();
    let dest = FsCollectionRepo::new_standalone(target.path().join("collections"));
    let after = count_requests(&dest, &name);
    (before, after, unopenable(&report))
}

#[test]
fn postman_fixture_exports_and_imports_again() {
    let ws = TempDir::new().expect("ws");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rocket-import/tests/fixtures/postman/full-collection.json");
    let first = importer(ws.path())
        .import_postman_collection(&fixture, "default")
        .expect("first import");
    let name = first.created_collections[0].clone();

    let (before, after, bad) = round_trip(ws.path(), &name);
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!(before, after, "requests lost in the Postman round trip");
}

#[test]
#[ignore = "reads the workspace named by ROCKET_ROUNDTRIP_WORKSPACE"]
fn postman_round_trip_of_real_workspace() {
    let Ok(dir) = std::env::var("ROCKET_ROUNDTRIP_WORKSPACE") else {
        panic!("set ROCKET_ROUNDTRIP_WORKSPACE to a workspace directory");
    };
    let workspace = PathBuf::from(dir);
    let mut problems = Vec::new();
    let entries = std::fs::read_dir(workspace.join("collections")).expect("collections dir");
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let source = FsCollectionRepo::new_standalone(workspace.join("collections"));
        if let Err(e) = source.get_strict(&name) {
            problems.push(format!("{name}: source does not read strictly: {e}"));
            continue;
        }
        let exported = std::panic::catch_unwind(|| round_trip(&workspace, &name));
        match exported {
            Ok((before, after, bad)) => {
                println!("{name}: {before} requests -> {after}");
                if before != after {
                    problems.push(format!("{name}: {before} requests became {after}"));
                }
                problems.extend(bad.into_iter().map(|b| format!("{name}: {b}")));
            }
            Err(_) => problems.push(format!("{name}: the export or import failed")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
