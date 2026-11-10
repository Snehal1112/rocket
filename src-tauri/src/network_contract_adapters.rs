//! Adapters of the contract check ports over the contract service and the collection repository.
//!
//! Both only read. The conversion rules live in `rocket_app::network` so they are tested there.

use std::path::Path;
use std::sync::Arc;

use rocket_app::network::contract_check_match::LocalRequest;
use rocket_app::network::{
    local_requests_of, policy_to_wire, wire_changes, ContractChangesPort, LocalRequestsPort,
    PendingContract,
};
use rocket_app::ContractService;
use rocket_collection::contract::repository::ContractError;
use rocket_collection::CollectionRepository;
use rocket_shared::error::{DomainError, DomainResult};
use ulid::Ulid;

/// Reads a contract's pending changes through the contract service.
pub struct ServiceContractChanges {
    service: Arc<ContractService>,
}

impl ServiceContractChanges {
    pub fn new(service: Arc<ContractService>) -> Self {
        Self { service }
    }
}

/// A fixed IPC-facing error for a contract read. The detail is only logged.
fn contract_error(err: ContractError) -> DomainError {
    match err {
        ContractError::NotFound(_) => DomainError::NotFound("The contract was not found.".into()),
        other => {
            tracing::debug!(error = %other, "contract check read failed");
            DomainError::Internal("The contract changes could not be read.".into())
        }
    }
}

impl ContractChangesPort for ServiceContractChanges {
    fn pending_changes(
        &self,
        collection_root: &str,
        contract_id: &str,
    ) -> DomainResult<PendingContract> {
        let id = Ulid::from_string(contract_id)
            .map_err(|_| DomainError::InvalidInput("The contract id is not valid.".into()))?;
        let root = Path::new(collection_root);
        let contract = self
            .service
            .get_contract(root, id)
            .map_err(contract_error)?;
        let entries = self
            .service
            .pending_changes(root, id)
            .map_err(contract_error)?;
        Ok(PendingContract {
            name: contract.title,
            version: contract.version,
            policy: policy_to_wire(&contract.policy.breaking_change_policy),
            changes: wire_changes(&entries),
        })
    }
}

/// Reads the HTTP requests of every collection of the active workspace.
///
/// The repository must follow the active workspace (`SharedPathCollectionRepo`), so a switch
/// changes what is read and no other workspace is ever read.
pub struct RepoLocalRequests {
    collections: Arc<dyn CollectionRepository>,
}

impl RepoLocalRequests {
    pub fn new(collections: Arc<dyn CollectionRepository>) -> Self {
        Self { collections }
    }
}

impl LocalRequestsPort for RepoLocalRequests {
    fn local_requests(&self) -> DomainResult<Vec<LocalRequest>> {
        let mut locals = Vec::new();
        // The strict reads fail on a collection that cannot load and on a request file that
        // cannot be parsed. Skipping either could turn a break into `not_used`.
        for summary in self.collections.list_strict()? {
            let collection = self.collections.get_strict(&summary.name)?;
            locals.extend(local_requests_of(&collection));
        }
        Ok(locals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_infra::{FsAttachmentStore, FsContractRepo, SharedPathCollectionRepo};
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn workspace_repo(workspace: &Path) -> Arc<dyn CollectionRepository> {
        Arc::new(SharedPathCollectionRepo::new(Arc::new(Mutex::new(
            workspace.to_path_buf(),
        ))))
    }

    fn contract_changes(workspace: &Path) -> ServiceContractChanges {
        ServiceContractChanges::new(Arc::new(ContractService::new(
            Arc::new(FsContractRepo),
            workspace_repo(workspace),
            Arc::new(FsAttachmentStore),
        )))
    }

    fn collection_root(workspace: &Path) -> PathBuf {
        workspace.join("collections").join("orders")
    }

    #[test]
    fn local_requests_fail_on_an_unparseable_request_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = workspace_repo(dir.path());
        repo.create("orders").expect("create collection");
        std::fs::write(
            collection_root(dir.path()).join("broken.yml"),
            "info:\n  name: Broken\n  type: http\nhttp:\n  method: GET\n  url: https://example.com\n  body:\n    type: bogus\n    data: x\n",
        )
        .expect("write broken.yml");
        // The lenient read skips the broken file, so a lenient adapter would return a list.
        assert!(repo
            .get("orders")
            .expect("lenient get")
            .root
            .items
            .is_empty());

        let result = RepoLocalRequests::new(repo).local_requests();

        assert!(
            result.is_err(),
            "expected an error, got {:?}",
            result.map(|l| l.len())
        );
    }

    #[test]
    fn pending_changes_refuses_an_invalid_contract_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = collection_root(dir.path());

        let err = contract_changes(dir.path())
            .pending_changes(&root.to_string_lossy(), "not-a-ulid")
            .expect_err("invalid id");

        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn pending_changes_maps_a_missing_contract_to_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = collection_root(dir.path());

        let err = contract_changes(dir.path())
            .pending_changes(&root.to_string_lossy(), &Ulid::new().to_string())
            .expect_err("missing contract");

        assert!(
            matches!(&err, DomainError::NotFound(m) if m == "The contract was not found."),
            "{err:?}"
        );
    }
}
