use rocket_shared::error::DomainResult;

use crate::template::Template;

pub trait TemplateRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Template>>;
    fn get(&self, name: &str) -> DomainResult<Template>;
    fn save(&self, template: &Template) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn TemplateRepository>) {}
    }
}
