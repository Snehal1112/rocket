use serde::{Deserialize, Serialize};

/// Lightweight request descriptor for sidebar display.
/// Contains only the fields the sidebar needs — body and auth are not loaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSummary {
    pub uid: String,
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}
