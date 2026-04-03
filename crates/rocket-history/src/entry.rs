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

    /// Verifies all five constructor parameters land in the correct fields.
    /// A future refactor that swaps positional args (e.g. status ↔ duration_ms)
    /// would be caught here.
    #[test]
    fn entry_fields_are_stored_correctly() {
        let entry = HistoryEntry::new("DELETE", "https://api.example.com/users/1", 404, 75, 512);
        assert_eq!(entry.method, "DELETE");
        assert_eq!(entry.url, "https://api.example.com/users/1");
        assert_eq!(entry.status, 404);
        assert_eq!(entry.duration_ms, 75);
        assert_eq!(entry.response_size, 512);
    }

    /// Verifies the generated id is a valid UUID v4 string (8-4-4-4-12 hex).
    /// If the id generation changed from UUID to something else (e.g. a
    /// monotonic counter), the frontend's id uniqueness contract would break.
    #[test]
    fn entry_id_is_valid_uuid_format() {
        let entry = HistoryEntry::new("GET", "/", 200, 10, 0);
        // UUID v4: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        let parts: Vec<&str> = entry.id.split('-').collect();
        assert_eq!(parts.len(), 5, "UUID must have 5 hyphen-separated segments");
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert_eq!(&parts[2][0..1], "4", "UUID version nibble must be '4'");
    }

    #[test]
    fn two_entries_have_distinct_ids() {
        let a = HistoryEntry::new("GET", "/", 200, 10, 0);
        let b = HistoryEntry::new("GET", "/", 200, 10, 0);
        assert_ne!(a.id, b.id, "each entry must get a unique UUID");
    }
}
