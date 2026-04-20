use std::path::Path;

use rocket_shared::error::DomainResult;
use crate::workspace::WorkspaceRegistry;

pub trait WorkspaceRepository: Send + Sync {
    fn load(&self) -> DomainResult<WorkspaceRegistry>;
    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()>;
    /// Create workspace root and its `collections/` and `environments/` subdirs.
    fn ensure_workspace_dirs(&self, path: &Path) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn WorkspaceRepository>) {}
    }
}
