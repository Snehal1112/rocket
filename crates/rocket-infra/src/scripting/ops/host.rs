//! Async ops that reach the host, such as `rok.sendRequest`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use deno_core::{op2, OpState};
use rocket_scripting::{HostError, HostRequest};
use tokio::sync::oneshot;

use crate::scripting::budget::ScriptLimits;
use crate::scripting::host_bridge::{HostCall, HostChannel};
use crate::scripting::ops::{redact, ScriptHostError};

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

/// rok.sendRequest(options) — sends one HTTP request through the host.
/// Takes a `HostRequest` and returns a `HostResponse`, both as JSON.
#[op2]
#[string]
pub async fn op_rok_send_request(
    state: Rc<RefCell<OpState>>,
    #[string] request_json: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.sendRequest";
    let request: HostRequest = serde_json::from_str(&request_json)
        .map_err(|e| ScriptHostError(format!("{API}: invalid options - {e}")))?;
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Send { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let state = state.borrow();
    match outcome {
        Ok(response) => serde_json::to_string(&response)
            .map_err(|e| ScriptHostError(format!("{API}: {e}"))),
        Err(error) => Err(host_error(&state, API, error)),
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
