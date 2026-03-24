use serde::{Deserialize, Serialize};

/// Lightweight summary for listing collections (no full tree).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub uid: String,
    pub name: String,
    pub path: String,
    pub request_count: usize,
}

impl CollectionSummary {
    pub fn new(uid: impl Into<String>, name: impl Into<String>, path: impl Into<String>, request_count: usize) -> Self {
        Self {
            uid: uid.into(),
            name: name.into(),
            path: path.into(),
            request_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_creation() {
        let s = CollectionSummary::new(String::new(), "my-api", "/path/to/my-api", 5);
        assert_eq!(s.name, "my-api");
        assert_eq!(s.request_count, 5);
    }
}
