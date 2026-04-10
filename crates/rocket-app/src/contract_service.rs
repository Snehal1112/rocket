use rocket_collection::contract::{
    changelog::{ChangelogEntry, ContractChangelog},
    diff::diff_signature,
    repository::{ContractRepository, ContractResult},
    snapshot::{ContractSnapshot, RequestSignatureSnapshot},
    types::{Contract, ContractEnforcementMode, ContractScope},
};
use std::{path::Path, sync::Arc};
use ulid::Ulid;

pub struct ContractService {
    repo: Arc<dyn ContractRepository>,
}

impl ContractService {
    pub fn new(repo: Arc<dyn ContractRepository>) -> Self {
        Self { repo }
    }

    /// Create a new contract and take an initial snapshot of all covered requests.
    pub fn attach_contract(
        &self,
        collection_root: &Path,
        mut contract: Contract,
        initial_snapshots: Vec<RequestSignatureSnapshot>,
    ) -> ContractResult<Contract> {
        contract.id = Ulid::new();
        // Force Informational — Model B variants not yet UI-reachable.
        contract.enforcement_mode = ContractEnforcementMode::Informational;

        self.repo.save_contract(collection_root, &contract)?;

        let mut snapshot = ContractSnapshot::new(contract.id);
        for snap in initial_snapshots {
            snapshot.upsert(snap);
        }
        self.repo.save_snapshot(collection_root, &snapshot)?;

        // Initialise empty changelog.
        let changelog = ContractChangelog::new(contract.id);
        self.repo.append_changelog(collection_root, &changelog)?;

        Ok(contract)
    }

    pub fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>> {
        self.repo.list_contracts(collection_root)
    }

    pub fn get_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract> {
        self.repo.load_contract(collection_root, id)
    }

    pub fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()> {
        self.repo.delete_contract(collection_root, id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rocket_collection::contract::repository::ContractError;
    use std::collections::HashMap;
    use std::path::PathBuf;
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

    fn make_service() -> ContractService {
        ContractService::new(Arc::new(MockContractRepo::default()))
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
            document_path: None,
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
        }
    }

    #[test]
    fn attach_and_list() {
        let svc = make_service();
        let contract = svc.attach_contract(root(), make_contract(), vec![]).unwrap();
        let list = svc.list_contracts(root()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, contract.id);
    }

    #[test]
    fn unchanged_save_produces_empty_changelog() {
        let svc = make_service();
        let snap = make_snap("requests/test.yml");
        let contract = svc
            .attach_contract(root(), make_contract(), vec![snap.clone()])
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
            .attach_contract(root(), make_contract(), vec![snap.clone()])
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
        let contract = svc.attach_contract(root(), make_contract(), vec![]).unwrap();
        svc.delete_contract(root(), contract.id).unwrap();
        assert!(svc.get_contract(root(), contract.id).is_err());
    }
}
