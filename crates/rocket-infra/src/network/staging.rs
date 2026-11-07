//! Stages a received collection ZIP in a file, chunk by chunk, and hashes it on the way.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use rocket_network::staging::{StagedFile, StagedZip, TransferStaging};
use rocket_network::transfer::TransferId;
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};

/// Staging folder of the receive side: `<dir>/<transfer_id>.part` while chunks arrive and
/// `<dir>/<transfer_id>.zip` once finished.
pub struct FsTransferStaging {
    dir: PathBuf,
}

impl FsTransferStaging {
    /// Does not touch the disk. The folder is created by the first `create`.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

/// The file being written. `file` is `None` once closed.
struct FsStagedZip {
    file: Option<File>,
    part: PathBuf,
    zip: PathBuf,
    hasher: Sha256,
    len: u64,
    finished: bool,
}

impl StagedZip for FsStagedZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| DomainError::Internal("The staged file is closed.".into()))?;
        file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.len += bytes.len() as u64;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> DomainResult<StagedFile> {
        let file = self
            .file
            .take()
            .ok_or_else(|| DomainError::Internal("The staged file is closed.".into()))?;
        file.sync_all()?;
        drop(file);
        // A rename onto an existing file fails on Windows, so a stale one goes first.
        let _ = fs::remove_file(&self.zip);
        fs::rename(&self.part, &self.zip)?;
        self.finished = true;
        Ok(StagedFile {
            path: self.zip.clone(),
            len: self.len,
            sha256: format!("{:x}", self.hasher.clone().finalize()),
        })
    }

    fn discard(self: Box<Self>) {
        // Dropping removes the part file.
        drop(self);
    }
}

impl Drop for FsStagedZip {
    fn drop(&mut self) {
        if !self.finished {
            self.file.take();
            let _ = fs::remove_file(&self.part);
        }
    }
}

impl TransferStaging for FsTransferStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        // The id becomes part of a file name, so it must be a plain 32-char hex id.
        let id = TransferId::parse(transfer_id)?;
        fs::create_dir_all(&self.dir)?;
        let part = self.dir.join(format!("{}.part", id.as_str()));
        let zip = self.dir.join(format!("{}.zip", id.as_str()));

        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&part).map_err(|e| {
            if e.kind() == ErrorKind::AlreadyExists {
                DomainError::AlreadyExists("That transfer is already being received.".into())
            } else {
                DomainError::from(e)
            }
        })?;
        Ok(Box::new(FsStagedZip {
            file: Some(file),
            part,
            zip,
            hasher: Sha256::new(),
            len: 0,
            finished: false,
        }))
    }

    fn release(&self, file: &StagedFile) -> DomainResult<()> {
        // Only a plain `<dir>/<name>.zip` is deleted, so a forged path cannot remove other files.
        let inside = file.path.parent() == Some(self.dir.as_path())
            && file.path.extension().is_some_and(|ext| ext == "zip")
            && file
                .path
                .components()
                .all(|c| !matches!(c, std::path::Component::ParentDir));
        if !inside {
            return Err(DomainError::InvalidInput(
                "The staged file is not in the staging folder.".into(),
            ));
        }
        match fs::remove_file(&file.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn purge(&self) -> DomainResult<()> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            // `file_type` does not follow a symlink, so a link is removed, not its target.
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const OTHER_ID: &str = "fedcba9876543210fedcba9876543210";
    const SHA_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const SHA_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn staging() -> (tempfile::TempDir, FsTransferStaging) {
        let dir = tempfile::tempdir().expect("tempdir");
        let staging = FsTransferStaging::new(dir.path().join("transfers"));
        (dir, staging)
    }

    fn listing(dir: &std::path::Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn new_does_not_create_the_folder() {
        let (dir, _staging) = staging();
        assert!(!dir.path().join("transfers").exists());
    }

    #[test]
    fn a_staged_file_is_finished_with_its_length_and_hash() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"a").expect("append");
        staged.append(b"bc").expect("append");
        let done = staged.finish().expect("finish");

        assert_eq!(done.len, 3);
        assert_eq!(done.sha256, SHA_ABC);
        assert_eq!(
            done.path,
            dir.path().join("transfers").join(format!("{ID}.zip"))
        );
        assert_eq!(fs::read(&done.path).expect("read"), b"abc");
        assert_eq!(
            listing(&dir.path().join("transfers")),
            vec![format!("{ID}.zip")]
        );
    }

    #[test]
    fn an_empty_file_has_the_empty_hash() {
        let (_dir, staging) = staging();
        let done = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        assert_eq!(done.len, 0);
        assert_eq!(done.sha256, SHA_EMPTY);
    }

    #[test]
    fn the_hash_covers_every_chunk() {
        let (_dir, staging) = staging();
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut staged = staging.create(ID).expect("create");
        for chunk in data.chunks(32 * 1024) {
            staged.append(chunk).expect("append");
        }
        let done = staged.finish().expect("finish");

        assert_eq!(done.len, data.len() as u64);
        assert_eq!(done.sha256, format!("{:x}", Sha256::digest(&data)));
        assert_eq!(fs::read(&done.path).expect("read"), data);
    }

    #[test]
    fn the_part_file_exists_while_chunks_arrive() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"x").expect("append");
        assert_eq!(
            listing(&dir.path().join("transfers")),
            vec![format!("{ID}.part")]
        );
        staged.discard();
    }

    #[test]
    fn discard_deletes_the_file() {
        let (dir, staging) = staging();
        let mut staged = staging.create(ID).expect("create");
        staged.append(b"data").expect("append");
        staged.discard();
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn dropping_a_staged_file_deletes_it() {
        let (dir, staging) = staging();
        {
            let mut staged = staging.create(ID).expect("create");
            staged.append(b"data").expect("append");
        }
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn a_finished_file_survives_dropping_the_staging_handle() {
        let (dir, staging) = staging();
        let done = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        assert!(done.path.exists());
        drop(staging);
        assert!(done.path.exists());
        assert_eq!(listing(&dir.path().join("transfers")).len(), 1);
    }

    #[test]
    fn purge_removes_everything_in_the_folder() {
        let (dir, staging) = staging();
        let root = dir.path().join("transfers");
        let _finished = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        let mut open = staging.create(OTHER_ID).expect("create");
        open.append(b"half").expect("append");
        fs::create_dir_all(root.join("stray-dir/inner")).expect("stray dir");
        fs::write(root.join("stray-dir/inner/file"), b"x").expect("stray file");
        fs::write(root.join("note.txt"), b"x").expect("note");

        staging.purge().expect("purge");

        assert!(listing(&root).is_empty());
        assert!(root.exists(), "the folder itself stays");
        // The open file lost its path; dropping it must not fail or panic.
        drop(open);
    }

    #[test]
    fn purge_on_a_missing_folder_is_fine() {
        let (_dir, staging) = staging();
        staging.purge().expect("a missing folder is not an error");
    }

    #[test]
    fn an_id_that_is_not_32_lowercase_hex_chars_is_refused() {
        let (dir, staging) = staging();
        let bad = [
            "",
            "abc",
            "../evil",
            "../../etc/passwd",
            "0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcde/",
            "0123456789abcdef0123456789abcd\\e",
            "0123456789abcdef0123456789abcd\0e",
            "0123456789abcdef0123456789abcdeg",
        ];
        for id in bad {
            assert!(staging.create(id).is_err(), "id {id:?} must be refused");
        }
        assert!(
            !dir.path().join("transfers").exists(),
            "nothing was created"
        );
        assert!(!dir.path().join("evil.part").exists());
    }

    #[test]
    fn a_second_create_for_the_same_id_is_refused_and_keeps_the_first() {
        let (dir, staging) = staging();
        let mut first = staging.create(ID).expect("create");
        first.append(b"first").expect("append");
        assert!(staging.create(ID).is_err());
        let done = first.finish().expect("finish");
        assert_eq!(fs::read(&done.path).expect("read"), b"first");
        assert_eq!(listing(&dir.path().join("transfers")).len(), 1);
    }

    #[test]
    fn two_transfers_stage_independently() {
        let (_dir, staging) = staging();
        let mut a = staging.create(ID).expect("a");
        let mut b = staging.create(OTHER_ID).expect("b");
        a.append(b"aaa").expect("append a");
        b.append(b"bb").expect("append b");
        let done_a = a.finish().expect("finish a");
        let done_b = b.finish().expect("finish b");
        assert_eq!((done_a.len, done_b.len), (3, 2));
        assert_ne!(done_a.sha256, done_b.sha256);
    }

    #[test]
    fn release_deletes_the_staged_file() {
        let (dir, staging) = staging();
        let done = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        assert!(done.path.exists());
        staging.release(&done).expect("release");
        assert!(!done.path.exists());
        assert!(listing(&dir.path().join("transfers")).is_empty());
    }

    #[test]
    fn releasing_twice_is_ok() {
        let (_dir, staging) = staging();
        let done = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        staging.release(&done).expect("first release");
        staging
            .release(&done)
            .expect("a missing file is not an error");
    }

    #[test]
    fn release_refuses_a_path_outside_the_staging_folder() {
        let (dir, staging) = staging();
        let outside = dir.path().join("outside.zip");
        fs::write(&outside, b"keep me").expect("outside file");
        let forged = StagedFile {
            path: outside.clone(),
            len: 7,
            sha256: SHA_ABC.to_string(),
        };
        let err = staging.release(&forged).expect_err("must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
        assert_eq!(fs::read(&outside).expect("read"), b"keep me");

        let climbing = StagedFile {
            path: dir.path().join("transfers").join("..").join("outside.zip"),
            len: 7,
            sha256: SHA_ABC.to_string(),
        };
        assert!(staging.release(&climbing).is_err());
        assert!(outside.exists(), "the outside file survives");
    }

    #[test]
    fn release_does_not_touch_another_transfers_files() {
        let (dir, staging) = staging();
        let first = staging
            .create(ID)
            .expect("create")
            .finish()
            .expect("finish");
        let second = staging
            .create(OTHER_ID)
            .expect("create")
            .finish()
            .expect("finish");
        let mut open = staging
            .create("00000000000000000000000000000001")
            .expect("create");
        open.append(b"half").expect("append");

        staging.release(&first).expect("release");

        assert!(!first.path.exists());
        assert!(second.path.exists());
        assert_eq!(
            listing(&dir.path().join("transfers")),
            vec![
                "00000000000000000000000000000001.part".to_string(),
                format!("{OTHER_ID}.zip"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_staged_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, staging) = staging();
        let _open = staging.create(ID).expect("create");
        let path = dir.path().join("transfers").join(format!("{ID}.part"));
        let mode = fs::metadata(path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o077, 0, "mode was {mode:o}");
    }
}
