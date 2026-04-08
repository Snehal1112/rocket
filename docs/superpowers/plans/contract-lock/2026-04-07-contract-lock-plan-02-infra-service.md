# Contract Lock — Plan 02: Infra Repo + ContractService + Save Hook

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add filesystem persistence for contracts and wire the silent audit-log hook into the existing request save path.

**Architecture:** `ContractRepository` trait in `rocket-collection`. `FsContractRepo` in `rocket-infra` writes three `.yml` files per contract under `{collection_root}/.rocket/contracts/`. `ContractService` in `rocket-app` orchestrates CRUD and owns the save hook that diffs + appends changelog silently.

**Tech Stack:** Rust, `serde_yaml`, `rocket-collection`, `rocket-infra`, `rocket-app`

**Depends on:** Plan 01 merged.

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-collection/src/contract/repository.rs` | Create — `ContractRepository` trait |
| `crates/rocket-collection/src/contract/mod.rs` | Modify — export `repository` |
| `crates/rocket-infra/src/fs_contract_repo.rs` | Create — filesystem implementation |
| `crates/rocket-infra/src/lib.rs` | Modify — export `fs_contract_repo` |
| `crates/rocket-app/src/contract_service.rs` | Create — `ContractService` + save hook |
| `crates/rocket-app/src/lib.rs` | Modify — export `contract_service` |

---

## Task 1: Repository trait

**Files:**
- Create: `crates/rocket-collection/src/contract/repository.rs`
- Modify: `crates/rocket-collection/src/contract/mod.rs`

- [ ] **Step 1: Create `repository.rs`**

```rust
use crate::contract::{
    changelog::ContractChangelog,
    snapshot::ContractSnapshot,
    types::Contract,
};
use std::path::Path;
use ulid::Ulid;

pub type ContractResult<T> = Result<T, ContractError>;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("Contract not found: {0}")]
    NotFound(Ulid),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_yaml::Error),
}

pub trait ContractRepository: Send + Sync {
    /// Persist a new or updated contract definition.
    fn save_contract(&self, collection_root: &Path, contract: &Contract) -> ContractResult<()>;

    /// Load a contract by id.
    fn load_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract>;

    /// List all contracts for a collection.
    fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>>;

    /// Delete contract + its snapshot + its changelog.
    fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()>;

    /// Persist snapshot (overwrite — snapshot always tracks latest state).
    fn save_snapshot(&self, collection_root: &Path, snapshot: &ContractSnapshot) -> ContractResult<()>;

    /// Load snapshot. Returns empty snapshot if file does not exist yet.
    fn load_snapshot(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractSnapshot>;

    /// Append entries to changelog (never overwrites existing entries).
    fn append_changelog(&self, collection_root: &Path, changelog: &ContractChangelog) -> ContractResult<()>;

    /// Load full changelog. Returns empty changelog if file does not exist.
    fn load_changelog(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog>;
}
```

- [ ] **Step 2: Export from `mod.rs`**

Add to `crates/rocket-collection/src/contract/mod.rs`:

```rust
pub mod repository;

pub use repository::{ContractError, ContractRepository, ContractResult};
```

- [ ] **Step 3: Verify compile**

```bash
cargo check -p rocket-collection
```

Expected: clean.

---

## Task 2: Filesystem implementation

**Files:**
- Create: `crates/rocket-infra/src/fs_contract_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Create `fs_contract_repo.rs`**

```rust
use rocket_collection::contract::{
    changelog::ContractChangelog,
    repository::{ContractError, ContractRepository, ContractResult},
    snapshot::ContractSnapshot,
    types::Contract,
};
use std::path::Path;
use ulid::Ulid;

pub struct FsContractRepo;

impl FsContractRepo {
    fn contracts_dir(collection_root: &Path) -> std::path::PathBuf {
        collection_root.join(".rocket").join("contracts")
    }

    fn contract_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}.yml", id))
    }

    fn snapshot_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}-snapshot.yml", id))
    }

    fn changelog_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}-changelog.yml", id))
    }

    fn ensure_dir(collection_root: &Path) -> ContractResult<()> {
        std::fs::create_dir_all(Self::contracts_dir(collection_root))?;
        Ok(())
    }
}

impl ContractRepository for FsContractRepo {
    fn save_contract(&self, collection_root: &Path, contract: &Contract) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::contract_path(collection_root, contract.id);
        let yaml = serde_yaml::to_string(contract)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    fn load_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract> {
        let path = Self::contract_path(collection_root, id);
        let yaml = std::fs::read_to_string(&path)
            .map_err(|_| ContractError::NotFound(id))?;
        Ok(serde_yaml::from_str(&yaml)?)
    }

    fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>> {
        let dir = Self::contracts_dir(collection_root);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut contracts = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            // only plain contract files, not -snapshot or -changelog
            if name.ends_with(".yml")
                && !name.contains("-snapshot")
                && !name.contains("-changelog")
            {
                let yaml = std::fs::read_to_string(&path)?;
                if let Ok(c) = serde_yaml::from_str::<Contract>(&yaml) {
                    contracts.push(c);
                }
            }
        }
        Ok(contracts)
    }

    fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()> {
        let _ = std::fs::remove_file(Self::contract_path(collection_root, id));
        let _ = std::fs::remove_file(Self::snapshot_path(collection_root, id));
        let _ = std::fs::remove_file(Self::changelog_path(collection_root, id));
        Ok(())
    }

    fn save_snapshot(&self, collection_root: &Path, snapshot: &ContractSnapshot) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::snapshot_path(collection_root, snapshot.contract_id);
        let yaml = serde_yaml::to_string(snapshot)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    fn load_snapshot(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractSnapshot> {
        let path = Self::snapshot_path(collection_root, contract_id);
        if !path.exists() {
            return Ok(ContractSnapshot::new(contract_id));
        }
        let yaml = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&yaml)?)
    }

    fn append_changelog(&self, collection_root: &Path, incoming: &ContractChangelog) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::changelog_path(collection_root, incoming.contract_id);
        // Load existing, append new entries, write back
        let mut existing = if path.exists() {
            let yaml = std::fs::read_to_string(&path)?;
            serde_yaml::from_str::<ContractChangelog>(&yaml)?
        } else {
            ContractChangelog::new(incoming.contract_id)
        };
        existing.append(incoming.entries.clone());
        let yaml = serde_yaml::to_string(&existing)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    fn load_changelog(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog> {
        let path = Self::changelog_path(collection_root, contract_id);
        if !path.exists() {
            return Ok(ContractChangelog::new(contract_id));
        }
        let yaml = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&yaml)?)
    }
}
```

- [ ] **Step 2: Export from `crates/rocket-infra/src/lib.rs`**

Add alongside other `pub mod` lines:

```rust
pub mod fs_contract_repo;
pub use fs_contract_repo::FsContractRepo;
```

- [ ] **Step 3: Verify compile**

```bash
cargo check -p rocket-infra
```

Expected: clean.

---

## Task 3: ContractService + save hook

**Files:**
- Create: `crates/rocket-app/src/contract_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`

- [ ] **Step 1: Create `contract_service.rs`**

```rust
use chrono::Utc;
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
        // Force Informational — Model B variants not yet UI-reachable
        contract.enforcement_mode = ContractEnforcementMode::Informational;

        self.repo.save_contract(collection_root, &contract)?;

        let mut snapshot = ContractSnapshot::new(contract.id);
        for snap in initial_snapshots {
            snapshot.upsert(snap);
        }
        self.repo.save_snapshot(collection_root, &snapshot)?;

        // Initialise empty changelog
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
                        // Warn and Block are defined but unreachable until Model B sprint
                        ContractEnforcementMode::Warn | ContractEnforcementMode::Block => {
                            // TODO(model-b): emit event / return error
                        }
                    }
                }
            }

            // Always update snapshot to track current state
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
    use rocket_collection::contract::types::{ContractEnforcementMode, ContractScope};
    use rocket_infra::FsContractRepo;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_service() -> (ContractService, TempDir) {
        let dir = TempDir::new().unwrap();
        let svc = ContractService::new(Arc::new(FsContractRepo));
        (svc, dir)
    }

    fn make_contract() -> Contract {
        Contract {
            id: Ulid::new(),
            title: "Test API".into(),
            provider: "Team A".into(),
            consumer: "Team B".into(),
            project: "Project X".into(),
            version: "v1.0".into(),
            effective_date: chrono::Utc::now().date_naive(),
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
        let (svc, dir) = make_service();
        let contract = svc.attach_contract(dir.path(), make_contract(), vec![]).unwrap();
        let list = svc.list_contracts(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, contract.id);
    }

    #[test]
    fn unchanged_save_produces_empty_changelog() {
        let (svc, dir) = make_service();
        let snap = make_snap("requests/test.yml");
        let contract = svc.attach_contract(dir.path(), make_contract(), vec![snap.clone()]).unwrap();
        svc.on_request_saved(dir.path(), snap).unwrap();
        let log = svc.get_changelog(dir.path(), contract.id).unwrap();
        assert!(log.entries.is_empty());
    }

    #[test]
    fn changed_method_logged_in_changelog() {
        let (svc, dir) = make_service();
        let snap = make_snap("requests/test.yml");
        let contract = svc.attach_contract(dir.path(), make_contract(), vec![snap.clone()]).unwrap();
        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(dir.path(), changed).unwrap();
        let log = svc.get_changelog(dir.path(), contract.id).unwrap();
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn delete_removes_all_files() {
        let (svc, dir) = make_service();
        let contract = svc.attach_contract(dir.path(), make_contract(), vec![]).unwrap();
        svc.delete_contract(dir.path(), contract.id).unwrap();
        assert!(svc.get_contract(dir.path(), contract.id).is_err());
    }
}
```

- [ ] **Step 2: Export from `crates/rocket-app/src/lib.rs`**

Add alongside other `pub mod` declarations:

```rust
pub mod contract_service;
pub use contract_service::ContractService;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-app contract
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-collection/src/contract/repository.rs
git add crates/rocket-collection/src/contract/mod.rs
git add crates/rocket-infra/src/fs_contract_repo.rs
git add crates/rocket-infra/src/lib.rs
git add crates/rocket-app/src/contract_service.rs
git add crates/rocket-app/src/lib.rs
git commit -m "feat(contract): FsContractRepo, ContractService, save hook with Model B seam"
```
