//! The requests an OpenAPI import wrote, kept so a later sync can tell what the user changed.

use crate::request::Request;
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Version written into the baseline file. A file with a higher version is refused.
pub const BASELINE_FILE_VERSION: u32 = 1;

/// A snapshot of the requests as the last import or sync saved them.
/// Persisted, so the struct keys are snake_case with no renaming. It never holds a token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApiBaseline {
    pub version: u32,
    /// The converter version that built the requests. A different one makes a diff cautious.
    pub converter_version: u32,
    /// The `servers[0]` URL of the spec at that time.
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub operations: Vec<BaselineOperation>,
}

/// One request exactly as it was saved, with the operation key it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineOperation {
    pub key: String,
    pub request: Request,
}

/// Reads and writes a collection's baseline snapshot.
pub trait OpenApiBaselineRepository: Send + Sync {
    /// `Ok(None)` when there is no snapshot. A file that cannot be read, or that was written
    /// by a newer version, is an error and never replaced by defaults.
    fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiBaseline>>;
    fn save(&self, collection_root: &Path, baseline: &OpenApiBaseline) -> DomainResult<()>;
    /// Removes the file. A missing file is not an error.
    fn delete(&self, collection_root: &Path) -> DomainResult<()>;
}
