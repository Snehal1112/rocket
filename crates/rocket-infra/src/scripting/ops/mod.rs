use deno_core::OpState;
use crate::scripting::state::ScriptInputState;

pub mod console;
pub mod req;
pub mod res;
pub mod rok;

/// JS-visible error for op failures (phase guard, unavailable state, etc.).
#[derive(Debug, thiserror::Error, deno_error::JsError)]
#[class(type)]
#[error("{0}")]
pub struct ScriptOpError(pub String);

/// Redacts every known secret value out of `msg`, replacing each occurrence
/// with `"••••••"`. Matching is content-based (against the actual variable
/// *value*, not its key or scope), so a script that copies a secret into a
/// differently-named variable is still caught. Used by the console ops
/// (`ops/console.rs`) and by `op_test_fail` (`engine.rs`) — the two places
/// script-emitted text reaches the UI/event bus. Never used by `req`/`res`
/// ops or `rok.interpolate` — those must carry the real value.
pub fn redact(state: &OpState, msg: String) -> String {
    let secrets = &state.borrow::<ScriptInputState>().secret_values;
    if secrets.is_empty() {
        return msg;
    }
    let mut out = msg;
    for s in secrets {
        out = out.replace(s.as_str(), "••••••");
    }
    out
}
