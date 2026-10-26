//! Local, git-ignored OpenAPI sync state, stored at `<collection>/.rocket/openapi-local.json`.

use crate::openapi_source::{SyncInterval, SyncStatus};
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
    /// How often this machine checks the spec. Off by default.
    #[serde(default, skip_serializing_if = "SyncInterval::is_off")]
    pub sync_interval: SyncInterval,
    /// RFC 3339 UTC time of the last check attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<SyncStatus>,
    /// Failed checks in a row. A success resets it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub consecutive_failures: u32,
    /// RFC 3339 UTC time before which no check runs, from a `Retry-After` answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    /// The canonical collection folder where the user turned the interval on. A state that
    /// arrives with a copied or committed folder names another folder, so it stays off here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opt_in_root: Option<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Default for OpenApiLocalState {
    fn default() -> Self {
        Self {
            version: LOCAL_STATE_FILE_VERSION,
            skips: BTreeMap::new(),
            sync_interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            consecutive_failures: 0,
            not_before: None,
            opt_in_root: None,
        }
    }
}

pub trait OpenApiLocalRepository: Send + Sync {
    fn load(&self, root: &Path) -> DomainResult<Option<OpenApiLocalState>>;
    fn save(&self, root: &Path, state: &OpenApiLocalState) -> DomainResult<()>;
}
