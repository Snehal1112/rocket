use serde::{Deserialize, Serialize};

/// Canvas coordinates of a node. Purely visual; it has no effect on execution order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// One node on a Flow canvas. `Request` nodes call an HTTP request when the
/// flow runs; `Input` nodes hold a constant/variable-backed value with no
/// incoming wires; `Output` nodes display whatever their single incoming
/// wire resolves to. `If` and `Switch` nodes route execution to one of their
/// named exits. `WaitForCallback` nodes wait for an inbound call on a local URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FlowNodeKind {
    Request {
        label: String,
        source: RequestSource,
        /// When true, a run reports the request as sent and its response.
        #[serde(default, skip_serializing_if = "is_false")]
        debug: bool,
        /// When set, the request is sent again until `condition` holds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repeat_until: Option<RepeatUntil>,
    },
    Input {
        label: String,
        value: rocket_shared::VariableValue,
    },
    Output {
        label: String,
    },
    /// Routes to the "true" exit when `!!(condition)` is truthy, otherwise to
    /// "false". Its output is its input, passed through unchanged.
    If {
        label: String,
        condition: String,
    },
    /// Routes to the first case whose `matches` equals `String(value)`,
    /// otherwise to "default". Its output is its input, passed through.
    Switch {
        label: String,
        value: String,
        cases: Vec<SwitchCase>,
    },
    /// Waits for an inbound HTTP call on a run-scoped local URL. Requests
    /// use the URL as `{{callback.<name>}}`. Its output is the received call,
    /// shaped like a response.
    WaitForCallback {
        label: String,
        /// Letters, digits and `_`. Unique within the flow.
        name: String,
        timeout_ms: u64,
        /// Optional script condition over `request`. Calls that do not
        /// match are answered and ignored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accept_when: Option<String>,
    },
}

// Keeps `debug: false` out of saved files, so old flows round-trip unchanged.
fn is_false(b: &bool) -> bool {
    !*b
}

/// Default, minimum and maximum `timeout_ms` of a Wait for callback node.
pub const CALLBACK_DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const CALLBACK_MIN_TIMEOUT_MS: u64 = 1000;
pub const CALLBACK_MAX_TIMEOUT_MS: u64 = 3_600_000;

/// Prefix of the run-scoped variable that holds a callback URL:
/// `callback.<name>`.
pub const CALLBACK_VAR_PREFIX: &str = "callback.";

/// One named case of a `Switch` node. Edges address a case by `id`, so
/// renaming its `label` never breaks a wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchCase {
    pub id: String,
    pub label: String,
    pub matches: String,
}

/// Polling settings of a Request node. The node sends its request until
/// `condition` is truthy, pausing `interval_ms` between attempts, and gives
/// up after `max_attempts` attempts or `timeout_ms` from the first send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatUntil {
    /// Script condition, evaluated like an If node's condition against `response`.
    pub condition: String,
    pub interval_ms: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
}

impl RepeatUntil {
    pub const DEFAULT_CONDITION: &'static str = "response.status === 200";
    pub const DEFAULT_INTERVAL_MS: u64 = 2000;
    pub const MIN_INTERVAL_MS: u64 = 100;
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 30;
    pub const MAX_MAX_ATTEMPTS: u32 = 1000;
    pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
    pub const MAX_TIMEOUT_MS: u64 = 3_600_000;
}

impl Default for RepeatUntil {
    fn default() -> Self {
        Self {
            condition: Self::DEFAULT_CONDITION.to_string(),
            interval_ms: Self::DEFAULT_INTERVAL_MS,
            max_attempts: Self::DEFAULT_MAX_ATTEMPTS,
            timeout_ms: Self::DEFAULT_TIMEOUT_MS,
        }
    }
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

/// Minimal ad hoc request carried by `RequestSource::Inline`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineRequestData {
    /// Plain HTTP method string such as "GET" or "POST". It is parsed into
    /// `rocket_shared::types::HttpMethod` at run time (Plan 05), not here.
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeader>,
    #[serde(default)]
    pub body: Option<String>,
}

/// One header of an `InlineRequestData`. Kept local instead of reusing
/// `rocket_shared::Header`, which is camelCase-renamed and carries
/// `enabled`/`description` fields this format does not have.
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
            debug: false,
            repeat_until: None,
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
    fn flow_node_kind_input_with_typed_value_roundtrip() {
        let kind = FlowNodeKind::Input {
            label: "Retries".to_string(),
            value: rocket_shared::VariableValue::typed("3", "number"),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Input\""), "got: {json}");
        assert!(json.contains("\"type\":\"number\""), "got: {json}");
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
                body: Some("{\"name\":\"alice\"}".to_string()),
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

    #[test]
    fn flow_node_kind_if_tagged_roundtrip() {
        let kind = FlowNodeKind::If {
            label: "Logged in?".to_string(),
            condition: "response.status === 200".to_string(),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"If\""), "got: {json}");
        assert!(json.contains("\"condition\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_switch_tagged_roundtrip_keeps_case_order() {
        let kind = FlowNodeKind::Switch {
            label: "Plan router".to_string(),
            value: "response.body.plan".to_string(),
            cases: vec![
                SwitchCase {
                    id: "c1".to_string(),
                    label: "Free".to_string(),
                    matches: "free".to_string(),
                },
                SwitchCase {
                    id: "c2".to_string(),
                    label: "Pro plan".to_string(),
                    matches: "pro".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Switch\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn switch_case_fields_are_snake_case_on_disk() {
        let case = SwitchCase {
            id: "c1".to_string(),
            label: "Free".to_string(),
            matches: "free".to_string(),
        };
        let json = serde_json::to_string(&case).expect("serialize SwitchCase");
        assert_eq!(json, r#"{"id":"c1","label":"Free","matches":"free"}"#);
    }

    #[test]
    fn repeat_until_default_uses_the_spec_values() {
        let r = RepeatUntil::default();
        assert_eq!(r.condition, "response.status === 200");
        assert_eq!(r.interval_ms, 2000);
        assert_eq!(r.max_attempts, 30);
        assert_eq!(r.timeout_ms, 60_000);
    }

    #[test]
    fn request_without_repeat_until_omits_the_key() {
        let kind = FlowNodeKind::Request {
            label: "Get".to_string(),
            source: RequestSource::Saved {
                request_path: "a.yml".to_string(),
            },
            debug: false,
            repeat_until: None,
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(!yaml.contains("repeat_until"), "got: {yaml}");
    }

    #[test]
    fn request_with_repeat_until_roundtrips_in_snake_case() {
        let kind = FlowNodeKind::Request {
            label: "Poll job".to_string(),
            source: RequestSource::Saved {
                request_path: "jobs/get-job.yml".to_string(),
            },
            debug: false,
            repeat_until: Some(RepeatUntil {
                condition: "response.body.status === \"done\"".to_string(),
                interval_ms: 2000,
                max_attempts: 30,
                timeout_ms: 60_000,
            }),
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(yaml.contains("repeat_until:"), "got: {yaml}");
        assert!(yaml.contains("interval_ms: 2000"), "got: {yaml}");
        assert!(yaml.contains("max_attempts: 30"), "got: {yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back, kind);
    }

    #[test]
    fn repeat_until_missing_a_field_is_rejected() {
        let yaml = "kind: Request\nlabel: P\nsource:\n  type: Saved\n  request_path: a.yml\nrepeat_until:\n  condition: x\n  interval_ms: 100\n  max_attempts: 3\n";
        let err = serde_yaml::from_str::<FlowNodeKind>(yaml).expect_err("timeout_ms is required");
        assert!(err.to_string().contains("timeout_ms"), "got: {err}");
    }

    #[test]
    fn flow_node_kind_wait_for_callback_roundtrips_in_yaml() {
        let kind = FlowNodeKind::WaitForCallback {
            label: "Payment done".to_string(),
            name: "payment".to_string(),
            timeout_ms: 60_000,
            accept_when: Some("request.body.event === \"payment.completed\"".to_string()),
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(yaml.contains("kind: WaitForCallback"), "got:\n{yaml}");
        assert!(
            yaml.contains("timeout_ms: 60000"),
            "snake_case on disk, got:\n{yaml}"
        );
        assert!(yaml.contains("accept_when:"), "got:\n{yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(kind, back);
    }

    #[test]
    fn wait_for_callback_without_accept_when_omits_the_key() {
        let kind = FlowNodeKind::WaitForCallback {
            label: "Hook".to_string(),
            name: "hook".to_string(),
            timeout_ms: CALLBACK_DEFAULT_TIMEOUT_MS,
            accept_when: None,
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(!yaml.contains("accept_when"), "got:\n{yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(kind, back);
    }
}
