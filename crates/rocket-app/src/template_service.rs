use rocket_history::{Template, TemplateRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::EventPublisher;

pub struct TemplateService {
    repo: Box<dyn TemplateRepository>,
    #[allow(dead_code)]
    events: Box<dyn EventPublisher>,
}

impl TemplateService {
    pub fn new(repo: Box<dyn TemplateRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn list(&self) -> DomainResult<Vec<Template>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Template> {
        self.repo.get(name)
    }

    pub fn save(&self, template: &Template) -> DomainResult<()> {
        self.repo.save(template)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_history::Template;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::Mutex;

    struct MockTemplateRepo {
        templates: Mutex<Vec<Template>>,
    }

    impl MockTemplateRepo {
        fn new() -> Self {
            Self { templates: Mutex::new(Vec::new()) }
        }
    }

    impl TemplateRepository for MockTemplateRepo {
        fn list(&self) -> DomainResult<Vec<Template>> {
            Ok(self.templates.lock().unwrap().clone())
        }

        fn get(&self, name: &str) -> DomainResult<Template> {
            self.templates
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }

        fn save(&self, template: &Template) -> DomainResult<()> {
            let mut templates = self.templates.lock().unwrap();
            if let Some(existing) = templates.iter_mut().find(|t| t.name == template.name) {
                *existing = template.clone();
            } else {
                templates.push(template.clone());
            }
            Ok(())
        }

        fn delete(&self, name: &str) -> DomainResult<()> {
            self.templates.lock().unwrap().retain(|t| t.name != name);
            Ok(())
        }
    }

    fn make_service() -> TemplateService {
        TemplateService::new(Box::new(MockTemplateRepo::new()), Box::new(NullEventPublisher))
    }

    #[test]
    fn save_and_list() {
        let svc = make_service();
        let t = Template::new("GET Users", HttpMethod::Get, "/users");
        svc.save(&t).unwrap();
        assert_eq!(svc.list().unwrap().len(), 1);
    }

    #[test]
    fn get_by_name() {
        let svc = make_service();
        let t = Template::new("Create", HttpMethod::Post, "/items");
        svc.save(&t).unwrap();
        let loaded = svc.get("Create").unwrap();
        assert_eq!(loaded.url, "/items");
    }

    #[test]
    fn delete_template() {
        let svc = make_service();
        svc.save(&Template::new("Temp", HttpMethod::Delete, "/x")).unwrap();
        svc.delete("Temp").unwrap();
        assert!(svc.list().unwrap().is_empty());
    }
}
