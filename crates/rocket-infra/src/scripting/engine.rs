use async_trait::async_trait;
use deno_core::{extension, JsRuntime, OpState, RuntimeOptions, op2};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::{console, req, res, rok};

/// JS scripting engine backed by `deno_core` (V8).
///
/// Creates one `JsRuntime` per `execute()` call — complete isolation between requests.
/// No Deno standard library, no file system, no network — only the `rok`, `req`,
/// `res`, `console`, `test`, `expect`, and `require` globals defined in `bootstrap.js`.
pub struct DenoScriptEngine;

impl DenoScriptEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DenoScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        // JsRuntime is !Send, so all V8 work must stay on one thread.
        let result = tokio::task::spawn_blocking(move || run_script(ctx))
            .await
            .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?;
        result
    }
}

// ── test runner ops ──────────────────────────────────────────────────────────

#[op2(fast)]
fn op_test_run(#[string] _name: String) {
    // Registration marker; no state side-effect needed.
}

#[op2(fast)]
fn op_test_pass(state: &mut OpState, #[string] name: String) {
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, true, None);
}

#[op2(fast)]
fn op_test_fail(state: &mut OpState, #[string] name: String, #[string] error: String) {
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, false, Some(error));
}

#[op2]
#[string]
fn op_require_module(#[string] name: String) -> String {
    // Returns the UMD source for the requested module, or empty string if unknown.
    // Module bundles added in SP3-05.
    match name.as_str() {
        _ => String::new(),
    }
}

extension!(
    rocket_scripting_ext,
    ops = [
        // rok ops
        rok::op_rok_get_var,
        rok::op_rok_set_var,
        rok::op_rok_get_env_var,
        rok::op_rok_set_env_var,
        rok::op_rok_has_env_var,
        rok::op_rok_delete_env_var,
        rok::op_rok_get_env_name,
        rok::op_rok_get_collection_var,
        rok::op_rok_set_collection_var,
        rok::op_rok_get_global_env_var,
        rok::op_rok_set_global_env_var,
        rok::op_rok_interpolate,
        rok::op_rok_set_next_request,
        rok::op_rok_skip_request,
        // req read ops
        req::op_req_get_url,
        req::op_req_get_host,
        req::op_req_get_path,
        req::op_req_get_query_string,
        req::op_req_get_method,
        req::op_req_get_auth_mode,
        req::op_req_get_header,
        req::op_req_get_headers,
        req::op_req_get_body,
        req::op_req_get_timeout,
        req::op_req_get_execution_mode,
        req::op_req_get_execution_platform,
        // req write ops
        req::op_req_set_url,
        req::op_req_set_method,
        req::op_req_set_header,
        req::op_req_set_headers,
        req::op_req_delete_header,
        req::op_req_delete_headers,
        req::op_req_set_body,
        req::op_req_set_timeout,
        req::op_req_set_max_redirects,
        // res ops
        res::op_res_get_status,
        res::op_res_get_status_text,
        res::op_res_get_header,
        res::op_res_get_headers,
        res::op_res_get_body,
        res::op_res_get_response_time,
        // console ops
        console::op_console_log,
        console::op_console_warn,
        console::op_console_error,
        // test runner ops
        op_test_run,
        op_test_pass,
        op_test_fail,
        op_require_module,
    ],
);

fn run_script(ctx: ScriptContext) -> DomainResult<ScriptResult> {
    let code = ctx.code;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        ..Default::default()
    });

    // Seed OpState with input and output state.
    {
        let op_state = runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.put(ScriptInputState {
            phase: ctx.phase,
            variables: ctx.variables,
            request: ctx.request,
            response: ctx.response,
            env_name: ctx.env_name,
            execution_mode: ctx.execution_mode,
            execution_platform: ctx.execution_platform,
        });
        state.put(ScriptOutputState::default());
    }

    const BOOTSTRAP: &str = include_str!("bootstrap.js");
    runtime
        .execute_script("<bootstrap>", BOOTSTRAP)
        .map_err(|e| DomainError::Internal(format!("bootstrap error: {e}")))?;

    // Capture script-level exceptions rather than propagating them as errors.
    let script_error = match runtime.execute_script("<user>", code) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };

    let out = {
        let op_state = runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.take::<ScriptOutputState>()
    };

    let request_mutations = if out.any_request_mutation {
        Some(out.request_mutations)
    } else {
        None
    };

    Ok(ScriptResult {
        request_mutations,
        runtime_vars: out.runtime_vars,
        env_var_writes: out.env_var_writes,
        collection_var_writes: out.collection_var_writes,
        global_env_var_writes: out.global_env_var_writes,
        next_request: out.next_request,
        skip_request: out.skip_request,
        test_results: out.test_results,
        console_entries: out.console_entries,
        error: script_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_scripting::{ScriptContext, ScriptPhase};
    use rocket_environment::VariableContext;
    use rocket_http::HttpRequest;
    use rocket_shared::types::HttpMethod;

    fn minimal_ctx(code: &str) -> ScriptContext {
        ScriptContext {
            code: code.into(),
            phase: ScriptPhase::BeforeRequest,
            variables: VariableContext::default(),
            request: HttpRequest::new(HttpMethod::Get, "https://example.com"),
            response: None,
            env_name: None,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    #[tokio::test]
    async fn console_log_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("console.log('hello from script')");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 1);
        assert!(result.console_entries[0].message.contains("hello from script"));
    }

    #[tokio::test]
    async fn script_error_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("throw new Error('deliberate')");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_some());
        let err = result.error.expect("error present");
        assert!(err.contains("deliberate"));
    }

    #[tokio::test]
    async fn rok_set_and_get_runtime_var() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('token', 'abc123')");
        let result = engine.execute(ctx).await.expect("execute");
        let val = result.runtime_vars.get("token").expect("token present");
        assert_eq!(val, "abc123");
    }

    #[tokio::test]
    async fn rok_get_env_var_reads_from_context() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("BASE_URL".into(), "https://api.example.com".into());
        let mut ctx = minimal_ctx("rok.setVar('url', rok.getEnvVar('BASE_URL'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let val = result.runtime_vars.get("url").expect("url present");
        assert_eq!(val, "https://api.example.com");
    }

    #[tokio::test]
    async fn rok_set_env_var_no_persist() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setEnvVar('SESSION', 'xyz')");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.env_var_writes.len(), 1);
        assert_eq!(result.env_var_writes[0].key, "SESSION");
        assert!(!result.env_var_writes[0].persist);
    }

    #[tokio::test]
    async fn rok_set_env_var_with_persist() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setEnvVar('TOKEN', 'abc', { persist: true })");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.env_var_writes[0].persist);
    }

    #[tokio::test]
    async fn rok_has_env_var() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("EXISTS".into(), "yes".into());
        let mut ctx = minimal_ctx("rok.setVar('found', rok.hasEnvVar('EXISTS') ? '1' : '0')");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("found").expect("found present"), "1");
    }

    #[tokio::test]
    async fn rok_interpolate() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("host".into(), "api.example.com".into());
        let mut ctx = minimal_ctx("rok.setVar('url', rok.interpolate('https://{{host}}/users'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("url").expect("url present"), "https://api.example.com/users");
    }

    #[tokio::test]
    async fn rok_runner_skip_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.runner.skipRequest()");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.skip_request);
    }

    #[tokio::test]
    async fn rok_runner_set_next_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.runner.setNextRequest('Poll Status')");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(matches!(result.next_request, Some(rocket_scripting::NextRequest::Name(s)) if s == "Poll Status"));
    }

    fn stub_response(status: u16) -> rocket_http::HttpResponse {
        rocket_http::HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![],
            body: String::new(),
            duration_ms: 0,
            ttfb_ms: 0,
            size_bytes: 0,
        }
    }

    #[tokio::test]
    async fn req_set_header_in_before_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("req.setHeader('x-custom', 'my-value')");
        let result = engine.execute(ctx).await.expect("execute");
        let mutations = result.request_mutations.expect("mutations present");
        assert_eq!(mutations.headers_set.get("x-custom").expect("header"), "my-value");
    }

    #[tokio::test]
    async fn req_set_url_in_before_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("req.setUrl('https://new.example.com/api')");
        let result = engine.execute(ctx).await.expect("execute");
        let mutations = result.request_mutations.expect("mutations present");
        assert_eq!(mutations.url.expect("url"), "https://new.example.com/api");
    }

    #[tokio::test]
    async fn req_mutation_rejected_in_after_response() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("req.setUrl('https://blocked.com')");
        ctx.phase = rocket_scripting::ScriptPhase::AfterResponse;
        ctx.response = Some(stub_response(200));
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_some(), "expected phase guard error");
        assert!(result.request_mutations.is_none());
    }

    #[tokio::test]
    async fn res_get_status_in_after_response() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("rok.setVar('code', String(res.getStatus()))");
        ctx.phase = rocket_scripting::ScriptPhase::AfterResponse;
        ctx.response = Some(stub_response(201));
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("code").expect("code"), "201");
    }

    #[tokio::test]
    async fn res_unavailable_in_before_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("res.getStatus()");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_some(), "expected res unavailable error");
    }
}
