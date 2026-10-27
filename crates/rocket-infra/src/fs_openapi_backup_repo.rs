//! Stores the reset backups at `<collection>/.rocket/openapi-reset-backups.json`.

use std::path::{Path, PathBuf};

use rocket_collection::openapi_backup::{
    OpenApiBackupRepository, OpenApiBackups, BACKUP_FILE_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// The filesystem implementation. It holds no state; every call is given the collection root.
pub struct FsOpenApiBackupRepo;

const BACKUP_FILE: &str = "openapi-reset-backups.json";

fn backup_path(root: &Path) -> PathBuf {
    root.join(".rocket").join(BACKUP_FILE)
}

impl OpenApiBackupRepository for FsOpenApiBackupRepo {
    fn load(&self, root: &Path) -> DomainResult<Option<OpenApiBackups>> {
        let text = match std::fs::read_to_string(backup_path(root)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(DomainError::Internal(format!(
                    "{BACKUP_FILE} could not be read: {e}"
                )))
            }
        };
        // Read the version first, so a newer file is reported as that.
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("{BACKUP_FILE} cannot be parsed: {e}")))?;
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        if version.is_some_and(|v| v > u64::from(BACKUP_FILE_VERSION)) {
            return Err(DomainError::Internal(format!(
                "{BACKUP_FILE} was written by a newer version of Rocket (it reads up to version {BACKUP_FILE_VERSION})"
            )));
        }
        let backups: OpenApiBackups = serde_json::from_value(value)
            .map_err(|e| DomainError::Internal(format!("{BACKUP_FILE} cannot be parsed: {e}")))?;
        Ok(Some(backups))
    }

    fn save(&self, root: &Path, backups: &OpenApiBackups) -> DomainResult<()> {
        let json = serde_json::to_vec_pretty(backups).map_err(|e| {
            DomainError::Internal(format!("{BACKUP_FILE} could not be written: {e}"))
        })?;
        // Ignore the file first, so a failure never leaves an unignored copy on disk.
        crate::rocket_dir::ensure_gitignore(root)?;
        atomic_write(&backup_path(root), &json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::openapi_backup::BackupEntry;
    use rocket_collection::Request;
    use rocket_shared::types::HttpMethod;
    use tempfile::tempdir;

    fn sample() -> OpenApiBackups {
        OpenApiBackups {
            version: BACKUP_FILE_VERSION,
            entries: vec![BackupEntry {
                key: "listPets".to_string(),
                uid: "u1".to_string(),
                request_path: "pets/list".to_string(),
                saved_at: "2026-10-27T10:00:00+00:00".to_string(),
                request: Request::new("List pets", HttpMethod::Get, "{{baseUrl}}/pets"),
            }],
        }
    }

    #[test]
    fn load_returns_none_for_a_missing_file() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(FsOpenApiBackupRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().expect("tempdir");
        let backups = sample();
        FsOpenApiBackupRepo
            .save(dir.path(), &backups)
            .expect("save");
        assert_eq!(
            FsOpenApiBackupRepo.load(dir.path()).expect("load"),
            Some(backups)
        );
    }

    #[test]
    fn a_newer_version_is_an_error_not_defaults() {
        let dir = tempdir().expect("tempdir");
        let mut backups = sample();
        backups.version = BACKUP_FILE_VERSION + 98;
        FsOpenApiBackupRepo
            .save(dir.path(), &backups)
            .expect("save");
        let err = FsOpenApiBackupRepo
            .load(dir.path())
            .expect_err("a newer file must not load");
        assert!(err.to_string().contains("newer"), "{err}");
    }

    #[test]
    fn a_corrupt_file_is_an_error_not_defaults() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("mkdir");
        std::fs::write(
            dir.path().join(".rocket/openapi-reset-backups.json"),
            "{oops",
        )
        .expect("write");
        assert!(FsOpenApiBackupRepo.load(dir.path()).is_err());
    }

    #[test]
    fn save_ignores_the_file_in_git() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiBackupRepo
            .save(dir.path(), &sample())
            .expect("save");
        let ignore = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("gi");
        assert!(
            ignore.lines().any(|l| l == "openapi-reset-backups.json"),
            "{ignore}"
        );
    }
}
