//! Keeps one send in one workspace.
//!
//! The environment repositories in production follow the active workspace and read its path
//! on every call. A send reads environments before the request and writes script results
//! after the response, so a workspace switch in between would write the first workspace's
//! values into the second. `with_send_pins` fixes the repositories for the whole send: every
//! environment lookup on the same task uses the workspace that was active when it started.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_shared::error::DomainResult;

use super::RequestExecutionService;

/// The repositories one send uses. `None` means the service's own repository is used.
pub(crate) struct SendPins {
    global: Option<Arc<dyn EnvironmentRepository>>,
    collection_envs: Option<Arc<dyn EnvironmentRepositoryFactory>>,
}

tokio::task_local! {
    static SEND_PINS: SendPins;
}

/// A shared handle to a pinned repository, so a lookup can hand out an owned repository.
struct PinnedEnvRepo(Arc<dyn EnvironmentRepository>);

impl EnvironmentRepository for PinnedEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        self.0.list()
    }

    fn get(&self, name: &str) -> DomainResult<Environment> {
        self.0.get(name)
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        self.0.save(env)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        self.0.delete(name)
    }
}

impl RequestExecutionService {
    /// Runs `fut` with the environment repositories fixed to the current workspace. A call
    /// made inside another send keeps that send's pins, so a nested run stays with it.
    pub(crate) async fn with_send_pins<F: Future>(&self, fut: F) -> F::Output {
        if SEND_PINS.try_with(|_| ()).is_ok() {
            return fut.await;
        }
        let pins = SendPins {
            global: self.env_repo.pinned().map(Arc::from),
            collection_envs: self
                .collection_env_repo_factory
                .as_ref()
                .and_then(|f| f.pinned())
                .map(Arc::from),
        };
        SEND_PINS.scope(pins, fut).await
    }

    /// The global environment repository for this send.
    pub(crate) fn global_env_repo(&self) -> Box<dyn EnvironmentRepository + '_> {
        let pinned = SEND_PINS.try_with(|p| p.global.clone()).ok().flatten();
        match pinned {
            Some(repo) => Box::new(PinnedEnvRepo(repo)),
            None => Box::new(super::RefEnvRepo(self.env_repo.as_ref())),
        }
    }

    /// One collection's environment repository for this send, when a factory is wired.
    pub(crate) fn pinned_collection_env_repo(
        &self,
        collection: &str,
    ) -> Option<Box<dyn EnvironmentRepository>> {
        let pinned = SEND_PINS
            .try_with(|p| p.collection_envs.clone())
            .ok()
            .flatten();
        match (pinned, &self.collection_env_repo_factory) {
            (Some(factory), _) => Some(factory.for_collection(collection)),
            (None, Some(factory)) => Some(factory.for_collection(collection)),
            (None, None) => None,
        }
    }

    /// One collection's folder for this send, when the wiring knows where collections live.
    pub(crate) fn pinned_collection_dir(&self, collection: &str) -> Option<PathBuf> {
        let pinned = SEND_PINS
            .try_with(|p| p.collection_envs.clone())
            .ok()
            .flatten();
        match pinned {
            Some(factory) => factory.collection_dir(collection),
            None => self
                .collection_env_repo_factory
                .as_ref()?
                .collection_dir(collection),
        }
    }
}
