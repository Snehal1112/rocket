use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

pub struct EnvironmentService {
    repo: Box<dyn EnvironmentRepository>,
    events: Box<dyn EventPublisher>,
}

impl EnvironmentService {
    pub fn new(repo: Box<dyn EnvironmentRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn list(&self) -> DomainResult<Vec<Environment>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Environment> {
        self.repo.get(name)
    }

    pub fn save(&self, env: &Environment) -> DomainResult<()> {
        self.repo.save(env)?;
        self.events.publish(DomainEvent::EnvironmentSaved { name: env.name.clone() });
        Ok(())
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.events.publish(DomainEvent::EnvironmentDeleted { name: name.to_string() });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockEnvRepo {
        envs: Mutex<Vec<Environment>>,
    }

    impl MockEnvRepo {
        fn new() -> Self {
            Self { envs: Mutex::new(Vec::new()) }
        }
    }

    impl EnvironmentRepository for MockEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.envs.lock().unwrap().clone())
        }

        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.envs
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }

        fn save(&self, env: &Environment) -> DomainResult<()> {
            let mut envs = self.envs.lock().unwrap();
            if let Some(existing) = envs.iter_mut().find(|e| e.name == env.name) {
                *existing = env.clone();
            } else {
                envs.push(env.clone());
            }
            Ok(())
        }

        fn delete(&self, name: &str) -> DomainResult<()> {
            self.envs.lock().unwrap().retain(|e| e.name != name);
            Ok(())
        }
    }

    fn make_service() -> EnvironmentService {
        EnvironmentService::new(Box::new(MockEnvRepo::new()), Box::new(NullEventPublisher))
    }

    #[test]
    fn save_and_list() {
        let svc = make_service();
        svc.save(&Environment::new("production")).unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "production");
    }

    #[test]
    fn get_by_name() {
        let svc = make_service();
        svc.save(&Environment::new("staging")).unwrap();
        let env = svc.get("staging").unwrap();
        assert_eq!(env.name, "staging");
    }

    #[test]
    fn delete_removes_environment() {
        let svc = make_service();
        svc.save(&Environment::new("temp")).unwrap();
        svc.delete("temp").unwrap();
        assert!(svc.list().unwrap().is_empty());
    }
}
