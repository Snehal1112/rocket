// The async_trait macro triggers this lint on newer toolchains.
#![allow(clippy::double_must_use)]
pub mod client_certificate_validation;
pub mod context;
pub mod dot_env;
pub mod dynamic_vars;
pub mod environment;
pub mod external_secret;
pub mod flatten;
mod invalid;
pub mod repository;
pub mod resolver;
pub mod secret_manager;
pub mod secret_store;
pub mod variable;
pub mod vault_secret_fetcher;

pub use client_certificate_validation::{
    validate_client_certificates, validate_vault_certificate_providers,
};
pub use context::VariableContext;
pub use dot_env::{
    collect_default_dot_env, collect_dot_env, nearest_dot_env_path, DotEnvFileInfo, DotEnvLoader,
    DotEnvValues,
};
pub use environment::{Environment, Extensions};
pub use external_secret::{ExternalSecretBinding, ExternalSecretRef};
pub use flatten::{
    find_extends_cycle, flatten_environment, flatten_with_repo, FlatBinding, FlatCertificate,
    FlatVariable, Flattened,
};
pub use repository::{EnvironmentRepository, EnvironmentRepositoryFactory};
pub use resolver::{resolve, resolve_with_env, ResolveResult};
pub use secret_manager::{
    ConnectionProvider, ProviderCapabilityLookup, ProviderConfig, SecretManagerConnection,
    SecretManagerRepository, SecretProviderKind,
};
pub use secret_store::{NullSecretStore, SecretStore};
pub use variable::Variable;
pub use vault_secret_fetcher::{
    NullVaultSecretFetcher, ProviderCapabilities, VaultCertificateMaterial,
    VaultCertificateSummary, VaultSecretFetcher,
};
