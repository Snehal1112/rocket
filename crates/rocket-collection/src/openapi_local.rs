//! Local, git-ignored OpenAPI sync state, stored at `<collection>/.rocket/openapi-local.json`.

use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const LOCAL_STATE_FILE_VERSION: u32 = 1;

/// Choices the user made on this machine only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiLocalState {
    pub version: u32,
    /// Maps an operation key to the fingerprint of the change the user chose to skip.
    #[serde(default)]
    pub skips: BTreeMap<String, String>,
}

impl Default for OpenApiLocalState {
    fn default() -> Self {
        Self {
            version: LOCAL_STATE_FILE_VERSION,
            skips: BTreeMap::new(),
        }
    }
}

pub trait OpenApiLocalRepository: Send + Sync {
    fn load(&self, root: &Path) -> DomainResult<Option<OpenApiLocalState>>;
    fn save(&self, root: &Path, state: &OpenApiLocalState) -> DomainResult<()>;
}
