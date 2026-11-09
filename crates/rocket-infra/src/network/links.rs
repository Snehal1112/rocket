//! Links of the sync feature: one folder per link under `~/.rocket-api/network/links/`.
//!
//! `<link_id>/link.yml` is the `LinkRecord`. `<link_id>/base/<path>` holds one copy of each
//! file at its agreed version. Machine-local: never inside a workspace or collection.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rocket_collection::file_tx::validate_rel_path;
use rocket_network::device::DeviceId;
use rocket_network::link::{path_is_valid, LinkId, LinkRecord, LinkState, Manifest};
use rocket_network::ports::{LinkRepository, QuarantineReport};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};

use crate::atomic_write;
use crate::network::lock::lock_ignore_poison;
use crate::yaml_io::quarantine_corrupt;

const RECORD_FILE: &str = "link.yml";

pub struct FsLinkRepo {
    dir: PathBuf,
    store: Mutex<()>,
    quarantined: Mutex<Option<String>>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_error(err: io::Error) -> DomainError {
    DomainError::Io(format!("Failed to write the link: {err}"))
}

/// Refuses a path that either the file rule or the manifest rule rejects.
fn check_path(path: &str) -> DomainResult<()> {
    // The file rule quotes the path in its message, so its text is replaced.
    if validate_rel_path(path).is_err() || !path_is_valid(path) {
        return Err(DomainError::InvalidInput(
            "A base path is not valid.".to_string(),
        ));
    }
    Ok(())
}

/// Every file (and link) under `root`, as `/` separated paths relative to it.
fn walk_files(root: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk_files(&entry.path(), &rel, out),
            Ok(_) => out.push(rel),
            Err(_) => {}
        }
    }
}

/// Removes empty directories below `root`, bottom-up. Errors are ignored.
fn remove_empty_dirs(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_dirs(&entry.path());
            let _ = fs::remove_dir(entry.path());
        }
    }
}

/// A record for a `link.yml` that could not be read. It keeps what can still be read.
fn broken_stub(id: &LinkId, bytes: &[u8]) -> LinkRecord {
    let value: Option<serde_yaml::Value> =
        serde_yaml::from_str(&String::from_utf8_lossy(bytes)).ok();
    let text = |key: &str| -> Option<String> {
        value
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    LinkRecord {
        link_id: id.clone(),
        peer_device_id: text("peer_device_id")
            .and_then(|s| DeviceId::parse_base32(&s).ok())
            .unwrap_or_else(|| DeviceId::from_bytes([0u8; 32])),
        collection_name: text("collection_name")
            .unwrap_or_else(|| "Unknown collection".to_string()),
        collection_root: text("collection_root").unwrap_or_default(),
        collection_uid: text("collection_uid").filter(|s| !s.is_empty()),
        base: Manifest::new(),
        state: LinkState::Broken,
        updated_at: text("updated_at").unwrap_or_default(),
    }
}

impl FsLinkRepo {
    /// Does not touch the disk. The folder is created by the first `save`.
    pub fn new(dir: PathBuf) -> FsLinkRepo {
        FsLinkRepo {
            dir,
            store: Mutex::new(()),
            quarantined: Mutex::new(None),
        }
    }

    fn folder(&self, id: &LinkId) -> PathBuf {
        self.dir.join(id.as_str())
    }

    fn base_path(&self, id: &LinkId, path: &str) -> DomainResult<PathBuf> {
        check_path(path)?;
        let mut target = self.folder(id).join("base");
        for segment in path.split('/') {
            target.push(segment);
        }
        Ok(target)
    }

    /// Reads one record. The store lock must be held.
    fn read_record(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        let path = self.folder(id).join(RECORD_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(DomainError::Io(format!("Failed to read link.yml: {err}")));
            }
        };
        match serde_yaml::from_slice::<LinkRecord>(&bytes) {
            Ok(record) if record.link_id == *id => return Ok(Some(record)),
            _ => {}
        }
        quarantine_corrupt(&path)
            .map_err(|err| DomainError::Io(format!("Failed to set link.yml aside: {err}")))?;
        let stub = broken_stub(id, &bytes);
        let yaml = serde_yaml::to_string(&stub)
            .map_err(|err| DomainError::Serialization(err.to_string()))?;
        *lock_ignore_poison(&self.quarantined) = Some(RECORD_FILE.to_string());
        if let Err(err) = atomic_write(&path, yaml.as_bytes()) {
            tracing::warn!(link = %id.as_str(), kind = ?err.kind(), "broken link stub was not saved");
        }
        tracing::warn!(link = %id.as_str(), "link file was damaged and has been set aside");
        Ok(Some(stub))
    }
}

impl LinkRepository for FsLinkRepo {
    fn list(&self) -> DomainResult<Vec<LinkRecord>> {
        let _guard = lock_ignore_poison(&self.store);
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(DomainError::Io(format!("Failed to list links: {err}")));
            }
        };
        let mut records = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(id) = LinkId::parse(&name) else {
                continue;
            };
            match self.read_record(&id) {
                Ok(Some(record)) => records.push(record),
                Ok(None) => {}
                Err(_) => {
                    tracing::warn!(link = %id.as_str(), "link could not be read and was skipped");
                }
            }
        }
        records.sort_by(|a, b| a.link_id.as_str().cmp(b.link_id.as_str()));
        Ok(records)
    }

    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        let _guard = lock_ignore_poison(&self.store);
        self.read_record(id)
    }

    // Order: validate, write the base copies, write link.yml, then prune. A crash in between
    // leaves old copies that no longer match the record, which read as missing, and
    // unreferenced copies, which the next save prunes.
    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
        // Validate everything before the first write, so the save is all or nothing.
        let mut targets = Vec::with_capacity(base_files.len());
        for (path, bytes) in base_files {
            let target = self.base_path(&record.link_id, path)?;
            if record.base.get(path) != Some(sha256_hex(bytes).as_str()) {
                return Err(DomainError::InvalidInput(
                    "A base copy does not match the manifest.".to_string(),
                ));
            }
            targets.push(target);
        }
        let yaml = serde_yaml::to_string(record)
            .map_err(|err| DomainError::Serialization(err.to_string()))?;

        let _guard = lock_ignore_poison(&self.store);
        let folder = self.folder(&record.link_id);
        let base_root = folder.join("base");

        // A stale copy that clashes with a new one goes first: one that differs only by case
        // (it would be overwritten and then pruned on a case-insensitive disk) or one that is
        // a file where the new path needs a folder, or the reverse.
        let mut stored = Vec::new();
        walk_files(&base_root, "", &mut stored);
        for stale in stored.iter().filter(|p| record.base.get(p).is_none()) {
            let stale_lower = stale.to_ascii_lowercase();
            let collides = base_files.iter().any(|(path, _)| {
                let new_lower = path.to_ascii_lowercase();
                new_lower == stale_lower
                    || new_lower.starts_with(&format!("{stale_lower}/"))
                    || stale_lower.starts_with(&format!("{new_lower}/"))
            });
            if collides {
                let _ = fs::remove_file(self.base_path(&record.link_id, stale)?);
            }
        }
        remove_empty_dirs(&base_root);

        for (target, (_, bytes)) in targets.iter().zip(base_files) {
            atomic_write(target, bytes).map_err(write_error)?;
        }
        atomic_write(&folder.join(RECORD_FILE), yaml.as_bytes()).map_err(write_error)?;

        let mut stored = Vec::new();
        walk_files(&base_root, "", &mut stored);
        for path in stored.iter().filter(|p| record.base.get(p).is_none()) {
            let mut target = base_root.clone();
            for segment in path.split('/') {
                target.push(segment);
            }
            let _ = fs::remove_file(target);
        }
        remove_empty_dirs(&base_root);
        Ok(())
    }

    fn delete(&self, id: &LinkId) -> DomainResult<()> {
        let _guard = lock_ignore_poison(&self.store);
        match fs::remove_dir_all(self.folder(id)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(DomainError::Io(format!("Failed to delete the link: {err}"))),
        }
    }

    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
        let target = self.base_path(id, path)?;
        let _guard = lock_ignore_poison(&self.store);
        let Some(record) = self.read_record(id)? else {
            return Ok(None);
        };
        let Some(expected) = record.base.get(path) else {
            return Ok(None);
        };
        let bytes = match fs::read(&target) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(DomainError::Io(format!(
                    "Failed to read a base copy: {err}"
                )));
            }
        };
        if sha256_hex(&bytes) != expected {
            tracing::warn!(link = %id.as_str(), path = %path, "base copy does not match the record");
            return Ok(None);
        }
        Ok(Some(bytes))
    }
}

impl QuarantineReport for FsLinkRepo {
    fn take_quarantined(&self) -> Option<String> {
        lock_ignore_poison(&self.quarantined).take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_network::device::DeviceId;
    use rocket_network::link::{LinkState, Manifest};
    use tempfile::TempDir;

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    }

    fn record(id: &LinkId, files: &[(&str, &[u8])]) -> LinkRecord {
        let mut base = Manifest::new();
        for (path, bytes) in files {
            base.insert(path, &sha(bytes)).expect("manifest entry");
        }
        LinkRecord {
            link_id: id.clone(),
            peer_device_id: DeviceId::from_bytes([7u8; 32]),
            collection_name: "Orders".to_string(),
            collection_root: "/data/collections/Orders".to_string(),
            collection_uid: Some("col-uid".to_string()),
            base,
            state: LinkState::InSync,
            updated_at: "2026-11-08T10:00:00Z".to_string(),
        }
    }

    fn owned(files: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
        files
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect()
    }

    fn repo() -> (TempDir, FsLinkRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsLinkRepo::new(dir.path().join("links"));
        (dir, repo)
    }

    #[test]
    fn a_saved_link_round_trips() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 2] = [("a.yml", b"a: 1\n"), ("users/b.yml", b"b: 2\n")];
        let rec = record(&id, &files);
        repo.save(&rec, &owned(&files)).expect("saves");
        assert_eq!(repo.get(&id).expect("gets"), Some(rec.clone()));
        assert_eq!(repo.list().expect("lists"), vec![rec]);
    }

    #[test]
    fn an_unknown_link_is_none_and_an_empty_store_lists_nothing() {
        let (_dir, repo) = repo();
        assert_eq!(repo.get(&LinkId::random()).expect("gets"), None);
        assert!(repo.list().expect("lists").is_empty());
    }

    #[test]
    fn base_files_come_back_byte_for_byte() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let crlf: &[u8] = b"a: 1\r\nb: caf\xc3\xa9\r\n";
        let binary: &[u8] = &[0, 159, 146, 150, 255, 10];
        let files: [(&str, &[u8]); 2] = [("deep/er/a.yml", crlf), ("lib/blob.js", binary)];
        repo.save(&record(&id, &files), &owned(&files))
            .expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "deep/er/a.yml").expect("reads"),
            Some(crlf.to_vec())
        );
        assert_eq!(
            repo.read_base_file(&id, "lib/blob.js").expect("reads"),
            Some(binary.to_vec())
        );
        assert_eq!(repo.read_base_file(&id, "nope.yml").expect("reads"), None);
    }

    #[test]
    fn a_state_only_save_keeps_the_base_copies() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        let mut rec = record(&id, &files);
        repo.save(&rec, &owned(&files)).expect("saves");
        rec.state = LinkState::OutOfDate;
        repo.save(&rec, &[]).expect("saves the state");
        assert_eq!(
            repo.get(&id).expect("gets").expect("exists").state,
            LinkState::OutOfDate
        );
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"a: 1\n".to_vec())
        );
    }

    #[test]
    fn a_save_replaces_a_changed_copy_and_prunes_a_path_that_left_the_manifest() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let first: [(&str, &[u8]); 2] = [("a.yml", b"old\n"), ("gone/b.yml", b"b\n")];
        repo.save(&record(&id, &first), &owned(&first))
            .expect("saves");
        let second: [(&str, &[u8]); 1] = [("a.yml", b"new\n")];
        repo.save(&record(&id, &second), &owned(&second))
            .expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"new\n".to_vec())
        );
        assert!(!dir
            .path()
            .join("links")
            .join(id.as_str())
            .join("base/gone/b.yml")
            .exists());
    }

    #[test]
    fn a_save_refuses_a_copy_that_does_not_match_the_manifest() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let rec = record(&id, &[("a.yml", b"right\n")]);
        let err = repo
            .save(&rec, &owned(&[("a.yml", b"wrong\n")]))
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let not_listed = repo
            .save(&rec, &owned(&[("other.yml", b"x\n")]))
            .expect_err("refused");
        assert!(matches!(not_listed, DomainError::InvalidInput(_)));
        // Nothing was written.
        assert!(!dir
            .path()
            .join("links")
            .join(id.as_str())
            .join("link.yml")
            .exists());
    }

    #[test]
    fn a_base_copy_that_does_not_match_the_record_reads_as_missing() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files))
            .expect("saves");
        let copy = dir
            .path()
            .join("links")
            .join(id.as_str())
            .join("base/a.yml");
        std::fs::write(copy, b"tampered\n").expect("tamper");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }

    #[test]
    fn a_damaged_link_is_quarantined_once_and_reads_as_broken() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files))
            .expect("saves");
        let folder = dir.path().join("links").join(id.as_str());
        std::fs::write(
            folder.join("link.yml"),
            "collection_name: Orders\n: : not yaml [",
        )
        .expect("damage");

        let first = repo.get(&id).expect("gets").expect("a stub");
        assert_eq!(first.state, LinkState::Broken);
        assert_eq!(first.link_id, id);
        assert!(first.base.is_empty());
        let second = repo.get(&id).expect("gets").expect("the same stub");
        assert_eq!(second, first);

        let corrupt: Vec<String> = std::fs::read_dir(&folder)
            .expect("lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("link.yml.corrupt-"))
            .collect();
        assert_eq!(corrupt.len(), 1, "one quarantined copy: {corrupt:?}");
        assert_eq!(repo.take_quarantined(), Some("link.yml".to_string()));
        assert_eq!(repo.take_quarantined(), None);
        // A broken link has no base, so no base copy is served.
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }

    #[test]
    fn delete_removes_the_folder() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files))
            .expect("saves");
        repo.delete(&id).expect("deletes");
        assert!(!dir.path().join("links").join(id.as_str()).exists());
        assert_eq!(repo.get(&id).expect("gets"), None);
        repo.delete(&id).expect("a second delete is fine");
    }

    #[test]
    fn path_traversal_in_a_base_path_is_refused() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        for bad in [
            "../evil.yml",
            "/abs.yml",
            "a/../b.yml",
            ".rocket/x.yml",
            "a\\b.yml",
            "",
        ] {
            let rec = record(&id, &[]);
            // The record does not list the path, so a path check must fire first or the
            // manifest check does. Either way the save is refused and nothing escapes.
            let err = repo
                .save(&rec, &owned(&[(bad, b"x")]))
                .expect_err("refused");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{bad}");
            let read = repo.read_base_file(&id, bad).expect_err("refused");
            assert!(matches!(read, DomainError::InvalidInput(_)), "{bad}");
            if !bad.is_empty() {
                assert!(!err.to_string().contains(bad), "save quotes {bad}");
                assert!(!read.to_string().contains(bad), "read quotes {bad}");
            }
        }
        // Only the links folder may exist next to the temp root contents.
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| n == "links"), "{names:?}");
        assert!(!dir.path().join("evil.yml").exists());
    }

    #[test]
    fn a_folder_that_is_not_a_link_id_is_ignored_by_list() {
        let (dir, repo) = repo();
        std::fs::create_dir_all(dir.path().join("links").join("not-a-link")).expect("mkdir");
        std::fs::write(dir.path().join("links").join("stray.txt"), "x").expect("write");
        assert!(repo.list().expect("lists").is_empty());
    }

    #[test]
    fn a_stale_copy_that_differs_only_by_case_is_replaced() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let first: [(&str, &[u8]); 1] = [("Api.yml", b"one\n")];
        repo.save(&record(&id, &first), &owned(&first))
            .expect("saves");
        let second: [(&str, &[u8]); 1] = [("api.yml", b"two\n")];
        repo.save(&record(&id, &second), &owned(&second))
            .expect("saves");
        let base = dir.path().join("links").join(id.as_str()).join("base");
        let names: Vec<String> = std::fs::read_dir(base)
            .expect("lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["api.yml".to_string()]);
        assert_eq!(
            repo.read_base_file(&id, "api.yml").expect("reads"),
            Some(b"two\n".to_vec())
        );
    }

    #[test]
    fn a_path_that_turns_from_a_file_into_a_folder_can_be_saved() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let first: [(&str, &[u8]); 1] = [("users", b"file\n")];
        repo.save(&record(&id, &first), &owned(&first))
            .expect("saves");
        let second: [(&str, &[u8]); 1] = [("users/a.yml", b"a\n")];
        repo.save(&record(&id, &second), &owned(&second))
            .expect("saves after the flip");
        assert_eq!(
            repo.read_base_file(&id, "users/a.yml").expect("reads"),
            Some(b"a\n".to_vec())
        );
    }

    #[test]
    fn a_path_that_turns_from_a_folder_into_a_file_can_be_saved() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let first: [(&str, &[u8]); 1] = [("users/a.yml", b"a\n")];
        repo.save(&record(&id, &first), &owned(&first))
            .expect("saves");
        let second: [(&str, &[u8]); 1] = [("users", b"file\n")];
        repo.save(&record(&id, &second), &owned(&second))
            .expect("saves after the flip");
        assert_eq!(
            repo.read_base_file(&id, "users").expect("reads"),
            Some(b"file\n".to_vec())
        );
    }

    #[test]
    fn an_unreadable_link_does_not_fail_the_list() {
        let (dir, repo) = repo();
        let good = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        let rec = record(&good, &files);
        repo.save(&rec, &owned(&files)).expect("saves");
        let bad = LinkId::random();
        // A folder where the file should be makes the read fail with a non-NotFound error.
        std::fs::create_dir_all(dir.path().join("links").join(bad.as_str()).join("link.yml"))
            .expect("mkdir");
        assert_eq!(repo.list().expect("lists"), vec![rec]);
    }
}
