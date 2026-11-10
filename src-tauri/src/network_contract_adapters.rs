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
        // An unreadable collection fails the read. Skipping it could turn a break into
        // `not_used`.
        for summary in self.collections.list()? {
            let collection = self.collections.get(&summary.name)?;
            locals.extend(local_requests_of(&collection));
        }
        Ok(locals)
    }
}
