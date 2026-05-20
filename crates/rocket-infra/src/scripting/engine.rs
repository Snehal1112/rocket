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
        rok::op_rok_get_var,
        req::op_req_get_url,
        res::op_res_get_status,
        console::op_console_log,
        console::op_console_warn,
        console::op_console_error,
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
}
