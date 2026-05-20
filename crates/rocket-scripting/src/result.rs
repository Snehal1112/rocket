use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// All side-effects produced by a single script execution.
///
/// Nothing in this struct is applied automatically — callers (in `rocket-app`)
/// read these fields and apply mutations to the request, variable context, and
/// persistence layer after the engine returns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptResult {
    /// Mutations to the outgoing request. Only meaningful from `BeforeRequest` phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_mutations: Option<RequestMutations>,

    /// Runtime variable writes via `rok.setVar(key, value)`.
    /// Cleared after the request completes — never persisted to disk.
    #[serde(default)]
    pub runtime_vars: HashMap<String, serde_json::Value>,

    /// Environment variable writes via `rok.setEnvVar(key, value, opts?)`.
    #[serde(default)]
    pub env_var_writes: Vec<EnvVarWrite>,

    /// Collection variable writes via `rok.setCollectionVar(key, value)`.
    #[serde(default)]
    pub collection_var_writes: Vec<CollectionVarWrite>,

    /// Global environment variable writes via `rok.setGlobalEnvVar(key, value)`.
    #[serde(default)]
    pub global_env_var_writes: Vec<EnvVarWrite>,

    /// Next request to run in a collection runner. `None` = no override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_request: Option<NextRequest>,

    /// When `true`, the runner skips this request entirely.
    #[serde(default)]
    pub skip_request: bool,

    /// `rok.test()` assertion outcomes. Populated from the `Tests` phase.
    #[serde(default)]
    pub test_results: Vec<TestResult>,

    /// `console.log/warn/error` entries emitted during execution, in order.
    #[serde(default)]
    pub console_entries: Vec<ConsoleEntry>,

    /// Script-level error message if execution threw an uncaught exception.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Mutations that a `before-request` script can apply to the outgoing `HttpRequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMutations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,

    /// Headers to add or overwrite (key = name, value = value).
    #[serde(default)]
    pub headers_set: HashMap<String, String>,

    /// Header names to remove.
    #[serde(default)]
    pub headers_deleted: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
}

/// A single variable write to an environment scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
    /// When `true`, write is persisted to the environment `.yml` file.
    pub persist: bool,
}

/// A single variable write to the collection's variable map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
}

/// Controls the next request to execute in a collection runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NextRequest {
    /// Jump to the request with this display name.
    Name(String),
    /// Stop the runner immediately.
    Stop,
}

/// The outcome of a single `rok.test(name, fn)` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Display name passed to `rok.test(name, ...)`.
    pub name: String,
    pub status: TestStatus,
    /// Error message if the assertion failed or threw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Pass/fail status for a single test assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
}

/// A single `console.log`, `console.warn`, or `console.error` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
}

/// Severity level of a console entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_result_default_is_empty() {
        let r = ScriptResult::default();
        assert!(r.request_mutations.is_none());
        assert!(r.runtime_vars.is_empty());
        assert!(r.env_var_writes.is_empty());
        assert!(r.test_results.is_empty());
        assert!(r.console_entries.is_empty());
        assert!(r.error.is_none());
    }

    #[test]
    fn env_var_write_serde_roundtrip() {
        let w = EnvVarWrite {
            key: "TOKEN".into(),
            value: serde_json::json!("abc123"),
            persist: true,
        };
        let json = serde_json::to_string(&w).expect("serialize");
        let back: EnvVarWrite = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.key, "TOKEN");
        assert_eq!(back.persist, true);
    }

    #[test]
    fn test_result_serde_roundtrip() {
        let t = TestResult {
            name: "status is 200".into(),
            status: TestStatus::Failed,
            error: Some("expected 200 got 404".into()),
        };
        let json = serde_json::to_string(&t).expect("serialize");
        let back: TestResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "status is 200");
        assert_eq!(back.status, TestStatus::Failed);
    }

    #[test]
    fn next_request_serde_roundtrip() {
        let n = NextRequest::Name("Poll Status".into());
        let json = serde_json::to_string(&n).expect("serialize");
        let back: NextRequest = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, NextRequest::Name(s) if s == "Poll Status"));
    }
}
