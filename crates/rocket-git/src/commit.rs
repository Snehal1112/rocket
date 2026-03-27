use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub id: String,
    pub full_id: String,
    pub message: String,
    pub author: String,
    pub author_email: String,
    pub timestamp: DateTime<Utc>,
    pub files_changed: usize,
}
