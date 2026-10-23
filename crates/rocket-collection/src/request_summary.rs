use crate::request_kind::RequestKind;
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
    /// Which protocol the file holds. Absent in older JSON, which means HTTP.
    #[serde(default, skip_serializing_if = "RequestKind::is_http")]
    pub kind: RequestKind,
    /// Names of the saved examples, in file order. Only HTTP requests have them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub example_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_summary_json_without_kind_is_http() {
        let json = r#"{"uid":"u","name":"A","method":"GET","url":"/a"}"#;
        let s: RequestSummary = serde_json::from_str(json).expect("old shape");
        assert_eq!(s.kind, RequestKind::Http);
    }

    #[test]
    fn summary_item_serializes_example_names_in_camel_case() {
        let item = crate::CollectionItem::Summary(RequestSummary {
            uid: "u".into(),
            name: "A".into(),
            method: "GET".into(),
            url: "/a".into(),
            file_name: Some("A.yml".into()),
            kind: RequestKind::Http,
            example_names: vec!["200 OK".into()],
        });
        let json = serde_json::to_value(&item).expect("ser");
        assert_eq!(json["fileName"], "A.yml");
        assert_eq!(json["exampleNames"][0], "200 OK");
    }

    #[test]
    fn http_summary_omits_kind_and_graphql_summary_keeps_it() {
        let mut s = RequestSummary {
            uid: "u".into(),
            name: "A".into(),
            method: "GET".into(),
            url: "/a".into(),
            file_name: None,
            kind: RequestKind::Http,
            example_names: Vec::new(),
        };
        assert!(serde_json::to_value(&s).expect("ser").get("kind").is_none());
        s.kind = RequestKind::GraphQl;
        assert_eq!(serde_json::to_value(&s).expect("ser")["kind"], "graphql");
    }
}
