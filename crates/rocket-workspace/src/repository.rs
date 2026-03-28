use rocket_shared::error::DomainResult;
use crate::workspace::WorkspaceRegistry;

pub trait WorkspaceRepository: Send + Sync {
    fn load(&self) -> DomainResult<WorkspaceRegistry>;
    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()>;
}
