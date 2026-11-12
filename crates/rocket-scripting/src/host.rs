//! The host side of async script calls such as `rok.sendRequest`.
//!
//! `rocket-infra` runs the script and forwards each call to a `ScriptHost`.
//! `rocket-app` implements it. The types are plain data, so this crate does no I/O.

use std::collections::HashMap;

use crate::script_cookie::{CookieEntry, CookieInput};
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

/// Environment, global and collection writes the calling script made and has not saved yet,
/// in the order it made them. `None` deletes the key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingWrites {
    pub env: Vec<(String, Option<String>)>,
    pub global_env: Vec<(String, Option<String>)>,
    pub collection: Vec<(String, Option<String>)>,
}

/// A `rok.runRequest` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRunRequest {
    /// Path relative to the collection root, without extension, as the script wrote it.
    pub path: String,
    /// The calling script's runtime variables, its own writes so far included.
    pub runtime_vars: HashMap<String, String>,
    /// The calling script's unsaved environment, global and collection writes. The nested run
    /// sees them in memory only; the caller still saves them when its script ends.
    pub pending: PendingWrites,
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
    /// The call has nothing to report.
    Done,
}

/// Where a script gRPC call goes.
///
/// The script engine builds it from JSON. It is not an IPC or persistence type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HostGrpcTarget {
    /// A saved gRPC request, found by its path in the collection.
    Saved { path: String },
    /// A call described in full by the script.
    AdHoc {
        url: String,
        service: String,
        method: String,
        proto_file: String,
        tls: bool,
    },
}

/// One gRPC call from `rok.grpc`. Each message is one JSON text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGrpcRequest {
    pub target: HostGrpcTarget,
    pub messages: Vec<String>,
    pub metadata: Vec<(String, String)>,
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub runtime_vars: HashMap<String, String>,
}

/// The final status of a gRPC call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGrpcStatus {
    pub code: i32,
    pub name: String,
    pub message: String,
}

/// The answer to a `HostGrpcRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGrpcResponse {
    pub status: HostGrpcStatus,
    pub messages: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub trailers: Vec<(String, String)>,
    /// True when the response hit a message or byte limit and was cut.
    pub truncated: bool,
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

    /// Makes one gRPC call for `rok.grpc`.
    async fn grpc(&self, _request: HostGrpcRequest) -> Result<HostGrpcResponse, HostError> {
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
    fn host_grpc_request_reads_the_engine_json_shape() {
        let saved: HostGrpcRequest = serde_json::from_str(
            r#"{"target":{"kind":"saved","path":"api/greet"},"messages":["{}"],"metadata":[["a","1"]],"timeoutMs":500,"runtimeVars":{"k":"v"}}"#,
        )
        .expect("parse");
        assert_eq!(
            saved.target,
            HostGrpcTarget::Saved {
                path: "api/greet".into()
            }
        );
        assert_eq!(saved.messages, vec!["{}".to_string()]);
        assert_eq!(saved.metadata, vec![("a".to_string(), "1".to_string())]);
        assert_eq!(saved.timeout_ms, Some(500));
        assert_eq!(saved.runtime_vars.get("k").map(String::as_str), Some("v"));

        let ad_hoc: HostGrpcRequest = serde_json::from_str(
            r#"{"target":{"kind":"adHoc","url":"localhost:50051","service":"s.Greeter","method":"SayHello","protoFile":"greeter.proto","tls":true},"messages":[],"metadata":[]}"#,
        )
        .expect("parse");
        assert_eq!(
            ad_hoc.target,
            HostGrpcTarget::AdHoc {
                url: "localhost:50051".into(),
                service: "s.Greeter".into(),
                method: "SayHello".into(),
                proto_file: "greeter.proto".into(),
                tls: true,
            }
        );
        assert!(ad_hoc.runtime_vars.is_empty());
        assert_eq!(ad_hoc.timeout_ms, None);
    }

    #[test]
    fn host_grpc_response_writes_camel_case_keys() {
        let response = HostGrpcResponse {
            status: HostGrpcStatus {
                code: 0,
                name: "OK".into(),
                message: String::new(),
            },
            messages: vec!["{}".into()],
            headers: vec![],
            trailers: vec![("t".into(), "1".into())],
            truncated: true,
        };
        let value = serde_json::to_value(&response).expect("serialize");
        assert_eq!(value["status"]["name"], "OK");
        assert_eq!(value["truncated"], true);
        assert_eq!(value["trailers"][0][0], "t");
        assert_eq!(value["messages"][0], "{}");
    }

    fn grpc_request() -> HostGrpcRequest {
        HostGrpcRequest {
            target: HostGrpcTarget::Saved { path: "p".into() },
            messages: vec![],
            metadata: vec![],
            timeout_ms: None,
            runtime_vars: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn script_host_grpc_defaults_to_unavailable() {
        struct Bare;
        #[async_trait]
        impl ScriptHost for Bare {}
        let out = Bare.grpc(grpc_request()).await;
        assert_eq!(out, Err(HostError::Unavailable));
    }

    #[test]
    fn script_host_is_object_safe() {
        fn _assert(_: &dyn ScriptHost) {}
    }
}
