use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{
    contract::{
        changelog::{ChangeType, ChangelogEntry, ContractChangelog},
        diff::diff_signature,
        repository::{ContractError, ContractRepository, ContractResult},
        snapshot::{ContractSnapshot, RequestSignatureSnapshot},
        types::{Contract, ContractEnforcementMode, ContractScope, ContractStatus},
        {transition_status, StatusEvent},
    },
    CollectionItem, CollectionRepository, Folder,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use ulid::Ulid;

/// Maximum allowed attachment file size (2 MB).
const MAX_ATTACHMENT_BYTES: u64 = 2 * 1024 * 1024;

/// File extensions accepted as contract attachments.
const ALLOWED_EXTENSIONS: &[&str] = &["pdf", "doc", "docx", "txt", "md", "png", "jpg", "jpeg"];

/// Returns the directory where attachment files for a contract are stored.
/// Mirrors the convention in `FsContractRepo::attachments_dir`.
fn attachments_dir(collection_root: &Path, contract_id: Ulid) -> PathBuf {
    collection_root
        .join(".rocket")
        .join("contracts")
        .join("attachments")
        .join(contract_id.to_string())
}

/// Validate and copy a list of absolute source paths into the contract's
/// attachments directory. Returns the relative paths stored in the contract.
///
/// Relative paths are relative to `collection_root`, so they are stable
/// across machines when the collection is shared via git.
fn copy_attachments(
    collection_root: &Path,
    contract_id: Ulid,
    absolute_paths: &[PathBuf],
) -> ContractResult<Vec<PathBuf>> {
    if absolute_paths.is_empty() {
        return Ok(vec![]);
    }

    let dest_dir = attachments_dir(collection_root, contract_id);
    std::fs::create_dir_all(&dest_dir)?;

    let mut relative_paths = Vec::with_capacity(absolute_paths.len());

    for src in absolute_paths {
        // Validate extension.
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            return Err(ContractError::Internal(format!(
                "Unsupported file type '.{}'. Allowed: {}",
                ext,
                ALLOWED_EXTENSIONS.join(", ")
            )));
        }

        // Validate size.
        let size = std::fs::metadata(src)?.len();
        if size > MAX_ATTACHMENT_BYTES {
            let mb = size as f64 / (1024.0 * 1024.0);
            return Err(ContractError::Internal(format!(
                "'{}' is {:.1} MB — attachments must be 2 MB or smaller.",
                src.file_name().unwrap_or_default().to_string_lossy(),
                mb
            )));
        }

        let file_name = src
            .file_name()
            .ok_or_else(|| ContractError::Internal("Attachment has no filename.".into()))?;
        let dest = dest_dir.join(file_name);
        std::fs::copy(src, &dest)?;

        // Store relative path so it is portable across machines.
        let rel = dest
            .strip_prefix(collection_root)
            .map_err(|_| ContractError::Internal("Attachment dest is outside collection root.".into()))?;
        relative_paths.push(rel.to_path_buf());
    }

    Ok(relative_paths)
}

pub struct ContractService {
    repo: Arc<dyn ContractRepository>,
    collection_repo: Arc<dyn CollectionRepository>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

/// Summary returned by `recompute_drift_for_collection` for each processed contract.
#[derive(Debug, Clone)]
pub struct ContractDriftSummary {
    pub contract_id: String,
    pub status: ContractStatus,
    pub drift_count: u32,
    pub breach_count: u32,
}

/// Lightweight summary of a contract returned by `list_summaries`.
#[derive(Debug, Clone)]
pub struct ContractSummary {
    pub id: String,
    pub title: String,
    pub status: ContractStatus,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
}

impl ContractService {
    pub fn new(
        repo: Arc<dyn ContractRepository>,
        collection_repo: Arc<dyn CollectionRepository>,
    ) -> Self {
        Self {
            repo,
            collection_repo,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Arc<dyn ContractRepository>,
        collection_repo: Arc<dyn CollectionRepository>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self { repo, collection_repo, audit }
    }

    /// Create a new contract and take a baseline snapshot of every covered request.
    ///
    /// `attachment_sources` is a list of absolute paths chosen by the user via
    /// the OS file picker. Each file is validated (type + 2 MB limit) and copied
    /// into `<collection_root>/.rocket/contracts/attachments/<id>/` so the
    /// attachments travel with the collection and are tracked by git. The stored
    /// `document_paths` on the returned contract are relative to `collection_root`.
    pub fn attach_contract(
        &self,
        collection_root: &Path,
        mut contract: Contract,
        initial_snapshots: Vec<RequestSignatureSnapshot>,
        attachment_sources: Vec<PathBuf>,
    ) -> ContractResult<Contract> {
        let collection_name = collection_root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ContractError::Internal("collection_root has no final path component".into()))?;

        contract.id = Ulid::new();
        let now = chrono::Utc::now();
        contract.created_at = Some(now);
        contract.updated_at = Some(now);
        // Force Informational — Model B variants not yet UI-reachable.
        contract.enforcement_mode = ContractEnforcementMode::Informational;

        // Load the collection and build the baseline snapshot BEFORE any
        // write. If the collection read fails, we must not leave an orphan
        // contract YAML behind with no snapshot and no changelog.
        // Errors from the collection repo are promoted to
        // `ContractError::Internal` so they are never swallowed.
        let collection = self
            .collection_repo
            .get(collection_name)
            .map_err(|e| ContractError::Internal(e.to_string()))?;

        let mut snapshot = ContractSnapshot::new(contract.id);
        let mut built = Vec::new();
        walk_folder(&collection.root, Path::new(""), &mut built);
        for (rel_path, request) in built {
            if !covers(&contract.scope, &rel_path) {
                continue;
            }
            snapshot.upsert(RequestSignatureSnapshot::from_request(&rel_path, request));
        }

        // Any snapshots supplied explicitly by the caller take precedence.
        for snap in initial_snapshots {
            snapshot.upsert(snap);
        }

        contract.endpoint_count = snapshot.entries.len() as u32;

        // Copy attachments into the collection before writing the contract YAML
        // so the relative paths are ready. If copy fails, no contract file is
        // written and no orphan is left behind.
        contract.document_paths = copy_attachments(collection_root, contract.id, &attachment_sources)?;

        // Only now write the contract, its baseline, and the empty changelog.
        self.repo.save_contract(collection_root, &contract)?;
        self.repo.save_snapshot(collection_root, &snapshot)?;
        // Seed the tracking snapshot with the same baseline so on_request_saved
        // has a correct starting point without touching the signed baseline.
        self.repo.save_tracking_snapshot(collection_root, &snapshot)?;
        let changelog = ContractChangelog::new(contract.id);
        self.repo.append_changelog(collection_root, &changelog)?;

        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::ContractAttached {
                contract_id: contract.id.to_string(),
                collection: collection_name.to_string(),
                scope: format!("{:?}", contract.scope),
            },
        );

        Ok(contract)
    }

    /// Update metadata fields of an existing contract (title, parties, project,
    /// version, dates, document_paths). Scope, snapshots, and changelog are
    /// intentionally preserved — changing scope would invalidate the baseline.
    ///
    /// `attachment_sources` contains absolute paths for *new* files to add.
    /// `kept_paths` contains relative paths (already stored) the user wants to
    /// retain. Files not in `kept_paths` are deleted from disk.
    pub fn update_contract(
        &self,
        collection_root: &Path,
        mut contract: Contract,
        attachment_sources: Vec<PathBuf>,
        kept_paths: Vec<PathBuf>,
    ) -> ContractResult<Contract> {
        // Confirm the contract exists before overwriting.
        self.repo.load_contract(collection_root, contract.id)?;

        // Remove attachment files the user explicitly de-listed.
        let attach_dir = attachments_dir(collection_root, contract.id);
        if attach_dir.exists() {
            for entry in std::fs::read_dir(&attach_dir)? {
                let entry = entry?;
                let rel = entry
                    .path()
                    .strip_prefix(collection_root)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|_| entry.path());
                if !kept_paths.contains(&rel) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        // Copy newly added attachments.
        let mut new_relative = copy_attachments(collection_root, contract.id, &attachment_sources)?;

        // Merge: kept existing paths + newly copied paths.
        let mut merged = kept_paths;
        merged.append(&mut new_relative);
        contract.document_paths = merged;

        contract.updated_at = Some(chrono::Utc::now());
        self.repo.save_contract(collection_root, &contract)?;
        Ok(contract)
    }

    pub fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>> {
        self.repo.list_contracts(collection_root)
    }

    pub fn get_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract> {
        self.repo.load_contract(collection_root, id)
    }

    pub fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()> {
        self.repo.delete_contract(collection_root, id)?;
        let collection = collection_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::ContractDeleted {
                contract_id: id.to_string(),
                collection,
            },
        );
        Ok(())
    }

    pub fn get_changelog(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog> {
        self.repo.load_changelog(collection_root, contract_id)
    }

    /// Called by the request save handler after every successful save.
    ///
    /// Checks whether any active contract covers `request_path`.
    /// If yes, diffs the new snapshot against the stored one.
    /// Any changes are silently appended to the changelog and the snapshot is updated.
    ///
    /// MODEL B SEAM: `enforcement_mode` is read here.
    /// Currently only `Informational` is reachable.
    /// Future Model B work adds `Warn` and `Block` arms — no other changes needed.
    pub fn on_request_saved(
        &self,
        collection_root: &Path,
        new_snap: RequestSignatureSnapshot,
    ) -> ContractResult<()> {
        let contracts = self.repo.list_contracts(collection_root)?;

        for contract in contracts {
            if !covers(&contract.scope, &new_snap.request_path) {
                continue;
            }

            let mut snapshot = self.repo.load_tracking_snapshot(collection_root, contract.id)?;

            if let Some(old_snap) = snapshot.get(&new_snap.request_path) {
                let author = std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .ok();
                let changes = diff_signature(old_snap, &new_snap, &contract.policy.breaking_change_policy, author);

                if !changes.is_empty() {
                    // MODEL B SEAM — match on enforcement_mode when Model B is built:
                    // ContractEnforcementMode::Warn  => emit warning event to frontend
                    // ContractEnforcementMode::Block => return Err(ContractViolation)
                    match contract.enforcement_mode {
                        ContractEnforcementMode::Informational => {
                            let entries: Vec<ChangelogEntry> = changes;
                            // Emit one ContractViolation audit event per field change
                            // before handing the entries to the changelog store.
                            for entry in &entries {
                                self.audit.publish(
                                    "system".into(),
                                    None,
                                    AuditEventKind::ContractViolation {
                                        contract_id: contract.id.to_string(),
                                        request_path: entry.request_path.to_string_lossy().into_owned(),
                                        field: entry.field.clone(),
                                    },
                                );
                            }
                            let mut incoming = ContractChangelog::new(contract.id);
                            incoming.append(entries);
                            self.repo.append_changelog(collection_root, &incoming)?;
                        }
                        // Warn and Block are defined but unreachable until Model B sprint.
                        ContractEnforcementMode::Warn | ContractEnforcementMode::Block => {
                            // TODO(model-b): emit event / return error.
                        }
                    }
                }
            }

            // Always update the tracking snapshot to track current state.
            // The baseline snapshot (used by recompute_drift_for_collection) is intentionally left untouched.
            snapshot.upsert(new_snap.clone());
            self.repo.save_tracking_snapshot(collection_root, &snapshot)?;
        }

        Ok(())
    }

    /// Drive the contract through the state machine with the given event.
    /// Returns the updated contract (already persisted).
    pub fn transition_contract_status(
        &self,
        collection_root: &Path,
        id: Ulid,
        event: StatusEvent,
    ) -> ContractResult<Contract> {
        let mut contract = self.repo.load_contract(collection_root, id)?;
        let new_status = transition_status(&contract.status, &event)
            .map_err(|e| ContractError::Internal(format!("invalid transition: {:?}", e.from)))?;
        contract.status = new_status;
        contract.updated_at = Some(chrono::Utc::now());
        self.repo.save_contract(collection_root, &contract)?;
        Ok(contract)
    }

    /// Transition a contract to Active and record the current set of endpoint snapshots.
    /// Any snapshots supplied are merged into the stored baseline.
    pub fn publish_contract(
        &self,
        collection_root: &Path,
        id: Ulid,
        snapshots: Vec<RequestSignatureSnapshot>,
    ) -> ContractResult<Contract> {
        let mut contract = self.repo.load_contract(collection_root, id)?;
        // Drift/Breach → Active uses Resign (re-sign); Draft → Active uses Publish.
        let event = match &contract.status {
            ContractStatus::Drift | ContractStatus::Breach => StatusEvent::Resign,
            _ => StatusEvent::Publish,
        };
        let new_status = transition_status(&contract.status, &event)
            .map_err(|e| ContractError::Internal(format!("invalid transition: {:?}", e.from)))?;
        contract.status = new_status;
        contract.updated_at = Some(chrono::Utc::now());
        let mut snapshot = self.repo.load_snapshot(collection_root, id)?;
        for snap in snapshots {
            snapshot.upsert(snap);
        }
        contract.endpoint_count = snapshot.entries.len() as u32;
        self.repo.save_snapshot(collection_root, &snapshot)?;
        // Reset the tracking snapshot to match the newly signed baseline so
        // on_request_saved compares against the fresh contract state going forward.
        self.repo.save_tracking_snapshot(collection_root, &snapshot)?;
        self.repo.save_contract(collection_root, &contract)?;
        Ok(contract)
    }

    /// Accept all detected drift and re-sign the contract at its current shape.
    ///
    /// Walks the live collection, rebuilds the baseline snapshot from the
    /// current request state (so `recompute_drift` finds zero changes after
    /// this call), resets both baseline and tracking snapshots, bumps the
    /// version, and transitions status back to Active via Resign.
    pub fn accept_drift(
        &self,
        collection_root: &Path,
        id: Ulid,
        new_version: String,
    ) -> ContractResult<Contract> {
        let mut contract = self.repo.load_contract(collection_root, id)?;

        let new_status = transition_status(&contract.status, &StatusEvent::Resign)
            .map_err(|e| ContractError::Internal(format!("invalid transition: {:?}", e.from)))?;

        // Walk the live collection to capture the current request state as the
        // new baseline — this is what makes accept idempotent: recompute_drift
        // will see zero diff because baseline now matches live collection.
        let collection_name = collection_root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ContractError::Internal("collection_root has no final path component".into()))?;
        let collection = self
            .collection_repo
            .get(collection_name)
            .map_err(|e| ContractError::Internal(e.to_string()))?;

        let mut new_snapshot = ContractSnapshot::new(id);
        let mut walked = Vec::new();
        walk_folder(&collection.root, Path::new(""), &mut walked);
        for (rel_path, request) in walked {
            if !covers(&contract.scope, &rel_path) {
                continue;
            }
            new_snapshot.upsert(RequestSignatureSnapshot::from_request(&rel_path, request));
        }

        contract.status = new_status;
        contract.version = new_version;
        contract.drift_count = 0;
        contract.breach_count = 0;
        contract.endpoint_count = new_snapshot.entries.len() as u32;
        contract.updated_at = Some(chrono::Utc::now());

        self.repo.save_snapshot(collection_root, &new_snapshot)?;
        self.repo.save_tracking_snapshot(collection_root, &new_snapshot)?;
        self.repo.save_contract(collection_root, &contract)?;

        Ok(contract)
    }

    /// Renew a contract: transition via `Renew` event, optionally set a new
    /// expiry date, and reset drift/breach counters.
    pub fn renew_contract(
        &self,
        collection_root: &Path,
        id: Ulid,
        new_expiry: Option<chrono::NaiveDate>,
    ) -> ContractResult<Contract> {
        let mut contract = self.repo.load_contract(collection_root, id)?;
        let new_status = transition_status(&contract.status, &StatusEvent::Renew)
            .map_err(|e| ContractError::Internal(format!("invalid transition: {:?}", e.from)))?;
        contract.status = new_status;
        contract.expiry_date = new_expiry;
        contract.drift_count = 0;
        contract.breach_count = 0;
        contract.updated_at = Some(chrono::Utc::now());
        self.repo.save_contract(collection_root, &contract)?;
        Ok(contract)
    }

    /// Scans all active contracts in the collection, diffs each against the
    /// current on-disk request shapes (loaded from the collection repo), updates
    /// drift/breach counts, transitions status via the state machine, appends
    /// changelog entries, and saves all modified contracts.
    pub fn recompute_drift_for_collection(
        &self,
        collection_root: &Path,
    ) -> ContractResult<Vec<ContractDriftSummary>> {
        // Build the current snapshot set by walking the live collection on disk.
        // This is the authoritative source of truth — the frontend cannot supply
        // request signatures because it operates in Option B (snapshot-less) mode.
        let collection_name = collection_root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ContractError::Internal("collection_root has no final path component".into()))?;
        let collection = self
            .collection_repo
            .get(collection_name)
            .map_err(|e| ContractError::Internal(e.to_string()))?;
        let mut walked = Vec::new();
        walk_folder(&collection.root, Path::new(""), &mut walked);
        let current_snapshots: Vec<RequestSignatureSnapshot> = walked
            .into_iter()
            .map(|(rel_path, request)| RequestSignatureSnapshot::from_request(&rel_path, request))
            .collect();

        let contracts = self.repo.list_contracts(collection_root)?;
        let mut summaries = Vec::new();

        for mut contract in contracts {
            // Skip contracts not in an active monitoring state
            match contract.status {
                ContractStatus::Draft
                | ContractStatus::Paused
                | ContractStatus::Expired
                | ContractStatus::InReview
                | ContractStatus::Archived => continue,
                _ => {}
            }

            // Load snapshot — skip if none (published but snapshot missing)
            let snapshot = match self.repo.load_snapshot(collection_root, contract.id) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Capture originals before any mutation so we can guard against
            // unnecessary disk writes.  The file watcher emits `collection-changed`
            // for every write; without this guard every save triggers another
            // recompute_drift call, producing an infinite feedback loop.
            let original_status = contract.status.clone();
            let original_drift = contract.drift_count;
            let original_breach = contract.breach_count;

            let mut all_entries: Vec<ChangelogEntry> = Vec::new();
            let mut drift_count = 0u32;
            let mut breach_count = 0u32;

            for snap_entry in &snapshot.entries {
                let current = current_snapshots
                    .iter()
                    .find(|s| s.request_path == snap_entry.request_path);

                match current {
                    None => {
                        // Request removed — always breaking
                        all_entries.push(ChangelogEntry {
                            timestamp: chrono::Utc::now(),
                            request_path: snap_entry.request_path.clone(),
                            field: "request".into(),
                            change_type: ChangeType::Removed,
                            old_value: Some(format!("{} {}", snap_entry.method, snap_entry.url_pattern)),
                            new_value: None,
                            is_breaking: true,
                            request_method: Some(snap_entry.method.clone()),
                            http_path: Some(snap_entry.url_pattern.clone()),
                            author: None,
                        });
                        drift_count += 1;
                        breach_count += 1;
                    }
                    Some(current_snap) => {
                        let changes = diff_signature(
                            snap_entry,
                            current_snap,
                            &contract.policy.breaking_change_policy,
                            None,
                        );
                        for entry in &changes {
                            drift_count += 1;
                            if entry.is_breaking {
                                breach_count += 1;
                            }
                        }
                        all_entries.extend(changes);
                    }
                }
            }

            // Determine new status via state machine
            let event = if breach_count > 0 {
                StatusEvent::BreachDetected
            } else if drift_count > 0 {
                StatusEvent::DriftDetected
            } else {
                match contract.status {
                    ContractStatus::Drift | ContractStatus::Breach => StatusEvent::Resign,
                    _ => {
                        summaries.push(ContractDriftSummary {
                            contract_id: contract.id.to_string(),
                            status: contract.status.clone(),
                            drift_count: 0,
                            breach_count: 0,
                        });
                        continue;
                    }
                }
            };

            let new_status = transition_status(&contract.status, &event)
                .unwrap_or_else(|_| contract.status.clone());

            // Guard: skip disk write when nothing changed.  Identical counts and
            // status mean the file content would only differ in `updated_at`,
            // which would retrigger the file watcher and restart the loop.
            if drift_count == original_drift
                && breach_count == original_breach
                && new_status == original_status
            {
                summaries.push(ContractDriftSummary {
                    contract_id: contract.id.to_string(),
                    status: original_status,
                    drift_count,
                    breach_count,
                });
                continue;
            }

            contract.status = new_status;
            contract.drift_count = drift_count;
            contract.breach_count = breach_count;
            contract.updated_at = Some(chrono::Utc::now());

            if !all_entries.is_empty() {
                let mut incoming = ContractChangelog::new(contract.id);
                incoming.append(all_entries);
                self.repo.append_changelog(collection_root, &incoming)?;
            }

            self.repo.save_contract(collection_root, &contract)?;

            summaries.push(ContractDriftSummary {
                contract_id: contract.id.to_string(),
                status: contract.status,
                drift_count,
                breach_count,
            });
        }

        Ok(summaries)
    }

    /// Create a copy of an existing contract in `Draft` status with a bumped
    /// patch version and a `" (copy)"` title suffix.
    pub fn duplicate_contract(
        &self,
        collection_root: &Path,
        id: Ulid,
    ) -> ContractResult<Contract> {
        let source = self.repo.load_contract(collection_root, id)?;
        let new_version = bump_patch_version(&source.version);
        let now = chrono::Utc::now();
        let duplicate = Contract {
            id: Ulid::new(),
            title: format!("{} (copy)", source.title),
            version: new_version,
            status: ContractStatus::Draft,
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_at: Some(now),
            updated_at: Some(now),
            ..source
        };
        self.repo.save_contract(collection_root, &duplicate)?;
        Ok(duplicate)
    }

    /// Return lightweight summaries for all contracts in the collection.
    pub fn list_summaries(
        &self,
        collection_root: &Path,
    ) -> ContractResult<Vec<ContractSummary>> {
        let contracts = self.repo.list_contracts(collection_root)?;
        Ok(contracts
            .into_iter()
            .map(|c| ContractSummary {
                id: c.id.to_string(),
                title: c.title,
                status: c.status,
                drift_count: c.drift_count,
                breach_count: c.breach_count,
                endpoint_count: c.endpoint_count,
            })
            .collect())
    }

    /// Generates a minimal OpenAPI 3.0 YAML stub for the given contract.
    /// Returns the YAML as a String — the frontend triggers the native save dialog.
    pub fn export_as_openapi_yaml(
        &self,
        collection_root: &std::path::Path,
        id: ulid::Ulid,
    ) -> ContractResult<String> {
        let contract = self.repo.load_contract(collection_root, id)?;
        let snapshot = self.repo.load_snapshot(collection_root, id).ok();

        let title = &contract.title;
        let version = &contract.version;

        let mut paths_yaml = String::new();
        if let Some(snap) = &snapshot {
            for entry in &snap.entries {
                let path_segment = entry.url_pattern.trim_start_matches('/');
                let method = entry.method.to_lowercase();
                paths_yaml.push_str(&format!(
                    "  /{}:\n    {}:\n      summary: '{} {}'\n      responses:\n        '200':\n          description: OK\n",
                    path_segment, method, entry.method, entry.url_pattern
                ));
            }
        }
        if paths_yaml.is_empty() {
            paths_yaml = "  /example:\n    get:\n      summary: Example endpoint\n      responses:\n        '200':\n          description: OK\n".to_string();
        }

        Ok(format!(
            "openapi: '3.0.3'\ninfo:\n  title: '{}'\n  version: '{}'\npaths:\n{}",
            title, version, paths_yaml
        ))
    }
}

/// Increments the patch segment of a semver-like version string.
/// E.g. `"1.2.3"` → `"1.2.4"`. Falls back to `"{v}-copy"` for non-standard strings.
fn bump_patch_version(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 3 {
        if let Ok(patch) = parts[2].parse::<u32>() {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    format!("{v}-copy")
}

/// Returns true if the contract scope covers the given request path.
fn covers(scope: &ContractScope, request_path: &Path) -> bool {
    match scope {
        ContractScope::Collection => true,
        ContractScope::Folder { rel_path } => request_path.starts_with(rel_path),
        ContractScope::Request { rel_path } => request_path == rel_path,
    }
}

/// Recursively collect every request in the folder tree along with its
/// collection-relative path. Folder segments prefer `dir_name` (the on-disk
/// directory name) to match the path format the save handler emits.
/// Requests with no `file_name` (e.g. freshly constructed in memory) are
/// skipped because we cannot key them the same way the save path will.
fn walk_folder<'a>(
    folder: &'a Folder,
    prefix: &Path,
    out: &mut Vec<(PathBuf, &'a rocket_collection::Request)>,
) {
    for item in &folder.items {
        match item {
            CollectionItem::Request(req) => {
                let Some(file_name) = req.file_name.as_deref() else {
                    continue;
                };
                let path = if prefix.as_os_str().is_empty() {
                    PathBuf::from(file_name)
                } else {
                    prefix.join(file_name)
                };
                out.push((path, req));
            }
            CollectionItem::Folder(sub) => {
                // Skip folders with no on-disk name. build_folder_tree always
                // sets dir_name; None here means a malformed in-memory tree.
                let Some(segment) = sub.dir_name.as_deref() else {
                    continue;
                };
                let next = if prefix.as_os_str().is_empty() {
                    PathBuf::from(segment)
                } else {
                    prefix.join(segment)
                };
                walk_folder(sub, &next, out);
            }
            CollectionItem::OpaqueItem(_) => {}
            // Summary items carry no file content; skip for contract audit.
            CollectionItem::Summary(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rocket_collection::{
        contract::{
            repository::ContractError,
            types::{ContractParty, ContractPolicy},
        },
        Collection, CollectionSettings, CollectionSummary, CollectionVariable, Request,
    };
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::types::HttpMethod;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory mock — keeps rocket-app's layering boundary intact
    /// (no dependency on rocket-infra for tests). Mirrors the inline-mock
    /// pattern used by other services in this crate.
    #[derive(Default)]
    struct MockContractRepo {
        contracts: Mutex<HashMap<Ulid, Contract>>,
        snapshots: Mutex<HashMap<Ulid, ContractSnapshot>>,
        changelogs: Mutex<HashMap<Ulid, ContractChangelog>>,
    }

    impl ContractRepository for MockContractRepo {
        fn save_contract(&self, _collection_root: &Path, contract: &Contract) -> ContractResult<()> {
            self.contracts.lock().unwrap().insert(contract.id, contract.clone());
            Ok(())
        }

        fn load_contract(&self, _collection_root: &Path, id: Ulid) -> ContractResult<Contract> {
            self.contracts
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .ok_or(ContractError::NotFound(id))
        }

        fn list_contracts(&self, _collection_root: &Path) -> ContractResult<Vec<Contract>> {
            Ok(self.contracts.lock().unwrap().values().cloned().collect())
        }

        fn delete_contract(&self, _collection_root: &Path, id: Ulid) -> ContractResult<()> {
            self.contracts.lock().unwrap().remove(&id);
            self.snapshots.lock().unwrap().remove(&id);
            self.changelogs.lock().unwrap().remove(&id);
            Ok(())
        }

        fn save_snapshot(&self, _collection_root: &Path, snapshot: &ContractSnapshot) -> ContractResult<()> {
            self.snapshots
                .lock()
                .unwrap()
                .insert(snapshot.contract_id, snapshot.clone());
            Ok(())
        }

        fn load_snapshot(&self, _collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractSnapshot> {
            Ok(self
                .snapshots
                .lock()
                .unwrap()
                .get(&contract_id)
                .cloned()
                .unwrap_or_else(|| ContractSnapshot::new(contract_id)))
        }

        fn append_changelog(&self, _collection_root: &Path, incoming: &ContractChangelog) -> ContractResult<()> {
            let mut guard = self.changelogs.lock().unwrap();
            let existing = guard
                .entry(incoming.contract_id)
                .or_insert_with(|| ContractChangelog::new(incoming.contract_id));
            existing.append(incoming.entries.clone());
            Ok(())
        }

        fn load_changelog(&self, _collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog> {
            Ok(self
                .changelogs
                .lock()
                .unwrap()
                .get(&contract_id)
                .cloned()
                .unwrap_or_else(|| ContractChangelog::new(contract_id)))
        }
    }

    /// In-memory collection mock. Only `get` is exercised by the service;
    /// the other methods are stubs so we can satisfy the trait without
    /// pulling in `rocket-infra`. `fail_get` lets tests simulate a
    /// collection-load failure to exercise error-propagation paths.
    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
        fail_get: bool,
    }

    impl MockCollectionRepo {
        fn with_collection(collection: Collection) -> Self {
            Self {
                collections: Mutex::new(vec![collection]),
                fail_get: false,
            }
        }

        fn failing() -> Self {
            Self {
                collections: Mutex::new(Vec::new()),
                fail_get: true,
            }
        }
    }

    impl CollectionRepository for MockCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, name: &str) -> DomainResult<Collection> {
            if self.fail_get {
                return Err(DomainError::NotFound(name.into()));
            }
            self.collections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
            self.get(name)
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<Request> {
            unimplemented!()
        }
        fn save_request(&self, _: &str, path: &str, _: &Request) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(CollectionSettings::default())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
    }

    const COLLECTION_NAME: &str = "demo";

    fn make_service() -> ContractService {
        // Default service for tests that don't care about the tree walk.
        // Seed an empty collection so `attach_contract` finds it.
        let mut empty = Collection::new(COLLECTION_NAME);
        empty.root.dir_name = Some(COLLECTION_NAME.into());
        ContractService::new(
            Arc::new(MockContractRepo::default()),
            Arc::new(MockCollectionRepo::with_collection(empty)),
        )
    }

    fn make_service_with_collection(collection: Collection) -> ContractService {
        ContractService::new(
            Arc::new(MockContractRepo::default()),
            Arc::new(MockCollectionRepo::with_collection(collection)),
        )
    }

    fn root() -> &'static Path {
        Path::new("/tmp/demo")
    }

    fn make_contract() -> Contract {
        Contract {
            id: Ulid::new(),
            title: "Test API".into(),
            provider: ContractParty::from_name("Team A"),
            consumers: vec![ContractParty::from_name("Team B")],
            project: "Project X".into(),
            version: "v1.0".into(),
            status: ContractStatus::default(),
            effective_date: Utc::now().date_naive(),
            expiry_date: None,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Collection,
            policy: ContractPolicy::default(),
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_by: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn make_snap(path: &str) -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from(path),
            method: "GET".into(),
            url_pattern: "/test".into(),
            query_param_keys: vec!["q".into()],
            header_keys: vec![],
            body_field_keys: vec![],
            auth_type: "none".into(),
            captured_at: Utc::now(),
            headers: vec![],
            query_params: vec![],
            body_content: None,
            form_fields: vec![],
            auth_detail: String::new(),
        }
    }

    /// Build a collection that mirrors what `build_folder_tree` produces:
    /// one request at the root and one request inside a subfolder, both
    /// carrying `file_name` so the walker can compute their on-disk paths.
    fn make_collection_with_two_requests() -> Collection {
        let mut collection = Collection::new(COLLECTION_NAME);
        collection.root.dir_name = Some(COLLECTION_NAME.into());

        let mut root_req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        root_req.file_name = Some("get-users.yml".into());
        collection.root.add_request(root_req);

        let mut subfolder = Folder::new("auth");
        subfolder.dir_name = Some("auth".into());
        let mut login = Request::new("Login", HttpMethod::Post, "https://api.example.com/login");
        login.file_name = Some("login.yml".into());
        subfolder.add_request(login);
        collection.root.add_subfolder(subfolder);

        collection
    }

    #[test]
    fn attach_and_list() {
        let svc = make_service();
        let contract = svc
            .attach_contract(root(), make_contract(), vec![], vec![])
            .unwrap();
        let list = svc.list_contracts(root()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, contract.id);
    }

    #[test]
    fn unchanged_save_produces_empty_changelog() {
        let svc = make_service();
        let snap = make_snap("requests/test.yml");
        let contract = svc
            .attach_contract(root(), make_contract(), vec![snap.clone()], vec![])
            .unwrap();
        svc.on_request_saved(root(), snap).unwrap();
        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(log.entries.is_empty());
    }

    #[test]
    fn changed_method_logged_in_changelog() {
        let svc = make_service();
        let snap = make_snap("requests/test.yml");
        let contract = svc
            .attach_contract(root(), make_contract(), vec![snap.clone()], vec![])
            .unwrap();
        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(root(), changed).unwrap();
        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn delete_removes_all_files() {
        let svc = make_service();
        let contract = svc
            .attach_contract(root(), make_contract(), vec![], vec![])
            .unwrap();
        svc.delete_contract(root(), contract.id).unwrap();
        assert!(svc.get_contract(root(), contract.id).is_err());
    }

    #[test]
    fn attach_takes_baseline_snapshot_of_all_covered_requests() {
        // Regression test for C1: when a contract is attached to a non-empty
        // collection, the service must walk the tree and capture a baseline
        // for every covered request. An empty `initial_snapshots` must not
        // leave an empty baseline.
        let collection = make_collection_with_two_requests();
        let svc = make_service_with_collection(collection);

        let contract = svc
            .attach_contract(root(), make_contract(), vec![], vec![])
            .unwrap();

        let snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
        assert_eq!(snapshot.entries.len(), 2);

        let root_entry = snapshot
            .get(Path::new("get-users.yml"))
            .expect("root-level request must be in the baseline");
        assert_eq!(root_entry.method, "GET");
        assert_eq!(root_entry.url_pattern, "https://api.example.com/users");

        let nested_entry = snapshot
            .get(Path::new("auth/login.yml"))
            .expect("nested request must be in the baseline");
        assert_eq!(nested_entry.method, "POST");
    }

    #[test]
    fn first_modification_after_attach_is_logged() {
        // End-to-end proof that C1 is fixed: attach on a non-empty collection,
        // then save a modified version of a pre-existing request. The first
        // modification must land in the changelog, not silently fall through.
        let collection = make_collection_with_two_requests();
        let svc = make_service_with_collection(collection);

        let contract = svc
            .attach_contract(root(), make_contract(), vec![], vec![])
            .unwrap();

        // Simulate the save hook emitting a changed shape for the nested
        // request — the method flips from POST to PUT.
        let mut changed_login = Request::new("Login", HttpMethod::Put, "https://api.example.com/login");
        changed_login.file_name = Some("login.yml".into());
        let new_snap = RequestSignatureSnapshot::from_request("auth/login.yml", &changed_login);
        svc.on_request_saved(root(), new_snap).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(!log.entries.is_empty(), "first modification must be logged");
        assert!(log.entries.iter().any(|e| e.field == "method"));
    }

    /// Build a collection with a single request at `login.yml` whose method
    /// is POST. Used to verify the walker-vs-explicit precedence rule.
    fn make_collection_with_single_login() -> Collection {
        let mut collection = Collection::new(COLLECTION_NAME);
        collection.root.dir_name = Some(COLLECTION_NAME.into());
        let mut login = Request::new("Login", HttpMethod::Post, "https://api.example.com/login");
        login.file_name = Some("login.yml".into());
        collection.root.add_request(login);
        collection
    }

    fn make_snap_with_method(path: &str, method: &str) -> RequestSignatureSnapshot {
        let mut snap = make_snap(path);
        snap.method = method.into();
        snap
    }

    #[test]
    fn initial_snapshots_override_walked_baseline() {
        // Explicit `initial_snapshots` must win over values the walker
        // produces for the same request path. This pins the loop order
        // so a future refactor cannot silently flip precedence.
        let svc = make_service_with_collection(make_collection_with_single_login());

        let override_snap = make_snap_with_method("login.yml", "CUSTOM");
        let contract = svc
            .attach_contract(
                root(),
                make_contract(),
                vec![override_snap],
                vec![],
            )
            .unwrap();

        let snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
        assert_eq!(snapshot.entries.len(), 1);
        let entry = snapshot
            .get(Path::new("login.yml"))
            .expect("login.yml must be in the baseline");
        assert_eq!(entry.method, "CUSTOM");
    }

    /// Build a two-folder collection: `a/x.yml` and `b/y.yml`.
    fn make_collection_with_two_folders() -> Collection {
        let mut collection = Collection::new(COLLECTION_NAME);
        collection.root.dir_name = Some(COLLECTION_NAME.into());

        let mut folder_a = Folder::new("a");
        folder_a.dir_name = Some("a".into());
        let mut req_x = Request::new("X", HttpMethod::Get, "/x");
        req_x.file_name = Some("x.yml".into());
        folder_a.add_request(req_x);
        collection.root.add_subfolder(folder_a);

        let mut folder_b = Folder::new("b");
        folder_b.dir_name = Some("b".into());
        let mut req_y = Request::new("Y", HttpMethod::Get, "/y");
        req_y.file_name = Some("y.yml".into());
        folder_b.add_request(req_y);
        collection.root.add_subfolder(folder_b);

        collection
    }

    #[test]
    fn attach_folder_scope_excludes_requests_outside_folder() {
        // ContractScope::Folder { rel_path: "a" } must snapshot only requests
        // under the `a` folder, not sibling folders.
        let svc = make_service_with_collection(make_collection_with_two_folders());

        let mut contract = make_contract();
        contract.scope = ContractScope::Folder { rel_path: PathBuf::from("a") };

        let attached = svc
            .attach_contract(root(), contract, vec![], vec![])
            .unwrap();

        let snapshot = svc.repo.load_snapshot(root(), attached.id).unwrap();
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].request_path, PathBuf::from("a/x.yml"));
    }

    #[test]
    fn attach_request_scope_snapshots_only_one_request() {
        // ContractScope::Request must snapshot exactly that one request,
        // ignoring everything else in the collection.
        let mut collection = Collection::new(COLLECTION_NAME);
        collection.root.dir_name = Some(COLLECTION_NAME.into());
        let mut foo = Request::new("Foo", HttpMethod::Get, "/foo");
        foo.file_name = Some("foo.yml".into());
        collection.root.add_request(foo);
        let mut bar = Request::new("Bar", HttpMethod::Get, "/bar");
        bar.file_name = Some("bar.yml".into());
        collection.root.add_request(bar);

        let svc = make_service_with_collection(collection);

        let mut contract = make_contract();
        contract.scope = ContractScope::Request { rel_path: PathBuf::from("foo.yml") };

        let attached = svc
            .attach_contract(root(), contract, vec![], vec![])
            .unwrap();

        let snapshot = svc.repo.load_snapshot(root(), attached.id).unwrap();
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].request_path, PathBuf::from("foo.yml"));
    }

    #[test]
    fn attach_propagates_collection_load_error() {
        // If the collection read fails, the service must surface the error
        // as ContractError::Internal and must NOT leave an orphan contract
        // file on disk. This is the regression guard for the fix that moves
        // the collection load ahead of all writes.
        let contract_repo = Arc::new(MockContractRepo::default());
        let svc = ContractService::new(
            Arc::clone(&contract_repo) as Arc<dyn ContractRepository>,
            Arc::new(MockCollectionRepo::failing()),
        );

        let result = svc.attach_contract(root(), make_contract(), vec![], vec![]);

        match result {
            Err(ContractError::Internal(_)) => {}
            other => panic!("expected ContractError::Internal, got {:?}", other),
        }

        // No contract should have been persisted.
        let contracts = svc.list_contracts(root()).unwrap();
        assert!(
            contracts.is_empty(),
            "collection load failure must not leave an orphan contract behind"
        );
    }

    struct CapturingPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    #[test]
    fn attach_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher { captured: Mutex::new(vec![]) });
        let mut empty = Collection::new(COLLECTION_NAME);
        empty.root.dir_name = Some(COLLECTION_NAME.into());
        let svc = ContractService::new_with_audit(
            Arc::new(MockContractRepo::default()),
            Arc::new(MockCollectionRepo::with_collection(empty)),
            publisher.clone(),
        );

        svc.attach_contract(root(), make_contract(), vec![], vec![])
            .unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            captured
                .iter()
                .any(|k| matches!(k, AuditEventKind::ContractAttached { .. })),
            "expected ContractAttached, got {:?}",
            *captured
        );
    }

    #[test]
    fn duplicate_contract_creates_draft_copy() {
        let svc = make_service();
        // Persist a contract with a semver version.
        let mut source = make_contract();
        source.version = "1.2.3".into();
        source.status = ContractStatus::Active;
        source.drift_count = 5;
        source.breach_count = 2;
        svc.repo.save_contract(root(), &source).unwrap();

        let copy = svc.duplicate_contract(root(), source.id).unwrap();

        // New identity
        assert_ne!(copy.id, source.id);
        // Title gets the suffix
        assert!(copy.title.contains("(copy)"), "title must contain '(copy)': {}", copy.title);
        // Version patch is bumped
        assert_eq!(copy.version, "1.2.4");
        // Status is reset to Draft regardless of source status
        assert_eq!(copy.status, ContractStatus::Draft);
        // Drift / breach counters are zeroed
        assert_eq!(copy.drift_count, 0);
        assert_eq!(copy.breach_count, 0);
        // Persisted
        svc.repo.load_contract(root(), copy.id).expect("duplicate must be persisted");
    }

    #[test]
    fn recompute_drift_no_snapshots_returns_empty_summaries() {
        // Contracts with no snapshot (Draft state) are skipped; result is empty.
        let svc = make_service();
        let result = svc.recompute_drift_for_collection(root());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn bump_patch_version_handles_edge_cases() {
        // Standard semver — happy path
        assert_eq!(super::bump_patch_version("1.2.3"), "1.2.4");
        assert_eq!(super::bump_patch_version("0.0.0"), "0.0.1");
        // Non-semver — fallback branch
        assert_eq!(super::bump_patch_version("v2"), "v2-copy");
        assert_eq!(super::bump_patch_version("1.0"), "1.0-copy");
        assert_eq!(super::bump_patch_version(""), "-copy");
    }

    // -----------------------------------------------------------------------
    // publish_contract / transition_contract_status / renew_contract
    // -----------------------------------------------------------------------

    fn make_contract_with_status(status: ContractStatus) -> Contract {
        let mut c = make_contract();
        c.status = status;
        c
    }

    #[test]
    fn publish_draft_contract_sets_active_status() {
        let svc = make_service();
        // Attach a Draft contract first so it is persisted.
        let draft = {
            let mut c = make_contract();
            c.status = ContractStatus::Draft;
            c.id = Ulid::new();
            svc.repo.save_contract(root(), &c).unwrap();
            c
        };
        let result = svc.publish_contract(root(), draft.id, vec![]).unwrap();
        assert_eq!(result.status, ContractStatus::Active);

        let persisted = svc.repo.load_contract(root(), draft.id).unwrap();
        assert_eq!(persisted.status, ContractStatus::Active);
    }

    #[test]
    fn publish_contract_with_snapshots_sets_endpoint_count() {
        let svc = make_service();
        let mut draft = make_contract();
        draft.status = ContractStatus::Draft;
        let draft = svc.attach_contract(root(), draft, vec![], vec![]).unwrap();

        let snaps = vec![make_snap("a.yml"), make_snap("b.yml")];
        let published = svc.publish_contract(root(), draft.id, snaps).unwrap();
        assert_eq!(published.endpoint_count, 2);

        let persisted = svc.repo.load_contract(root(), draft.id).unwrap();
        assert_eq!(persisted.endpoint_count, 2);
        assert_eq!(persisted.status, ContractStatus::Active);
    }

    #[test]
    fn transition_contract_status_pause_sets_paused() {
        let svc = make_service();
        let mut contract = make_contract_with_status(ContractStatus::Active);
        contract.id = Ulid::new();
        svc.repo.save_contract(root(), &contract).unwrap();

        let result = svc
            .transition_contract_status(root(), contract.id, StatusEvent::Pause)
            .unwrap();
        assert_eq!(result.status, ContractStatus::Paused);
    }

    #[test]
    fn renew_expired_contract_resets_counts() {
        let svc = make_service();
        let mut contract = make_contract_with_status(ContractStatus::Expired);
        contract.id = Ulid::new();
        contract.drift_count = 5;
        contract.breach_count = 3;
        svc.repo.save_contract(root(), &contract).unwrap();

        let result = svc
            .renew_contract(root(), contract.id, None)
            .unwrap();
        assert_eq!(result.status, ContractStatus::Active);
        assert_eq!(result.drift_count, 0);
        assert_eq!(result.breach_count, 0);
    }

    #[test]
    fn publish_contract_from_breach_resigns_to_active() {
        let svc = make_service();
        let mut contract = make_contract_with_status(ContractStatus::Breach);
        contract.id = Ulid::new();
        svc.repo.save_contract(root(), &contract).unwrap();

        // Calling publish_contract on a Breach contract must succeed via Resign.
        let result = svc.publish_contract(root(), contract.id, vec![]).unwrap();
        assert_eq!(result.status, ContractStatus::Active);
    }

    #[test]
    fn publish_contract_from_drift_resigns_to_active() {
        let svc = make_service();
        let mut contract = make_contract_with_status(ContractStatus::Drift);
        contract.id = Ulid::new();
        svc.repo.save_contract(root(), &contract).unwrap();

        let result = svc.publish_contract(root(), contract.id, vec![]).unwrap();
        assert_eq!(result.status, ContractStatus::Active);
    }

    #[test]
    fn recompute_drift_idempotent_when_counts_unchanged() {
        // Verifies the file-watcher feedback loop guard: calling recompute_drift
        // twice with the same (empty) snapshot set must NOT write the contract
        // file on the second call. The test checks this by asserting that
        // updated_at does not change between the two calls.
        let svc = make_service();
        let mut contract = make_contract_with_status(ContractStatus::Active);
        contract.id = Ulid::new();
        contract.drift_count = 2;
        contract.breach_count = 2;
        let fixed_time = chrono::Utc::now();
        contract.updated_at = Some(fixed_time);
        svc.repo.save_contract(root(), &contract).unwrap();

        // First call — counts already match what empty collection would compute.
        let summaries1 = svc
            .recompute_drift_for_collection(root())
            .unwrap();
        let persisted1 = svc.repo.load_contract(root(), contract.id).unwrap();

        // Second call — same collection, same result; must NOT update updated_at.
        let summaries2 = svc
            .recompute_drift_for_collection(root())
            .unwrap();
        let persisted2 = svc.repo.load_contract(root(), contract.id).unwrap();

        // Both calls return a summary for the contract.
        assert!(!summaries1.is_empty());
        assert!(!summaries2.is_empty());

        // Second call must NOT have written the contract file — updated_at unchanged.
        assert_eq!(
            persisted1.updated_at, persisted2.updated_at,
            "recompute_drift wrote the contract file on a no-op second call — \
             this would trigger the file-watcher feedback loop"
        );
    }

    #[test]
    fn folder_scope_logs_changes_inside_folder() {
        // Attach a contract scoped to the "auth" folder with a baseline snapshot
        // for auth/login.yml, then save a modified version of that request.
        // Expect: one changelog entry (method change).
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Folder { rel_path: PathBuf::from("auth") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap.clone()], vec![])
            .unwrap();

        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(root(), changed).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert_eq!(
            log.entries.len(),
            1,
            "folder-scoped contract must log changes to requests inside the folder"
        );
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn folder_scope_ignores_requests_outside_folder() {
        // Attach a contract scoped to the "auth" folder, then save a request
        // from a DIFFERENT folder. Expect: no changelog entries.
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Folder { rel_path: PathBuf::from("auth") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap], vec![])
            .unwrap();

        // "payments/pay.yml" is outside the "auth" folder.
        let outside = make_snap_with_method("payments/pay.yml", "POST");
        svc.on_request_saved(root(), outside).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(
            log.entries.is_empty(),
            "folder-scoped contract must not log changes to requests outside the folder"
        );
    }

    #[test]
    fn request_scope_logs_changes_for_covered_request() {
        // Attach a contract scoped to the single request "auth/login.yml", then
        // save a modified version of that exact request.
        // Expect: one changelog entry (method change).
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Request { rel_path: PathBuf::from("auth/login.yml") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap.clone()], vec![])
            .unwrap();

        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(root(), changed).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert_eq!(
            log.entries.len(),
            1,
            "request-scoped contract must log changes to the covered request"
        );
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn request_scope_ignores_other_requests() {
        // Attach a contract scoped to "auth/login.yml", then save a DIFFERENT
        // request. Expect: no changelog entries.
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Request { rel_path: PathBuf::from("auth/login.yml") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap], vec![])
            .unwrap();

        let other = make_snap_with_method("auth/register.yml", "POST");
        svc.on_request_saved(root(), other).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(
            log.entries.is_empty(),
            "request-scoped contract must not log changes to other requests"
        );
    }

    // ── extract_server_and_path ──────────────────────────────────────────────
    #[test]
    fn extract_full_https_url() {
        let (server, path) = super::openapi::extract_server_and_path(
            "https://api.example.com/users",
        );
        assert_eq!(server, Some("https://api.example.com".to_string()));
        assert_eq!(path, "/users");
    }

    #[test]
    fn extract_full_http_url_with_port_and_nested_path() {
        let (server, path) = super::openapi::extract_server_and_path(
            "http://localhost:3000/api/v1/users",
        );
        assert_eq!(server, Some("http://localhost:3000".to_string()));
        assert_eq!(path, "/api/v1/users");
    }

    #[test]
    fn extract_path_only_url() {
        let (server, path) = super::openapi::extract_server_and_path("/users");
        assert_eq!(server, None);
        assert_eq!(path, "/users");
    }

    #[test]
    fn extract_template_variable_url() {
        let (server, path) = super::openapi::extract_server_and_path("{{baseUrl}}/users");
        assert_eq!(server, Some("{{baseUrl}}".to_string()));
        assert_eq!(path, "/users");
    }

    #[test]
    fn extract_url_strips_query_string_from_path() {
        let (server, path) = super::openapi::extract_server_and_path(
            "https://api.example.com/users?page=1&limit=20",
        );
        assert_eq!(server, Some("https://api.example.com".to_string()));
        assert_eq!(path, "/users");
    }

    // ── tag_from_request_path ────────────────────────────────────────────────
    #[test]
    fn tag_from_nested_path_returns_first_segment() {
        use std::path::Path;
        assert_eq!(
            super::openapi::tag_from_request_path(Path::new("users/get-users.yml")),
            Some("users".to_string()),
        );
    }

    #[test]
    fn tag_from_deep_nested_path_returns_first_segment() {
        use std::path::Path;
        assert_eq!(
            super::openapi::tag_from_request_path(Path::new("auth/v2/login.yml")),
            Some("auth".to_string()),
        );
    }

    #[test]
    fn tag_from_root_level_file_returns_none() {
        use std::path::Path;
        assert_eq!(
            super::openapi::tag_from_request_path(Path::new("root-request.yml")),
            None,
        );
    }
}

// ─── OpenAPI export types ─────────────────────────────────────────────────

mod openapi {
    use serde::Serialize;
    use std::collections::BTreeMap;

    /// Splits a raw url_pattern into (optional_server_base, path_string).
    ///
    /// "https://api.example.com/users"  → (Some("https://api.example.com"), "/users")
    /// "http://localhost:3000/api/v1"   → (Some("http://localhost:3000"), "/api/v1")
    /// "/users"                          → (None, "/users")
    /// "{{baseUrl}}/users"              → (Some("{{baseUrl}}"), "/users")
    pub fn extract_server_and_path(url_pattern: &str) -> (Option<String>, String) {
        // Strip query string first.
        let url = url_pattern.split('?').next().unwrap_or(url_pattern);

        // Handle http:// and https:// URLs.
        for scheme in &["https://", "http://"] {
            if let Some(after_scheme) = url.strip_prefix(scheme) {
                return if let Some(slash_pos) = after_scheme.find('/') {
                    let server = format!("{}{}", scheme, &after_scheme[..slash_pos]);
                    let path = after_scheme[slash_pos..].to_string();
                    (Some(server), path)
                } else {
                    // URL with no path component (e.g. "https://api.example.com")
                    (Some(url.to_string()), "/".to_string())
                };
            }
        }

        // Handle template variable prefix like "{{baseUrl}}/users".
        if let Some(brace_end) = url.find("}/") {
            let server = url[..brace_end + 1].to_string();
            let path = url[brace_end + 1..].to_string();
            return (Some(server), path);
        }

        // Path-only URL — ensure leading slash.
        let path = if url.starts_with('/') {
            url.to_string()
        } else {
            format!("/{}", url)
        };
        (None, path)
    }

    /// Returns the first directory segment of a collection-relative request path
    /// as an OpenAPI tag, or None for root-level files.
    ///
    /// "users/get-users.yml" → Some("users")
    /// "auth/v2/login.yml"   → Some("auth")
    /// "root-request.yml"    → None
    pub fn tag_from_request_path(request_path: &std::path::Path) -> Option<String> {
        use std::path::Component;
        let mut components = request_path.components();
        let first = components.next()?;
        // If there's no second component, the file is at the root (no folder tag).
        let _has_more = components.next()?;
        if let Component::Normal(s) = first {
            s.to_str().map(|s| s.to_string())
        } else {
            None
        }
    }

    #[derive(Serialize)]
    pub struct OpenApiDoc {
        pub openapi: &'static str,
        pub info: InfoObject,
        pub servers: Vec<ServerObject>,
        pub paths: BTreeMap<String, BTreeMap<String, OperationObject>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub components: Option<ComponentsObject>,
    }

    #[derive(Serialize)]
    pub struct ServerObject {
        pub url: String,
    }

    #[derive(Serialize)]
    pub struct InfoObject {
        pub title: String,
        pub version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub contact: Option<ContactObject>,
        #[serde(rename = "x-contract-id")]
        pub x_contract_id: String,
        #[serde(rename = "x-contract-status")]
        pub x_contract_status: String,
        #[serde(rename = "x-contract-enforcement-mode")]
        pub x_contract_enforcement_mode: String,
        #[serde(rename = "x-contract-provider")]
        pub x_contract_provider: PartyValue,
        #[serde(rename = "x-contract-consumers")]
        pub x_contract_consumers: Vec<PartyValue>,
        #[serde(rename = "x-contract-effective-date")]
        pub x_contract_effective_date: String,
        #[serde(rename = "x-contract-expiry-date", skip_serializing_if = "Option::is_none")]
        pub x_contract_expiry_date: Option<String>,
        #[serde(rename = "x-contract-policy")]
        pub x_contract_policy: PolicyValue,
        #[serde(rename = "x-contract-scope")]
        pub x_contract_scope: String,
        #[serde(rename = "x-contract-drift-count")]
        pub x_contract_drift_count: u32,
        #[serde(rename = "x-contract-breach-count")]
        pub x_contract_breach_count: u32,
        #[serde(rename = "x-contract-endpoint-count")]
        pub x_contract_endpoint_count: u32,
        #[serde(rename = "x-contract-document-paths", skip_serializing_if = "Vec::is_empty")]
        pub x_contract_document_paths: Vec<String>,
        #[serde(rename = "x-contract-created-by", skip_serializing_if = "Option::is_none")]
        pub x_contract_created_by: Option<String>,
        #[serde(rename = "x-contract-created-at", skip_serializing_if = "Option::is_none")]
        pub x_contract_created_at: Option<String>,
        #[serde(rename = "x-contract-updated-at", skip_serializing_if = "Option::is_none")]
        pub x_contract_updated_at: Option<String>,
    }

    #[derive(Serialize)]
    pub struct ContactObject {
        pub name: String,
    }

    #[derive(Serialize)]
    pub struct PartyValue {
        pub id: String,
        pub name: String,
        pub kind: String,
    }

    #[derive(Serialize)]
    pub struct PolicyValue {
        #[serde(rename = "breakingChangePolicy")]
        pub breaking_change_policy: String,
        #[serde(rename = "noticeDays")]
        pub notice_days: u32,
        #[serde(rename = "uptimeSla", skip_serializing_if = "Option::is_none")]
        pub uptime_sla: Option<f32>,
    }

    #[derive(Serialize)]
    pub struct ParameterObject {
        pub name: String,
        #[serde(rename = "in")]
        pub location: &'static str,
        pub schema: SchemaObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub example: Option<String>,
    }

    #[derive(Serialize)]
    pub struct SchemaObject {
        #[serde(rename = "type")]
        pub schema_type: &'static str,
    }

    #[derive(Serialize)]
    pub struct RequestBodyObject {
        pub required: bool,
        pub content: BTreeMap<String, MediaTypeObject>,
    }

    #[derive(Serialize)]
    pub struct MediaTypeObject {
        pub schema: SchemaObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub example: Option<serde_yaml::Value>,
    }

    #[derive(Serialize)]
    pub struct ResponseObject {
        pub description: &'static str,
    }

    #[derive(Serialize)]
    pub struct OperationObject {
        #[serde(rename = "operationId")]
        pub operation_id: String,
        pub summary: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub tags: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub parameters: Vec<ParameterObject>,
        #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
        pub request_body: Option<RequestBodyObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub security: Option<Vec<BTreeMap<String, Vec<String>>>>,
        pub responses: BTreeMap<String, ResponseObject>,
        #[serde(rename = "x-source-path", skip_serializing_if = "Option::is_none")]
        pub x_source_path: Option<String>,
        #[serde(rename = "x-captured-at", skip_serializing_if = "Option::is_none")]
        pub x_captured_at: Option<String>,
        #[serde(rename = "x-auth-detail", skip_serializing_if = "Option::is_none")]
        pub x_auth_detail: Option<String>,
    }

    #[derive(Serialize)]
    pub struct ComponentsObject {
        #[serde(rename = "securitySchemes")]
        pub security_schemes: BTreeMap<String, SecuritySchemeObject>,
    }

    #[derive(Serialize)]
    pub struct SecuritySchemeObject {
        #[serde(rename = "type")]
        pub scheme_type: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub scheme: Option<&'static str>,
        #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
        pub location: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub flows: Option<serde_yaml::Value>,
    }
}
