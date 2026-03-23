use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl Serialize for DomainError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<serde_json::Error> for DomainError {
    fn from(err: serde_json::Error) -> Self {
        DomainError::Serialization(err.to_string())
    }
}

pub type DomainResult<T> = Result<T, DomainError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_error_display_not_found() {
        let err = DomainError::NotFound("Collection 'foo'".into());
        assert_eq!(err.to_string(), "Not found: Collection 'foo'");
    }

    #[test]
    fn domain_error_display_invalid_input() {
        let err = DomainError::InvalidInput("name cannot be empty".into());
        assert_eq!(err.to_string(), "Invalid input: name cannot be empty");
    }

    #[test]
    fn domain_error_serializes_to_string() {
        let err = DomainError::NotFound("test".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Not found: test\"");
    }
}
