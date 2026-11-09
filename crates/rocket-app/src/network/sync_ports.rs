//! Ports of the collection sync, and the base helpers that read a sent ZIP.
//!
//! Every method of `SyncCollectionPort` blocks (it reads and writes files under the
//! collection lock), so async callers use `spawn_blocking`.

use std::io::{Read, Seek};
use std::path::Path;

use rocket_import::ImportFiles;
use rocket_network::link::Manifest;
use rocket_shared::error::DomainResult;

/// One synced file as the export carries it: collection-relative path and scrubbed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// One change of a sync apply. `apply` runs them in a fixed order, see `SyncCollectionPort`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    /// Writes the file. A YAML file is merged into the local one, so local secrets stay. Any
    /// other file (`.js`, `.proto`) is replaced outright.
    Put { path: String, bytes: Vec<u8> },
    /// Writes a new file, written as received. The path must be free.
    PutAs { path: String, bytes: Vec<u8> },
    /// Deletes the exact path. On a case-insensitive disk that can remove a file that differs
    /// from it only by case.
    Delete { path: String },
    /// Moves a local file. The local bytes move as they are. `from` matches the exact path,
    /// like `Delete`. A rename that changes only letter case is refused (`InvalidInput`), so
    /// the service must show the user that such a rename cannot be synced.
    Rename { from: String, to: String },
}

/// The collections a sync reads and writes.
pub trait SyncCollectionPort: Send + Sync {
    fn exists(&self, collection: &str) -> DomainResult<bool>;
    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>>;
    /// The absolute folder of the collection, as the link record stores it.
    fn collection_root(&self, collection: &str) -> DomainResult<String>;
    /// The path and hash of every synced file, as the export would send them.
    fn manifest(&self, collection: &str) -> DomainResult<Manifest>;
    /// The exported (scrubbed) files for `paths`, in the asked order.
    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>>;
    /// Applies the changes all or nothing. The order is fixed whatever the input order:
    /// renames, then deletes, then puts and put-as. A Put, PutAs or Rename target that equals
    /// a different existing path ignoring case is refused.
    ///
    /// The sync service never calls this: it has no check against a local save that landed
    /// since the manifest was read. It uses `apply_if_unchanged`.
    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()>;
    /// Like `apply`, but inside the collection lock it reads the manifest again without taking
    /// the lock a second time. When that manifest differs from `expected` nothing is applied
    /// and the result is `DomainError::Conflict`.
    fn apply_if_unchanged(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()>;
}

/// The base of a new link read from the sent ZIP: its manifest and the files behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipBase {
    pub manifest: Manifest,
    pub files: Vec<(String, Vec<u8>)>,
}

/// Lowercase hex SHA-256 of some bytes, as the manifest stores it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Reads the base from a ZIP. Paths the sync does not carry (environments, for one) are left
/// out. A name that clashes with another by letter case is refused by the manifest.
pub fn base_from_reader<R: Read + Seek>(reader: R) -> DomainResult<ZipBase> {
    let mut manifest = Manifest::new();
    let mut files = Vec::new();
    for (path, bytes) in rocket_export::zip_entries(reader)? {
        if !rocket_export::is_synced_path(&path) {
            continue;
        }
        manifest.insert(&path, &sha256_hex(&bytes))?;
        files.push((path, bytes));
    }
    Ok(ZipBase { manifest, files })
}

/// Like `base_from_reader` for a ZIP file read through the import port.
pub fn base_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<ZipBase> {
    let reader = files.open_read(zip_path)?;
    base_from_reader(reader)
}

/// The manifest of `base_from_zip`.
pub fn manifest_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<Manifest> {
    base_from_zip(files, zip_path).map(|base| base.manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_infra::FsImportFiles;
    use rocket_shared::error::DomainError;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default())
                .expect("start");
            w.write_all(body.as_bytes()).expect("write");
        }
        w.finish().expect("finish").into_inner()
    }

    #[test]
    fn a_zip_base_has_collection_relative_paths_and_hashes() {
        let zip = make_zip(&[
            ("Orders/opencollection.yml", "a: 1\n"),
            ("Orders/users/get.yml", "b: 2\n"),
        ]);
        let base = base_from_reader(Cursor::new(zip)).expect("reads");
        let paths: Vec<&str> = base.manifest.paths().collect();
        assert_eq!(paths, vec!["opencollection.yml", "users/get.yml"]);
        assert_eq!(
            base.manifest.get("users/get.yml"),
            Some(sha256_hex(b"b: 2\n").as_str())
        );
        assert_eq!(
            base.files[1],
            ("users/get.yml".to_string(), b"b: 2\n".to_vec())
        );
    }

    #[test]
    fn a_zip_base_skips_paths_the_sync_does_not_carry() {
        // A first send with "include environments" puts environments in the ZIP.
        let zip = make_zip(&[
            ("Orders/opencollection.yml", "a: 1\n"),
            ("Orders/environments/dev.yml", "name: dev\n"),
            ("Orders/notes.txt", "x"),
        ]);
        let base = base_from_reader(Cursor::new(zip)).expect("reads");
        let paths: Vec<&str> = base.manifest.paths().collect();
        assert_eq!(paths, vec!["opencollection.yml"]);
        assert_eq!(base.files.len(), 1);
    }

    #[test]
    fn a_zip_base_reads_a_file_through_the_import_port() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sent.zip");
        std::fs::write(&path, make_zip(&[("Orders/a.yml", "a: 1\n")])).expect("write");
        let manifest = manifest_from_zip(&FsImportFiles, &path).expect("reads");
        assert_eq!(manifest.len(), 1);
        let base = base_from_zip(&FsImportFiles, &path).expect("reads");
        assert_eq!(base.manifest, manifest);
    }

    #[test]
    fn a_zip_that_is_not_a_zip_is_refused() {
        let err = base_from_reader(Cursor::new(b"nope".to_vec())).expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_zip_with_names_that_differ_only_by_case_is_refused_with_a_fixed_text() {
        let zip = make_zip(&[
            ("Orders/Users/a.yml", "a: 1\n"),
            ("Orders/users/a.yml", "b: 2\n"),
        ]);
        let err = base_from_reader(Cursor::new(zip)).expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!err.to_string().contains("users"));
    }
}
