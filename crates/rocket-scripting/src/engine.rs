use crate::{ScriptContext, ScriptHost, ScriptResult};
use async_trait::async_trait;
use rocket_shared::error::DomainResult;

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
    /// nothing — callers apply mutations after this call returns. Host calls such as
    /// `rok.sendRequest` reject because no host is attached.
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult>;

    /// Like `execute`, with a host that serves `rok.sendRequest` and similar calls.
    ///
    /// The default ignores the host, so test engines keep working unchanged.
    async fn execute_with_host(
        &self,
        ctx: ScriptContext,
        host: &dyn ScriptHost,
    ) -> DomainResult<ScriptResult> {
        let _ = host;
        self.execute(ctx).await
    }
}
