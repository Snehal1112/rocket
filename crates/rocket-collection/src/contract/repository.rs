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
    #[error("Internal error: {0}")]
    Internal(String),
}

pub trait ContractRepository: Send + Sync {
    /// Persist a new or updated contract definition.
    fn save_contract(&self, collection_root: &Path, contract: &Contract) -> ContractResult<()>;

    /// Load a contract by id.
    fn load_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract>;

    /// List all contracts for a collection.
    fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>>;

    /// Delete contract + its snapshot + its changelog + its attachments directory.
    fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()>;

    /// Persist snapshot (overwrite — snapshot always tracks latest state).
    fn save_snapshot(&self, collection_root: &Path, snapshot: &ContractSnapshot) -> ContractResult<()>;

    /// Load snapshot. Returns empty snapshot if file does not exist yet.
    fn load_snapshot(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractSnapshot>;

    /// Append entries to changelog (never overwrites existing entries).
    fn append_changelog(&self, collection_root: &Path, changelog: &ContractChangelog) -> ContractResult<()>;

    /// Load full changelog. Returns empty changelog if file does not exist.
    fn load_changelog(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog>;

    /// Load the tracking snapshot (updated on every request save).
    /// Falls back to the baseline snapshot if no tracking file exists yet.
    fn load_tracking_snapshot(
        &self,
        collection_root: &Path,
        contract_id: Ulid,
    ) -> ContractResult<ContractSnapshot> {
        // Default: fall back to the baseline snapshot so existing impls work.
        self.load_snapshot(collection_root, contract_id)
    }

    /// Persist the tracking snapshot (updated on every request save).
    /// The baseline snapshot (`save_snapshot`) is left untouched.
    fn save_tracking_snapshot(
        &self,
        collection_root: &Path,
        snapshot: &ContractSnapshot,
    ) -> ContractResult<()> {
        // Default: no-op (tracking not persisted by this impl).
        let _ = (collection_root, snapshot);
        Ok(())
    }
}
