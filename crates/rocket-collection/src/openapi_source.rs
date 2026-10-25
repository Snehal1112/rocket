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
