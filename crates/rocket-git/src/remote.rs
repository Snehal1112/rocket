use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

/// Summary returned after a successful fetch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchResult {
    /// Refs that were created or updated, e.g. `"refs/remotes/origin/main"`.
    pub updated_refs: Vec<String>,
    /// Total objects transferred from the remote.
    pub received_objects: usize,
    /// Bytes transferred from the remote.
    pub received_bytes: usize,
}
