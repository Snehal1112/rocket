use serde::{Deserialize, Serialize};

/// Filter criteria for searching history entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilter {
    /// Match entries whose HTTP method equals this value (case-insensitive).
    pub method: Option<String>,
    /// Match entries whose URL contains this substring.
    pub url_contains: Option<String>,
    /// Minimum status code (inclusive).
    pub status_min: Option<u16>,
    /// Maximum status code (inclusive).
    pub status_max: Option<u16>,
}
