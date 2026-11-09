//! In-memory fakes of the sync ports, for the `SyncService` tests of Plan 3.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use rocket_network::link::{LinkId, LinkRecord, Manifest};
use rocket_network::ports::LinkRepository;
use rocket_shared::error::{DomainError, DomainResult};

use super::sync_collections::plan_changes;
use super::sync_ports::{sha256_hex, FileChange, SyncCollectionPort, SyncFile};

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn missing() -> DomainError {
    DomainError::NotFound("The collection is not there.".into())
}

struct FakeCollection {
    uid: Option<String>,
    /// The exported (scrubbed) view of each file.
    files: BTreeMap<String, Vec<u8>>,
}

/// Collections kept in memory. `apply` plans with the same code as the real port (so the
/// order, the merge and the case rule match) and swaps all files at once, so it is atomic.
/// `fail_next_apply` makes the next apply fail before any change.
#[derive(Default)]
pub(crate) struct FakeSyncCollections {
    state: Mutex<HashMap<String, FakeCollection>>,
    fail_apply: AtomicBool,
    applied: Mutex<Vec<(String, Vec<FileChange>)>>,
}

impl FakeSyncCollections {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&self, name: &str, uid: Option<&str>, files: &[(&str, &str)]) {
        let files = files
            .iter()
            .map(|(path, text)| (path.to_string(), text.as_bytes().to_vec()))
            .collect();
        locked(&self.state).insert(
            name.to_string(),
            FakeCollection {
                uid: uid.map(str::to_string),
                files,
            },
        );
    }

    pub(crate) fn set_file(&self, name: &str, path: &str, text: &str) {
        if let Some(c) = locked(&self.state).get_mut(name) {
            c.files.insert(path.to_string(), text.as_bytes().to_vec());
        }
    }

    pub(crate) fn remove_file(&self, name: &str, path: &str) {
        if let Some(c) = locked(&self.state).get_mut(name) {
            c.files.remove(path);
        }
    }

    pub(crate) fn file(&self, name: &str, path: &str) -> Option<Vec<u8>> {
        locked(&self.state)
            .get(name)
            .and_then(|c| c.files.get(path).cloned())
    }

    pub(crate) fn paths(&self, name: &str) -> Vec<String> {
        locked(&self.state)
            .get(name)
            .map(|c| c.files.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn fail_next_apply(&self) {
        self.fail_apply.store(true, Ordering::SeqCst);
    }

    pub(crate) fn applied(&self) -> Vec<(String, Vec<FileChange>)> {
        locked(&self.applied).clone()
    }

    fn manifest_of(files: &BTreeMap<String, Vec<u8>>) -> DomainResult<Manifest> {
        let mut manifest = Manifest::new();
        for (path, bytes) in files {
            manifest.insert(path, &sha256_hex(bytes))?;
        }
        Ok(manifest)
    }

    fn apply_inner(
        &self,
        collection: &str,
        expected: Option<&Manifest>,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        locked(&self.applied).push((collection.to_string(), changes.to_vec()));
        if self.fail_apply.swap(false, Ordering::SeqCst) {
            return Err(DomainError::Internal(
                "The fake apply was told to fail.".into(),
            ));
        }
        let mut state = locked(&self.state);
        let c = state.get_mut(collection).ok_or_else(missing)?;
        if let Some(expected) = expected {
            if &Self::manifest_of(&c.files)? != expected {
                return Err(DomainError::Conflict(
                    "The collection changed while the sync was running.".into(),
                ));
            }
        }
        // The fake has no other files on disk, so nothing else can hold a path.
        let overlay = plan_changes(&c.files, changes, &|_| Ok(false))?;
        for (path, entry) in overlay {
            match entry {
                Some(bytes) => {
                    c.files.insert(path, bytes);
                }
                None => {
                    c.files.remove(&path);
                }
            }
        }
        Ok(())
    }
}

impl SyncCollectionPort for FakeSyncCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(locked(&self.state).contains_key(collection))
    }

    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>> {
        let state = locked(&self.state);
        Ok(state.get(collection).ok_or_else(missing)?.uid.clone())
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        let state = locked(&self.state);
        state.get(collection).ok_or_else(missing)?;
        Ok(format!("/fake/{collection}"))
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        let state = locked(&self.state);
        Self::manifest_of(&state.get(collection).ok_or_else(missing)?.files)
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let state = locked(&self.state);
        let c = state.get(collection).ok_or_else(missing)?;
        paths
            .iter()
            .map(|path| match c.files.get(path) {
                Some(bytes) => Ok(SyncFile {
                    path: path.clone(),
                    bytes: bytes.clone(),
                }),
                None => Err(DomainError::NotFound(
                    "A requested file is not in the collection.".into(),
                )),
            })
            .collect()
    }

    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()> {
        self.apply_inner(collection, None, changes)
    }

    fn apply_if_unchanged(
        &self,
        collection: &str,
        expected: &Manifest,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        self.apply_inner(collection, Some(expected), changes)
    }
}

/// Links kept in memory with the delta rule of `LinkRepository::save`.
#[derive(Default)]
pub(crate) struct FakeLinkRepo {
    links: Mutex<HashMap<String, (LinkRecord, HashMap<String, Vec<u8>>)>>,
}

impl FakeLinkRepo {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl LinkRepository for FakeLinkRepo {
    fn list(&self) -> DomainResult<Vec<LinkRecord>> {
        let links = locked(&self.links);
        let mut records: Vec<LinkRecord> = links.values().map(|(r, _)| r.clone()).collect();
        records.sort_by(|a, b| a.link_id.as_str().cmp(b.link_id.as_str()));
        Ok(records)
    }

    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        Ok(locked(&self.links).get(id.as_str()).map(|(r, _)| r.clone()))
    }

    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
        // Every copy is checked before anything is written.
        for (path, bytes) in base_files {
            let expected = record.base.get(path).ok_or_else(|| {
                DomainError::InvalidInput("A base copy is not in the base.".into())
            })?;
            if sha256_hex(bytes) != expected {
                return Err(DomainError::InvalidInput(
                    "A base copy does not match its hash.".into(),
                ));
            }
        }
        let mut links = locked(&self.links);
        let mut kept = links
            .remove(record.link_id.as_str())
            .map(|(_, files)| files)
            .unwrap_or_default();
        kept.retain(|path, _| record.base.get(path).is_some());
        for (path, bytes) in base_files {
            kept.insert(path.clone(), bytes.clone());
        }
        links.insert(record.link_id.as_str().to_string(), (record.clone(), kept));
        Ok(())
    }

    fn delete(&self, id: &LinkId) -> DomainResult<()> {
        locked(&self.links).remove(id.as_str());
        Ok(())
    }

    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
        let links = locked(&self.links);
        let Some((record, files)) = links.get(id.as_str()) else {
            return Ok(None);
        };
        // A copy that no longer matches the base hash reads as none.
        Ok(files
            .get(path)
            .filter(|bytes| record.base.get(path) == Some(sha256_hex(bytes).as_str()))
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_network::link::{LinkId, LinkRecord, LinkState, Manifest};
    use rocket_network::ports::LinkRepository;
    use rocket_network::DeviceId;
    use rocket_shared::error::DomainError;

    use crate::network::sync_ports::{sha256_hex, FileChange, SyncCollectionPort};

    #[test]
    fn the_fake_apply_merges_like_the_real_one_and_is_atomic() {
        let fake = FakeSyncCollections::new();
        fake.add(
            "c",
            Some("u"),
            &[
                ("a.yml", "http:\n  url: https://a\n  token: s\n"),
                ("b.yml", "x: 1\n"),
            ],
        );
        let received = b"http:\n  url: https://b\n  token: ''\n".to_vec();
        fake.apply(
            "c",
            &[FileChange::Put {
                path: "a.yml".into(),
                bytes: received,
            }],
        )
        .expect("applies");
        let text = String::from_utf8(fake.file("c", "a.yml").expect("file")).expect("utf8");
        assert!(text.contains("https://b") && text.contains("token: s"));

        let before = fake.file("c", "b.yml");
        let failing = [
            FileChange::Delete {
                path: "b.yml".into(),
            },
            FileChange::Rename {
                from: "gone.yml".into(),
                to: "z.yml".into(),
            },
        ];
        assert!(fake.apply("c", &failing).is_err());
        assert_eq!(fake.file("c", "b.yml"), before);

        fake.fail_next_apply();
        assert!(fake
            .apply(
                "c",
                &[FileChange::Delete {
                    path: "b.yml".into()
                }]
            )
            .is_err());
        assert!(fake.file("c", "b.yml").is_some());
        fake.apply(
            "c",
            &[FileChange::Delete {
                path: "b.yml".into(),
            }],
        )
        .expect("applies");
        assert!(fake.file("c", "b.yml").is_none());
        assert_eq!(fake.applied().len(), 4);
    }

    #[test]
    fn the_fake_apply_follows_the_real_order_and_case_rule() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("Users/a.yml", "a: 1\n"), ("b.yml", "b: 1\n")]);
        let err = fake
            .apply(
                "c",
                &[FileChange::Put {
                    path: "users/a.yml".into(),
                    bytes: b"a: 2\n".to_vec(),
                }],
            )
            .expect_err("refused");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
        fake.apply(
            "c",
            &[
                FileChange::Delete {
                    path: "b.yml".into(),
                },
                FileChange::Rename {
                    from: "b.yml".into(),
                    to: "moved/b.yml".into(),
                },
            ],
        )
        .expect("renames run before deletes");
        assert_eq!(fake.paths("c"), vec!["Users/a.yml", "moved/b.yml"]);
    }

    #[test]
    fn the_fake_apply_if_unchanged_refuses_a_changed_manifest() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("a.yml", "a: 1\n")]);
        let expected = fake.manifest("c").expect("manifest");
        fake.set_file("c", "a.yml", "a: 2\n");
        let err = fake
            .apply_if_unchanged(
                "c",
                &expected,
                &[FileChange::Delete {
                    path: "a.yml".into(),
                }],
            )
            .expect_err("conflict");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert!(fake.file("c", "a.yml").is_some());
        let fresh = fake.manifest("c").expect("manifest");
        fake.apply_if_unchanged(
            "c",
            &fresh,
            &[FileChange::Delete {
                path: "a.yml".into(),
            }],
        )
        .expect("applies");
        assert!(fake.file("c", "a.yml").is_none());
    }

    #[test]
    fn the_fake_manifest_hashes_the_stored_bytes() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("a.yml", "a: 1\n")]);
        let manifest = fake.manifest("c").expect("manifest");
        assert_eq!(manifest.get("a.yml"), Some(sha256_hex(b"a: 1\n").as_str()));
        assert!(!fake.exists("nope").expect("exists"));
        assert!(matches!(
            fake.manifest("nope"),
            Err(DomainError::NotFound(_))
        ));
    }

    fn record(id: &LinkId, files: &[(&str, &[u8])]) -> LinkRecord {
        let mut base = Manifest::new();
        for (path, bytes) in files {
            base.insert(path, &sha256_hex(bytes)).expect("valid entry");
        }
        LinkRecord {
            link_id: id.clone(),
            peer_device_id: DeviceId::from_bytes([7u8; 32]),
            collection_name: "c".into(),
            collection_root: "/c".into(),
            collection_uid: None,
            base,
            state: LinkState::InSync,
            updated_at: "2026-11-08T10:00:00Z".into(),
        }
    }

    #[test]
    fn the_fake_link_repo_keeps_a_delta_and_prunes() {
        let repo = FakeLinkRepo::new();
        let id = LinkId::random();
        let with_a = record(&id, &[("a.yml", b"one"), ("b.yml", b"two")]);
        repo.save(&with_a, &[("a.yml".to_string(), b"one".to_vec())])
            .expect("saves");
        // A state-only save keeps the copy.
        repo.save(&with_a, &[]).expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"one".to_vec())
        );
        assert_eq!(repo.read_base_file(&id, "b.yml").expect("reads"), None);
        // A record without the path prunes the copy.
        let without_a = record(&id, &[("b.yml", b"two")]);
        repo.save(&without_a, &[]).expect("saves");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
        assert_eq!(repo.list().expect("lists"), vec![without_a.clone()]);
        assert_eq!(repo.get(&id).expect("gets"), Some(without_a));
        repo.delete(&id).expect("deletes");
        assert_eq!(repo.get(&id).expect("gets"), None);
    }

    #[test]
    fn the_fake_link_repo_validates_before_it_writes() {
        let repo = FakeLinkRepo::new();
        let id = LinkId::random();
        let rec = record(&id, &[("a.yml", b"one"), ("b.yml", b"two")]);
        // The second copy does not match its hash, so the first is not written either.
        let err = repo
            .save(
                &rec,
                &[
                    ("a.yml".to_string(), b"one".to_vec()),
                    ("b.yml".to_string(), b"wrong".to_vec()),
                ],
            )
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!err.to_string().contains("b.yml"));
        assert_eq!(repo.get(&id).expect("gets"), None);
        // A path outside the base is refused.
        let err = repo
            .save(&rec, &[("c.yml".to_string(), b"one".to_vec())])
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // A copy whose hash no longer matches reads as none.
        repo.save(&rec, &[("a.yml".to_string(), b"one".to_vec())])
            .expect("saves");
        let changed = record(&id, &[("a.yml", b"changed")]);
        repo.save(&changed, &[]).expect("saves");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }
}
