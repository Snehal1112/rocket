use rocket_collection::{Collection, CollectionRepository, CollectionSummary, Request};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get(name)
    }

    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let col = self.repo.create(name)?;
        self.events.publish(DomainEvent::CollectionCreated { name: name.to_string() });
        Ok(col)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.events.publish(DomainEvent::CollectionDeleted { name: name.to_string() });
        Ok(())
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        self.repo.rename(old_name, new_name)?;
        self.events.publish(DomainEvent::CollectionRenamed {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        });
        Ok(())
    }

    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<()> {
        self.repo.save_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn rename_request(&self, collection: &str, old_path: &str, new_name: &str) -> DomainResult<()> {
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        self.repo.save_request(collection, new_name, &request)?;
        // Only delete the old file if the name actually changed.
        if old_path != new_name {
            let _ = self.repo.delete_request(collection, old_path);
        }
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: new_name.to_string(),
        });
        Ok(())
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_request(collection, path)?;
        self.events.publish(DomainEvent::RequestDeleted {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo.move_item(src_collection, src_path, dst_collection, dst_path)?;
        self.events.publish(DomainEvent::ItemMoved {
            src_collection: src_collection.to_string(),
            src_path: src_path.to_string(),
            dst_collection: dst_collection.to_string(),
            dst_path: dst_path.to_string(),
        });
        Ok(())
    }

    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.repo.save_settings(name, settings)?;
        // Use RequestSaved event to trigger collection-changed in the frontend.
        self.events.publish(DomainEvent::RequestSaved {
            collection: name.to_string(),
            path: "collection.json".to_string(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()) }
        }
    }

    impl CollectionRepository for MockCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            let cols = self.collections.lock().unwrap();
            Ok(cols
                .iter()
                .map(|c| CollectionSummary::new(&c.name, "", c.request_count()))
                .collect())
        }

        fn get(&self, name: &str) -> DomainResult<Collection> {
            let cols = self.collections.lock().unwrap();
            cols.iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }

        fn create(&self, name: &str) -> DomainResult<Collection> {
            let mut cols = self.collections.lock().unwrap();
            if cols.iter().any(|c| c.name == name) {
                return Err(DomainError::AlreadyExists(name.into()));
            }
            let col = Collection::new(name);
            cols.push(col.clone());
            Ok(col)
        }

        fn delete(&self, name: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            cols.retain(|c| c.name != name);
            Ok(())
        }

        fn rename(&self, old: &str, new: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            if let Some(c) = cols.iter_mut().find(|c| c.name == old) {
                c.name = new.to_string();
                Ok(())
            } else {
                Err(DomainError::NotFound(old.into()))
            }
        }

        fn get_request(&self, _: &str, _: &str) -> DomainResult<Request> {
            unimplemented!()
        }
        fn save_request(&self, _: &str, _: &str, _: &Request) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_settings(&self, _: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            Ok(rocket_collection::CollectionSettings::default())
        }
        fn save_settings(&self, _: &str, _: &rocket_collection::CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
    }

    fn make_service() -> CollectionService {
        CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn create_and_list() {
        let svc = make_service();
        svc.create("my-api").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn create_validates_name() {
        let svc = make_service();
        assert!(svc.create("").is_err());
        assert!(svc.create("has/slash").is_err());
    }

    #[test]
    fn rename() {
        let svc = make_service();
        svc.create("old").unwrap();
        svc.rename("old", "new").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list[0].name, "new");
    }
}
