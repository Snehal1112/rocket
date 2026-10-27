//! Backups of requests the user reset to the spec, stored at
//! `<collection>/.rocket/openapi-reset-backups.json`. The file is local and git-ignored.

use crate::Request;
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const BACKUP_FILE_VERSION: u32 = 1;

/// The user's version of one request, kept until the next sync touches that endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupEntry {
    /// The operation key from the spec.
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub uid: String,
    /// The request's path inside the collection, without the file extension.
    #[serde(default)]
    pub request_path: String,
    /// RFC 3339 UTC time of the reset.
    #[serde(default)]
    pub saved_at: String,
    pub request: Request,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApiBackups {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<BackupEntry>,
}

impl Default for OpenApiBackups {
    fn default() -> Self {
        Self {
            version: BACKUP_FILE_VERSION,
            entries: Vec::new(),
        }
    }
}

impl OpenApiBackups {
    /// A copy without the entries of the given operation keys.
    pub fn without_keys(&self, keys: &[String]) -> OpenApiBackups {
        OpenApiBackups {
            version: self.version,
            entries: self
                .entries
                .iter()
                .filter(|e| !keys.contains(&e.key))
                .cloned()
                .collect(),
        }
    }
}

pub trait OpenApiBackupRepository: Send + Sync {
    /// `Ok(None)` when there is no file. A corrupt or newer file is an error, never replaced
    /// by an empty one.
    fn load(&self, root: &Path) -> DomainResult<Option<OpenApiBackups>>;
    fn save(&self, root: &Path, backups: &OpenApiBackups) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use rocket_shared::types::HttpMethod;

    fn entry(key: &str) -> BackupEntry {
        BackupEntry {
            key: key.to_string(),
            uid: format!("uid-{key}"),
            request_path: format!("pets/{key}"),
            saved_at: "2026-10-27T10:00:00+00:00".to_string(),
            request: Request::new(key, HttpMethod::Get, "{{baseUrl}}/pets"),
        }
    }

    #[test]
    fn a_backup_file_round_trips() {
        let backups = OpenApiBackups {
            version: BACKUP_FILE_VERSION,
            entries: vec![entry("listPets")],
        };
        let json = serde_json::to_string(&backups).expect("json");
        let back: OpenApiBackups = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, backups);
        assert!(
            json.contains("request_path") && json.contains("saved_at"),
            "{json}"
        );
    }

    #[test]
    fn an_old_file_without_entries_loads_with_defaults() {
        let backups: OpenApiBackups = serde_json::from_str(r#"{"version":1}"#).expect("parse");
        assert!(backups.entries.is_empty());
        assert_eq!(OpenApiBackups::default().version, BACKUP_FILE_VERSION);
    }

    #[test]
    fn without_keys_drops_only_the_named_entries() {
        let backups = OpenApiBackups {
            version: BACKUP_FILE_VERSION,
            entries: vec![entry("a"), entry("b"), entry("c")],
        };
        let kept = backups.without_keys(&["a".to_string(), "c".to_string()]);
        let keys: Vec<&str> = kept.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["b"]);
        assert_eq!(backups.entries.len(), 3);
    }
}
