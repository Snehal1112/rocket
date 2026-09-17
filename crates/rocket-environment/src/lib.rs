pub mod context;
pub mod dynamic_vars;
pub mod environment;
pub mod repository;
pub mod resolver;
pub mod secret_store;
pub mod variable;

pub use context::VariableContext;
pub use environment::{Environment, Extensions};
pub use repository::{EnvironmentRepository, EnvironmentRepositoryFactory};
pub use resolver::{resolve, resolve_with_env, ResolveResult};
pub use secret_store::{NullSecretStore, SecretStore};
pub use variable::Variable;
