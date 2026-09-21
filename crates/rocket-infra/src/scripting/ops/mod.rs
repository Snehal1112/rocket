use deno_core::OpState;
use crate::scripting::state::ScriptInputState;

pub mod console;
pub mod fs;
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
///
/// Secrets are replaced longest-first. `HashSet` iteration order is
/// unspecified, and when one secret's value is a prefix of another's,
/// replacing the shorter one first breaks the longer one's exact-substring
/// match — its own replace then becomes a no-op and a fragment of it survives
/// in plaintext. Sorting by descending length before replacing guarantees the
/// longest, most-specific match always consumes first, closing that leak.
pub(crate) fn redact(state: &OpState, msg: String) -> String {
    let secrets = &state.borrow::<ScriptInputState>().secret_values;
    if secrets.is_empty() {
        return msg;
    }
    let mut ordered: Vec<&str> = secrets.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
    ordered.sort_unstable_by_key(|s| std::cmp::Reverse(s.len()));
    let mut out = msg;
    for s in ordered {
        out = out.replace(s, "••••••");
    }
    out
}
