//! The host side of async script calls such as `rok.sendRequest`.
//!
//! `rocket-infra` runs the script and forwards each call to a `ScriptHost`.
//! `rocket-app` implements it. The types are plain data, so this crate does no I/O.

use std::collections::HashMap;

use async_trait::async_trait;
use rocket_http::{CookieEntry, CookieInput};
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

/// A `rok.runRequest` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRunRequest {
    /// Path relative to the collection root, without extension, as the script wrote it.
    pub path: String,
    /// The calling script's runtime variables, its own writes so far included.
    pub runtime_vars: HashMap<String, String>,
}

/// Variable scopes as stored after a nested run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostScopes {
    pub env: HashMap<String, String>,
    pub global_env: HashMap<String, String>,
    pub collection: HashMap<String, String>,
    /// Secret values of those scopes, added to the caller's redaction list.
    pub secret_values: Vec<String>,
}

/// The outcome of a `rok.runRequest` call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostRunOutcome {
    /// The response, or `None` when the item is not an HTTP request and was skipped.
    pub response: Option<HostResponse>,
    /// Runtime variables the nested run set or changed.
    pub runtime_set: HashMap<String, String>,
    /// Runtime variables the nested run removed.
    pub runtime_removed: Vec<String>,
    /// The scopes after the nested run, or `None` when nothing ran.
    pub scopes: Option<HostScopes>,
    /// The scopes as stored just before the nested run. A key counts as changed
    /// by the nested run only when it differs from here, not from the calling
    /// script's snapshot, which can be older than the storage. `None` falls back
    /// to the snapshot.
    pub scopes_before: Option<HostScopes>,
}

/// One cookie call from `rok.cookies` or `rok.cookies.jar()`.
///
/// The script engine builds it from JSON. It is not an IPC or persistence type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum HostCookieOp {
    /// The cookies that match `url`.
    Find { url: String },
    /// Adds or replaces one cookie for `url`.
    Upsert { url: String, cookie: CookieInput },
    /// Removes the cookie called `name` that matches `url`.
    Remove { url: String, name: String },
    /// Removes every cookie that matches `url`.
    ClearUrl { url: String },
    /// Empties the whole jar.
    ClearAll,
}

/// The answer to a `HostCookieOp`.
#[derive(Debug, Clone, PartialEq)]
pub enum HostCookieResult {
    /// The cookies now matching the URL.
    Cookies(Vec<CookieEntry>),
    /// How many cookies the call changed.
    Count(usize),
    /// The call has nothing to report.
    Done,
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

    /// Runs a saved request through the full pipeline for `rok.runRequest`.
    async fn run_request(&self, _request: HostRunRequest) -> Result<HostRunOutcome, HostError> {
        Err(HostError::Unavailable)
    }

    /// Reads or changes the cookie jar for `rok.cookies`.
    async fn cookies(&self, _op: HostCookieOp) -> Result<HostCookieResult, HostError> {
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
    fn host_cookie_op_reads_the_engine_json_shape() {
        let op: HostCookieOp =
            serde_json::from_str(r#"{"op":"remove","url":"https://x.test","name":"a"}"#)
                .expect("parse");
        assert_eq!(
            op,
            HostCookieOp::Remove {
                url: "https://x.test".into(),
                name: "a".into()
            }
        );
        let op: HostCookieOp = serde_json::from_str(
            r#"{"op":"upsert","url":"u","cookie":{"key":"a","value":"1","httpOnly":true}}"#,
        )
        .expect("parse");
        match op {
            HostCookieOp::Upsert { cookie, .. } => {
                assert_eq!(cookie.key, "a");
                assert_eq!(cookie.http_only, Some(true));
            }
            other => panic!("unexpected {other:?}"),
        }
        let op: HostCookieOp = serde_json::from_str(r#"{"op":"clearAll"}"#).expect("parse");
        assert_eq!(op, HostCookieOp::ClearAll);
    }

    #[tokio::test]
    async fn script_host_cookies_defaults_to_unavailable() {
        struct Bare;
        #[async_trait]
        impl ScriptHost for Bare {}
        let out = Bare.cookies(HostCookieOp::ClearAll).await;
        assert_eq!(out, Err(HostError::Unavailable));
    }

    #[test]
    fn script_host_is_object_safe() {
        fn _assert(_: &dyn ScriptHost) {}
    }
}
