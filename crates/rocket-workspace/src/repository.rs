use std::path::Path;

use crate::workspace::WorkspaceRegistry;
use rocket_shared::error::DomainResult;

pub trait WorkspaceRepository: Send + Sync {
    fn load(&self) -> DomainResult<WorkspaceRegistry>;
    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()>;
    /// Create workspace root and its `collections/` and `environments/` subdirs.
    fn ensure_workspace_dirs(&self, path: &Path) -> DomainResult<()>;
    /// True when `<path>/workspace.yml` exists.
    fn workspace_config_exists(&self, path: &Path) -> bool;
    /// True when `<path>/opencollection.yml` exists.
    fn collection_manifest_exists(&self, path: &Path) -> bool;
    /// Deletes the workspace directory and everything in it. A missing directory is Ok.
    /// A failure maps to `DomainError::Io("Failed to delete workspace directory: <cause>")`.
    fn remove_workspace_dir(&self, path: &Path) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn WorkspaceRepository>) {}
    }
}
