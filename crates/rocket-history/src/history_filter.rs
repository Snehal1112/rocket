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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `#[serde(rename_all = "camelCase")]` is in effect:
    /// `url_contains` must serialise to `urlContains` in JSON, not
    /// `url_contains`.  If the annotation is removed or the field is renamed,
    /// this test catches it before the frontend breaks.
    #[test]
    fn filter_serialises_url_contains_as_camel_case() {
        let f = HistoryFilter {
            url_contains: Some("/api".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&f).unwrap();
        assert!(
            json.contains("\"urlContains\""),
            "url_contains must serialise as 'urlContains' (camelCase); got: {json}"
        );
        assert!(
            !json.contains("url_contains"),
            "snake_case key must not appear in serialised output; got: {json}"
        );
    }

    /// Verifies the round-trip: a camelCase JSON payload from the frontend
    /// deserialises into the correct Rust fields.
    #[test]
    fn filter_deserialises_from_camel_case_json() {
        let json = r#"{"method":"GET","urlContains":"/users","statusMin":200,"statusMax":299}"#;
        let f: HistoryFilter = serde_json::from_str(json).unwrap();
        assert_eq!(f.method.as_deref(), Some("GET"));
        assert_eq!(f.url_contains.as_deref(), Some("/users"));
        assert_eq!(f.status_min, Some(200));
        assert_eq!(f.status_max, Some(299));
    }

    /// An all-None filter must deserialise from an empty JSON object — i.e.
    /// every field is optional.  If any field is made non-optional by mistake
    /// the frontend will break on partial filter payloads.
    #[test]
    fn filter_deserialises_partial_json_with_missing_fields_as_none() {
        let f: HistoryFilter = serde_json::from_str("{}").unwrap();
        assert!(f.method.is_none());
        assert!(f.url_contains.is_none());
        assert!(f.status_min.is_none());
        assert!(f.status_max.is_none());
    }
}
