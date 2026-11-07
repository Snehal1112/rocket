//! `ImportService::created_roots` lists exactly the collection folders one import made, even
//! when the import fails afterwards. The team network adapter removes only those.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_import::{DirEntryInfo, EntryKind, ImportFiles, ImportService, ReadSeek, WorkDir};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo, FsImportFiles};
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

/// The real port, except that folders inside `<workspace>/collections/` cannot be made.
struct FailingCollectionDirs {
    collections: PathBuf,
}

impl ImportFiles for FailingCollectionDirs {
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
        FsImportFiles.list_dir(dir)
    }
    fn kind(&self, path: &Path) -> io::Result<EntryKind> {
        FsImportFiles.kind(path)
    }
    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        if dir.starts_with(&self.collections) {
            return Err(io::Error::other("disk full"));
        }
        FsImportFiles.create_dir_all(dir)
    }
    fn copy(&self, src: &Path, dest: &Path) -> io::Result<()> {
        FsImportFiles.copy(src, dest)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        FsImportFiles.read_to_string(path)
    }
    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        FsImportFiles.write(path, text)
    }
    fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>> {
        FsImportFiles.regular_file_len(path)
    }
    fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        FsImportFiles.read_up_to(path, max_bytes)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        FsImportFiles.canonicalize(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
        FsImportFiles.open_read(path)
    }
    fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
        FsImportFiles.temp_dir()
    }
    fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64> {
        FsImportFiles.write_stream(path, from)
    }
}

fn service(workspace: &Path, files: Arc<dyn ImportFiles>) -> ImportService {
    let base = workspace.to_path_buf();
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
        files,
    )
}

/// A modern collection with one request in a subfolder, stored as `<stem>.zip`.
fn zip_with_subfolder(dir: &Path, stem: &str) -> PathBuf {
    let path = dir.join(format!("{stem}.zip"));
    let file = std::fs::File::create(&path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    for (name, text) in [
        (
            "c/opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: c\n",
        ),
        (
            "c/sub/get.yml",
            "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n",
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start file");
        writer.write_all(text.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip");
    path
}

#[test]
fn created_roots_names_the_folder_a_successful_import_made() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let svc = service(ws.path(), Arc::new(FsImportFiles));

    svc.import_auto_from_zip(&zip, "default", false)
        .expect("import");

    assert_eq!(
        svc.created_roots(),
        vec![ws.path().join("collections").join("Orders")]
    );
}

#[test]
fn created_roots_is_filled_when_the_import_fails_after_the_folder_was_made() {
    let ws = TempDir::new().expect("tempdir");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let files = FailingCollectionDirs {
        collections: ws.path().join("collections"),
    };
    let svc = service(ws.path(), Arc::new(files));

    let result = svc.import_auto_from_zip(&zip, "default", false);

    assert!(
        result.is_err(),
        "the subfolder cannot be made, so the import fails"
    );
    let root = ws.path().join("collections").join("Orders");
    assert!(root.is_dir(), "create made the root before the failure");
    assert_eq!(svc.created_roots(), vec![root]);
}

#[test]
fn created_roots_is_empty_when_create_refuses_the_folder() {
    let ws = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(ws.path().join("collections")).expect("dir");
    // A plain file where the folder must go: `list()` does not see it, `create` refuses it.
    std::fs::write(ws.path().join("collections").join("Orders"), b"a file").expect("blocker");
    let zips = TempDir::new().expect("tempdir");
    let zip = zip_with_subfolder(zips.path(), "Orders");
    let svc = service(ws.path(), Arc::new(FsImportFiles));

    assert!(svc.import_auto_from_zip(&zip, "default", false).is_err());
    assert!(
        svc.created_roots().is_empty(),
        "nothing was made, so nothing may be removed"
    );
    let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
    assert!(repo.list().expect("list").is_empty());
}
