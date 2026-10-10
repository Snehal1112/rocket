//! `trust.yml` in the app data directory: what the user allowed per collection.
//!
//! Persistence structs use snake_case keys and no `rename_all`. The file is read on every
//! check, so a grant made in another window is seen at once. A file that cannot be read
//! fails closed: nothing is granted.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rocket_collection::{
    CollectionGrant, CollectionIdentity, CollectionTrustStore, GrantSource, MigrationNoticeEntry,
};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::atomic_write;

const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustFile {
    version: u32,
    #[serde(default)]
    migrated: bool,
    #[serde(default)]
    migration_notice: Vec<NoticeRecord>,
    #[serde(default)]
    collections: Vec<CollectionRecord>,
}

impl TrustFile {
    fn empty(migrated: bool) -> Self {
        Self {
            version: FILE_VERSION,
            migrated,
            migration_notice: Vec::new(),
            collections: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoticeRecord {
    root: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollectionRecord {
    root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uid: Option<String>,
    #[serde(default)]
    developer_mode: bool,
    #[serde(default)]
    context_roots: Vec<String>,
    #[serde(default)]
    agent_run: bool,
    #[serde(default)]
    process_env: bool,
    #[serde(default)]
    source: String,
    #[serde(default)]
    updated_at: String,
}

impl CollectionRecord {
    fn new(id: &CollectionIdentity, grant: CollectionGrant) -> Self {
        Self {
            root: root_text(id),
            uid: id.uid.clone(),
            developer_mode: grant.developer_mode,
            context_roots: grant.context_roots,
            agent_run: grant.agent_run,
            process_env: grant.process_env,
            source: source_text(grant.source).to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn matches(&self, id: &CollectionIdentity) -> bool {
        self.root == root_text(id) && (self.uid.is_none() || self.uid == id.uid)
    }

    fn to_grant(&self) -> CollectionGrant {
        CollectionGrant {
            developer_mode: self.developer_mode,
            context_roots: self.context_roots.clone(),
            agent_run: self.agent_run,
            process_env: self.process_env,
            source: match self.source.as_str() {
                "created" => GrantSource::Created,
                "migrated" => GrantSource::Migrated,
                _ => GrantSource::User,
            },
        }
    }
}

fn root_text(id: &CollectionIdentity) -> String {
    id.canonical_root.to_string_lossy().into_owned()
}

fn source_text(source: GrantSource) -> &'static str {
    match source {
        GrantSource::User => "user",
        GrantSource::Created => "created",
        GrantSource::Migrated => "migrated",
    }
}

/// What reading the file found.
enum Loaded {
    Missing,
    Ok(TrustFile),
    /// Present but unreadable, or from an unknown version.
    Corrupt,
}

pub struct FsCollectionTrustStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl FsCollectionTrustStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    fn load(&self) -> DomainResult<Loaded> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::Missing),
            // Present but unreadable (for example a bad encoding or permissions).
            Err(_) => return Ok(Loaded::Corrupt),
        };
        match serde_yaml::from_str::<TrustFile>(&text) {
            Ok(file) if file.version == FILE_VERSION => Ok(Loaded::Ok(file)),
            _ => Ok(Loaded::Corrupt),
        }
    }

    fn unreadable() -> DomainError {
        DomainError::Internal("The trust settings could not be read".into())
    }

    fn save(&self, file: &TrustFile) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(file)
            .map_err(|_| DomainError::Internal("Failed to serialize the trust settings".into()))?;
        atomic_write(&self.path, yaml.as_bytes())?;
        Ok(())
    }

    /// Read-modify-write under the store lock. A corrupt file is moved aside first and
    /// replaced with a fresh one that counts as migrated, so grandfathering never re-runs.
    fn update<T>(&self, change: impl FnOnce(&mut TrustFile) -> T) -> DomainResult<T> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = match self.load()? {
            Loaded::Ok(file) => file,
            Loaded::Missing => TrustFile::empty(false),
            Loaded::Corrupt => {
                self.quarantine(&self.path)?;
                TrustFile::empty(true)
            }
        };
        let out = change(&mut file);
        self.save(&file)?;
        Ok(out)
    }

    fn quarantine(&self, path: &Path) -> DomainResult<()> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut name = path.as_os_str().to_os_string();
        name.push(format!(".corrupt-{secs}"));
        fs::rename(path, PathBuf::from(name))?;
        Ok(())
    }
}

impl CollectionTrustStore for FsCollectionTrustStore {
    fn grant_for(&self, id: &CollectionIdentity) -> DomainResult<Option<CollectionGrant>> {
        match self.load()? {
            Loaded::Missing => Ok(None),
            Loaded::Corrupt => Err(Self::unreadable()),
            Loaded::Ok(file) => Ok(file
                .collections
                .iter()
                .find(|r| r.matches(id))
                .map(CollectionRecord::to_grant)),
        }
    }

    fn put(&self, id: &CollectionIdentity, grant: CollectionGrant) -> DomainResult<()> {
        self.update(|file| {
            let root = root_text(id);
            file.collections.retain(|r| r.root != root);
            file.collections.push(CollectionRecord::new(id, grant));
        })
    }

    fn remove(&self, id: &CollectionIdentity) -> DomainResult<()> {
        match self.load()? {
            Loaded::Missing => return Ok(()),
            Loaded::Corrupt => return Err(Self::unreadable()),
            Loaded::Ok(_) => {}
        }
        self.update(|file| file.collections.retain(|r| !r.matches(id)))
    }

    fn rekey(&self, old: &CollectionIdentity, new: &CollectionIdentity) -> DomainResult<()> {
        match self.load()? {
            Loaded::Missing => return Ok(()),
            Loaded::Corrupt => return Err(Self::unreadable()),
            Loaded::Ok(_) => {}
        }
        self.update(|file| {
            let Some(index) = file.collections.iter().position(|r| r.matches(old)) else {
                return;
            };
            let mut record = file.collections.remove(index);
            let new_root = root_text(new);
            file.collections.retain(|r| r.root != new_root);
            record.root = new_root;
            record.uid = new.uid.clone();
            record.updated_at = chrono::Utc::now().to_rfc3339();
            file.collections.push(record);
        })
    }

    fn migrated(&self) -> DomainResult<bool> {
        Ok(match self.load()? {
            Loaded::Missing => false,
            Loaded::Ok(file) => file.migrated,
            // A file exists, so the migration never runs again.
            Loaded::Corrupt => true,
        })
    }

    fn complete_migration(
        &self,
        grants: Vec<(CollectionIdentity, CollectionGrant)>,
        notice: Vec<MigrationNoticeEntry>,
    ) -> DomainResult<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = match self.load()? {
            Loaded::Ok(file) if file.migrated => return Ok(()),
            Loaded::Ok(file) => file,
            Loaded::Missing => TrustFile::empty(false),
            Loaded::Corrupt => return Ok(()),
        };
        for (id, grant) in grants {
            // A grant the user already made before the migration ran is kept.
            if file.collections.iter().any(|r| r.root == root_text(&id)) {
                continue;
            }
            file.collections.push(CollectionRecord::new(&id, grant));
        }
        file.migration_notice = notice
            .into_iter()
            .map(|n| NoticeRecord {
                root: n.root,
                capabilities: n.capabilities,
            })
            .collect();
        file.migrated = true;
        self.save(&file)
    }

    fn migration_notice(&self) -> DomainResult<Vec<MigrationNoticeEntry>> {
        Ok(match self.load()? {
            Loaded::Ok(file) => file
                .migration_notice
                .into_iter()
                .map(|n| MigrationNoticeEntry {
                    root: n.root,
                    capabilities: n.capabilities,
                })
                .collect(),
            Loaded::Missing | Loaded::Corrupt => Vec::new(),
        })
    }

    fn dismiss_migration_notice(&self) -> DomainResult<()> {
        match self.load()? {
            Loaded::Missing => return Ok(()),
            Loaded::Corrupt => return Err(Self::unreadable()),
            Loaded::Ok(file) if file.migration_notice.is_empty() => return Ok(()),
            Loaded::Ok(_) => {}
        }
        self.update(|file| file.migration_notice.clear())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(root: &str, uid: Option<&str>) -> CollectionIdentity {
        CollectionIdentity {
            canonical_root: PathBuf::from(root),
            uid: uid.map(str::to_string),
        }
    }

    fn dev_grant() -> CollectionGrant {
        CollectionGrant {
            developer_mode: true,
            context_roots: vec!["../shared".into()],
            agent_run: false,
            process_env: true,
            source: GrantSource::User,
        }
    }

    fn store() -> (tempfile::TempDir, FsCollectionTrustStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FsCollectionTrustStore::new(dir.path().join("trust.yml"));
        (dir, store)
    }

    #[test]
    fn missing_file_grants_nothing_and_is_not_migrated() {
        let (_dir, store) = store();
        assert_eq!(store.grant_for(&id("/a", None)).expect("read"), None);
        assert!(!store.migrated().expect("migrated"));
    }

    #[test]
    fn a_grant_round_trips_with_snake_case_keys() {
        let (dir, store) = store();
        store.put(&id("/a", Some("u1")), dev_grant()).expect("put");
        assert_eq!(
            store.grant_for(&id("/a", Some("u1"))).expect("read"),
            Some(dev_grant())
        );
        let yaml = fs::read_to_string(dir.path().join("trust.yml")).expect("file");
        assert!(yaml.contains("developer_mode: true"), "{yaml}");
        assert!(yaml.contains("version: 1"), "{yaml}");
        assert!(!yaml.contains("developerMode"), "{yaml}");
    }

    #[test]
    fn a_different_uid_does_not_match() {
        let (_dir, store) = store();
        store.put(&id("/a", Some("u1")), dev_grant()).expect("put");
        assert_eq!(store.grant_for(&id("/a", Some("u2"))).expect("read"), None);
        assert_eq!(store.grant_for(&id("/a", None)).expect("read"), None);
        assert_eq!(store.grant_for(&id("/b", Some("u1"))).expect("read"), None);
    }

    #[test]
    fn a_record_without_a_uid_matches_on_path_alone() {
        let (_dir, store) = store();
        store.put(&id("/a", None), dev_grant()).expect("put");
        assert!(store
            .grant_for(&id("/a", Some("later")))
            .expect("read")
            .is_some());
    }

    #[test]
    fn put_replaces_the_record_for_the_same_root() {
        let (_dir, store) = store();
        store.put(&id("/a", Some("u1")), dev_grant()).expect("put");
        store
            .put(&id("/a", Some("u2")), CollectionGrant::default())
            .expect("put");
        assert_eq!(store.grant_for(&id("/a", Some("u1"))).expect("read"), None);
        assert!(store.grant_for(&id("/a", Some("u2"))).expect("read").is_some());
    }

    #[test]
    fn remove_and_rekey() {
        let (_dir, store) = store();
        store.put(&id("/a", Some("u1")), dev_grant()).expect("put");
        store
            .rekey(&id("/a", Some("u1")), &id("/b", Some("u1")))
            .expect("rekey");
        assert_eq!(store.grant_for(&id("/a", Some("u1"))).expect("read"), None);
        assert!(store.grant_for(&id("/b", Some("u1"))).expect("read").is_some());
        store.remove(&id("/b", Some("u1"))).expect("remove");
        assert_eq!(store.grant_for(&id("/b", Some("u1"))).expect("read"), None);
    }

    #[test]
    fn corrupt_file_fails_closed_and_counts_as_migrated() {
        let (dir, store) = store();
        fs::write(dir.path().join("trust.yml"), "{{{ nope").expect("write");
        assert!(store.grant_for(&id("/a", None)).is_err());
        assert!(store.migrated().expect("migrated"));
        assert!(store.migration_notice().expect("notice").is_empty());
    }

    #[test]
    fn unknown_version_fails_closed() {
        let (dir, store) = store();
        fs::write(
            dir.path().join("trust.yml"),
            "version: 99\nmigrated: true\ncollections: []\n",
        )
        .expect("write");
        assert!(store.grant_for(&id("/a", None)).is_err());
    }

    #[test]
    fn next_write_quarantines_a_corrupt_file() {
        let (dir, store) = store();
        fs::write(dir.path().join("trust.yml"), "{{{ nope").expect("write");
        store.put(&id("/a", None), dev_grant()).expect("put");
        assert!(store.grant_for(&id("/a", None)).expect("read").is_some());
        assert!(store.migrated().expect("migrated"));
        let quarantined = fs::read_dir(dir.path())
            .expect("dir")
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("trust.yml.corrupt-"));
        assert!(quarantined);
    }

    #[test]
    fn migration_runs_once_and_fills_the_notice() {
        let (_dir, store) = store();
        let notice = vec![MigrationNoticeEntry {
            root: "/a".into(),
            capabilities: vec!["developerMode".into()],
        }];
        store
            .complete_migration(vec![(id("/a", None), dev_grant())], notice.clone())
            .expect("migrate");
        assert!(store.migrated().expect("migrated"));
        assert!(store.grant_for(&id("/a", None)).expect("read").is_some());
        assert_eq!(store.migration_notice().expect("notice"), notice);

        // A second run changes nothing.
        store
            .complete_migration(vec![(id("/b", None), dev_grant())], Vec::new())
            .expect("again");
        assert_eq!(store.grant_for(&id("/b", None)).expect("read"), None);
        assert_eq!(store.migration_notice().expect("notice"), notice);

        store.dismiss_migration_notice().expect("dismiss");
        assert!(store.migration_notice().expect("notice").is_empty());
    }

    #[test]
    fn migration_keeps_a_grant_made_before_it_ran() {
        let (_dir, store) = store();
        store
            .put(&id("/a", None), CollectionGrant::default())
            .expect("put");
        assert!(!store.migrated().expect("migrated"));
        store
            .complete_migration(vec![(id("/a", None), dev_grant())], Vec::new())
            .expect("migrate");
        assert_eq!(
            store.grant_for(&id("/a", None)).expect("read"),
            Some(CollectionGrant::default())
        );
    }
}
