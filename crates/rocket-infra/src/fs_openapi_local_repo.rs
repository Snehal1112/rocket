//! Stores the local OpenAPI sync state at `<collection>/.rocket/openapi-local.json`.

use std::path::{Path, PathBuf};

use rocket_collection::openapi_local::{
    OpenApiLocalRepository, OpenApiLocalState, LOCAL_STATE_FILE_VERSION,
};
use rocket_collection::openapi_source::SyncInterval;
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// The filesystem implementation. It holds no state; every call is given the collection root.
pub struct FsOpenApiLocalRepo;

const LOCAL_FILE: &str = "openapi-local.json";

fn local_path(root: &Path) -> PathBuf {
    root.join(".rocket").join(LOCAL_FILE)
}

impl OpenApiLocalRepository for FsOpenApiLocalRepo {
    fn load(&self, root: &Path) -> DomainResult<Option<OpenApiLocalState>> {
        let text = match std::fs::read_to_string(local_path(root)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(DomainError::Internal(format!(
                    "{LOCAL_FILE} could not be read: {e}"
                )))
            }
        };
        // Read the version first, so a newer file is reported as that.
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("{LOCAL_FILE} cannot be parsed: {e}")))?;
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        if version.is_some_and(|v| v > u64::from(LOCAL_STATE_FILE_VERSION)) {
            return Err(DomainError::Internal(format!(
                "{LOCAL_FILE} was written by a newer version of Rocket (it reads up to version {LOCAL_STATE_FILE_VERSION})"
            )));
        }
        let unknown_interval = value
            .get("sync_interval")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| SyncInterval::parse(name).is_none());
        let mut state: OpenApiLocalState = serde_json::from_value(value)
            .map_err(|e| DomainError::Internal(format!("{LOCAL_FILE} cannot be parsed: {e}")))?;
        state.interval_unknown = unknown_interval;
        Ok(Some(state))
    }

    fn save(&self, root: &Path, state: &OpenApiLocalState) -> DomainResult<()> {
        let json = serde_json::to_vec_pretty(state).map_err(|e| {
            DomainError::Internal(format!("{LOCAL_FILE} could not be written: {e}"))
        })?;
        // Ignore the file first, so a failure never leaves an unignored copy on disk.
        crate::rocket_dir::ensure_gitignore(root)?;
        atomic_write(&local_path(root), &json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample() -> OpenApiLocalState {
        let mut state = OpenApiLocalState::default();
        state
            .skips
            .insert("listOrders".to_string(), "abc123".to_string());
        state
    }

    #[test]
    fn skips_round_trip() {
        let dir = tempdir().expect("tempdir");
        FsOpenApiLocalRepo
            .save(dir.path(), &sample())
            .expect("save");
        assert_eq!(
            FsOpenApiLocalRepo.load(dir.path()).expect("load"),
            Some(sample())
        );
    }

    #[test]
    fn an_unknown_interval_loads_as_off_and_is_flagged() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("dir");
        let path = dir.path().join(".rocket/openapi-local.json");
        std::fs::write(&path, r#"{"version":1,"sync_interval":"every_2_hours"}"#).expect("write");
        let state = FsOpenApiLocalRepo
            .load(dir.path())
            .expect("load")
            .expect("some");
        assert_eq!(state.sync_interval, SyncInterval::Off);
        assert!(state.interval_unknown);
        std::fs::write(&path, r#"{"version":1,"sync_interval":"every_5_minutes"}"#).expect("write");
        let state = FsOpenApiLocalRepo
            .load(dir.path())
            .expect("load")
            .expect("some");
        assert_eq!(state.sync_interval, SyncInterval::Min5);
        assert!(!state.interval_unknown);
        // The flag is never written back.
        FsOpenApiLocalRepo.save(dir.path(), &state).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(!text.contains("interval_unknown"), "{text}");
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(FsOpenApiLocalRepo.load(dir.path()).expect("load"), None);
    }

    #[test]
    fn newer_version_is_an_error() {
        let dir = tempdir().expect("tempdir");
        let mut state = sample();
        state.version = LOCAL_STATE_FILE_VERSION + 98;
        FsOpenApiLocalRepo.save(dir.path(), &state).expect("save");
        let err = FsOpenApiLocalRepo.load(dir.path()).expect_err("newer");
        assert!(err.to_string().contains("newer version"), "{err}");
    }

    #[test]
    fn unreadable_file_is_an_error() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".rocket")).expect("dir");
        std::fs::write(dir.path().join(".rocket/openapi-local.json"), "{nope").expect("write");
        assert!(FsOpenApiLocalRepo.load(dir.path()).is_err());
    }

    #[test]
    fn saving_writes_the_gitignore_first() {
        let dir = tempdir().expect("tempdir");
        // A folder at the file's path makes the write fail after the gitignore step.
        std::fs::create_dir_all(dir.path().join(".rocket/openapi-local.json")).expect("dir");
        assert!(FsOpenApiLocalRepo.save(dir.path(), &sample()).is_err());
        let ignore =
            std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("ignore");
        assert!(
            ignore.lines().any(|l| l == "openapi-local.json"),
            "{ignore}"
        );
    }

    #[test]
    fn gitignore_lists_the_new_names() {
        let dir = tempdir().expect("tempdir");
        crate::rocket_dir::ensure_gitignore(dir.path()).expect("ignore");
        let ignore =
            std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("ignore");
        for name in ["openapi-local.json", "apply-journal.json", "staging/"] {
            assert!(
                ignore.lines().any(|l| l == name),
                "{name} missing: {ignore}"
            );
        }
    }
}
