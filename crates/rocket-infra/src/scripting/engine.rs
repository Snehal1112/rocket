use async_trait::async_trait;
use deno_core::{extension, op2, v8, JsRuntime, OpState, RuntimeOptions};
use rocket_scripting::{SandboxMode, ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::scripting::ops::{console, fs, process, redact, req, res, rok};
use crate::scripting::state::{ScriptInputState, ScriptOutputState};

/// JS scripting engine backed by `deno_core` (V8).
///
/// Creates one `JsRuntime` per `execute()` call — complete isolation between requests.
/// No Deno standard library, no file system, no network — only the `rok`, `req`,
/// `res`, `console`, `test`, `expect`, and `require` globals defined in `bootstrap.js`.
///
/// `bootstrap.js` deletes both the `Deno` global and the `__bootstrap` global
/// deno_core parks the same ops table on, as its last two acts, so a user
/// script sees `typeof Deno === 'undefined'` and `typeof __bootstrap ===
/// 'undefined'` and cannot reach `deno_core`'s built-in ops such as
/// `op_print` or `op_panic` directly through either handle. The wrappers
/// keep working because they call through an ops reference captured in a
/// closure before those deletions.
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

/// Wall-clock budget for a single script execution.
///
/// Five seconds comfortably exceeds any legitimate pre-request, post-response,
/// or test script. Those scripts do in-memory templating, signing, and small
/// JSON manipulation. They have no network or filesystem access at all, so
/// there is nothing legitimate for them to wait on.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);

/// Best-effort V8 heap cap for a single script execution.
///
/// Generous on purpose. Legitimate scripts manipulate small JSON payloads, so
/// this only catches runaway allocation and never ordinary work.
const SCRIPT_HEAP_LIMIT_BYTES: usize = 256 * 1024 * 1024;

#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        run_script_with_timeout(ctx, SCRIPT_TIMEOUT).await
    }
}

/// Runs a script on a blocking thread and aborts it if `timeout` elapses.
///
/// Cancelling the async future alone would not stop the OS thread running V8,
/// so on timeout we ask V8 itself to abort the script through the isolate
/// handle the thread published on start. Tests call this directly with a short
/// timeout so the suite never waits the full `SCRIPT_TIMEOUT`.
async fn run_script_with_timeout(
    ctx: ScriptContext,
    timeout: Duration,
) -> DomainResult<ScriptResult> {
    // JsRuntime is !Send, so all V8 work must stay on one thread.
    let (handle_tx, handle_rx) = oneshot::channel();
    let join = tokio::task::spawn_blocking(move || run_script(ctx, handle_tx));

    match tokio::time::timeout(timeout, join).await {
        Ok(join_result) => {
            join_result.map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?
        }
        Err(_elapsed) => {
            // Terminate whenever the handle arrives, however late. Bounding
            // this wait would abandon a script that had not started yet: it
            // would then run unterminated and pin a blocking thread forever,
            // since dropping a spawn_blocking JoinHandle detaches rather than
            // cancels it.
            //
            // This must be a plain OS thread, not a `tokio::spawn`ed task: a
            // detached async task is tied to this call's Tokio runtime, and
            // on a short-lived runtime (every #[tokio::test] creates and
            // drops one per test) it can be cancelled before it ever gets
            // polled, deadlocking against the `spawn_blocking` thread that
            // Runtime::Drop waits on. A `std::thread` keeps running
            // regardless of what happens to the runtime that spawned it.
            //
            // Terminating makes the blocking thread's execute_script return
            // an "execution terminated" error; it then tears the runtime
            // down on its own and its result is discarded, so we do not wait
            // for it here.
            std::thread::spawn(move || {
                if let Ok(isolate_handle) = handle_rx.blocking_recv() {
                    isolate_handle.terminate_execution();
                }
            });
            Err(DomainError::Internal(format!(
                "script execution timed out after {timeout:?}"
            )))
        }
    }
}

// ── test runner ops ──────────────────────────────────────────────────────────

#[op2(fast)]
fn op_test_run(#[string] _name: String) {
    // Registration marker; no state side-effect needed.
}

#[op2(fast)]
fn op_test_pass(state: &mut OpState, #[string] name: String) {
    let redacted_name = redact(state, name);
    state
        .borrow_mut::<ScriptOutputState>()
        .add_test_result(redacted_name, true, None);
}

#[op2(fast)]
fn op_test_fail(state: &mut OpState, #[string] name: String, #[string] error: String) {
    let redacted_name = redact(state, name);
    let redacted_error = redact(state, error);
    state.borrow_mut::<ScriptOutputState>().add_test_result(
        redacted_name,
        false,
        Some(redacted_error),
    );
}

#[op2]
#[string]
fn op_require_module(#[string] name: String) -> String {
    match name.as_str() {
        "chai" => include_str!("modules/chai.js").to_string(),
        "crypto-js" => include_str!("modules/crypto-js.js").to_string(),
        "jsonwebtoken" => include_str!("modules/jsonwebtoken.js").to_string(),
        "jsrsasign" => include_str!("modules/jsrsasign.js").to_string(),
        "uuid" => include_str!("modules/uuid.js").to_string(),
        "moment" => include_str!("modules/moment.js").to_string(),
        "nanoid" => include_str!("modules/nanoid.js").to_string(),
        "tv4" => include_str!("modules/tv4.js").to_string(),
        "axios" => include_str!("modules/axios.js").to_string(),
        "lodash" => include_str!("modules/lodash.js").to_string(),
        "atob" | "btoa" => include_str!("modules/atob-btoa.js").to_string(),
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
        rok::op_rok_get_secret_var,
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
        req::op_req_get_header_list,
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
        res::op_res_get_header_list,
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

// Only registered when `ScriptContext.sandbox_mode == SandboxMode::Developer`
// (see `run_script` below). In Safe Mode these ops do not exist in the
// isolate at all — `typeof fs` / `typeof process` are `'undefined'`, not
// "defined but throws" — matching the same narrowly-enumerated op table
// philosophy as `rocket_scripting_ext` above, just gated per-run.
extension!(
    rocket_scripting_dev_ext,
    ops = [
        fs::op_fs_read_file,
        fs::op_fs_write_file,
        fs::op_fs_read_dir,
        fs::op_fs_exists,
        fs::op_fs_mkdir,
        fs::op_fs_remove,
        process::op_process_exec,
    ],
);

fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
) -> DomainResult<ScriptResult> {
    let code = ctx.code;
    let sandbox_mode = ctx.sandbox_mode;

    let mut extensions = vec![rocket_scripting_ext::init()];
    if sandbox_mode == SandboxMode::Developer {
        extensions.push(rocket_scripting_dev_ext::init());
    }

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions,
        create_params: Some(v8::CreateParams::default().heap_limits(0, SCRIPT_HEAP_LIMIT_BYTES)),
        ..Default::default()
    });

    let isolate_handle = runtime.v8_isolate().thread_safe_handle();

    // Publish the isolate handle before running any script code, so a timeout
    // can always reach it. A send failure only means the caller already gave
    // up, and there is nothing useful to do about that here.
    let _ = handle_tx.send(isolate_handle.clone());

    // Without this callback V8's default near-OOM behaviour is to abort the
    // whole process, which would be worse than the problem we are fixing.
    // Terminating the isolate turns an out-of-memory into an ordinary script
    // error instead. The raised limit returned here is only headroom for V8 to
    // unwind in. Termination is what actually stops the script.
    runtime.add_near_heap_limit_callback(move |current, _initial| {
        isolate_handle.terminate_execution();
        current + (current / 4)
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
    use rocket_environment::VariableContext;
    use rocket_http::HttpRequest;
    use rocket_scripting::{ScriptContext, ScriptPhase};
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
            sandbox_mode: SandboxMode::Safe,
        }
    }

    #[tokio::test]
    async fn console_log_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("console.log('hello from script')");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 1);
        assert!(result.console_entries[0]
            .message
            .contains("hello from script"));
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
        vars.env
            .insert("BASE_URL".into(), "https://api.example.com".into());
        let mut ctx = minimal_ctx("rok.setVar('url', rok.getEnvVar('BASE_URL'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let val = result.runtime_vars.get("url").expect("url present");
        assert_eq!(val, "https://api.example.com");
    }

    #[tokio::test]
    async fn rok_get_secret_var_reads_from_context() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.external_secrets
            .insert("payments.stripeKey".into(), "sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("rok.setVar('key', rok.getSecretVar('payments.stripeKey'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let val = result.runtime_vars.get("key").expect("key present");
        assert_eq!(val, "sk-live-abcdef123");
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
        assert_eq!(
            result.runtime_vars.get("found").expect("found present"),
            "1"
        );
    }

    #[tokio::test]
    async fn rok_interpolate() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("host".into(), "api.example.com".into());
        let mut ctx = minimal_ctx("rok.setVar('url', rok.interpolate('https://{{host}}/users'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("url").expect("url present"),
            "https://api.example.com/users"
        );
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
        assert!(
            matches!(result.next_request, Some(rocket_scripting::NextRequest::Name(s)) if s == "Poll Status")
        );
    }

    #[tokio::test]
    async fn console_warn_and_error_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("console.warn('watch out'); console.error('bad thing')");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 2);
        assert_eq!(
            result.console_entries[0].level,
            rocket_scripting::ConsoleLevel::Warn
        );
        assert_eq!(
            result.console_entries[1].level,
            rocket_scripting::ConsoleLevel::Error
        );
    }

    #[tokio::test]
    async fn console_log_redacts_secret_env_var() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
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
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log('token=' + rok.getEnvVar('API_KEY'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "token=••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_a_single_line_of_a_multi_line_vault_value() {
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0B\nAQEFAASCBKcwggSjAgEAAoIB\n-----END PRIVATE KEY-----\n";
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.external_secrets.insert("vault.key".into(), pem.into());
        // The app layer adds the whole value and each body line (`redaction_forms`).
        // The script layer masks every member of the set on its own.
        vars.secret_values.insert(pem.into());
        vars.secret_values.insert("MIIEvQIBADANBgkqhkiG9w0B".into());
        vars.secret_values.insert("AQEFAASCBKcwggSjAgEAAoIB".into());
        let mut ctx = minimal_ctx(
            "const pem = rok.getSecretVar('vault.key'); \
             console.log(pem); \
             console.log(pem.split('\\n')[1]); \
             console.log('line2=' + pem.split('\\n')[2]);",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 3);
        assert_eq!(result.console_entries[0].message, "••••••");
        assert_eq!(result.console_entries[1].message, "••••••");
        assert_eq!(result.console_entries[2].message, "line2=••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_value_copied_to_different_scope_key() {
        // Redaction is content-based, not name/scope-based: a secret value
        // placed in the runtime scope under a *different* key from where it
        // was originally read is still caught.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime
            .insert("copy".into(), "sk-live-abcdef123".into());
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
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
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
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
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
        // SHORT's value is a prefix of LONG's value. redact() must not panic,
        // must fully redact LONG's raw value, and — the sharper assertion —
        // must not leave any fragment of LONG's suffix in plaintext either
        // (a naive replace-in-arbitrary-order can consume SHORT first,
        // breaking LONG's exact-substring match and leaking its tail).
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("SHORT".into(), "abcdef1".into());
        vars.env.insert("LONG".into(), "abcdef123456".into());
        vars.secret_values.insert("abcdef1".into());
        vars.secret_values.insert("abcdef123456".into());
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('LONG'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let message = &result.console_entries[0].message;
        assert!(!message.contains("abcdef123456"));
        assert!(
            !message.contains("23456"),
            "a fragment of the longer secret must not survive: {message}"
        );
        assert_eq!(message, "••••••");
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
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "rok.test('leaks secret', () => { throw new Error(rok.getEnvVar('API_KEY')) })",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.test_results.len(), 1);
        assert_eq!(
            result.test_results[0].status,
            rocket_scripting::TestStatus::Failed
        );
        let err = result.test_results[0]
            .error
            .as_ref()
            .expect("error message present");
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
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx =
            minimal_ctx("req.setHeader('Authorization', 'Bearer ' + rok.getEnvVar('API_KEY'))");
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
    async fn rok_test_passing_name_redacts_secret_value() {
        // A test name is display text on the same observability surface as
        // console output and failure messages — a secret embedded in the
        // name (e.g. rok.test(apiKey, () => {...})) must be redacted too.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("rok.test(rok.getEnvVar('API_KEY'), () => {})");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.test_results.len(), 1);
        assert_eq!(
            result.test_results[0].status,
            rocket_scripting::TestStatus::Passed
        );
        assert_eq!(result.test_results[0].name, "••••••");
    }

    #[tokio::test]
    async fn rok_test_failing_name_redacts_secret_value() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env
            .insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx =
            minimal_ctx("rok.test(rok.getEnvVar('API_KEY'), () => { throw new Error('boom') })");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.test_results.len(), 1);
        assert_eq!(
            result.test_results[0].status,
            rocket_scripting::TestStatus::Failed
        );
        assert_eq!(result.test_results[0].name, "••••••");
    }

    #[tokio::test]
    async fn require_chai_and_use_expect() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const chai = require('chai');
            const chaiExpect = chai.expect;
            rok.setVar('result', 'pass');
            chaiExpect(1 + 1).to.equal(2);
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("result").expect("result"), "pass");
    }

    #[tokio::test]
    async fn require_uuid_v4() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const { v4: uuidv4 } = require('uuid');
            const id = uuidv4();
            rok.setVar('id', id);
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none());
        let id = result
            .runtime_vars
            .get("id")
            .expect("id present")
            .as_str()
            .expect("id is string");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }

    #[tokio::test]
    async fn transform_style_body_can_require_uuid() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const out = (function () {
                const fn = new Function('response', "const { v4 } = require('uuid'); return v4();");
                return fn({ body: null });
            })();
            rok.setVar('out', out);
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        let id = result
            .runtime_vars
            .get("out")
            .expect("out")
            .as_str()
            .expect("string");
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn transform_style_body_unknown_module_errors() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const fn = new Function('response', "return require('fs');");
            fn({ body: null });
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result
            .error
            .as_ref()
            .expect("error expected")
            .contains("Module not found: fs"));
    }

    #[tokio::test]
    async fn require_axios_loads_but_calling_it_throws_clear_error() {
        let engine = DenoScriptEngine::new();
        // require() itself must succeed (there's no wiring gap like the old
        // jsrsasign bug), but calling it must fail immediately and clearly —
        // there is no outbound-HTTP bridge from inside the script sandbox.
        let ctx = minimal_ctx(
            r#"
            const axios = require('axios');
            axios.get('https://example.com');
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        let err = result
            .error
            .expect("axios.get() must throw, not silently succeed");
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
        assert!(result
            .error
            .as_ref()
            .expect("error")
            .contains("Module not found"));
    }

    #[tokio::test]
    async fn require_lodash_group_by_and_get() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const _ = require('lodash');
            const g = _.groupBy([{t:'a'},{t:'b'},{t:'a'}], 't');
            rok.setVar('n', String(g.a.length));
            rok.setVar('deep', String(_.get({a:{b:[7]}}, 'a.b[0]')));
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("n").expect("n"), "2");
        assert_eq!(result.runtime_vars.get("deep").expect("deep"), "7");
    }

    #[tokio::test]
    async fn atob_btoa_polyfill_roundtrip() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('r', atob(btoa('hello world!')))");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("r").expect("r"), "hello world!");
    }

    #[tokio::test]
    async fn require_jsonwebtoken_sign_and_verify_roundtrip() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: '123' }, 'my-secret');
            rok.setVar('ok', jwt.verify(token, 'my-secret'));
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("ok").expect("ok"), true);
    }

    #[tokio::test]
    async fn require_jsonwebtoken_verify_rejects_tampered_token() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: '123' }, 'my-secret');
            rok.setVar('wrongSecret', jwt.verify(token, 'not-the-secret'));
            rok.setVar('tampered', jwt.verify(token + 'x', 'my-secret'));
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result.runtime_vars.get("wrongSecret").expect("wrongSecret"),
            false
        );
        assert_eq!(
            result.runtime_vars.get("tampered").expect("tampered"),
            false
        );
    }

    #[tokio::test]
    async fn require_jsonwebtoken_decode_reads_claims_without_verifying() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const jwt = require('jsonwebtoken');
            const token = jwt.sign({ sub: 'abc123' }, 'my-secret');
            const claims = jwt.decode(token);
            rok.setVar('sub', claims.sub);
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
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
            ..Default::default()
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
        let ctx = minimal_ctx(
            "req.deleteHeader('Authorization'); req.setHeader('Authorization', 'Bearer tok')",
        );
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

    fn response_with(
        headers: Vec<rocket_shared::types::Header>,
        body: &str,
    ) -> rocket_http::HttpResponse {
        rocket_http::HttpResponse {
            status: 201,
            status_text: "Created".into(),
            headers,
            body: body.into(),
            duration_ms: 42,
            ttfb_ms: 0,
            size_bytes: body.len(),
        }
    }

    async fn run_after_response(
        code: &str,
        response: rocket_http::HttpResponse,
    ) -> rocket_scripting::ScriptResult {
        let mut ctx = minimal_ctx(code);
        ctx.phase = rocket_scripting::ScriptPhase::AfterResponse;
        ctx.response = Some(response);
        DenoScriptEngine::new().execute(ctx).await.expect("execute")
    }

    fn json_response() -> rocket_http::HttpResponse {
        use rocket_shared::types::Header;
        response_with(
            vec![
                Header::new("Content-Type", "application/json"),
                Header::new("X-Req", "abc"),
            ],
            r#"{"a":1}"#,
        )
    }

    #[tokio::test]
    async fn res_properties_mirror_the_getters() {
        let result = run_after_response(
            "rok.setVar('s', JSON.stringify([res.status, res.statusText, res.body.a, \
             res.responseTime, res.headers['content-type'], res.headers['Content-Type'] === undefined]))",
            json_response(),
        )
        .await;
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("s").expect("s"),
            r#"[201,"Created",1,42,"application/json",true]"#
        );
    }

    #[tokio::test]
    async fn res_body_property_falls_back_to_the_raw_string() {
        let result = run_after_response(
            "rok.setVar('b', res.body + '|' + res.getBody({ raw: true }))",
            response_with(vec![], "plain text"),
        )
        .await;
        assert_eq!(
            result.runtime_vars.get("b").expect("b"),
            "plain text|plain text"
        );
    }

    #[tokio::test]
    async fn res_get_header_is_case_insensitive_and_undefined_when_missing() {
        let result = run_after_response(
            "rok.setVar('h', res.getHeader('CONTENT-TYPE') + '|' + String(res.getHeader('nope')))",
            json_response(),
        )
        .await;
        assert_eq!(
            result.runtime_vars.get("h").expect("h"),
            "application/json|undefined"
        );
    }

    #[tokio::test]
    async fn res_get_headers_lowercases_keys() {
        let result = run_after_response(
            "rok.setVar('k', Object.keys(res.getHeaders()).join(','))",
            json_response(),
        )
        .await;
        assert_eq!(
            result.runtime_vars.get("k").expect("k"),
            "content-type,x-req"
        );
    }

    #[tokio::test]
    async fn res_header_list_reads() {
        let result = run_after_response(
            "const l = res.headerList;\
             rok.setVar('r', JSON.stringify([\
               l.count(), l.get('x-req'), l.one('X-REQ'), l.has('x-req', 'abc'), l.has('x-req', 'no'),\
               l.has({ key: 'content-type' }), l.find((h) => h.key === 'X-Req').value,\
               l.filter((h) => h.key.startsWith('X-')).length, l.indexOf('x-req'),\
               l.map((h) => h.key), l.reduce((n) => n + 1, 0),\
               l.toString(), l.toJSON().length, Object.keys(l.toObject(false, true))\
             ]))",
            json_response(),
        )
        .await;
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("r").expect("r"),
            r#"[2,"abc",{"key":"X-Req","value":"abc"},true,false,true,"abc",1,1,["Content-Type","X-Req"],2,"Content-Type: application/json\nX-Req: abc",2,["Content-Type","X-Req"]]"#
        );
    }

    #[tokio::test]
    async fn res_header_list_rejects_every_write() {
        let result = run_after_response(
            "const l = res.headerList; const out = [];\
             for (const [name, args] of [['add', ['a', 'b']], ['upsert', ['a', 'b']], ['remove', ['a']],\
               ['clear', []], ['populate', [[]]], ['repopulate', [[]]], ['assimilate', [[]]]]) {\
               try { l[name](...args); out.push('no throw ' + name); }\
               catch (e) { out.push(String(e.message)); }\
             }\
             rok.setVar('w', out.join('|'))",
            json_response(),
        )
        .await;
        let expected = ["HeaderList is read-only"; 7].join("|");
        assert_eq!(result.runtime_vars.get("w").expect("w"), &expected);
    }

    #[tokio::test]
    async fn res_properties_do_not_break_serialising_res_before_the_response_exists() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('j', JSON.stringify(res))");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.runtime_vars.get("j").expect("j"), "{}");
    }

    fn request_with_headers() -> rocket_scripting::ScriptContext {
        use rocket_shared::types::Header;
        let mut ctx = minimal_ctx("");
        ctx.request.headers = vec![
            Header::new("Content-Type", "application/json"),
            Header::disabled("X-Off", "1"),
        ];
        ctx
    }

    #[tokio::test]
    async fn req_get_header_is_undefined_when_missing_or_disabled() {
        let mut ctx = request_with_headers();
        ctx.code =
            "rok.setVar('h', req.getHeader('content-type') + '|' + String(req.getHeader('nope'))\
                     + '|' + String(req.getHeader('x-off')))"
                .into();
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("h").expect("h"),
            "application/json|undefined|undefined"
        );
    }

    #[tokio::test]
    async fn req_get_headers_lowercases_keys_and_skips_disabled() {
        let mut ctx = request_with_headers();
        ctx.code = "rok.setVar('k', Object.keys(req.getHeaders()).join(','))".into();
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("k").expect("k"), "content-type");
    }

    #[tokio::test]
    async fn req_header_list_keeps_disabled_headers_visible() {
        let mut ctx = request_with_headers();
        ctx.code =
            "rok.setVar('l', JSON.stringify([req.headerList.count(), req.headerList.one('x-off'),\
                     req.headerList.toObject(true), req.headerList.toString()]))"
                .into();
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("l").expect("l"),
            r#"[2,{"key":"X-Off","value":"1","disabled":true},{"content-type":"application/json"},"Content-Type: application/json"]"#
        );
    }

    #[tokio::test]
    async fn req_header_list_writes_queue_mutations_and_read_back() {
        use rocket_scripting::HeaderMutation;
        let mut ctx = request_with_headers();
        ctx.code = "const l = req.headerList;\
                     const first = l.upsert('x-new', '1');\
                     const second = l.upsert('X-NEW', '2');\
                     l.add('Accept: text/plain');\
                     l.remove('x-off');\
                     req.setHeader('x-via-set', 'v');\
                     rok.setVar('r', JSON.stringify([first, second, req.getHeader('x-new'),\
                       req.getHeader('accept'), l.has('x-off'), req.getHeader('x-via-set'), l.count()]))"
            .into();
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("r").expect("r"),
            r#"[true,false,"2","text/plain",false,"v",4]"#
        );
        let mutations = result.request_mutations.expect("mutations present");
        let summary: Vec<String> = mutations
            .headers
            .iter()
            .map(|m| match m {
                HeaderMutation::Set { name, value } => format!("set {name}={value}"),
                HeaderMutation::Delete { name } => format!("del {name}"),
            })
            .collect();
        assert_eq!(
            summary,
            [
                "set x-new=1",
                "set X-NEW=2",
                "set Accept=text/plain",
                "del x-off",
                "set x-via-set=v"
            ]
        );
    }

    #[tokio::test]
    async fn req_header_list_bulk_writes() {
        let mut ctx = request_with_headers();
        ctx.code = "const l = req.headerList;\
                     l.populate([{ key: 'content-type', value: 'ignored' }, { key: 'x-a', value: '1' }]);\
                     const afterPopulate = l.toObject();\
                     l.assimilate([{ key: 'x-b', value: '2' }], true);\
                     const afterAssimilate = l.toObject();\
                     l.repopulate('x-c: 3\\nx-d: 4');\
                     const afterRepopulate = l.toObject();\
                     l.clear();\
                     rok.setVar('r', JSON.stringify([afterPopulate, afterAssimilate, afterRepopulate, l.count()]))"
            .into();
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("r").expect("r"),
            r#"[{"content-type":"application/json","x-off":"1","x-a":"1"},{"x-b":"2"},{"x-c":"3","x-d":"4"},0]"#
        );
    }

    #[tokio::test]
    async fn req_header_list_write_is_rejected_after_response() {
        let result = run_after_response("req.headerList.add('x-late', '1')", json_response()).await;
        assert!(result.error.is_some(), "expected phase guard error");
        assert!(result.request_mutations.is_none());
    }

    #[tokio::test]
    async fn req_get_body_returns_object_raw_string_or_undefined() {
        use rocket_shared::types::{Body, BodyMode};
        let body = |mode, content: Option<&str>| Body {
            mode,
            content: content.map(String::from),
            form_data: None,
            file_path: None,
        };
        let run = |request_body: Option<Body>| async move {
            let mut ctx = minimal_ctx(
                "const b = req.getBody(); rok.setVar('t', typeof b + ':' + (typeof b === 'object' ? JSON.stringify(b) : String(b)))",
            );
            ctx.request.body = request_body;
            let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
            assert!(result.error.is_none(), "{:?}", result.error);
            result.runtime_vars.get("t").expect("t").clone()
        };
        assert_eq!(
            run(Some(body(BodyMode::Json, Some(r#"{"a":1}"#)))).await,
            r#"object:{"a":1}"#
        );
        assert_eq!(
            run(Some(body(BodyMode::Text, Some("hello")))).await,
            "string:hello"
        );
        assert_eq!(
            run(Some(body(BodyMode::Text, None))).await,
            "undefined:undefined"
        );
        assert_eq!(run(None).await, "undefined:undefined");
    }

    #[tokio::test]
    async fn req_get_path_params_have_type_path() {
        let mut ctx = minimal_ctx("rok.setVar('p', JSON.stringify(req.getPathParams().map((p) => [p.name, p.value, p.type])))");
        ctx.path_params = vec![rocket_shared::types::PathParam {
            name: "id".into(),
            value: "7".into(),
            description: None,
        }];
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("p").expect("p"),
            r#"[["id","7","path"]]"#
        );
    }

    #[tokio::test]
    async fn req_get_auth_mode_names_every_auth_type() {
        use rocket_shared::types::Auth;
        let pair = || (String::from("u"), String::from("p"));
        let cases: Vec<(Auth, &str)> = vec![
            (Auth::None, "none"),
            (Auth::Inherit, "inherit"),
            (Auth::Bearer { token: "t".into() }, "bearer"),
            (
                Auth::Basic {
                    username: pair().0,
                    password: pair().1,
                },
                "basic",
            ),
            (
                Auth::Digest {
                    username: pair().0,
                    password: pair().1,
                },
                "digest",
            ),
            (
                Auth::Wsse {
                    username: pair().0,
                    password: pair().1,
                },
                "wsse",
            ),
            (
                Auth::Ntlm {
                    username: pair().0,
                    password: pair().1,
                    domain: "d".into(),
                },
                "ntlm",
            ),
            (
                Auth::ApiKey {
                    key: "k".into(),
                    value: "v".into(),
                    placement: "header".into(),
                },
                "apikey",
            ),
            (Auth::OAuth1(Box::default()), "oauth1"),
        ];
        for (auth, expected) in cases {
            let mut ctx = minimal_ctx("rok.setVar('m', req.getAuthMode())");
            ctx.request.auth = auth;
            let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
            assert_eq!(result.runtime_vars.get("m").expect("m"), expected);
        }
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

    // ── sandbox lockdown ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn deno_global_is_hidden_from_user_scripts() {
        // bootstrap.js must delete globalThis.Deno once it has wired up the
        // intended globals, so the user script sees no Deno at all.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('typeofDeno', typeof Deno)");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result
                .runtime_vars
                .get("typeofDeno")
                .expect("typeofDeno present"),
            "undefined"
        );
    }

    #[tokio::test]
    async fn deno_core_ops_unreachable_from_user_scripts() {
        // op_print is a deno_core built-in that writes straight to the host
        // process stdout, bypassing the captured console surface. Reaching it
        // must now fail the same way any other missing global does.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"Deno.core.ops.op_print("pwned\n", false)"#);
        let result = engine.execute(ctx).await.expect("execute");
        let err = result
            .error
            .expect("reaching Deno.core.ops must be a script error, not a silent success");
        assert!(
            err.contains("Deno is not defined"),
            "expected a ReferenceError for the deleted Deno global, got: {err}"
        );
    }

    #[tokio::test]
    async fn bootstrap_ops_table_unreachable_from_user_scripts() {
        // deno_core's own setup also parks the *same* core object (and therefore
        // the same ops table) on globalThis.__bootstrap.core, via
        // ObjectAssign(globalThis.Deno.core, {...}) returning its target. Deleting
        // only `Deno` leaves this handle standing, so op_print/op_panic would
        // still be reachable through it. bootstrap.js must delete this handle too.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(r#"globalThis.__bootstrap.core.ops.op_print("pwned\n", false)"#);
        let result = engine.execute(ctx).await.expect("execute");
        let err = result
            .error
            .expect("reaching __bootstrap.core.ops must be a script error, not a silent success");
        assert!(
            err.contains("Cannot read properties of undefined"),
            "expected a TypeError for the deleted __bootstrap global, got: {err}"
        );
    }

    #[tokio::test]
    async fn no_internal_globals_are_reachable_from_user_scripts() {
        // Encodes the lockdown as an invariant over globalThis itself, rather
        // than as the spelling of one specific exploit, so a future deno_core
        // upgrade that adds or renames an internal handle fails this test
        // instead of silently reopening the ops table.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "rok.setVar('leaks', Reflect.ownKeys(globalThis).filter(k => \
             typeof k === 'string' && (k === 'Deno' || k.startsWith('__'))))",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result
                .runtime_vars
                .get("leaks")
                .expect("leaks present")
                .to_string(),
            "[]"
        );
    }

    #[tokio::test]
    async fn fs_and_process_are_undefined_in_safe_mode() {
        // minimal_ctx sets sandbox_mode: SandboxMode::Safe, so this is the
        // regression test that would catch a future change accidentally
        // registering the dev ops unconditionally.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "rok.setVar('typeofFs', typeof fs); rok.setVar('typeofProcess', typeof process)",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result.runtime_vars.get("typeofFs").expect("typeofFs"),
            "undefined"
        );
        assert_eq!(
            result
                .runtime_vars
                .get("typeofProcess")
                .expect("typeofProcess"),
            "undefined"
        );
    }

    #[tokio::test]
    async fn fs_write_then_read_roundtrips_in_developer_mode() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir
            .path()
            .join("script-output.txt")
            .to_string_lossy()
            .to_string();
        let path_json = serde_json::to_string(&path).expect("json path");
        let code = format!(
            "fs.writeFile({path_json}, 'hello from script'); \
             rok.setVar('content', fs.readFile({path_json}))"
        );
        let engine = DenoScriptEngine::new();
        let ctx = ScriptContext {
            sandbox_mode: SandboxMode::Developer,
            ..minimal_ctx(&code)
        };
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result.runtime_vars.get("content").expect("content"),
            "hello from script"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_exec_runs_a_real_command_in_developer_mode() {
        // Per-platform command coverage for op_process_exec itself already
        // lives in ops/process.rs's own tests — this only needs to prove the
        // wiring (mode gating, JS<->op marshalling) works end to end on one
        // platform.
        let engine = DenoScriptEngine::new();
        let ctx = ScriptContext {
            sandbox_mode: SandboxMode::Developer,
            ..minimal_ctx(
                "const result = process.exec('echo', ['hello-from-script']); \
                 rok.setVar('stdout', result.stdout); \
                 rok.setVar('exitCode', result.exitCode)",
            )
        };
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result
                .runtime_vars
                .get("stdout")
                .expect("stdout")
                .as_str()
                .expect("string")
                .trim(),
            "hello-from-script"
        );
        assert_eq!(result.runtime_vars.get("exitCode").expect("exitCode"), 0);
    }

    #[tokio::test]
    async fn fs_mkdir_and_exists_work_in_developer_mode() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let nested = dir.path().join("a").join("b").to_string_lossy().to_string();
        let nested_json = serde_json::to_string(&nested).expect("json path");
        let code = format!(
            "fs.mkdir({nested_json}, {{recursive: true}}); \
             rok.setVar('exists', fs.exists({nested_json}))"
        );
        let engine = DenoScriptEngine::new();
        let ctx = ScriptContext {
            sandbox_mode: SandboxMode::Developer,
            ..minimal_ctx(&code)
        };
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("exists").expect("exists"), true);
    }

    // ── execution timeout ────────────────────────────────────────────────────

    /// Short budget so the timeout tests finish fast instead of waiting the
    /// real five-second SCRIPT_TIMEOUT.
    const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

    #[tokio::test]
    async fn infinite_loop_script_is_terminated_by_timeout() {
        let ctx = minimal_ctx("while (true) {}");

        let started = std::time::Instant::now();
        let outcome = run_script_with_timeout(ctx, TEST_TIMEOUT).await;
        let elapsed = started.elapsed();

        let err = outcome.expect_err("an infinite loop must not return Ok");
        assert!(
            err.to_string().contains("timed out"),
            "expected a timeout error, got: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "execute() must return promptly after the deadline, took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn fast_script_is_unaffected_by_the_timeout() {
        let ctx = minimal_ctx("console.log('quick'); rok.setVar('x', 'ok')");

        // Deliberately uses the same 200ms budget as the timeout test. A normal
        // script must complete well inside it with a fully populated result.
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("a fast script must not be affected by the timeout");

        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("x").expect("x present"), "ok");
        assert_eq!(result.console_entries.len(), 1);
        assert!(result.console_entries[0].message.contains("quick"));
    }

    #[tokio::test]
    async fn repeated_timeouts_do_not_crash_or_wedge_the_engine() {
        // Five back-to-back terminations. Each one leaves a blocking thread to
        // unwind, so this also checks those threads are actually released
        // rather than leaked until the pool is exhausted.
        for attempt in 0..5 {
            let ctx = minimal_ctx("while (true) {}");
            let outcome = run_script_with_timeout(ctx, TEST_TIMEOUT).await;
            assert!(outcome.is_err(), "attempt {attempt} should have timed out");
        }

        // Reaching this line at all proves the process did not abort. A working
        // script afterwards proves the engine is not wedged.
        let ctx = minimal_ctx("rok.setVar('alive', 'yes')");
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("engine must still work after repeated terminations");
        assert_eq!(
            result.runtime_vars.get("alive").expect("alive present"),
            "yes"
        );
    }

    #[tokio::test]
    async fn memory_hog_script_does_not_abort_the_process() {
        // Allocates via many distinct arrays rather than one growing string.
        // A single-string version (`s += 'x'.repeat(1000000)`) hits V8's own
        // built-in max-string-length RangeError almost instantly, independent
        // of both the wall-clock timeout and SCRIPT_HEAP_LIMIT_BYTES — so it
        // would pass identically even with the heap limit reverted. This
        // shape genuinely exercises the near-heap-limit callback: either
        // outcome below is acceptable (the heap limit firing or the
        // wall-clock timeout firing first), what must never happen is the
        // process aborting.
        let ctx = minimal_ctx("const a = []; while (true) { a.push(new Array(10000).fill(0)); }");

        let outcome = run_script_with_timeout(ctx, std::time::Duration::from_secs(5)).await;

        match outcome {
            Ok(result) => assert!(
                result.error.is_some(),
                "a heap-exhausting script must report an error, got a clean result"
            ),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("timed out") || msg.contains("terminated"),
                    "unexpected error: {msg}"
                );
            }
        }

        // Reaching this line proves the process survived. A working script
        // afterwards proves the engine is not wedged.
        let ctx = minimal_ctx("rok.setVar('alive', 'yes')");
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("engine must still work after a heap-limit termination");
        assert_eq!(
            result.runtime_vars.get("alive").expect("alive present"),
            "yes"
        );
    }

    #[test]
    fn queued_script_that_times_out_before_starting_is_still_terminated() {
        // Regression test for a real bug found by final review: the isolate
        // handle wait used to be bounded, so a script whose spawn_blocking
        // task had not even been scheduled yet when the deadline fired was
        // abandoned unterminated and pinned a blocking-pool thread forever.
        // Force that ordering with a pool of exactly one blocking thread.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .expect("build runtime");

        runtime.block_on(async {
            // Occupy the pool's only thread for longer than TEST_TIMEOUT, so
            // the real script below is still queued behind it when its own
            // deadline fires.
            let occupier =
                tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(500)));
            tokio::time::sleep(Duration::from_millis(50)).await;

            let ctx = minimal_ctx("while (true) {}");
            let outcome = run_script_with_timeout(ctx, TEST_TIMEOUT).await;
            assert!(
                outcome.is_err(),
                "a queued script must still report a timeout"
            );

            occupier.await.expect("occupier task");

            // If the queued script had been abandoned unterminated (the bug
            // this test guards against), it would now occupy the pool's only
            // thread forever, and this call would queue behind it forever
            // too. The inner timeout below gives a regression a clear
            // diagnostic on its way to that hang -- the process still blocks
            // afterward at Runtime::Drop waiting on the abandoned
            // spawn_blocking task, same as the original bug, since nothing
            // outside that task can force it to stop.
            let ctx = minimal_ctx("rok.setVar('alive', 'yes')");
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                run_script_with_timeout(ctx, TEST_TIMEOUT),
            )
            .await
            .expect(
                "the pool's only thread must be freed -- a queued script that timed out \
                 before it started must still be terminated once it runs, not left pinning \
                 the pool forever",
            )
            .expect("engine must still work after a queued-then-terminated script");
            assert_eq!(
                result.runtime_vars.get("alive").expect("alive present"),
                "yes"
            );
        });
    }
}
