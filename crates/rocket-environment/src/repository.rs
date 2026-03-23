use rocket_shared::error::DomainResult;

use crate::environment::Environment;

pub trait EnvironmentRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Environment>>;
    fn get(&self, name: &str) -> DomainResult<Environment>;
    fn save(&self, env: &Environment) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn EnvironmentRepository>) {}
    }
}
