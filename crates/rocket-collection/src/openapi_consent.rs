//! Machine-local consent for OpenAPI sync.
//!
//! A record says: on this machine, for this collection folder and source id, the user
//! approved checks (and the keychain token) for this exact spec location. The record lives
//! outside the collection folder, so a cloned, copied or restored folder never carries it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rocket_shared::error::DomainResult;

/// The approval the user gave for one source of one collection folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiConsent {
    pub source_id: String,
    /// The normalized spec location (`display_url`) that was approved.
    pub location: String,
    /// RFC 3339 UTC time of the approval.
    pub granted_at: String,
}

/// Where the approvals are kept. A store that cannot be read answers `None`, never a grant.
pub trait OpenApiConsentStore: Send + Sync {
    fn get(&self, root: &Path, source_id: &str) -> DomainResult<Option<OpenApiConsent>>;
    fn grant(&self, root: &Path, source_id: &str, location: &str) -> DomainResult<()>;
    fn revoke(&self, root: &Path, source_id: &str) -> DomainResult<()>;
}

/// How a source's approval compares with its current location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentState {
    Granted,
    Missing,
    LocationChanged,
}

/// The key of a collection folder: canonical when it exists, else as given.
pub fn consent_root_key(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// A store that keeps approvals in memory only. Used by tests and as a stand-in.
#[derive(Default)]
pub struct InMemoryConsentStore {
    records: Mutex<HashMap<(String, String), OpenApiConsent>>,
}

impl InMemoryConsentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OpenApiConsentStore for InMemoryConsentStore {
    fn get(&self, root: &Path, source_id: &str) -> DomainResult<Option<OpenApiConsent>> {
        let key = (consent_root_key(root), source_id.to_string());
        let map = self.records.lock().unwrap_or_else(|e| e.into_inner());
        Ok(map.get(&key).cloned())
    }

    fn grant(&self, root: &Path, source_id: &str, location: &str) -> DomainResult<()> {
        let key = (consent_root_key(root), source_id.to_string());
        let record = OpenApiConsent {
            source_id: source_id.to_string(),
            location: location.to_string(),
            granted_at: chrono::Utc::now().to_rfc3339(),
        };
        let mut map = self.records.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(key, record);
        Ok(())
    }

    fn revoke(&self, root: &Path, source_id: &str) -> DomainResult<()> {
        let key = (consent_root_key(root), source_id.to_string());
        let mut map = self.records.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&key);
        Ok(())
    }
}
