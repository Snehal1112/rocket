//! The team network import names the new collection from the ZIP file stem. These tests pin
//! that, so the adapter in `src-tauri` can rely on it. A modern-format ZIP is built by hand
//! with an `opencollection.yml` whose `info.name` differs from the file name.

use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_import::ImportService;
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

struct FsEnvFactory(PathBuf);

impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn for_collection(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
        ))
    }
}

fn service(workspace: &Path) -> ImportService {
    let base = workspace.to_path_buf();
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
        std::sync::Arc::new(rocket_infra::FsImportFiles),
    )
}

fn repo(workspace: &Path) -> FsCollectionRepo {
    FsCollectionRepo::new_standalone(workspace.join("collections"))
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    for (name, text) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("start file");
        writer.write_all(text.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip");
}

const COLLECTION: [(&str, &str); 2] = [
    (
        "Inner Name/opencollection.yml",
        "opencollection: \"1.0.0\"\ninfo:\n  name: Inner Name\n",
    ),
    (
        "Inner Name/get.yml",
        "info:\n  name: get\nhttp:\n  method: GET\n  url: https://example.test\n",
    ),
];

/// Imports `COLLECTION` stored under `<stem>.zip` and returns the created names.
fn import_as(workspace: &Path, stem: &str) -> Vec<String> {
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join(format!("{stem}.zip"));
    write_zip(&zip_path, &COLLECTION);
    service(workspace)
        .import_auto_from_zip(&zip_path, "default", false)
        .expect("import")
        .created_collections
}

fn collection_names(workspace: &Path) -> Vec<String> {
    let mut names: Vec<String> = repo(workspace)
        .list()
        .expect("list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.sort();
    names
}

#[test]
fn the_zip_stem_names_the_collection_not_opencollection_yml() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Renamed Copy");
    assert_eq!(created, vec!["Renamed Copy".to_string()]);
    assert_eq!(
        collection_names(ws.path()),
        vec!["Renamed Copy".to_string()]
    );
    assert!(
        !ws.path().join("collections/Inner Name").exists(),
        "the wrapper folder and info.name must not name the collection"
    );
}

#[test]
fn a_second_import_of_the_same_stem_gets_a_numeric_suffix_and_overwrites_nothing() {
    let ws = TempDir::new().expect("tempdir");
    assert_eq!(import_as(ws.path(), "Orders"), vec!["Orders".to_string()]);
    let marker = ws.path().join("collections/Orders/keep.txt");
    std::fs::write(&marker, "mine").expect("marker");

    assert_eq!(import_as(ws.path(), "Orders"), vec!["Orders-1".to_string()]);
    assert_eq!(std::fs::read_to_string(&marker).expect("marker"), "mine");
    assert_eq!(
        collection_names(ws.path()),
        vec!["Orders".to_string(), "Orders-1".to_string()]
    );
}

#[test]
fn a_stem_with_a_suffix_like_text_is_kept_as_written() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Orders (from Ana)");
    assert_eq!(created, vec!["Orders (from Ana)".to_string()]);
}

#[test]
fn a_non_ascii_zip_stem_is_the_collection_name() {
    let ws = TempDir::new().expect("tempdir");
    let created = import_as(ws.path(), "Café – Ünï 日本語");
    assert_eq!(created, vec!["Café – Ünï 日本語".to_string()]);
    assert!(ws
        .path()
        .join("collections/Café – Ünï 日本語/opencollection.yml")
        .exists());
}

#[test]
fn a_zip_with_one_root_file_and_no_wrapper_folder_imports() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join("Flat.zip");
    write_zip(
        &zip_path,
        &[(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: x\n",
        )],
    );
    let report = service(ws.path())
        .import_auto_from_zip(&zip_path, "default", false)
        .expect("import");
    assert_eq!(report.created_collections, vec!["Flat".to_string()]);
}

#[test]
fn a_zip_with_one_file_and_no_marker_creates_nothing() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip_path = zips.path().join("Notes.zip");
    write_zip(&zip_path, &[("notes.txt", "hello")]);
    let result = service(ws.path()).import_auto_from_zip(&zip_path, "default", false);
    assert!(result.is_err());
    assert!(collection_names(ws.path()).is_empty());
}
