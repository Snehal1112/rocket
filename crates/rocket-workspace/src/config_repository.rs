use std::path::Path;
use rocket_shared::error::DomainResult;
use crate::config::WorkspaceConfig;

/// Repository trait for reading/writing per-workspace `workspace.yml` config.
/// The `workspace_path` parameter is the root directory of the workspace.
pub trait WorkspaceConfigRepository: Send + Sync {
    /// Load the workspace config from `workspace_path/workspace.yml`.
    /// Returns a default config (derived from directory name) if the file does not exist.
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig>;

    /// Save the workspace config to `workspace_path/workspace.yml`.
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()>;

    /// Read the `name` field from `collection_dir/opencollection.yml`.
    /// Returns `None` if the file is absent or has no `name` field.
    fn read_collection_name(&self, collection_dir: &Path) -> DomainResult<Option<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn WorkspaceConfigRepository>) {}
    }
}
