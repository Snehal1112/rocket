use async_trait::async_trait;
use deno_core::{extension, JsRuntime, OpState, RuntimeOptions, op2};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::{console, redact, req, res, rok};

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
    let redacted = redact(state, error);
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, false, Some(redacted));
}

#[op2]
#[string]
fn op_require_module(#[string] name: String) -> String {
    match name.as_str() {
        "chai"          => include_str!("modules/chai.js").to_string(),
        "crypto-js"     => include_str!("modules/crypto-js.js").to_string(),
        "jsonwebtoken"  => include_str!("modules/jsonwebtoken.js").to_string(),
        "jsrsasign"     => include_str!("modules/jsrsasign.js").to_string(),
        "uuid"          => include_str!("modules/uuid.js").to_string(),
        "moment"        => include_str!("modules/moment.js").to_string(),
        "nanoid"        => include_str!("modules/nanoid.js").to_string(),
        "tv4"           => include_str!("modules/tv4.js").to_string(),
        "axios"         => include_str!("modules/axios.js").to_string(),
        "atob" | "btoa" => include_str!("modules/atob-btoa.js").to_string(),
        _               => String::new(),
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
        req::op_req_get_name,
        req::op_req_get_tags,
        req::op_req_get_path_params,
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
        let secret_values = ctx.variables.secret_values.clone();
        state.put(ScriptInputState {
            phase: ctx.phase,
            variables: ctx.variables,
            request: ctx.request,
            response: ctx.response,
            env_name: ctx.env_name,
            execution_mode: ctx.execution_mode,
            execution_platform: ctx.execution_platform,
            request_name: ctx.request_name,
            request_tags: ctx.request_tags,
            path_params: ctx.path_params,
            secret_values,
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
            request_name: String::new(),
            request_tags: vec![],
            path_params: vec![],
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

    #[tokio::test]
    async fn console_warn_and_error_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("console.warn('watch out'); console.error('bad thing')");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 2);
        assert_eq!(result.console_entries[0].level, rocket_scripting::ConsoleLevel::Warn);
        assert_eq!(result.console_entries[1].level, rocket_scripting::ConsoleLevel::Error);
    }

    #[tokio::test]
    async fn console_log_redacts_secret_env_var() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('API_KEY'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 1);
        assert_eq!(result.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_secret_substring_in_larger_string() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log('token=' + rok.getEnvVar('API_KEY'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "token=••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_value_copied_to_different_scope_key() {
        // Redaction is content-based, not name/scope-based: a secret value
        // placed in the runtime scope under a *different* key from where it
        // was originally read is still caught.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("copy".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log(rok.getVar('copy'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn two_phase_set_var_then_get_var_redacts_copied_secret() {
        // Reproduces the real production flow: a before-request script
        // copies a secret into a runtime var with rok.setVar, the host
        // (RequestExecutionService::apply_script_side_effects, rocket-app)
        // merges that write into VariableContext.runtime for the next
        // phase, and a later phase's console.log of the copy is still
        // redacted — even though op_rok_get_var only ever reads the
        // *input* snapshot, never the current phase's own writes.
        let engine = DenoScriptEngine::new();

        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());

        let mut ctx1 = minimal_ctx("rok.setVar('copy', rok.getEnvVar('API_KEY'))");
        ctx1.variables = vars.clone();
        let result1 = engine.execute(ctx1).await.expect("execute phase 1");
        let copied = result1
            .runtime_vars
            .get("copy")
            .and_then(|v| v.as_str())
            .expect("copy runtime var present")
            .to_string();
        assert_eq!(copied, "sk-live-abcdef123");

        // Simulate apply_script_side_effects merging runtime_vars into the
        // context carried forward to the next phase.
        vars.runtime.insert("copy".into(), copied);

        let mut ctx2 = minimal_ctx("console.log(rok.getVar('copy'))");
        ctx2.variables = vars;
        let result2 = engine.execute(ctx2).await.expect("execute phase 2");
        assert_eq!(result2.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn console_warn_and_error_redact_secret_values() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "console.warn(rok.getEnvVar('API_KEY')); console.error('key: ' + rok.getEnvVar('API_KEY'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 2);
        assert_eq!(result.console_entries[0].message, "••••••");
        assert_eq!(result.console_entries[1].message, "key: ••••••");
    }

    #[tokio::test]
    async fn non_secret_variable_value_is_not_redacted() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("PLAIN".into(), "plain-value-123".into());
        // secret_values intentionally left empty.
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('PLAIN'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "plain-value-123");
    }

    #[tokio::test]
    async fn overlapping_secret_substrings_do_not_panic() {
        // One secret's value is a substring of another's. Whichever order
        // redact()'s HashSet iteration replaces them in, this must not
        // panic, and the longer secret's full raw value must not survive.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("SHORT".into(), "abcdef1".into());
        vars.env.insert("LONG".into(), "abcdef123456".into());
        vars.secret_values.insert("abcdef1".into());
        vars.secret_values.insert("abcdef123456".into());
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('LONG'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert!(!result.console_entries[0].message.contains("abcdef123456"));
    }

    #[tokio::test]
    async fn short_secret_not_in_secret_values_is_not_redacted() {
        // Documents the MIN_REDACTION_LEN trade-off (enforced upstream in
        // RequestExecutionService::build_variable_scopes, rocket-app, Task
        // 2 of this plan): a secret this short is never added to
        // secret_values, so redact() has nothing to match and the raw
        // value passes through unchanged. This is the deliberate,
        // documented limitation from spec §3.4.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("SHORT".into(), "abc".into());
        // secret_values intentionally does NOT contain "abc" — mirrors
        // what rocket-app does for a value under MIN_REDACTION_LEN.
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('SHORT'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "abc");
    }

    #[tokio::test]
    async fn rok_test_failure_message_redacts_secret_value() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "rok.test('leaks secret', () => { throw new Error(rok.getEnvVar('API_KEY')) })",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.test_results.len(), 1);
        assert_eq!(result.test_results[0].status, rocket_scripting::TestStatus::Failed);
        let err = result.test_results[0].error.as_ref().expect("error message present");
        // JS `String(new Error(msg))` formats as "Error: <msg>".
        assert_eq!(err, "Error: ••••••");
    }

    #[tokio::test]
    async fn req_set_header_with_secret_value_is_not_redacted() {
        // Redaction is an observability-surface-only concern (console/test
        // output). req.setHeader must still carry the real secret so the
        // actual outgoing HTTP request functions correctly.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "req.setHeader('Authorization', 'Bearer ' + rok.getEnvVar('API_KEY'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let mutations = result.request_mutations.expect("mutations present");
        assert!(matches!(
            mutations.headers.as_slice(),
            [rocket_scripting::HeaderMutation::Set { name, value }]
                if name == "Authorization" && value == "Bearer sk-live-abcdef123"
        ));
    }

    #[tokio::test]
    async fn require_chai_and_use_expect() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"
            const chai = require('chai');
            const chaiExpect = chai.expect;
            rok.setVar('result', 'pass');
            chaiExpect(1 + 1).to.equal(2);
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("result").expect("result"), "pass");
    }

    #[tokio::test]
    async fn require_uuid_v4() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"
            const { v4: uuidv4 } = require('uuid');
            const id = uuidv4();
            rok.setVar('id', id);
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none());
        let id = result.runtime_vars.get("id").expect("id present").as_str().expect("id is string");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }

    #[tokio::test]
    async fn require_axios_loads_but_calling_it_throws_clear_error() {
        let engine = DenoScriptEngine::new();
        // require() itself must succeed (there's no wiring gap like the old
        // jsrsasign bug), but calling it must fail immediately and clearly —
        // there is no outbound-HTTP bridge from inside the script sandbox.
        let ctx = minimal_ctx(r#"
            const axios = require('axios');
            axios.get('https://example.com');
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("axios.get() must throw, not silently succeed");
        assert!(
            err.contains("not supported in RocketAPI scripts"),
            "unexpected error message: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_require_returns_error() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("require('not-a-real-module')");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().expect("error").contains("Module not found"));
    }

    #[tokio::test]
    async fn atob_btoa_polyfill_roundtrip() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('r', atob(btoa('hello world!')))");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("r").expect("r"), "hello world!");
    }

    #[tokio::test]
    async fn require_jsonwebtoken_sign_and_verify_roundtrip() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: '123' }, 'my-secret');
            rok.setVar('ok', jwt.verify(token, 'my-secret'));
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("ok").expect("ok"), true);
    }

    #[tokio::test]
    async fn require_jsonwebtoken_verify_rejects_tampered_token() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: '123' }, 'my-secret');
            rok.setVar('wrongSecret', jwt.verify(token, 'not-the-secret'));
            rok.setVar('tampered', jwt.verify(token + 'x', 'my-secret'));
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("wrongSecret").expect("wrongSecret"), false);
        assert_eq!(result.runtime_vars.get("tampered").expect("tampered"), false);
    }

    #[tokio::test]
    async fn require_jsonwebtoken_decode_reads_claims_without_verifying() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: 'abc123' }, 'my-secret');
            const claims = jwt.decode(token);
            rok.setVar('sub', claims.sub);
        "#);
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("sub").expect("sub"), "abc123");
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
        assert!(matches!(
            mutations.headers.as_slice(),
            [rocket_scripting::HeaderMutation::Set { name, value }]
                if name == "x-custom" && value == "my-value"
        ));
    }

    #[tokio::test]
    async fn req_delete_then_set_header_preserves_order() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("req.deleteHeader('Authorization'); req.setHeader('Authorization', 'Bearer tok')");
        let result = engine.execute(ctx).await.expect("execute");
        let mutations = result.request_mutations.expect("mutations present");
        assert!(matches!(
            mutations.headers.as_slice(),
            [
                rocket_scripting::HeaderMutation::Delete { name: d },
                rocket_scripting::HeaderMutation::Set { name: s, value }
            ] if d == "Authorization" && s == "Authorization" && value == "Bearer tok"
        ));
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

    #[tokio::test]
    async fn req_get_name_returns_context_name() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("rok.setVar('name', req.getName())");
        ctx.request_name = "Get User".into();
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("name").expect("name"), "Get User");
    }

    #[tokio::test]
    async fn req_get_tags_returns_json_array() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("rok.setVar('tags', JSON.stringify(req.getTags()))");
        ctx.request_tags = vec!["smoke".into(), "auth".into()];
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("tags").expect("tags"),
            r#"["smoke","auth"]"#
        );
    }

    #[tokio::test]
    async fn req_get_path_params_returns_json_array() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx(
            "rok.setVar('id', req.getPathParams()[0].name + '=' + req.getPathParams()[0].value)",
        );
        ctx.path_params = vec![rocket_shared::types::PathParam {
            name: "id".into(),
            value: "123".into(),
            description: None,
        }];
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("id").expect("id"), "id=123");
    }
}
