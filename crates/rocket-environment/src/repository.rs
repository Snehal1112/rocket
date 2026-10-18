use rocket_shared::error::DomainResult;

use crate::environment::Environment;

pub trait EnvironmentRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Environment>>;
    fn get(&self, name: &str) -> DomainResult<Environment>;

    /// Like `list`, but nothing is skipped or left unresolved: an environment file that cannot
    /// be read or parsed, and a secret value the secret store cannot return, are errors. For
    /// masking text that is stored, where a missed secret would leak it. The default is the
    /// lenient `list`.
    ///
    /// The default delegates to the lenient method, so it is fail-open for any implementer or
    /// decorator that does not override it. Every production implementer overrides it.
    fn list_strict(&self) -> DomainResult<Vec<Environment>> {
        self.list()
    }

    fn save(&self, env: &Environment) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;

    /// A repository fixed to the directory this one points at right now.
    ///
    /// A repository that follows the active workspace returns one, so a
    /// read-modify-write stays in one workspace even if the user switches
    /// in between. A repository with a fixed directory returns `None`.
    fn pinned(&self) -> Option<Box<dyn EnvironmentRepository>> {
        None
    }

    /// The folder a relative `dotEnvFilePath` resolves against. `None` means unknown.
    fn dot_env_base(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// Resolves an `EnvironmentRepository` scoped to one collection's own
/// `environments/` directory. A single `EnvironmentRepository` instance is
/// rooted at one fixed directory, which is correct for the workspace-level
/// GLOBAL environment but wrong for REGULAR (per-collection) environments —
/// a long-lived service that handles many collections (request execution)
/// needs to pick the right directory per call instead.
pub trait EnvironmentRepositoryFactory: Send + Sync {
    fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository>;

    /// The folder that holds one collection, which is the parent of its `environments/`
    /// directory. It only joins paths and does no I/O. `None` means the factory does not know
    /// where collections live, so relative file paths in an environment cannot be resolved.
    fn collection_dir(&self, _collection: &str) -> Option<std::path::PathBuf> {
        None
    }

    /// A factory fixed to the workspace this one points at right now, or `None` for a
    /// factory with a fixed location. Used to keep one send in one workspace.
    fn pinned(&self) -> Option<Box<dyn EnvironmentRepositoryFactory>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn EnvironmentRepository>) {}
    }
}
