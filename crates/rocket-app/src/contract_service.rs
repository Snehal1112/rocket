use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{
    contract::{
        changelog::{ChangelogEntry, ContractChangelog},
        diff::diff_signature,
        repository::{ContractError, ContractRepository, ContractResult},
        snapshot::{ContractSnapshot, RequestSignatureSnapshot},
        types::{Contract, ContractEnforcementMode, ContractScope},
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
        collection_name: &str,
        mut contract: Contract,
        initial_snapshots: Vec<RequestSignatureSnapshot>,
        attachment_sources: Vec<PathBuf>,
    ) -> ContractResult<Contract> {
        contract.id = Ulid::new();
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

        // Copy attachments into the collection before writing the contract YAML
        // so the relative paths are ready. If copy fails, no contract file is
        // written and no orphan is left behind.
        contract.document_paths = copy_attachments(collection_root, contract.id, &attachment_sources)?;

        // Only now write the contract, its baseline, and the empty changelog.
        self.repo.save_contract(collection_root, &contract)?;
        self.repo.save_snapshot(collection_root, &snapshot)?;
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

            let mut snapshot = self.repo.load_snapshot(collection_root, contract.id)?;

            if let Some(old_snap) = snapshot.get(&new_snap.request_path) {
                let changes = diff_signature(old_snap, &new_snap);

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

            // Always update snapshot to track current state.
            snapshot.upsert(new_snap.clone());
            self.repo.save_snapshot(collection_root, &snapshot)?;
        }

        Ok(())
    }
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rocket_collection::{
        contract::repository::ContractError, Collection, CollectionSettings, CollectionSummary,
        CollectionVariable, Request,
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
        Path::new("/tmp/mock")
    }

    fn make_contract() -> Contract {
        Contract {
            id: Ulid::new(),
            title: "Test API".into(),
            provider: "Team A".into(),
            consumer: "Team B".into(),
            project: "Project X".into(),
            version: "v1.0".into(),
            effective_date: Utc::now().date_naive(),
            expiry_date: None,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Collection,
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![snap.clone()], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![snap.clone()], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![])
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
                COLLECTION_NAME,
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
            .attach_contract(root(), COLLECTION_NAME, contract, vec![], vec![])
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
            .attach_contract(root(), COLLECTION_NAME, contract, vec![], vec![])
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

        let result = svc.attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![]);

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

        svc.attach_contract(root(), COLLECTION_NAME, make_contract(), vec![], vec![])
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
}
