use rocket_environment::VariableContext;
use rocket_http::{HttpRequest, HttpResponse};
use rocket_shared::types::PathParam;
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

    /// The collection-level name of the request being executed, for `req.getName()`.
    pub request_name: String,

    /// Tags on the request being executed, for `req.getTags()`.
    pub request_tags: Vec<String>,

    /// Path parameters on the request being executed, for `req.getPathParams()`.
    pub path_params: Vec<PathParam>,
}

impl ScriptContext {
    /// Convenience constructor for a `BeforeRequest` context.
    #[allow(clippy::too_many_arguments)]
    pub fn before_request(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        env_name: Option<String>,
        request_name: String,
        request_tags: Vec<String>,
        path_params: Vec<PathParam>,
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
            request_name,
            request_tags,
            path_params,
        }
    }

    /// Convenience constructor for an `AfterResponse` context.
    #[allow(clippy::too_many_arguments)]
    pub fn after_response(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
        request_name: String,
        request_tags: Vec<String>,
        path_params: Vec<PathParam>,
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
            request_name,
            request_tags,
            path_params,
        }
    }

    /// Convenience constructor for a `Tests` context.
    #[allow(clippy::too_many_arguments)]
    pub fn tests(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
        request_name: String,
        request_tags: Vec<String>,
        path_params: Vec<PathParam>,
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
            request_name,
            request_tags,
            path_params,
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
            String::new(),
            vec![],
            vec![],
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
            String::new(),
            vec![],
            vec![],
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
            String::new(),
            vec![],
            vec![],
        );
        assert_eq!(ctx.phase, ScriptPhase::Tests);
        assert!(ctx.response.is_some());
    }

    #[test]
    fn carries_name_tags_and_path_params() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            "Get User".into(),
            vec!["smoke".into()],
            vec![PathParam {
                name: "id".into(),
                value: "123".into(),
                description: None,
            }],
        );
        assert_eq!(ctx.request_name, "Get User");
        assert_eq!(ctx.request_tags, vec!["smoke".to_string()]);
        assert_eq!(ctx.path_params.len(), 1);
        assert_eq!(ctx.path_params[0].name, "id");
    }
}
