use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// One node on a Flow canvas. `Request` nodes call an HTTP request when the
/// flow runs; `Input` nodes hold a constant/variable-backed value with no
/// incoming wires; `Output` nodes display whatever their single incoming
/// wire resolves to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FlowNodeKind {
    Request { label: String, source: RequestSource },
    Input { label: String, value: rocket_shared::VariableValue },
    Output { label: String },
}

/// Where a `Request` node's method/url/headers/body/auth come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestSource {
    /// Live reference into the collection tree — `request_path` is relative
    /// to the collection root, e.g. "auth/login.yml". Resolved at run time
    /// (Plan 05), not snapshotted here.
    Saved { request_path: String },
    /// A full ad hoc request embedded directly in the flow file. Deliberately
    /// a minimal, crate-local shape — not a reuse of `rocket-http::HttpRequest`
    /// — so `rocket-flow` stays free of cross-domain-crate dependencies.
    Inline { request: InlineRequestData },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineRequestData {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeader>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineHeader {
    pub name: String,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_node_kind_request_tagged_roundtrip() {
        let kind = FlowNodeKind::Request {
            label: "Get Auth Token".to_string(),
            source: RequestSource::Saved {
                request_path: "auth/login.yml".to_string(),
            },
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Request\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_input_tagged_roundtrip() {
        let kind = FlowNodeKind::Input {
            label: "Username".to_string(),
            value: rocket_shared::VariableValue::simple("alice"),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Input\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_output_tagged_roundtrip() {
        let kind = FlowNodeKind::Output {
            label: "Result".to_string(),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Output\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn request_source_saved_tagged_roundtrip() {
        let source = RequestSource::Saved {
            request_path: "auth/login.yml".to_string(),
        };
        let json = serde_json::to_string(&source).expect("serialize RequestSource");
        assert!(json.contains("\"type\":\"Saved\""), "got: {json}");
        let back: RequestSource = serde_json::from_str(&json).expect("deserialize RequestSource");
        assert_eq!(source, back);
    }

    #[test]
    fn request_source_inline_tagged_roundtrip() {
        let source = RequestSource::Inline {
            request: InlineRequestData {
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: vec![InlineHeader {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                }],
                body: None,
            },
        };
        let json = serde_json::to_string(&source).expect("serialize RequestSource");
        assert!(json.contains("\"type\":\"Inline\""), "got: {json}");
        let back: RequestSource = serde_json::from_str(&json).expect("deserialize RequestSource");
        assert_eq!(source, back);
    }

    #[test]
    fn inline_request_data_headers_roundtrip_when_empty() {
        let data = InlineRequestData {
            method: "GET".to_string(),
            url: "https://api.example.com".to_string(),
            headers: Vec::new(),
            body: None,
        };
        let json = serde_json::to_string(&data).expect("serialize InlineRequestData");
        let back: InlineRequestData =
            serde_json::from_str(&json).expect("deserialize InlineRequestData");
        assert!(back.headers.is_empty());
        assert_eq!(back.body, None);
    }

    #[test]
    fn inline_request_data_defaults_when_headers_and_body_absent() {
        let json = r#"{"method":"GET","url":"https://api.example.com"}"#;
        let data: InlineRequestData =
            serde_json::from_str(json).expect("deserialize minimal InlineRequestData");
        assert!(data.headers.is_empty());
        assert_eq!(data.body, None);
    }
}
