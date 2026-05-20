use async_trait::async_trait;
use rocket_shared::DomainResult;
use crate::{ScriptContext, ScriptResult};

/// Contract for a JS script execution engine.
///
/// `rocket-infra` provides `DenoScriptEngine` which implements this using `deno_core`.
/// `rocket-app` depends on this trait via `Box<dyn ScriptEngine>` — it never
/// constructs `DenoScriptEngine` directly.
#[async_trait]
pub trait ScriptEngine: Send + Sync {
    /// Execute `ctx.code` in a sandboxed JS runtime for the given lifecycle phase.
    ///
    /// Returns a `ScriptResult` carrying all side-effects to apply (variable mutations,
    /// request mutations, test outcomes, console entries). The engine itself applies
    /// nothing — callers apply mutations after this call returns.
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult>;
}
