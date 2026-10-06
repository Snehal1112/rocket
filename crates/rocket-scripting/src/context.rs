use crate::ScriptPhase;
use rocket_environment::VariableContext;
use rocket_http::{HttpRequest, HttpResponse};
use rocket_shared::types::PathParam;
use std::path::PathBuf;

/// How the request carrying a script was dispatched.
///
/// Maps 1:1 to the string `req.getExecutionMode()` returns inside the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// A single send, e.g. from the Request tab.
    #[default]
    Standalone,
    /// A step dispatched by the Collection Runner.
    Runner,
}

impl ExecutionMode {
    /// The exact string the sandbox exposes. Do not change these values.
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionMode::Standalone => "standalone",
            ExecutionMode::Runner => "runner",
        }
    }
}

/// JS sandbox capability level for the collection this script belongs to.
///
/// Mirrors `rocket_collection::settings::SandboxMode` — kept as a separate
/// type deliberately, to avoid `rocket-scripting` depending on
/// `rocket-collection`. `rocket-app` maps one to the other when building a
/// `ScriptContext`. Never serialized — this never crosses the JS boundary
/// directly; it only decides which `deno_core` extensions `rocket-infra`
/// registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxMode {
    #[default]
    Safe,
    Developer,
}

/// Where a script may load local `.js` files from.
///
/// Plain data only. `rocket-infra` does the file reading and the root checks.
/// Whether `additional_roots` is honoured depends on `ScriptContext.sandbox_mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFileScope {
    /// Absolute path of the collection directory.
    pub collection_root: PathBuf,
    /// Extra roots from `additionalContextRoots`. Relative entries are resolved
    /// against `collection_root` by the engine.
    pub additional_roots: Vec<PathBuf>,
}

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

    /// JS sandbox capability level — defaults to `Safe` on every constructor.
    pub sandbox_mode: SandboxMode,

    /// Always `"app"` for the desktop app.
    pub execution_platform: String,

    /// The collection-level name of the request being executed, for `req.getName()`.
    pub request_name: String,

    /// Tags on the request being executed, for `req.getTags()`.
    pub request_tags: Vec<String>,

    /// Path parameters on the request being executed, for `req.getPathParams()`.
    pub path_params: Vec<PathParam>,

    /// Local-file `require()` scope. `None` disables local requires.
    pub file_scope: Option<ScriptFileScope>,
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
            sandbox_mode: SandboxMode::Safe,
            execution_platform: "app".into(),
            request_name,
            request_tags,
            path_params,
            file_scope: None,
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
            sandbox_mode: SandboxMode::Safe,
            execution_platform: "app".into(),
            request_name,
            request_tags,
            path_params,
            file_scope: None,
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
            sandbox_mode: SandboxMode::Safe,
            execution_platform: "app".into(),
            request_name,
            request_tags,
            path_params,
            file_scope: None,
        }
    }

    /// Overrides the execution mode. The three constructors default to
    /// `Standalone`; the Collection Runner sets `Runner` on every context it
    /// builds, so `req.getExecutionMode()` reports the truth.
    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode.as_str().to_string();
        self
    }

    /// Overrides the sandbox mode. Defaults to `Safe`; `rocket-app` sets this
    /// from the collection's `sandbox_mode` setting for every phase.
    pub fn with_sandbox_mode(mut self, mode: SandboxMode) -> Self {
        self.sandbox_mode = mode;
        self
    }

    /// Sets the local-file `require()` scope. `rocket-app` builds it from the
    /// collection's location and `additionalContextRoots` setting.
    pub fn with_file_scope(mut self, scope: Option<ScriptFileScope>) -> Self {
        self.file_scope = scope;
        self
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
            ..Default::default()
        }
    }

    #[test]
    fn file_scope_defaults_to_none_and_can_be_set() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        );
        assert!(ctx.file_scope.is_none());
        let scope = ScriptFileScope {
            collection_root: "/tmp/col".into(),
            additional_roots: vec!["../shared".into()],
        };
        let ctx = ctx.with_file_scope(Some(scope.clone()));
        assert_eq!(ctx.file_scope, Some(scope));
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

    #[test]
    fn execution_mode_as_str_matches_script_api_strings() {
        assert_eq!(ExecutionMode::Standalone.as_str(), "standalone");
        assert_eq!(ExecutionMode::Runner.as_str(), "runner");
        assert_eq!(ExecutionMode::default(), ExecutionMode::Standalone);
    }

    #[test]
    fn with_execution_mode_overrides_the_standalone_default() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        )
        .with_execution_mode(ExecutionMode::Runner);
        assert_eq!(ctx.execution_mode, "runner");
    }

    #[test]
    fn sandbox_mode_defaults_to_safe() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        );
        assert_eq!(ctx.sandbox_mode, SandboxMode::Safe);
    }

    #[test]
    fn with_sandbox_mode_overrides_the_safe_default() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        )
        .with_sandbox_mode(SandboxMode::Developer);
        assert_eq!(ctx.sandbox_mode, SandboxMode::Developer);
    }
}
