use rocket_shared::error::DomainResult;

use crate::environment::Environment;

pub trait EnvironmentRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Environment>>;
    fn get(&self, name: &str) -> DomainResult<Environment>;
    fn save(&self, env: &Environment) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;
}

/// Resolves an `EnvironmentRepository` scoped to one collection's own
/// `environments/` directory. A single `EnvironmentRepository` instance is
/// rooted at one fixed directory, which is correct for the workspace-level
/// GLOBAL environment but wrong for REGULAR (per-collection) environments —
/// a long-lived service that handles many collections (request execution)
/// needs to pick the right directory per call instead.
pub trait EnvironmentRepositoryFactory: Send + Sync {
    fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn EnvironmentRepository>) {}
    }
}
