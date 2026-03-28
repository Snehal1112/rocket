use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    pub ours: String,
    pub theirs: String,
    pub ancestor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "camelCase")]
pub enum ConflictResolution {
    Ours,
    Theirs,
    Custom { content: String },
}
