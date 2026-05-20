use rocket_environment::VariableContext;
use rocket_http::{HttpRequest, HttpResponse};
use crate::ScriptPhase;

/// Everything the JS sandbox needs to read at execution time.
///
/// This is a snapshot — immutable once constructed. The engine returns
/// `ScriptResult` carrying any mutations; it never writes back to this struct.
#[derive(Debug, Clone)]
pub struct ScriptContext {
    /// The JavaScript source code to execute.
    pub code: String,

    /// Which lifecycle phase this script runs in.
    pub phase: ScriptPhase,

    /// Resolved variable scopes (read-only snapshot at send time).
    pub variables: VariableContext,

    /// The outgoing HTTP request. In `BeforeRequest` phase, `req.set*` mutations
    /// are collected into `ScriptResult.request_mutations`. In later phases,
    /// `req.set*` calls return a JS error.
    pub request: HttpRequest,

    /// The completed HTTP response. `None` in `BeforeRequest` phase.
    /// Accessing `res` in `BeforeRequest` throws a JS error.
    pub response: Option<HttpResponse>,

    /// Name of the currently active environment, for `rok.getEnvName()`.
    pub env_name: Option<String>,

    /// `"runner"` when executing inside a collection run, `"standalone"` otherwise.
    pub execution_mode: String,

    /// Always `"app"` for the desktop app.
    pub execution_platform: String,
}

impl ScriptContext {
    /// Convenience constructor for a `BeforeRequest` context.
    pub fn before_request(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::BeforeRequest,
            variables,
            request,
            response: None,
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    /// Convenience constructor for an `AfterResponse` context.
    pub fn after_response(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::AfterResponse,
            variables,
            request,
            response: Some(response),
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    /// Convenience constructor for a `Tests` context.
    pub fn tests(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::Tests,
            variables,
            request,
            response: Some(response),
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::VariableContext;
    use rocket_http::HttpRequest;
    use rocket_shared::types::HttpMethod;

    fn stub_request() -> HttpRequest {
        HttpRequest::new(HttpMethod::Get, "https://example.com")
    }

    fn stub_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: String::new(),
            duration_ms: 0,
            ttfb_ms: 0,
            size_bytes: 0,
        }
    }

    #[test]
    fn before_request_has_no_response() {
        let ctx = ScriptContext::before_request(
            "console.log('hi')".into(),
            VariableContext::default(),
            stub_request(),
            None,
        );
        assert_eq!(ctx.phase, ScriptPhase::BeforeRequest);
        assert!(ctx.response.is_none());
        assert_eq!(ctx.execution_platform, "app");
        assert_eq!(ctx.execution_mode, "standalone");
    }

    #[test]
    fn after_response_has_response() {
        let ctx = ScriptContext::after_response(
            String::new(),
            VariableContext::default(),
            stub_request(),
            stub_response(),
            Some("dev".into()),
        );
        assert_eq!(ctx.phase, ScriptPhase::AfterResponse);
        assert!(ctx.response.is_some());
        assert_eq!(ctx.env_name, Some("dev".into()));
    }

    #[test]
    fn tests_phase_has_response() {
        let ctx = ScriptContext::tests(
            String::new(),
            VariableContext::default(),
            stub_request(),
            stub_response(),
            None,
        );
        assert_eq!(ctx.phase, ScriptPhase::Tests);
        assert!(ctx.response.is_some());
    }
}
