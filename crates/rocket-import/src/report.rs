use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub total_files: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedItem>,
    pub created_workspace: Option<String>,
    pub created_collections: Vec<String>,
    /// "collection" or "workspace" — set by the import methods before returning.
    pub detected_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedItem {
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "detail", rename_all = "camelCase")]
pub enum SkipReason {
    UnsupportedRequestType(String),
    UnsupportedAuthType(String),
    ParseError(String),
}
