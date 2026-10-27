//! Where a collection created by an OpenAPI import came from.

use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version written into the source file of a link source. Older builds read it.
pub const SOURCE_FILE_VERSION: u32 = 1;

/// Version written when the location is a local file. A build that reads only version 1
/// refuses such a file instead of misreading the location.
pub const SOURCE_FILE_VERSION_WITH_FILES: u32 = 2;

/// The highest version this build reads. A file with a higher version is refused.
pub const SOURCE_FILE_MAX_VERSION: u32 = SOURCE_FILE_VERSION_WITH_FILES;

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
    /// The files the spec refers to by link, with their validators from the import.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_refs: Vec<RemoteRefState>,
    /// True when a referenced file could not be loaded at import, so a check must fetch
    /// every file again instead of trusting a 304.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remote_refs_incomplete: bool,
}

impl OpenApiSource {
    /// The version to write: 2 for a local file source, so older builds refuse it, and 1 for
    /// every other source. A higher version already on the record is kept.
    pub fn write_version(&self) -> u32 {
        let needed = match self.location {
            SpecLocation::File { .. } => SOURCE_FILE_VERSION_WITH_FILES,
            _ => SOURCE_FILE_VERSION,
        };
        self.version.max(needed)
    }

    /// A fresh random id for a new source.
    pub fn new_source_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// How often a collection checks its spec. Persisted as snake_case names. A name this build
/// does not know reads as `Off`, so a newer file never breaks status or the scheduler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", from = "String")]
pub enum SyncInterval {
    #[default]
    Off,
    #[serde(rename = "every_5_minutes")]
    Min5,
    #[serde(rename = "every_15_minutes")]
    Min15,
    #[serde(rename = "every_30_minutes")]
    Min30,
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl From<String> for SyncInterval {
    fn from(text: String) -> Self {
        SyncInterval::parse(&text).unwrap_or(SyncInterval::Off)
    }
}

impl SyncInterval {
    pub(crate) fn is_off(&self) -> bool {
        matches!(self, SyncInterval::Off)
    }

    /// The interval in seconds, or `None` when checks are off.
    pub fn as_secs(self) -> Option<u64> {
        match self {
            SyncInterval::Off => None,
            SyncInterval::Min5 => Some(300),
            SyncInterval::Min15 => Some(900),
            SyncInterval::Min30 => Some(1800),
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
            "every_5_minutes" => Some(SyncInterval::Min5),
            "every_15_minutes" => Some(SyncInterval::Min15),
            "every_30_minutes" => Some(SyncInterval::Min30),
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
    /// Keeps a copy of the spec as it was last applied, so it can be shown offline.
    fn save_applied(&self, collection_root: &Path, spec: &[u8]) -> DomainResult<()>;
    /// `Ok(None)` when there is no applied copy.
    fn load_applied(&self, collection_root: &Path) -> DomainResult<Option<Vec<u8>>>;
    /// True when an applied copy exists and its SHA-256 (lower-case hex) equals `expected`.
    /// The copy is git-ignored while the source file is tracked, so a pull or a branch switch
    /// can leave it behind. Implementations hash the file as a stream (it can be 25 MB).
    fn applied_matches(&self, collection_root: &Path, expected_sha256: &str) -> DomainResult<bool>;
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
    /// Any other `https` URL, or `http` to this machine.
    Https { url: String },
    /// A spec file on this computer. A relative path is relative to the collection folder.
    File { path: String },
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
            SpecLocation::File { path } => format!("file://{path}"),
        }
    }

    /// The file to read for a `File` location, with a relative path joined to `root`.
    pub fn resolve_file(&self, root: &Path) -> Option<PathBuf> {
        match self {
            SpecLocation::File { path } => {
                let path = Path::new(path);
                Some(if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    root.join(path)
                })
            }
            _ => None,
        }
    }

    /// True when the ref is a full commit id, so the file can never change.
    pub fn ref_is_immutable(&self) -> bool {
        match self {
            SpecLocation::Github { git_ref, .. } => {
                git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit())
            }
            SpecLocation::Https { .. } | SpecLocation::File { .. } => false,
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
    fn short_intervals_have_their_names_and_lengths() {
        for (name, interval, secs) in [
            ("every_5_minutes", SyncInterval::Min5, 300),
            ("every_15_minutes", SyncInterval::Min15, 900),
            ("every_30_minutes", SyncInterval::Min30, 1800),
        ] {
            assert_eq!(SyncInterval::parse(name), Some(interval));
            assert_eq!(interval.as_secs(), Some(secs));
            let json = serde_json::to_string(&interval).expect("write");
            assert_eq!(json, format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<SyncInterval>(&json).expect("read"),
                interval
            );
        }
    }

    #[test]
    fn an_unknown_interval_reads_as_off_without_an_error() {
        let read: SyncInterval = serde_json::from_str("\"every_2_hours\"").expect("read");
        assert_eq!(read, SyncInterval::Off);
        assert_eq!(SyncInterval::parse("every_2_hours"), None);
        let old: SyncInterval = serde_json::from_str("\"hourly\"").expect("read");
        assert_eq!(old, SyncInterval::Hourly);
        let yaml = "version: 1\nlocation:\n  kind: https\n  url: https://example.com/a.json\nsync_interval: every_2_hours\n";
        let source: OpenApiSource = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(source.sync_interval, SyncInterval::Off);
    }

    #[test]
    fn remote_refs_round_trip_and_are_skipped_when_empty() {
        let yaml = "version: 1\nlocation:\n  kind: https\n  url: https://example.com/a.json\n";
        let mut source: OpenApiSource = serde_yaml::from_str(yaml).expect("parse");
        assert!(source.remote_refs.is_empty());
        assert!(!source.remote_refs_incomplete);
        let out = serde_yaml::to_string(&source).expect("write");
        assert!(!out.contains("remote_refs"), "{out}");
        source.remote_refs = vec![RemoteRefState {
            url: "https://example.com/d.yaml".to_string(),
            etag: Some("\"e\"".to_string()),
            last_modified: None,
        }];
        source.remote_refs_incomplete = true;
        let out = serde_yaml::to_string(&source).expect("write");
        let back: OpenApiSource = serde_yaml::from_str(&out).expect("parse");
        assert_eq!(back, source);
        assert!(out.contains("remote_refs_incomplete: true"), "{out}");
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

    fn file(path: &str) -> SpecLocation {
        SpecLocation::File {
            path: path.to_string(),
        }
    }

    #[test]
    fn file_location_round_trips_through_yaml() {
        let yaml = "version: 2\nlocation:\n  kind: file\n  path: specs/api.yaml\n";
        let source: OpenApiSource = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(source.location, file("specs/api.yaml"));
        let out = serde_yaml::to_string(&source).expect("write");
        assert!(out.contains("kind: file"), "{out}");
        let again: OpenApiSource = serde_yaml::from_str(&out).expect("again");
        assert_eq!(again.location, source.location);
    }

    #[test]
    fn a_file_location_needs_version_2_and_others_stay_at_1() {
        let mut source: OpenApiSource =
            serde_yaml::from_str("version: 1\nlocation:\n  kind: file\n  path: a.yaml\n")
                .expect("parse");
        assert_eq!(source.write_version(), 2);
        source.location = SpecLocation::Https {
            url: "https://example.com/a.json".to_string(),
        };
        assert_eq!(source.write_version(), 1);
        // A higher version is never lowered.
        source.version = 7;
        assert_eq!(source.write_version(), 7);
    }

    #[test]
    fn display_url_of_a_file_is_a_file_url() {
        assert_eq!(
            file("specs/api.yaml").display_url(),
            "file://specs/api.yaml"
        );
        assert_eq!(file("/srv/api.yaml").display_url(), "file:///srv/api.yaml");
        assert!(!file("/srv/api.yaml").ref_is_immutable());
    }

    #[test]
    fn resolve_file_joins_the_collection_root() {
        let root = Path::new("/work/shop");
        assert_eq!(
            file("specs/api.yaml").resolve_file(root),
            Some(PathBuf::from("/work/shop/specs/api.yaml"))
        );
        assert_eq!(
            file("../api.yaml").resolve_file(root),
            Some(PathBuf::from("/work/shop/../api.yaml"))
        );
        assert_eq!(
            file("/srv/api.yaml").resolve_file(root),
            Some(PathBuf::from("/srv/api.yaml"))
        );
        let https = SpecLocation::Https {
            url: "https://example.com/a.json".to_string(),
        };
        assert_eq!(https.resolve_file(root), None);
    }
}
