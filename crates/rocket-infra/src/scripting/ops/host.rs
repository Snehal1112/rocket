//! Async ops that reach the host, such as `rok.sendRequest`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use deno_core::{op2, OpState};
use rocket_scripting::{ConsoleLevel, HostError, HostRequest, HostRunOutcome, HostRunRequest};
use tokio::sync::oneshot;

use crate::scripting::budget::ScriptLimits;
use crate::scripting::host_bridge::{HostCall, HostChannel};
use crate::scripting::ops::{redact, ScriptHostError};
use crate::scripting::state::{ScriptInputState, ScriptOutputState};

/// The error for a call that has no host, or whose host went away.
pub(crate) fn unavailable(api: &str) -> ScriptHostError {
    ScriptHostError(format!("{api} is not available here"))
}

/// Queues `call` for the host. Fails when the script runs without one.
pub(crate) fn send_host_call(
    state: &Rc<RefCell<OpState>>,
    call: HostCall,
    api: &str,
) -> Result<(), ScriptHostError> {
    let state = state.borrow();
    let sender = state
        .try_borrow::<HostChannel>()
        .and_then(|channel| channel.0.as_ref())
        .ok_or_else(|| unavailable(api))?;
    sender.send(call).map_err(|_| unavailable(api))
}

/// The error a script sees for a failed host call, with secret values masked.
pub(crate) fn host_error(state: &OpState, api: &str, error: HostError) -> ScriptHostError {
    let message = match error {
        HostError::Unavailable => format!("{api} is not available here"),
        HostError::Failed(message) => message,
    };
    ScriptHostError(redact(state, message))
}

/// rok.sendRequest(options) — sends one HTTP request through the host and adds
/// a Console line for it. Takes a `HostRequest` and returns a `HostResponse`,
/// both as JSON. Secret values are masked in the line and in the error.
#[op2]
#[string]
pub async fn op_rok_send_request(
    state: Rc<RefCell<OpState>>,
    #[string] request_json: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.sendRequest";
    let request: HostRequest = serde_json::from_str(&request_json)
        .map_err(|e| ScriptHostError(format!("{API}: invalid options - {e}")))?;
    let label = format!("{API} {} {}", request.method, request.url);
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Send { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let mut state = state.borrow_mut();
    match outcome {
        Ok(response) => {
            let line = redact(
                &state,
                format!(
                    "{label} -> {} ({} ms)",
                    response.status, response.response_time_ms
                ),
            );
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Log, line);
            serde_json::to_string(&response).map_err(|e| ScriptHostError(format!("{API}: {e}")))
        }
        Err(error) => {
            let error = host_error(&state, API, error);
            let line = redact(&state, format!("{label} failed: {}", error.0));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Error, line);
            Err(error)
        }
    }
}

/// rok.sleep(ms) — waits without blocking the event loop. The value is clamped
/// to 0 and the run's sleep cap (60 s by default). The JS wrapper rejects
/// non-numbers first.
#[op2]
pub async fn op_rok_sleep(state: Rc<RefCell<OpState>>, ms: f64) {
    let cap = state
        .borrow()
        .try_borrow::<ScriptLimits>()
        .map(|limits| limits.sleep_cap)
        .unwrap_or(ScriptLimits::DEFAULT.sleep_cap);
    let cap_ms = cap.as_millis() as f64;
    let ms = if ms.is_nan() { 0.0 } else { ms.clamp(0.0, cap_ms) };
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
}

/// Runtime variables the calling script sees right now: its snapshot plus its own writes.
fn current_runtime_vars(state: &OpState) -> HashMap<String, String> {
    let mut vars = state.borrow::<ScriptInputState>().variables.runtime.clone();
    let out = state.borrow::<ScriptOutputState>();
    for (key, value) in &out.runtime_vars {
        match value {
            serde_json::Value::Null => {}
            serde_json::Value::String(text) => {
                vars.insert(key.clone(), text.clone());
            }
            other => {
                vars.insert(key.clone(), other.to_string());
            }
        }
    }
    for key in &out.runtime_var_deletes {
        vars.remove(key);
    }
    vars
}

/// Keys whose values a nested run changed, per scope. The script drops them
/// from its read-your-writes overlay, so its next read sees the nested value.
#[derive(Default, serde::Serialize)]
struct ChangedKeys {
    runtime: Vec<String>,
    env: Vec<String>,
    global: Vec<String>,
    collection: Vec<String>,
}

/// Keys present in either map whose values differ, sorted.
fn changed_keys(before: &HashMap<String, String>, after: &HashMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Merges a nested run into the calling script's snapshot and output.
///
/// The nested run happened after the caller's earlier writes, so it wins: a
/// pending caller write to a key the nested run changed is dropped.
fn merge_run_outcome(state: &mut OpState, run: &HostRunOutcome) -> ChangedKeys {
    let mut changed = ChangedKeys::default();
    {
        let input = state.borrow_mut::<ScriptInputState>();
        for (key, value) in &run.runtime_set {
            input.variables.runtime.insert(key.clone(), value.clone());
        }
        for key in &run.runtime_removed {
            input.variables.runtime.remove(key);
        }
        if let Some(scopes) = &run.scopes {
            changed.env = changed_keys(&input.variables.env, &scopes.env);
            changed.global = changed_keys(&input.variables.global_env, &scopes.global_env);
            changed.collection = changed_keys(&input.variables.collection, &scopes.collection);
            input.variables.env = scopes.env.clone();
            input.variables.global_env = scopes.global_env.clone();
            input.variables.collection = scopes.collection.clone();
            for value in &scopes.secret_values {
                input.secret_values.insert(value.clone());
                input.variables.secret_values.insert(value.clone());
            }
        }
    }
    let out = state.borrow_mut::<ScriptOutputState>();
    for (key, value) in &run.runtime_set {
        out.runtime_var_deletes.retain(|k| k != key);
        out.runtime_vars
            .insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    for key in &run.runtime_removed {
        out.runtime_vars.remove(key);
        out.runtime_var_deletes.push(key.clone());
    }
    out.env_var_writes.retain(|w| !changed.env.contains(&w.key));
    out.global_env_var_writes
        .retain(|w| !changed.global.contains(&w.key));
    out.collection_var_writes
        .retain(|w| !changed.collection.contains(&w.key));
    changed.runtime = run
        .runtime_set
        .keys()
        .chain(run.runtime_removed.iter())
        .cloned()
        .collect();
    changed.runtime.sort();
    changed
}

/// rok.runRequest(path) — runs a saved request through the host and merges its
/// variable writes into this script. Returns `{"response": .., "changed": ..}` as JSON.
#[op2]
#[string]
pub async fn op_rok_run_request(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.runRequest";
    let runtime_vars = current_runtime_vars(&state.borrow());
    let request = HostRunRequest {
        path: path.clone(),
        runtime_vars,
    };
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Run { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let mut state = state.borrow_mut();
    match outcome {
        Err(error) => {
            let error = host_error(&state, API, error);
            let line = redact(&state, format!("{API} {path} failed: {}", error.0));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Error, line);
            Err(error)
        }
        Ok(run) => {
            let changed = merge_run_outcome(&mut state, &run);
            let status = run
                .response
                .as_ref()
                .map(|r| r.status.to_string())
                .unwrap_or_else(|| "skipped".to_string());
            let line = redact(&state, format!("{API} {path} -> {status}"));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Log, line);
            Ok(serde_json::json!({ "response": run.response, "changed": changed }).to_string())
        }
    }
}
