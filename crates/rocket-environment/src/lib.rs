pub mod context;
pub mod environment;
pub mod repository;
pub mod resolver;
pub mod variable;

pub use context::VariableContext;
pub use environment::{Environment, Extensions};
pub use repository::EnvironmentRepository;
pub use resolver::{resolve, resolve_with_env, ResolveResult};
pub use variable::Variable;
