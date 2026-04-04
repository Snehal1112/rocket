use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub total_files: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedItem>,
    pub created_workspace: Option<String>,
    pub created_collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
