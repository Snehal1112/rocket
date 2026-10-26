//! Where a collection created by an OpenAPI import came from.

use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Version written into the source file. A file with a higher version is refused.
pub const SOURCE_FILE_VERSION: u32 = 1;

/// What a collection remembers about its OpenAPI import: where the spec came from, how it was
/// laid out, what the spec looked like, and which request came from which operation.
/// Persisted, so keys are snake_case with no renaming. It never holds a token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApiSource {
    pub version: u32,
    pub location: SpecLocation,
    /// `"tags"` or `"paths"`.
    #[serde(default)]
    pub group_by: String,
    /// Hex SHA-256 of the spec text that was imported.
    #[serde(default)]
    pub spec_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub operations: Vec<SourceOperation>,
    /// Stable id of this source, used to key the keychain token. Empty in files from Plan 2.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_id: String,
    /// Legacy, read only. The interval now lives in the local state and this value is ignored.
    #[serde(default, skip_serializing)]
    pub sync_interval: SyncInterval,
    /// Legacy, read only. A fallback for the status view until the local state has a check.
    #[serde(default, skip_serializing)]
    pub last_checked_at: Option<String>,
    /// Legacy, read only. A fallback for the status view until the local state has a check.
    #[serde(default, skip_serializing)]
    pub last_status: Option<SyncStatus>,
    /// Hex SHA-256 of the newer spec kept in the pending copy, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_sha256: Option<String>,
}

impl OpenApiSource {
    /// A fresh random id for a new source.
    pub fn new_source_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// How often a collection checks its spec. Persisted as snake_case names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncInterval {
    #[default]
    Off,
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl SyncInterval {
    pub(crate) fn is_off(&self) -> bool {
        matches!(self, SyncInterval::Off)
    }

    /// The interval in seconds, or `None` when checks are off.
    pub fn as_secs(self) -> Option<u64> {
        match self {
            SyncInterval::Off => None,
            SyncInterval::Hourly => Some(3600),
            SyncInterval::Daily => Some(86_400),
            SyncInterval::Weekly => Some(604_800),
            SyncInterval::Monthly => Some(2_592_000),
        }
    }

    /// Reads the snake_case name. Anything else is `None`.
    pub fn parse(text: &str) -> Option<SyncInterval> {
        match text {
            "off" => Some(SyncInterval::Off),
            "hourly" => Some(SyncInterval::Hourly),
            "daily" => Some(SyncInterval::Daily),
            "weekly" => Some(SyncInterval::Weekly),
            "monthly" => Some(SyncInterval::Monthly),
            _ => None,
        }
    }
}

/// The result of the last check. Persisted, tagged by `state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SyncStatus {
    Unchanged,
    Changed,
    /// `code` is a short machine word such as `not_found`, `rate_limited` or `not_a_spec`.
    Failed {
        code: String,
    },
}

/// One imported request and the spec operation it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOperation {
    /// The operation key from the spec (`operationId`, else `METHOD /path`).
    pub key: String,
    pub uid: String,
    /// The request's path inside the collection, without the file extension.
    pub request_path: String,
    /// Hash of the imported fields at import time.
    pub baseline_hash: String,
}

/// Reads and writes a collection's OpenAPI source file.
pub trait OpenApiSourceRepository: Send + Sync {
    /// `Ok(None)` when the collection has no source file. A file that cannot be read, or that
    /// was written by a newer version, is an error and never replaced by defaults.
    fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiSource>>;
    fn save(&self, collection_root: &Path, source: &OpenApiSource) -> DomainResult<()>;
    /// Removes the file. A missing file is not an error.
    fn delete(&self, collection_root: &Path) -> DomainResult<()>;
    /// Keeps a newer spec next to the source file until the user applies or drops it.
    fn save_pending(&self, collection_root: &Path, spec: &[u8]) -> DomainResult<()>;
    /// `Ok(None)` when there is no pending copy.
    fn load_pending(&self, collection_root: &Path) -> DomainResult<Option<Vec<u8>>>;
    /// Removes the pending copy. A missing file is not an error.
    fn clear_pending(&self, collection_root: &Path) -> DomainResult<()>;
}

/// The validators of one file a spec refers to, kept so a check can ask "has it changed?"
/// without downloading it. Persisted, so keys are snake_case with no renaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRefState {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// The place a spec is fetched from. Persisted, so keys are snake_case with no renaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpecLocation {
    /// A file in a GitHub repository. `git_ref` is a branch, tag or commit.
    Github {
        owner: String,
        repo: String,
        git_ref: String,
        path: String,
    },
    /// Any other `https` URL.
    Https { url: String },
}

impl SpecLocation {
    /// The canonical link for this location, as a user would paste it.
    pub fn display_url(&self) -> String {
        match self {
            SpecLocation::Github {
                owner,
                repo,
                git_ref,
                path,
            } => format!("https://github.com/{owner}/{repo}/blob/{git_ref}/{path}"),
            SpecLocation::Https { url } => url.clone(),
        }
    }

    /// True when the ref is a full commit id, so the file can never change.
    pub fn ref_is_immutable(&self) -> bool {
        match self {
            SpecLocation::Github { git_ref, .. } => {
                git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit())
            }
            SpecLocation::Https { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_source_file_loads_with_defaults() {
        let yaml = "version: 1\nlocation:\n  kind: https\n  url: https://example.com/a.json\ngroup_by: tags\nspec_sha256: ab\noperations: []\n";
        let source: OpenApiSource = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(source.source_id, "");
        assert_eq!(source.sync_interval, SyncInterval::Off);
        assert_eq!(source.last_checked_at, None);
        assert_eq!(source.last_status, None);
        assert_eq!(source.pending_sha256, None);
    }

    #[test]
    fn legacy_timing_fields_are_read_but_never_written() {
        let yaml = "version: 1\nlocation:\n  kind: https\n  url: https://example.com/a.json\nsync_interval: hourly\nlast_checked_at: '2026-10-26T12:00:00+00:00'\nlast_status:\n  state: changed\n";
        let source: OpenApiSource = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(source.sync_interval, SyncInterval::Hourly);
        assert_eq!(source.last_status, Some(SyncStatus::Changed));
        let out = serde_yaml::to_string(&source).expect("write");
        assert!(!out.contains("sync_interval"), "{out}");
        assert!(!out.contains("last_checked_at"), "{out}");
        assert!(!out.contains("last_status"), "{out}");
    }

    #[test]
    fn interval_seconds() {
        assert_eq!(SyncInterval::Off.as_secs(), None);
        assert_eq!(SyncInterval::Hourly.as_secs(), Some(3600));
        assert_eq!(SyncInterval::Daily.as_secs(), Some(86400));
        assert_eq!(SyncInterval::Weekly.as_secs(), Some(604800));
        assert_eq!(SyncInterval::Monthly.as_secs(), Some(2_592_000));
        assert_eq!(SyncInterval::parse("daily"), Some(SyncInterval::Daily));
        assert_eq!(SyncInterval::parse("Daily"), None);
        assert_eq!(SyncInterval::parse(""), None);
    }

    #[test]
    fn new_source_ids_are_unique() {
        assert_ne!(
            OpenApiSource::new_source_id(),
            OpenApiSource::new_source_id()
        );
    }
}
