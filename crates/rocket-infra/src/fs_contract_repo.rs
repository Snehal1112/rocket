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

    pub fn attachments_dir(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join("attachments").join(id.to_string())
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
        // Remove the entire attachments directory for this contract.
        let attachments = Self::attachments_dir(collection_root, id);
        if attachments.exists() {
            std::fs::remove_dir_all(attachments)?;
        }
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
        // Load existing, append new entries, write back.
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
