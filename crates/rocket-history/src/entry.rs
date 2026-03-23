use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub response_size: usize,
    pub timestamp: DateTime<Utc>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
}

impl HistoryEntry {
    pub fn new(
        method: impl Into<String>,
        url: impl Into<String>,
        status: u16,
        duration_ms: u64,
        response_size: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: method.into(),
            url: url.into(),
            status,
            duration_ms,
            response_size,
            timestamp: Utc::now(),
            collection: None,
            request_name: None,
        }
    }

    pub fn with_collection(mut self, collection: impl Into<String>, request_name: impl Into<String>) -> Self {
        self.collection = Some(collection.into());
        self.request_name = Some(request_name.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entry_has_id_and_timestamp() {
        let entry = HistoryEntry::new("GET", "https://api.example.com", 200, 150, 1024);
        assert!(!entry.id.is_empty());
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.status, 200);
        assert!(entry.collection.is_none());
    }

    #[test]
    fn entry_with_collection() {
        let entry = HistoryEntry::new("POST", "/api", 201, 50, 128)
            .with_collection("my-api", "Create User");
        assert_eq!(entry.collection, Some("my-api".into()));
        assert_eq!(entry.request_name, Some("Create User".into()));
    }
}
