use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unknown SSH host {host}:{port} (algorithm {algorithm}, fingerprint {fingerprint})")]
    SshUnknownHost {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },

    #[error(
        "SSH host key changed for {host}:{port} (algorithm {algorithm}, fingerprint {fingerprint})"
    )]
    SshHostKeyChanged {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },

    #[error(
        "SSH host verification unavailable for {host}:{port} (algorithm {algorithm}, fingerprint {fingerprint})"
    )]
    SshHostVerificationUnavailable {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
}

impl From<std::io::Error> for DomainError {
    fn from(err: std::io::Error) -> Self {
        DomainError::Io(err.to_string())
    }
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

    #[test]
    fn ssh_errors_display_actionable_verification_details() {
        let errors = [
            DomainError::SshUnknownHost {
                host: "git.example.com".into(),
                port: 22,
                algorithm: "ssh-ed25519".into(),
                fingerprint: "SHA256:unknown".into(),
            },
            DomainError::SshHostKeyChanged {
                host: "git.example.com".into(),
                port: 2222,
                algorithm: "rsa-sha2-512".into(),
                fingerprint: "SHA256:changed".into(),
            },
            DomainError::SshHostVerificationUnavailable {
                host: "git.example.com".into(),
                port: 22,
                algorithm: "unknown".into(),
                fingerprint: "SHA256:unavailable".into(),
            },
        ];

        for error in errors {
            let display = error.to_string();
            assert!(display.contains("git.example.com"));
            assert!(display.contains("algorithm"));
            assert!(display.contains("fingerprint SHA256:"));
        }
    }
}
