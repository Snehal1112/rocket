//! `openapi-sync.yml` in the app data directory: where the user approved OpenAPI sync.
//!
//! Records are keyed by the canonical collection folder and the source id. The file is read
//! on every lookup. A file that cannot be read fails closed: every lookup answers `None`, and
//! a write refuses to replace it, so a damaged file is never overwritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rocket_collection::{consent_root_key, OpenApiConsent, OpenApiConsentStore};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::atomic_write;

const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConsentFile {
    version: u32,
    #[serde(default)]
    sources: Vec<ConsentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConsentRecord {
    root: String,
    source_id: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    granted_at: String,
}

enum Loaded {
    Missing,
    Ok(ConsentFile),
    /// Present but unreadable, or from an unknown version.
    Corrupt,
}

pub struct FsOpenApiConsentRepo {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl FsOpenApiConsentRepo {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Loaded {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
            Err(_) => return Loaded::Corrupt,
        };
        match serde_yaml::from_str::<ConsentFile>(&text) {
            Ok(file) if file.version == FILE_VERSION => Loaded::Ok(file),
            _ => Loaded::Corrupt,
        }
    }

    fn unreadable() -> DomainError {
        DomainError::Internal("The OpenAPI sync approvals could not be read".into())
    }

    /// Read-modify-write under the store lock. A damaged file is left as it is.
    fn update(&self, change: impl FnOnce(&mut ConsentFile)) -> DomainResult<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = match self.load() {
            Loaded::Ok(file) => file,
            Loaded::Missing => ConsentFile {
                version: FILE_VERSION,
                sources: Vec::new(),
            },
            Loaded::Corrupt => return Err(Self::unreadable()),
        };
        change(&mut file);
        let yaml = serde_yaml::to_string(&file).map_err(|_| {
            DomainError::Internal("Failed to serialize the OpenAPI sync approvals".into())
        })?;
        atomic_write(&self.path, yaml.as_bytes())?;
        Ok(())
    }
}

impl OpenApiConsentStore for FsOpenApiConsentRepo {
    fn get(&self, root: &Path, source_id: &str) -> DomainResult<Option<OpenApiConsent>> {
        let key = consent_root_key(root);
        Ok(match self.load() {
            Loaded::Ok(file) => file
                .sources
                .into_iter()
                .find(|r| r.root == key && r.source_id == source_id)
                .map(|r| OpenApiConsent {
                    source_id: r.source_id,
                    location: r.location,
                    granted_at: r.granted_at,
                }),
            // Missing and damaged files both mean: nothing is approved.
            Loaded::Missing | Loaded::Corrupt => None,
        })
    }

    fn grant(&self, root: &Path, source_id: &str, location: &str) -> DomainResult<()> {
        let key = consent_root_key(root);
        self.update(|file| {
            file.sources
                .retain(|r| !(r.root == key && r.source_id == source_id));
            file.sources.push(ConsentRecord {
                root: key.clone(),
                source_id: source_id.to_string(),
                location: location.to_string(),
                granted_at: chrono::Utc::now().to_rfc3339(),
            });
        })
    }

    fn revoke(&self, root: &Path, source_id: &str) -> DomainResult<()> {
        let key = consent_root_key(root);
        match self.load() {
            Loaded::Missing => return Ok(()),
            Loaded::Corrupt => return Err(Self::unreadable()),
            Loaded::Ok(_) => {}
        }
        self.update(|file| {
            file.sources
                .retain(|r| !(r.root == key && r.source_id == source_id));
        })
    }
}

#[cfg(test)]
mod openapi_consent_tests {
    use super::*;
    use tempfile::TempDir;

    fn repo(dir: &TempDir) -> FsOpenApiConsentRepo {
        FsOpenApiConsentRepo::new(dir.path().join("openapi-sync.yml"))
    }

    #[test]
    fn grant_then_get_returns_the_location() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        let store = repo(&dir);
        assert!(store.get(root.path(), "s1").expect("get").is_none());
        store
            .grant(root.path(), "s1", "https://github.com/o/r/blob/main/a.json")
            .expect("grant");
        let got = store.get(root.path(), "s1").expect("get").expect("record");
        assert_eq!(got.location, "https://github.com/o/r/blob/main/a.json");
        assert_eq!(got.source_id, "s1");
        assert!(!got.granted_at.is_empty());
        // Another source id or root has no record.
        assert!(store.get(root.path(), "s2").expect("get").is_none());
        let other = TempDir::new().expect("other");
        assert!(store.get(other.path(), "s1").expect("get").is_none());
    }

    #[test]
    fn grant_replaces_the_earlier_record() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        let store = repo(&dir);
        store.grant(root.path(), "s1", "one").expect("grant");
        store.grant(root.path(), "s1", "two").expect("grant");
        let got = store.get(root.path(), "s1").expect("get").expect("record");
        assert_eq!(got.location, "two");
        let text = fs::read_to_string(dir.path().join("openapi-sync.yml")).expect("read");
        assert_eq!(text.matches("source_id: s1").count(), 1);
    }

    #[test]
    fn revoke_removes_the_record() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        let store = repo(&dir);
        store.grant(root.path(), "s1", "one").expect("grant");
        store.revoke(root.path(), "s1").expect("revoke");
        assert!(store.get(root.path(), "s1").expect("get").is_none());
        // Revoking again, or with no file at all, is fine.
        store.revoke(root.path(), "s1").expect("revoke again");
        let empty = TempDir::new().expect("empty");
        repo(&empty).revoke(root.path(), "s1").expect("no file");
    }

    #[test]
    fn a_record_survives_a_new_store_on_the_same_file() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        repo(&dir).grant(root.path(), "s1", "one").expect("grant");
        assert!(repo(&dir).get(root.path(), "s1").expect("get").is_some());
    }

    #[test]
    fn a_corrupt_file_fails_closed_and_is_not_overwritten() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        let path = dir.path().join("openapi-sync.yml");
        fs::write(&path, "{{{ not yaml").expect("write");
        let store = repo(&dir);
        assert!(store.get(root.path(), "s1").expect("get").is_none());
        assert!(store.grant(root.path(), "s1", "one").is_err());
        assert!(store.revoke(root.path(), "s1").is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{{{ not yaml");
        assert!(store.get(root.path(), "s1").expect("get").is_none());
    }

    #[test]
    fn an_unknown_version_fails_closed_and_is_kept() {
        let dir = TempDir::new().expect("dir");
        let root = TempDir::new().expect("root");
        let path = dir.path().join("openapi-sync.yml");
        let text = "version: 9\nsources: []\n";
        fs::write(&path, text).expect("write");
        let store = repo(&dir);
        assert!(store.grant(root.path(), "s1", "one").is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }
}
