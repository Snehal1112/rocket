use serde::{Deserialize, Serialize};

fn default_ref_type() -> String {
    "embedded".to_string()
}

/// Lightweight summary for listing collections (no full tree).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub uid: String,
    pub name: String,
    pub path: String,
    pub request_count: usize,
    pub modified_at: Option<String>,
    /// "embedded" (default) or "external". Set by the workspace layer.
    #[serde(default = "default_ref_type")]
    pub ref_type: String,
}

impl CollectionSummary {
    pub fn new(uid: impl Into<String>, name: impl Into<String>, path: impl Into<String>, request_count: usize, modified_at: Option<String>) -> Self {
        Self {
            uid: uid.into(),
            name: name.into(),
            path: path.into(),
            request_count,
            modified_at,
            ref_type: default_ref_type(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_creation() {
        let s = CollectionSummary::new(String::new(), "my-api", "/path/to/my-api", 5, None);
        assert_eq!(s.name, "my-api");
        assert_eq!(s.request_count, 5);
    }

    #[test]
    fn summary_defaults_ref_type_to_embedded() {
        let json = r#"{"uid":"","name":"test","path":"","requestCount":0,"modifiedAt":null}"#;
        let s: CollectionSummary = serde_json::from_str(json).unwrap();
        assert_eq!(s.ref_type, "embedded");
    }
}
