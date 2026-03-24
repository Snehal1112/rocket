use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::folder::Folder;
use crate::settings::CollectionSettings;

/// Collection aggregate root.
/// A collection is a named group of API requests organized in a folder tree.
/// Identity: the collection name (unique within the workspace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub name: String,
    pub root: Folder,
    pub settings: CollectionSettings,
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            root: Folder::new(&name),
            name,
            settings: CollectionSettings::default(),
        }
    }

    /// Rename the collection. Validates the new name.
    pub fn rename(&mut self, new_name: impl Into<String>) -> DomainResult<()> {
        let new_name = new_name.into();
        Self::validate_name(&new_name)?;
        self.name = new_name;
        Ok(())
    }

    /// Validate a collection name.
    pub fn validate_name(name: &str) -> DomainResult<()> {
        if name.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "Collection name cannot be empty".into(),
            ));
        }
        if name.contains('/') || name.contains('\\') {
            return Err(DomainError::InvalidInput(
                "Collection name cannot contain path separators".into(),
            ));
        }
        if name.starts_with('.') {
            return Err(DomainError::InvalidInput(
                "Collection name cannot start with a dot".into(),
            ));
        }
        Ok(())
    }

    /// Total request count across all folders.
    pub fn request_count(&self) -> usize {
        self.root.request_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collection_is_empty() {
        let col = Collection::new("my-api");
        assert_eq!(col.name, "my-api");
        assert_eq!(col.root.request_count(), 0);
    }

    #[test]
    fn rename_collection() {
        let mut col = Collection::new("old-name");
        col.rename("new-name").unwrap();
        assert_eq!(col.name, "new-name");
    }

    #[test]
    fn rename_to_empty_fails() {
        let mut col = Collection::new("test");
        let result = col.rename("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_name_rejects_invalid_chars() {
        assert!(Collection::validate_name("valid-name").is_ok());
        assert!(Collection::validate_name("also_valid.name").is_ok());
        assert!(Collection::validate_name("").is_err());
        assert!(Collection::validate_name("has/slash").is_err());
        assert!(Collection::validate_name("has\\backslash").is_err());
    }
}
