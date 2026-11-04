//! A collection name that is safe to join onto a workspace path.

use std::fmt;
use std::path::{Component, Path};

use rocket_shared::error::{DomainError, DomainResult};

use crate::Collection;

/// A collection name that follows the domain rule and is one plain path component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionName(String);

impl CollectionName {
    /// The `Collection::validate_name` rule, plus no NUL byte, not `.` or `..`, and exactly
    /// one normal path component. The component check also refuses a Windows drive prefix
    /// such as `C:x`, which makes `Path::join` drop the workspace path.
    pub fn parse(name: &str) -> DomainResult<Self> {
        let invalid = || DomainError::InvalidInput("Invalid collection name".to_string());
        if name.contains('\0') || name == "." || name == ".." {
            return Err(invalid());
        }
        Collection::validate_name(name)?;
        let mut components = Path::new(name).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => Ok(Self(name.to_string())),
            _ => Err(invalid()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CollectionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CollectionName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_plain_names() {
        for name in ["my-api", "also_valid.name", "with space"] {
            assert!(CollectionName::parse(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn parse_rejects_empty_and_blank() {
        assert!(CollectionName::parse("").is_err());
        assert!(CollectionName::parse("   ").is_err());
    }

    #[test]
    fn parse_rejects_separators() {
        assert!(CollectionName::parse("a/b").is_err());
        assert!(CollectionName::parse("a\\b").is_err());
    }

    #[test]
    fn parse_rejects_dot_names() {
        for name in [".hidden", ".", ".."] {
            assert!(CollectionName::parse(name).is_err(), "{name}");
        }
    }

    #[test]
    fn parse_rejects_a_nul_byte() {
        assert!(CollectionName::parse("a\0b").is_err());
    }

    #[test]
    fn parse_keeps_the_text() {
        let name = CollectionName::parse("my-api").expect("valid");
        assert_eq!(name.as_str(), "my-api");
        assert_eq!(name.to_string(), "my-api");
    }

    #[cfg(windows)]
    #[test]
    fn parse_rejects_a_drive_prefix() {
        assert!(CollectionName::parse("C:x").is_err());
    }
}
