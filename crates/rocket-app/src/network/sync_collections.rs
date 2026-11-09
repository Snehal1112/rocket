//! The real `SyncCollectionPort`, backed by the export and the file transaction.
//!
//! The manifest and the exported files come from `export_bruno(collection, false)`, so a hash
//! is the hash of the scrubbed bytes a peer would receive. `apply` plans every change against
//! the local files first, so a failing change touches nothing, then stages the result in one
//! file transaction. All methods block.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use rocket_collection::{
    validate_rel_path, CollectionFileTxFactory, CollectionRepository, RecoveryOutcome,
};
use rocket_export::{is_synced_path, merge_file, zip_entries};
use rocket_network::link::Manifest;
use rocket_shared::error::{DomainError, DomainResult};

use super::sync_ports::{sha256_hex, FileChange, SyncCollectionPort, SyncFile};
use crate::collection_export_service::CollectionExportService;
use crate::collection_lock::{collection_lock, with_collection_lock};

pub struct ExportBackedSyncCollections {
    export: Arc<CollectionExportService>,
    collections: Arc<dyn CollectionRepository>,
    tx: Arc<dyn CollectionFileTxFactory>,
}

const REFUSED: &str = "The sync names a file it may not write.";
const TAKEN: &str = "The sync names a file that is already taken.";
const CASE_TAKEN: &str = "The sync names a file that differs from another only by letter case.";
const UNREADABLE: &str = "The collection files could not be read.";
const NOT_WRITTEN: &str = "The sync could not write the collection files.";
const UNFINISHED: &str = "An earlier sync apply is not finished yet.";
const CHANGED: &str = "The collection changed while the sync was running.";

/// The state of each touched path after the plan: `Some` is a put, `None` a delete.
pub(super) type Overlay = BTreeMap<String, Option<Vec<u8>>>;

fn checked(path: &str) -> DomainResult<()> {
    if !is_synced_path(path) {
        return Err(DomainError::InvalidInput(REFUSED.into()));
    }
    validate_rel_path(path).map_err(|_| DomainError::InvalidInput(REFUSED.into()))
}

/// Logs the kind of an error and returns a fixed sentence of the same kind. The inner texts
/// quote paths and file contents, which must not reach a peer.
fn quiet(e: &DomainError, code: &str, fixed: &str) -> DomainError {
    let (kind, error) = match e {
        DomainError::NotFound(_) => ("not_found", DomainError::NotFound(fixed.into())),
        DomainError::InvalidInput(_) => ("invalid_input", DomainError::InvalidInput(fixed.into())),
        DomainError::AlreadyExists(_) => {
            ("already_exists", DomainError::AlreadyExists(fixed.into()))
        }
        DomainError::Conflict(_) => ("conflict", DomainError::Conflict(fixed.into())),
        _ => ("other", DomainError::Internal(fixed.into())),
    };
    tracing::warn!(code, kind, "a sync step failed");
    error
}

fn is_yaml(path: &str) -> bool {
    path.ends_with(".yml") || path.ends_with(".yaml")
}

/// The planning state: the local files with the changes made so far laid over them.
struct Plan<'a> {
    local: &'a BTreeMap<String, Vec<u8>>,
    overlay: Overlay,
    on_disk: &'a dyn Fn(&str) -> DomainResult<bool>,
}

impl Plan<'_> {
    /// The current bytes of a path, or `None` when it is absent or deleted.
    fn lookup(&self, path: &str) -> Option<&Vec<u8>> {
        match self.overlay.get(path) {
            Some(entry) => entry.as_ref(),
            None => self.local.get(path),
        }
    }

    /// True when a different live path equals `path` ignoring case.
    fn case_clash(&self, path: &str) -> bool {
        let folded = path.to_lowercase();
        let live_local = self
            .local
            .keys()
            .filter(|p| !self.overlay.contains_key(*p))
            .map(String::as_str);
        let live_overlay = self
            .overlay
            .iter()
            .filter(|(_, entry)| entry.is_some())
            .map(|(p, _)| p.as_str());
        live_local
            .chain(live_overlay)
            .any(|p| p != path && p.to_lowercase() == folded)
    }

    /// Free means no live file, no case variant and nothing else on disk at the path.
    fn is_free(&self, path: &str) -> DomainResult<bool> {
        if self.case_clash(path) {
            return Ok(false);
        }
        match self.overlay.get(path) {
            Some(entry) => Ok(entry.is_none()),
            None => Ok(!self.local.contains_key(path) && !(self.on_disk)(path)?),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> DomainResult<()> {
        checked(from)?;
        checked(to)?;
        let Some(bytes) = self.lookup(from).cloned() else {
            return Err(DomainError::NotFound(
                "The sync moves a file that is not there.".into(),
            ));
        };
        if from == to {
            return Err(DomainError::InvalidInput(REFUSED.into()));
        }
        if self.case_clash(to) {
            return Err(DomainError::AlreadyExists(CASE_TAKEN.into()));
        }
        if !self.is_free(to)? {
            return Err(DomainError::AlreadyExists(TAKEN.into()));
        }
        // The local bytes move as they are, so the local secrets travel with the file.
        self.overlay.insert(from.to_string(), None);
        self.overlay.insert(to.to_string(), Some(bytes));
        Ok(())
    }

    fn delete(&mut self, path: &str) -> DomainResult<()> {
        checked(path)?;
        self.overlay.insert(path.to_string(), None);
        Ok(())
    }

    fn put(&mut self, path: &str, bytes: &[u8]) -> DomainResult<()> {
        checked(path)?;
        if self.case_clash(path) {
            return Err(DomainError::AlreadyExists(CASE_TAKEN.into()));
        }
        let existing = self.lookup(path).cloned();
        if existing.is_none() && !self.is_free(path)? {
            // Something that is not a synced file sits at the path (a folder, for one).
            return Err(DomainError::AlreadyExists(TAKEN.into()));
        }
        let merged = if is_yaml(path) {
            merge_file(existing.as_deref(), bytes)?
        } else {
            bytes.to_vec()
        };
        self.overlay.insert(path.to_string(), Some(merged));
        Ok(())
    }

    fn put_as(&mut self, path: &str, bytes: &[u8]) -> DomainResult<()> {
        checked(path)?;
        if self.case_clash(path) {
            return Err(DomainError::AlreadyExists(CASE_TAKEN.into()));
        }
        if !self.is_free(path)? {
            return Err(DomainError::AlreadyExists(TAKEN.into()));
        }
        let written = if is_yaml(path) {
            merge_file(None, bytes)?
        } else {
            bytes.to_vec()
        };
        self.overlay.insert(path.to_string(), Some(written));
        Ok(())
    }
}

/// Plans the changes against `local` and returns the final state of each touched path. The
/// order is fixed whatever the input order: renames, then deletes, then puts and put-as, each
/// group in input order. `on_disk` tells whether anything else sits at a path the local files
/// do not hold. A failing change fails the plan.
pub(super) fn plan_changes(
    local: &BTreeMap<String, Vec<u8>>,
    changes: &[FileChange],
    on_disk: &dyn Fn(&str) -> DomainResult<bool>,
) -> DomainResult<Overlay> {
    let mut plan = Plan {
        local,
        overlay: Overlay::new(),
        on_disk,
    };
    for change in changes {
        if let FileChange::Rename { from, to } = change {
            plan.rename(from, to)?;
        }
    }
    for change in changes {
        if let FileChange::Delete { path } = change {
            plan.delete(path)?;
        }
    }
    for change in changes {
        match change {
            FileChange::Put { path, bytes } => plan.put(path, bytes)?,
            FileChange::PutAs { path, bytes } => plan.put_as(path, bytes)?,
            FileChange::Delete { .. } | FileChange::Rename { .. } => {}
        }
    }
    Ok(plan.overlay)
}

impl ExportBackedSyncCollections {
    pub fn new(
        export: Arc<CollectionExportService>,
        collections: Arc<dyn CollectionRepository>,
        tx: Arc<dyn CollectionFileTxFactory>,
    ) -> ExportBackedSyncCollections {
        ExportBackedSyncCollections {
            export,
            collections,
            tx,
        }
    }

    /// Runs `f` with the repository pinned to the active workspace when there is one.
    fn with_repo<T>(&self, f: impl FnOnce(&dyn CollectionRepository) -> T) -> T {
        let pinned = self.collections.pinned();
        let repo = pinned.as_deref().unwrap_or(self.collections.as_ref());
        f(repo)
    }

    /// The export without environments, as `(path, scrubbed bytes)`. No lock.
    fn exported_entries(&self, collection: &str) -> DomainResult<Vec<(String, Vec<u8>)>> {
        let out = self
            .export
            .export_bruno(collection, false)
            .map_err(|e| quiet(&e, "export", UNREADABLE))?;
        let entries =
            zip_entries(Cursor::new(out.bytes)).map_err(|e| quiet(&e, "zip", UNREADABLE))?;
        // The same filter as the base of a link, so both sides name the same files.
        Ok(entries
            .into_iter()
            .filter(|(path, _)| is_synced_path(path))
            .collect())
    }

    /// The manifest of the export. No lock, so the caller holds it or does not need it.
    fn manifest_unlocked(&self, collection: &str) -> DomainResult<Manifest> {
        let mut manifest = Manifest::new();
        for (path, bytes) in self.exported_entries(collection)? {
            manifest.insert(&path, &sha256_hex(&bytes))?;
        }
        Ok(manifest)
    }

    fn apply_inner(
        &self,
        collection: &str,
        expected: Option<&Manifest>,
        changes: &[FileChange],
    ) -> DomainResult<()> {
        // A plain apply of nothing has nothing to do. A checked one still compares, so the
        // caller never records a base over a save that landed meanwhile.
        if changes.is_empty() && expected.is_none() {
            return Ok(());
        }
        self.with_repo(|repo| {
            let root = repo.collection_root_path(collection)?;
            let lock = collection_lock(repo, &root);
            let _guard = lock.lock();
            // An earlier apply that was cut short is settled before anything is read.
            self.tx
                .recover(&root)
                .map_err(|e| {
                    quiet(&e, "recover", UNFINISHED);
                    DomainError::Conflict(UNFINISHED.into())
                })?;
            if let Some(expected) = expected {
                // The lock is not reentrant, so this read takes none.
                if &self.manifest_unlocked(collection)? != expected {
                    return Err(DomainError::Conflict(CHANGED.into()));
                }
            }
            if changes.is_empty() {
                return Ok(());
            }
            let local: BTreeMap<String, Vec<u8>> = self
                .export
                .raw_files(collection)
                .map_err(|e| quiet(&e, "raw_files", UNREADABLE))?
                .into_iter()
                .map(|file| (file.rel_path, file.bytes))
                .collect();
            let overlay = plan_changes(&local, changes, &|path| {
                repo.path_exists(collection, path)
                    .map_err(|e| quiet(&e, "path_exists", NOT_WRITTEN))
            })?;
            let mut tx = self
                .tx
                .begin(&root)
                .map_err(|e| quiet(&e, "begin", NOT_WRITTEN))?;
            for (path, entry) in &overlay {
                let staged = match entry {
                    Some(bytes) => tx.stage_put(path, bytes),
                    None => tx.stage_delete(path),
                };
                if let Err(e) = staged {
                    tx.abort();
                    return Err(quiet(&e, "stage", NOT_WRITTEN));
                }
            }
            if let Err(e) = tx.commit() {
                // The staging area was lost or taken over. A journal may be live elsewhere.
                if matches!(e, DomainError::Conflict(_)) {
                    return Err(quiet(&e, "commit", NOT_WRITTEN));
                }
                // A failed commit can leave a journal that recovery finishes or undoes.
                return match self.tx.recover(&root) {
                    Ok(RecoveryOutcome::Finished { .. }) => Ok(()),
                    Ok(_) => Err(quiet(&e, "commit", "The sync could not be applied; nothing was changed.")),
                    Err(re) => {
                        quiet(&re, "recover_after_commit", UNFINISHED);
                        Err(quiet(&e, "commit", "The sync could not be applied; it will be finished or undone the next time Rocket starts."))
                    }
                };
            }
            Ok(())
        })
    }
}

impl SyncCollectionPort for ExportBackedSyncCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        self.with_repo(|repo| match repo.collection_root_path(collection) {
            Ok(_) => Ok(true),
            Err(DomainError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        })
    }

    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>> {
        self.with_repo(|repo| Ok(repo.collection_identity(collection)?.uid))
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        self.with_repo(|repo| {
            Ok(repo
                .collection_root_path(collection)?
                .to_string_lossy()
                .into_owned())
        })
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        self.with_repo(|repo| {
            with_collection_lock(repo, collection, || self.manifest_unlocked(collection))
        })
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let entries = self.with_repo(|repo| {
            with_collection_lock(repo, collection, || self.exported_entries(collection))
        })?;
        let by_path: BTreeMap<String, Vec<u8>> = entries.into_iter().collect();
        paths
            .iter()
            .map(|path| match by_path.get(path) {
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
