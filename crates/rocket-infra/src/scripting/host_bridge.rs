//! Carries host calls from the script thread to the task that owns the `ScriptHost`.
//!
//! The script thread has its own Tokio runtime and `JsRuntime` is not `Send`, so
//! ops never touch the host. They send a `HostCall` with a oneshot reply, and
//! `run_script_bounded` in `engine.rs` serves it on the caller's task.

use rocket_scripting::{
    HostError, HostRequest, HostResponse, HostRunOutcome, HostRunRequest, ScriptHost,
};
use tokio::sync::{mpsc, oneshot};

/// One call from a script to the host.
pub enum HostCall {
    /// `rok.sendRequest`.
    Send {
        request: HostRequest,
        reply: oneshot::Sender<Result<HostResponse, HostError>>,
    },
    /// `rok.runRequest`.
    Run {
        request: HostRunRequest,
        reply: oneshot::Sender<Result<HostRunOutcome, HostError>>,
    },
}

/// Where ops send host calls. `None` when the script runs without a host.
pub struct HostChannel(pub Option<mpsc::UnboundedSender<HostCall>>);

/// Serves one call and sends the answer back. A failed send means the script is gone.
pub async fn serve_host_call(host: &dyn ScriptHost, call: HostCall) {
    match call {
        HostCall::Send { request, reply } => {
            let _ = reply.send(host.send_request(request).await);
        }
        HostCall::Run { request, reply } => {
            let _ = reply.send(host.run_request(request).await);
        }
    }
}
