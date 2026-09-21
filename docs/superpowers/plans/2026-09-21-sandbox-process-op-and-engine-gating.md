# Sandbox Process Op and Engine Gating Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `process.exec` (with its own independent timeout), then make `fs.*`/`process.exec` actually reachable from a script — but only when the collection's `sandbox_mode` is `Developer` — by conditionally registering a second `deno_core` extension. This is the plan that makes Developer Mode functionally real end-to-end.

**Architecture:** `process.exec` follows the same `*_impl` / thin-`#[op2]`-wrapper split as the fs ops (previous plan), but needs a concurrent pipe-draining design: polling `Child::try_wait()` alone would deadlock once a child's stdout/stderr fills its OS pipe buffer (commonly 64KB) before exiting, since nothing would be draining it. Two background threads drain stdout/stderr into channels while the main thread polls for exit-or-timeout. `run_script()` (`engine.rs`) then picks `rocket_scripting_ext` alone for Safe Mode, or that plus a new `rocket_scripting_dev_ext` for Developer Mode — so in Safe Mode the dev ops don't exist in the isolate at all, matching the existing narrowly-enumerated op table philosophy instead of a weaker per-call runtime check. `bootstrap.js` feature-detects the ops' presence to decide whether to define `globalThis.fs`/`globalThis.process`.

**Tech Stack:** Rust (rocket-infra crate), `deno_core`, `std::process`, `std::thread`/`mpsc`, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md`

## Global Constraints

- No command allowlist/blocklist — fully unrestricted, matching Bruno exactly (spec §3, explicit user decision).
- `process.exec` invokes the binary directly (`Command::new(command).args(args)`) — no shell string interpretation (spec §3).
- `env` merges onto, not replaces, the app's own environment — `Command::env(k, v)` already does this by default (inherits the parent environment unless `.env_clear()` is called, which this plan never calls).
- `process.exec` needs its own timeout independent of `SCRIPT_TIMEOUT` — `v8::IsolateHandle::terminate_execution()` cannot interrupt a blocked native Rust thread (spec §3).
- The timeout kill targets the direct child process only, not any grandchildren it spawned — an accepted, documented limitation (spec §3).
- Structural gating, not per-call runtime checks — in Safe Mode, `fs`/`process` must not exist in the JS isolate at all (spec §2).
- Rust: avoid `.unwrap` panics in production code paths; test code uses `.expect("message")` for fallible setup calls.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-21-sandbox-developer-mode-frontend-ui.md` next (plan 4 of 4, final plan, in the sandbox-developer-mode sequence).

---

### Task 1: `process.exec` op

**Files:**
- Create: `crates/rocket-infra/src/scripting/ops/process.rs`
- Modify: `crates/rocket-infra/src/scripting/ops/mod.rs` (register the new module)
- Test: `crates/rocket-infra/src/scripting/ops/process.rs` (same file, `#[cfg(test)] mod tests` at end)

**Interfaces:**
- Produces: `exec_impl(command: &str, args: &[String], cwd: Option<&str>, env: &HashMap<String, String>, timeout_ms: u64) -> Result<(String, String, i32), ScriptOpError>` (stdout, stderr, exit code) and its thin wrapper `op_process_exec` — consumed by Task 2 (this plan).

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-infra/src/scripting/ops/process.rs`:

```rust
use deno_core::op2;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use crate::scripting::ops::ScriptOpError;

/// Runs `command` with `args`, draining stdout/stderr on background threads
/// while polling for exit so a child that fills its OS pipe buffer before
/// exiting can never deadlock this call. Kills the child (not any
/// grandchildren it spawned) if `timeout_ms` elapses first.
fn exec_impl(
    command: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &HashMap<String, String>,
    timeout_ms: u64,
) -> Result<(String, String, i32), ScriptOpError> {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| ScriptOpError(format!("{command}: {e}")))?;

    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| ScriptOpError(format!("{command}: missing stdout pipe")))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| ScriptOpError(format!("{command}: missing stderr pipe")))?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        let _ = stdout_tx.send(buf);
    });
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        let _ = stderr_tx.send(buf);
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ScriptOpError(format!("{command}: timed out after {timeout_ms}ms")));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(ScriptOpError(format!("{command}: {e}"))),
        }
    };

    let stdout = stdout_rx.recv().unwrap_or_default();
    let stderr = stderr_rx.recv().unwrap_or_default();
    let exit_code = status.code().unwrap_or(-1);
    Ok((stdout, stderr, exit_code))
}

#[op2]
#[string]
pub fn op_process_exec(
    #[string] command: String,
    #[string] args_json: String,
    #[string] cwd: String,
    #[string] env_json: String,
    timeout_ms: u32,
) -> Result<String, ScriptOpError> {
    let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
    let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();
    let cwd_opt = if cwd.is_empty() { None } else { Some(cwd.as_str()) };
    let (stdout, stderr, exit_code) = exec_impl(&command, &args, cwd_opt, &env, timeout_ms as u64)?;
    Ok(serde_json::to_string(&serde_json::json!({
        "stdout": stdout,
        "stderr": stderr,
        "exitCode": exit_code,
    }))
    .unwrap_or_else(|_| "{}".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn exec_runs_a_command_and_captures_stdout() {
        let (stdout, _stderr, exit_code) =
            exec_impl("echo", &["hello".to_string()], None, &HashMap::new(), 5000).expect("exec");
        assert_eq!(stdout.trim(), "hello");
        assert_eq!(exit_code, 0);
    }

    #[cfg(windows)]
    #[test]
    fn exec_runs_a_command_and_captures_stdout() {
        let (stdout, _stderr, exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "echo".to_string(), "hello".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "hello");
        assert_eq!(exit_code, 0);
    }

    #[cfg(unix)]
    #[test]
    fn exec_captures_non_zero_exit_code() {
        let (_stdout, _stderr, exit_code) = exec_impl(
            "sh",
            &["-c".to_string(), "exit 7".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(exit_code, 7);
    }

    #[cfg(windows)]
    #[test]
    fn exec_captures_non_zero_exit_code() {
        let (_stdout, _stderr, exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "exit 7".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(exit_code, 7);
    }

    #[cfg(unix)]
    #[test]
    fn exec_times_out_and_returns_an_error() {
        let result = exec_impl("sleep", &["5".to_string()], None, &HashMap::new(), 200);
        assert!(result.is_err(), "expected a timeout error");
    }

    #[cfg(windows)]
    #[test]
    fn exec_times_out_and_returns_an_error() {
        let result = exec_impl(
            "cmd",
            &["/C".to_string(), "timeout".to_string(), "/T".to_string(), "5".to_string()],
            None,
            &HashMap::new(),
            200,
        );
        assert!(result.is_err(), "expected a timeout error");
    }

    #[cfg(unix)]
    #[test]
    fn exec_passes_custom_env_vars() {
        let mut env = HashMap::new();
        env.insert("ROCKET_TEST_VAR".to_string(), "sandbox-value".to_string());
        let (stdout, _stderr, _exit_code) = exec_impl(
            "sh",
            &["-c".to_string(), "echo $ROCKET_TEST_VAR".to_string()],
            None,
            &env,
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "sandbox-value");
    }

    #[cfg(windows)]
    #[test]
    fn exec_passes_custom_env_vars() {
        let mut env = HashMap::new();
        env.insert("ROCKET_TEST_VAR".to_string(), "sandbox-value".to_string());
        let (stdout, _stderr, _exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "echo %ROCKET_TEST_VAR%".to_string()],
            None,
            &env,
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "sandbox-value");
    }

    #[cfg(unix)]
    #[test]
    fn exec_respects_cwd_option() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "x").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();
        let (stdout, _stderr, _exit_code) =
            exec_impl("ls", &[], Some(&cwd), &HashMap::new(), 5000).expect("exec");
        assert!(stdout.contains("marker.txt"));
    }

    #[cfg(windows)]
    #[test]
    fn exec_respects_cwd_option() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "x").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();
        let (stdout, _stderr, _exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "dir".to_string(), "/B".to_string()],
            Some(&cwd),
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert!(stdout.contains("marker.txt"));
    }
}
```

In `crates/rocket-infra/src/scripting/ops/mod.rs`, find:

```rust
pub mod console;
pub mod fs;
pub mod req;
pub mod res;
pub mod rok;
```

Replace with:

```rust
pub mod console;
pub mod fs;
pub mod process;
pub mod req;
pub mod res;
pub mod rok;
```

Run: `cargo test -p rocket-infra --lib scripting::ops::process` on your development platform (only the `#[cfg(unix)]` or `#[cfg(windows)]` variants matching it will compile and run).
Expected: this file is written complete (implementation and tests together, like the fs ops plan) — verify RED the same way: temporarily replace `exec_impl`'s body with `unimplemented!()`, confirm every test panics, then restore the real body above and confirm all pass.

- [ ] **Step 2: Verify the tests pass**

Run: `cargo test -p rocket-infra --lib scripting::ops::process`
Expected: PASS (6 tests on your platform — the timeout test takes slightly over 200ms by design, don't be surprised it's not instant). Then `cargo check -p rocket-infra` to confirm `op_process_exec` compiles (unused-function warning expected and fine — not wired in yet).

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/ops/process.rs crates/rocket-infra/src/scripting/ops/mod.rs
```

Commit message along the lines of: `feat(scripting): add process.exec op with its own timeout (not yet wired)`.

---

### Task 2: Conditional extension registration

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs:1-10` (imports)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (new `rocket_scripting_dev_ext` extension, conditional registration in `run_script`)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (conditional `fs`/`process` globals)

**Interfaces:**
- Consumes: `fs::op_fs_*`/`process::op_process_exec` (Task 1, this plan, and the previous plan), `rocket_scripting::SandboxMode` (from the data-model plan earlier in this sequence).
- Produces: `rocket_scripting_dev_ext` (new `deno_core::extension!`) — consumed only by `run_script` in this same file. Nothing outside this file depends on it.

**Note on verification:** this task has no dedicated new test of its own — its correctness can only be meaningfully observed by running a real script through the full JS pipeline, which is exactly what Task 3 (next) adds. This task's own gate is that every *existing* test in this crate — all of which run in Safe Mode via `minimal_ctx`, which now explicitly sets `sandbox_mode: SandboxMode::Safe` — keeps passing completely unchanged.

- [ ] **Step 1: Wire the conditional extension**

In `crates/rocket-infra/src/scripting/engine.rs`, find the imports:

```rust
use async_trait::async_trait;
use deno_core::{extension, v8, JsRuntime, OpState, RuntimeOptions, op2};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::{console, redact, req, res, rok};
```

Replace with:

```rust
use async_trait::async_trait;
use deno_core::{extension, v8, JsRuntime, OpState, RuntimeOptions, op2};
use rocket_scripting::{SandboxMode, ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::{console, fs, process, redact, req, res, rok};
```

Find the `extension!` block that ends the fixed op table:

```rust
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
```

Replace with (adds the new `rocket_scripting_dev_ext` block immediately after the existing one, leaving every existing line of `rocket_scripting_ext` untouched):

```rust
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

/// Only registered when `ScriptContext.sandbox_mode == SandboxMode::Developer`
/// (see `run_script` below). In Safe Mode these ops do not exist in the
/// isolate at all — `typeof fs` / `typeof process` are `'undefined'`, not
/// "defined but throws" — matching the same narrowly-enumerated op table
/// philosophy as `rocket_scripting_ext` above, just gated per-run.
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
```

Find `run_script`'s runtime construction:

```rust
fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
) -> DomainResult<ScriptResult> {
    let code = ctx.code;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        create_params: Some(
            v8::CreateParams::default().heap_limits(0, SCRIPT_HEAP_LIMIT_BYTES),
        ),
        ..Default::default()
    });
```

Replace with:

```rust
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
        create_params: Some(
            v8::CreateParams::default().heap_limits(0, SCRIPT_HEAP_LIMIT_BYTES),
        ),
        ..Default::default()
    });
```

In `crates/rocket-infra/src/scripting/bootstrap.js`, find:

```js
  // rok.test / rok.expect aliases so both calling styles work.
  globalThis.rok.test   = globalThis.test;
  globalThis.rok.expect = globalThis.expect;

  // Every global is wired up now. Remove the raw Deno global so the user script,
```

Replace with:

```js
  // rok.test / rok.expect aliases so both calling styles work.
  globalThis.rok.test   = globalThis.test;
  globalThis.rok.expect = globalThis.expect;

  // ── fs / process (Developer Mode only) ────────────────────────────────────
  // These ops only exist in the isolate when the collection's sandbox mode is
  // Developer (see rocket_scripting_dev_ext in engine.rs) — feature-detected
  // here rather than assumed, so Safe Mode leaves both globals entirely
  // undefined instead of defined-but-throwing.
  if (typeof __ops.op_fs_read_file === 'function') {
    globalThis.fs = {
      readFile:  (path, opts)          => __ops.op_fs_read_file(path, (opts && opts.encoding) || 'utf8'),
      writeFile: (path, content, opts) => __ops.op_fs_write_file(path, content, (opts && opts.encoding) || 'utf8'),
      readDir:   (path)                => JSON.parse(__ops.op_fs_read_dir(path)),
      exists:    (path)                => __ops.op_fs_exists(path),
      mkdir:     (path, opts)          => __ops.op_fs_mkdir(path, !!(opts && opts.recursive)),
      remove:    (path, opts)          => __ops.op_fs_remove(path, !!(opts && opts.recursive)),
    };
  }
  if (typeof __ops.op_process_exec === 'function') {
    globalThis.process = {
      exec: (command, args, opts) => JSON.parse(__ops.op_process_exec(
        command,
        JSON.stringify(args || []),
        (opts && opts.cwd) || '',
        JSON.stringify((opts && opts.env) || {}),
        (opts && opts.timeoutMs) || 5000,
      )),
    };
  }

  // Every global is wired up now. Remove the raw Deno global so the user script,
```

- [ ] **Step 2: Run the full existing scripting test suite**

Run: `cargo test -p rocket-infra --lib scripting`
Expected: PASS, in full, with zero regressions — every pre-existing `engine.rs` test uses `minimal_ctx`, which sets `sandbox_mode: SandboxMode::Safe` (from the data-model plan earlier in this sequence), so none of them should newly gain access to `fs`/`process` or change behavior in any way. Then run `cargo check -p rocket` to confirm the whole workspace still compiles.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/engine.rs crates/rocket-infra/src/scripting/bootstrap.js
```

Commit message along the lines of: `feat(scripting): gate fs/process ops behind Developer Mode`.

---

### Task 3: Prove it — regression test and integration test

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: everything from Tasks 1-2 (this plan) and the data-model plan earlier in this sequence. Nothing new produced — this task is pure verification, the payoff of the whole plan sequence so far.

- [ ] **Step 1: Write the tests**

In `crates/rocket-infra/src/scripting/engine.rs`, add these tests to the `#[cfg(test)] mod tests` block, near the existing "sandbox lockdown" tests (after `no_internal_globals_are_reachable_from_user_scripts`):

```rust
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
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("typeofFs").expect("typeofFs"), "undefined");
        assert_eq!(
            result.runtime_vars.get("typeofProcess").expect("typeofProcess"),
            "undefined"
        );
    }

    #[tokio::test]
    async fn fs_write_then_read_roundtrips_in_developer_mode() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("script-output.txt").to_string_lossy().to_string();
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
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
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
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
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
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("exists").expect("exists"), true);
    }
```

Run: `cargo test -p rocket-infra --lib scripting::engine`
Expected: FAIL — `fs_and_process_are_undefined_in_safe_mode` should already PASS (nothing about Safe Mode changed), but if Task 2 was skipped or done incorrectly, `fs_write_then_read_roundtrips_in_developer_mode`, `process_exec_runs_a_real_command_in_developer_mode`, and `fs_mkdir_and_exists_work_in_developer_mode` would fail with a script error like `"fs is not defined"`. Since Task 2 already landed correctly in this plan, this step should actually show PASS immediately — treat a failure here as a signal to re-check Task 2's diff, not as an expected outcome to work through.

- [ ] **Step 2: Verify all tests pass**

Run: `cargo test -p rocket-infra --lib scripting` (full scripting module, not just these 4 new tests)
Expected: PASS. Then run `cargo test -p rocket-infra` (full crate), `cargo check -p rocket`, and `cargo test --workspace -j 4` to confirm the whole workspace is green — this is the plan that finally makes Developer Mode functionally real, so a full workspace pass here matters more than at any earlier plan in this sequence.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-infra/src/scripting/engine.rs
```

Commit message along the lines of: `test(scripting): prove Developer Mode gating end to end`.

---

## Final verification (after all 3 tasks)

- [ ] Run `cargo test -p rocket-infra` — expect PASS.
- [ ] Run `cargo test --workspace -j 4` — expect PASS, exit 0, zero failures across the whole workspace.
- [ ] Run `cargo check -p rocket` — expect PASS.
- [ ] Manually confirm (`grep -n "op_fs_\|op_process_exec" crates/rocket-infra/src/scripting/engine.rs`) that both extensions are registered and the conditional exists exactly once, in `run_script`.
