//! The host side of async script calls such as `rok.sendRequest`.
//!
//! `rocket-infra` runs the script and forwards each call to a `ScriptHost`.
//! `rocket-app` implements it. The types are plain data, so this crate does no I/O.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// An HTTP request a script sends with `rok.sendRequest`.
///
/// The script engine builds it from JSON. It is not an IPC or persistence type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    /// True when the script passed an object or array, so `body` is JSON text.
    #[serde(default)]
    pub body_is_json: bool,
    /// Time limit for this request in milliseconds.
    pub timeout_ms: u64,
}

/// A response handed back to a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub response_time_ms: u64,
}

/// Why a host call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// No host serves this call, for example in a Flow transform node.
    Unavailable,
    /// The call ran and failed. The text is shown to the script after redaction.
    Failed(String),
}

/// Calls a script makes that need the application, such as network requests.
///
/// Every method has a default that reports `Unavailable`, so a host implements
/// only what it supports.
#[async_trait]
pub trait ScriptHost: Send + Sync {
    /// Sends one HTTP request for `rok.sendRequest`.
    async fn send_request(&self, _request: HostRequest) -> Result<HostResponse, HostError> {
        Err(HostError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_request_reads_the_engine_json_shape() {
        let json = r#"{"method":"POST","url":"https://x.test","headers":[["a","1"]],"body":"{}","body_is_json":true,"timeout_ms":30000}"#;
        let request: HostRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(request.headers, vec![("a".to_string(), "1".to_string())]);
        assert_eq!(request.body.as_deref(), Some("{}"));
        assert!(request.body_is_json);
        assert_eq!(request.timeout_ms, 30_000);
    }

    #[test]
    fn host_request_defaults_its_optional_fields() {
        let request: HostRequest =
            serde_json::from_str(r#"{"method":"GET","url":"u","timeout_ms":1}"#).expect("parse");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
        assert!(!request.body_is_json);
    }

    #[test]
    fn host_response_writes_snake_case_keys() {
        let response = HostResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: String::new(),
            response_time_ms: 3,
        };
        let json = serde_json::to_string(&response).expect("serialize");
        assert!(json.contains("\"status_text\":\"OK\""), "{json}");
        assert!(json.contains("\"response_time_ms\":3"), "{json}");
    }

    #[test]
    fn script_host_is_object_safe() {
        fn _assert(_: &dyn ScriptHost) {}
    }
}
