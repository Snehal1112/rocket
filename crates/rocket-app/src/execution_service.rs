use crate::env_audit;
use crate::redaction::MIN_REDACTION_LEN;
use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{
    inherited_headers, resolve_folder_auth, settings::SandboxMode as CollectionSandboxMode,
    CollectionRepository, CollectionSettings, FolderSettings, ScriptFlow,
};
use rocket_environment::{
    resolve, Environment, EnvironmentRepository, EnvironmentRepositoryFactory,
    SecretManagerRepository, SecretStore, VariableContext, VaultSecretFetcher,
};
use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_http::{
    run_load_test as http_run_load_test, CookieRepository, HttpExecutor, HttpRequest, HttpResponse,
    LoadTestConfig, LoadTestResult, RequestOptions, ResolvedClientCertificate,
};
use rocket_scripting::{
    context::SandboxMode, ConsoleEntry, ConsoleLevel, ExecutionMode, NextRequest, ScriptContext,
    ScriptEngine, ScriptFileScope, ScriptHost, ScriptResult, TestResult, TestStatus,
};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::{Auth, Body, BodyMode, Header, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod websocket_resolution;

#[cfg(test)]
mod folder_chain_e2e_tests;

#[cfg(test)]
mod folder_var_script_tests;
pub(crate) mod script_chain;
pub(crate) mod run_request;
pub(crate) mod script_host;
#[cfg(test)]
mod script_host_tests;
use self::script_chain::{
    folder_labels, folder_mentions, script_mentions, ChainedScript, PhaseScripts,
};

/// Request path prefix of an inline Flow request. It names no file, so it has no folder chain.
pub(crate) const FLOW_INLINE_PATH_PREFIX: &str = "__flow_inline__/";

/// An external secret binding whose values could not be fetched.
struct UnresolvedBinding {
    alias: String,
    error: rocket_shared::error::DomainError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequestInput {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
    pub environment_name: Option<String>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
    /// Path of the request file relative to the collection root (e.g. "auth/login.yml").
    /// Used to load folder-chain and request-level variables.
    #[serde(default)]
    pub request_path: Option<String>,
    /// Tags on the request, exposed to scripts via `req.getTags()`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Path parameters on the request, exposed to scripts via `req.getPathParams()`.
    #[serde(default)]
    pub path_params: Vec<rocket_shared::types::PathParam>,

    /// JS script to run before the request is sent.
    #[serde(default)]
    pub pre_request_script: Option<String>,
    /// JS script to run after the response is received.
    #[serde(default)]
    pub post_response_script: Option<String>,
    /// JS test script to run after after-response.
    #[serde(default)]
    pub tests_script: Option<String>,

    /// Name of the active global environment. Used to apply `rok.setGlobalEnvVar` writes.
    #[serde(default)]
    pub global_env_name: Option<String>,

    /// Declarative assertions to evaluate after the tests-script phase.
    #[serde(default)]
    pub assertions: Vec<rocket_shared::Assertion>,

    /// Declarative `set-variable` actions, evaluated (via jsonq) at the
    /// `before-request`/`after-response` phase they declare.
    #[serde(default)]
    pub actions: Vec<rocket_shared::ActionSetVariable>,
    /// Opt-in per-workspace policy: when a BeforeRequest script redirects the
    /// request via req.setUrl(), validate the new host against a blocklist of
    /// internal/loopback ranges before dispatch. Defaults to fully permissive.
    #[serde(default)]
    pub request_guard_policy: rocket_workspace::RequestGuardPolicy,
    /// When true, the History entry is not saved. It is returned in
    /// `ExecuteRequestOutput::deferred_history` so the caller can save it
    /// later, as a Flow poll does for its final attempt only.
    #[serde(default)]
    pub skip_history: bool,
    /// Run-scoped variables a Flow run adds, such as `callback.<name>`.
    /// They resolve like runtime variables. Empty for every other caller.
    #[serde(default)]
    pub flow_vars: std::collections::HashMap<String, String>,
    /// When true, no folder scripts run for this send. GraphQL introspection
    /// sets it, because it runs none of the request's own scripts either.
    #[serde(default)]
    pub skip_folder_scripts: bool,
}

/// Borrows an `EnvironmentRepository` instead of owning it, so
/// `regular_env_repo()` can hand back the shared `env_repo` field (a fallback
/// for tests/mocks) with the same `Box<dyn EnvironmentRepository>` shape as a
/// freshly built collection-scoped repo.
struct RefEnvRepo<'a>(&'a dyn EnvironmentRepository);

impl<'a> EnvironmentRepository for RefEnvRepo<'a> {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        self.0.list()
    }

    fn get(&self, name: &str) -> DomainResult<Environment> {
        self.0.get(name)
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        self.0.save(env)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        self.0.delete(name)
    }
}

/// Extended response from `execute()` that includes HTTP response plus script outputs.
#[derive(Debug, Clone)]
pub struct ExecuteRequestOutput {
    pub response: HttpResponse,
    pub test_results: Vec<TestResult>,
    pub console_entries: Vec<ConsoleEntry>,
    pub script_error: Option<String>,
    /// The History entry of a request run with `skip_history`, not yet saved.
    pub deferred_history: Option<HistoryEntry>,
}

/// What `apply_script_side_effects` needs to keep vault secrets off disk.
pub(crate) struct VaultGuard<'a> {
    /// Masked forms of the RocketVault values.
    pub forms: &'a [String],
    /// Where the warning for a held-back write goes.
    pub console: &'a mut Vec<ConsoleEntry>,
}

/// Holds back a value that holds a RocketVault secret, and returns whether it did.
///
/// Such a value must never reach disk. It is kept as a runtime variable instead, and a warning
/// that names the scope and the key, never the value, goes to the log and the script console. A
/// key that itself matches a vault value is shown as a placeholder. The check is best-effort: it
/// misses encodings such as base64 or URL-encoding, slices of a secret, and secrets shorter than
/// `MIN_REDACTION_LEN`.
fn hold_back_text_if_vault_secret(
    scope: &str,
    key: &str,
    text: &str,
    vault_forms: &[String],
    var_ctx: &mut VariableContext,
    console: &mut Vec<ConsoleEntry>,
) -> bool {
    if !crate::redaction::contains_secret(text, vault_forms) {
        return false;
    }
    var_ctx.runtime.insert(key.to_string(), text.to_string());
    let shown = if crate::redaction::contains_secret(key, vault_forms) {
        "<redacted key>"
    } else {
        key
    };
    tracing::warn!(key = %shown, scope = %scope, "write holds a vault secret, kept in memory only");
    console.push(ConsoleEntry {
        level: ConsoleLevel::Warn,
        message: format!(
            "Variable \"{shown}\" ({scope}) was not saved because it contains a vault secret. \
             It is kept in memory for this run only."
        ),
    });
    true
}

/// Like `hold_back_text_if_vault_secret`, for a JSON write value.
///
/// It checks the same text that would be persisted: the string itself, or the JSON text of any
/// other non-null value, so an object that wraps a secret is caught too. `null` deletes a
/// variable and is never held back.
fn hold_back_if_vault_secret(
    scope: &str,
    key: &str,
    value: &serde_json::Value,
    vault_forms: &[String],
    var_ctx: &mut VariableContext,
    console: &mut Vec<ConsoleEntry>,
) -> bool {
    if value.is_null() {
        return false;
    }
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    hold_back_text_if_vault_secret(scope, key, &text, vault_forms, var_ctx, console)
}

/// Returns the writes that may be persisted, holding back those with a vault secret.
fn hold_back_vault_writes(
    writes: &[rocket_scripting::EnvVarWrite],
    scope: &str,
    vault_forms: &[String],
    var_ctx: &mut VariableContext,
    console: &mut Vec<ConsoleEntry>,
) -> Vec<rocket_scripting::EnvVarWrite> {
    writes
        .iter()
        .filter(|w| {
            !hold_back_if_vault_secret(scope, &w.key, &w.value, vault_forms, var_ctx, console)
        })
        .cloned()
        .collect()
}

/// Mutable state threaded through the phases of one request execution.
///
/// `RequestExecutionService::execute()` and `CollectionRunnerService` both drive
/// the same phase methods against this struct, so phase orchestration is never
/// duplicated between the single-send path and the runner.
pub(crate) struct PhaseState {
    /// The resolved request. A before-request script can still mutate it.
    pub http_request: HttpRequest,
    /// Scope-separated variables. `runtime` accumulates across phases.
    pub var_ctx: VariableContext,
    /// First script error seen, in phase order.
    pub script_error: Option<String>,
    /// Console output collected from every phase that ran.
    pub console: Vec<ConsoleEntry>,
    /// Test results from the tests phase plus declarative assertions.
    pub test_results: Vec<TestResult>,
    /// Last `next_request` set by any phase that ran — later phase wins, the
    /// same "later overrides earlier" rule `runtime_vars` merging already uses.
    /// Only the Collection Runner reads this.
    pub next_request: Option<NextRequest>,
    /// Set by a before-request script calling `rok.runner.skipRequest()`.
    /// Only the Collection Runner reads this; `execute()` always sends.
    pub skip_request: bool,
    /// Masked forms of the RocketVault values. A script write holding one is never persisted.
    pub vault_forms: Vec<String>,
    /// Resolved once in `begin_phases` from the collection's `sandbox_mode`
    /// setting, applied to every phase's `ScriptContext`.
    pub sandbox_mode: SandboxMode,
    /// Local-file `require()` scope, resolved once in `begin_phases`.
    pub file_scope: Option<ScriptFileScope>,
    /// Scripts of every phase in run order: the folder chain and the request's
    /// own script, built once in `begin_phases`.
    pub scripts: PhaseScripts,
    /// Body set by an after-response `res.setBody`, shown to the tests script only.
    pub response_body_override: Option<String>,
}

impl PhaseState {
    /// Seeds the runtime scope with variables carried over from earlier steps
    /// of the same collection run (spec §8.2). No-op for a single send.
    pub(crate) fn seed_runtime(&mut self, carried: &std::collections::HashMap<String, String>) {
        for (k, v) in carried {
            self.var_ctx.runtime.insert(k.clone(), v.clone());
        }
    }
}

pub struct RequestExecutionService {
    /// Correct only for the workspace-level GLOBAL environment (`global_env_name`).
    /// REGULAR (per-collection) environment lookups must go through
    /// `regular_env_repo()`, which prefers `collection_env_repo_factory` below.
    env_repo: Box<dyn EnvironmentRepository>,
    /// Resolves the REGULAR environment repo for a given collection. `None`
    /// falls back to `env_repo` (used by tests/mocks that don't care about
    /// per-collection scoping).
    collection_env_repo_factory: Option<Box<dyn EnvironmentRepositoryFactory>>,
    executor: Arc<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    // Reserved for automatic cookie persistence in future requests.
    #[allow(dead_code)]
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
    script_engine: Option<Box<dyn ScriptEngine>>,
    /// App-level RocketVault connection registry — `resolve_external_secrets`
    /// looks up the `SecretManagerConnection` named by each binding's
    /// `connection_id`. No I/O in this crate; concrete impl lives in
    /// `rocket-infra` (Plan 04's `FsSecretManagerRepo`).
    secret_manager_repo: Box<dyn SecretManagerRepository>,
    /// Client-secret storage for vault connections — a distinct
    /// `SecretStore` instance from the one `FsEnvironmentRepo` uses for
    /// local `secret: true` variables (that one is scoped to
    /// `com.rocketapi.env-secrets`; this one to
    /// `com.rocketapi.vault-connection`, via Plan 04's
    /// `KeyringSecretStore::new_vault_connections()`).
    vault_connection_secret_store: Arc<dyn SecretStore>,
    /// Fetches live secret values from a configured RocketVault connection.
    /// One shared instance serves every connection, exactly like
    /// `executor: Arc<dyn HttpExecutor>` serves every request regardless of
    /// target host.
    vault_fetcher: Arc<dyn VaultSecretFetcher>,
}

impl RequestExecutionService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Arc<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
        secret_manager_repo: Box<dyn SecretManagerRepository>,
        vault_connection_secret_store: Arc<dyn SecretStore>,
        vault_fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        Self {
            env_repo,
            collection_env_repo_factory: None,
            executor,
            history_repo,
            collection_repo,
            cookie_repo,
            events,
            audit: Arc::new(NullSecurityAuditPublisher),
            script_engine: None,
            secret_manager_repo,
            vault_connection_secret_store,
            vault_fetcher,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_audit(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Arc<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
        secret_manager_repo: Box<dyn SecretManagerRepository>,
        vault_connection_secret_store: Arc<dyn SecretStore>,
        vault_fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        Self {
            env_repo,
            collection_env_repo_factory: None,
            executor,
            history_repo,
            collection_repo,
            cookie_repo,
            events,
            audit,
            script_engine: None,
            secret_manager_repo,
            vault_connection_secret_store,
            vault_fetcher,
        }
    }

    /// Attach a factory that resolves the REGULAR (per-collection) environment
    /// repo per call, instead of the single workspace-level `env_repo`. Call
    /// this after construction in the DI layer.
    pub fn with_collection_env_repo_factory(
        mut self,
        factory: Box<dyn EnvironmentRepositoryFactory>,
    ) -> Self {
        self.collection_env_repo_factory = Some(factory);
        self
    }

    /// Resolves the `EnvironmentRepository` to use for a REGULAR
    /// (per-collection) environment lookup — anywhere `environment_name` (not
    /// `global_env_name`) is involved. Falls back to `env_repo` when no
    /// factory or no collection is available, so existing tests/mocks that
    /// construct the service directly keep working unchanged.
    fn regular_env_repo<'a>(
        &'a self,
        collection: Option<&str>,
    ) -> Box<dyn EnvironmentRepository + 'a> {
        match (&self.collection_env_repo_factory, collection) {
            (Some(factory), Some(col)) => factory.for_collection(col),
            _ => Box::new(RefEnvRepo(self.env_repo.as_ref())),
        }
    }

    /// Attach a script engine. Call this after construction in the DI layer.
    pub fn with_script_engine(mut self, engine: Box<dyn ScriptEngine>) -> Self {
        self.script_engine = Some(engine);
        self
    }

    /// Fetches real values for every `ExternalSecretRef` in the named
    /// environment's `external_secrets` bindings. Returns a flat map keyed
    /// `"{alias}.{secretName}" -> value`.
    ///
    /// Reads the named environment through `regular_env_repo(collection)`, not
    /// `self.env_repo` directly — the environment a real request uses is
    /// collection-scoped in the normal case, served by a different repo than
    /// the app-level ("global") one `self.env_repo` points at. A missing
    /// environment soft-fails to an empty map, matching
    /// `build_variable_scopes`'s own convention for this lookup.
    ///
    /// `None` (no active environment) short-circuits to an empty map with zero
    /// network activity — nothing to resolve. A binding whose ref resolves to
    /// `Ok(None)` (deleted on the RocketVault side since the last "Fetch
    /// Secrets") is silently omitted from the result, not an error. A
    /// `DomainError::NotFound` — the binding's `connection_id` no longer
    /// refers to an existing Secret Manager connection, e.g. it was deleted
    /// from Settings after the binding was saved — likewise soft-fails: that
    /// one binding's secrets are skipped, not the whole call, since deleting
    /// a connection is a supported action with no cross-check against
    /// existing environment bindings. Any other `Err` aborts the whole call
    /// immediately: a *configured, previously successfully fetched* external
    /// secret name failing to resolve means the network/vault is unreachable
    /// right now, not that the secret was never set — partially populating
    /// the map and continuing would risk a request going out with some
    /// vault-sourced values silently missing (spec §4.6).
    pub async fn resolve_external_secrets(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
    ) -> DomainResult<std::collections::HashMap<String, String>> {
        let (result, failures) = self
            .resolve_external_secrets_partial(collection, environment_name)
            .await;
        match failures.into_iter().next() {
            Some(failure) => Err(failure.error),
            None => Ok(result),
        }
    }

    /// Like `resolve_external_secrets`, but a binding that fails to resolve
    /// is reported in the second value instead of aborting the call. The
    /// first failing secret of a binding ends that binding, because its
    /// connection is the likely cause of every later failure. Bindings after
    /// it are still resolved. The caller decides whether the failure matters
    /// for the request in hand.
    async fn resolve_external_secrets_partial(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
    ) -> (
        std::collections::HashMap<String, String>,
        Vec<UnresolvedBinding>,
    ) {
        let mut result = std::collections::HashMap::new();
        let mut failures = Vec::new();

        let Some(name) = environment_name else {
            return (result, failures);
        };

        let Ok(env) = self.regular_env_repo(collection).get(name) else {
            return (result, failures);
        };
        'bindings: for binding in &env.external_secrets {
            for secret_ref in &binding.secret_names {
                let value = match crate::vault_secret_resolution::resolve_vault_secret_value(
                    self.secret_manager_repo.as_ref(),
                    self.vault_connection_secret_store.as_ref(),
                    self.vault_fetcher.as_ref(),
                    &binding.connection_id,
                    &binding.vault_name,
                    &secret_ref.secret_id,
                )
                .await
                {
                    Ok(value) => value,
                    // The connection this binding pointed at was deleted from
                    // Settings after the binding was saved — soft-fail this
                    // one binding's secrets rather than aborting every
                    // request that happens to use this environment,
                    // mirroring how a deleted-from-the-vault secret (fetcher
                    // Ok(None)) is already handled just below.
                    Err(rocket_shared::error::DomainError::NotFound(_)) => continue,
                    Err(error) => {
                        failures.push(UnresolvedBinding {
                            alias: binding.alias.clone(),
                            error,
                        });
                        continue 'bindings;
                    }
                };

                if let Some(value) = value {
                    result.insert(format!("{}.{}", binding.alias, secret_ref.name), value);
                }
            }
        }

        (result, failures)
    }

    /// Whether anything this send can read refers to a secret of `alias`:
    /// the request input (URL, headers, body, auth, scripts and so on) or a
    /// variable value from any scope. The match is the plain text `alias.`,
    /// so it covers `{{alias.name}}` and `getSecretVar('alias.name')`. It can
    /// over-match, and then the send is refused as before, never the reverse.
    fn references_alias(
        &self,
        input: &ExecuteRequestInput,
        alias: &str,
        resolved: &std::collections::HashMap<String, String>,
    ) -> bool {
        let needle = format!("{alias}.");
        // A full-line `//` comment in a script is not a use, so a disabled
        // `getSecretVar` line does not count. The scripts are checked line by
        // line and left out of the whole-input check below.
        let scripts = [
            &input.pre_request_script,
            &input.post_response_script,
            &input.tests_script,
        ];
        if scripts
            .iter()
            .filter_map(|script| script.as_deref())
            .any(|script| script_mentions(script, &needle))
        {
            return true;
        }
        // Folder scripts run, and folder headers and auth are sent, with this
        // request too. A chain that cannot be read is not checked here, because
        // `begin_phases` then fails the send with an error naming the folder.
        if let Ok(chain) =
            self.folder_chain(input.collection.as_deref(), input.request_path.as_deref())
        {
            let with_scripts = !input.skip_folder_scripts;
            if chain
                .iter()
                .any(|folder| folder_mentions(folder, &needle, with_scripts))
            {
                return true;
            }
        }
        let mut rest = input.clone();
        rest.pre_request_script = None;
        rest.post_response_script = None;
        rest.tests_script = None;
        if serde_json::to_string(&rest).map_or(true, |text| text.contains(&needle)) {
            return true;
        }
        let ctx = self.build_variable_scopes(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            resolved,
        );
        [
            &ctx.global_env,
            &ctx.collection,
            &ctx.env,
            &ctx.folder,
            &ctx.request,
        ]
        .iter()
        .any(|scope| scope.values().any(|value| value.contains(&needle)))
    }

    /// Builds a scope-separated `VariableContext` from all backend-accessible
    /// scopes (global env, collection, environment, folder-chain,
    /// request-level).
    ///
    /// Reused by `build_variable_context()` and `execute()`.
    fn build_variable_scopes(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> VariableContext {
        // Precedence (lowest → highest): global_env < collection < env < folder < request.
        let mut ctx = VariableContext::default();

        if let Some(name) = global_env_name {
            if let Ok(global_env) = self.env_repo.get(name) {
                for var in global_env.variables.iter().filter(|v| v.enabled) {
                    ctx.global_env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }

        let effective_val = |cv: &rocket_collection::CollectionVariable| -> String {
            if cv.value.is_empty() {
                cv.initial_value.clone()
            } else {
                cv.value.clone()
            }
        };

        if let Some(col) = collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = effective_val(cv);
                ctx.collection.insert(cv.key.clone(), val.clone());
                if cv.secret && val.len() >= MIN_REDACTION_LEN {
                    ctx.secret_values.insert(val);
                }
            }
        }

        if let Some(name) = environment_name {
            if let Ok(env) = self.regular_env_repo(collection).get(name) {
                for var in env.variables.iter().filter(|v| v.enabled) {
                    ctx.env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(folder_vars) = self.collection_repo.get_folder_chain_variables(col, path) {
                for cv in folder_vars.iter().filter(|v| v.enabled) {
                    ctx.folder.insert(cv.key.clone(), effective_val(cv));
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
                for cv in request_vars.iter().filter(|v| v.enabled) {
                    ctx.request.insert(cv.key.clone(), effective_val(cv));
                }
            }
        }

        ctx.external_secrets = external_secrets.clone();
        for value in external_secrets.values() {
            ctx.secret_values
                .extend(crate::redaction::redaction_forms(value));
        }

        ctx
    }

    /// Collects the values a run must never print: secret variables from the
    /// global environment, collection and environment, plus external secrets.
    pub(crate) fn secret_values(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashSet<String> {
        self.build_variable_scopes(
            global_env_name,
            collection,
            environment_name,
            None,
            external_secrets,
        )
        .secret_values
    }

    /// Builds a flattened variable map from all backend-accessible scopes
    /// (global env, collection, environment, folder-chain, request-level).
    ///
    /// Reused by `resolve_request()`, `run_load_test()`, and OAuth2 commands.
    pub fn build_variable_context(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        self.build_variable_scopes(
            global_env_name,
            collection,
            environment_name,
            request_path,
            external_secrets,
        )
        .flatten()
    }

    /// Same as `build_variable_context`, plus the OS environment as `process.env.NAME`
    /// (lowest priority). HTTP requests get these from the frontend, so the protocols
    /// that resolve in the backend (WebSocket, GraphQL subscriptions, gRPC) use this.
    pub fn build_variable_context_with_process_env(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        let mut scopes = self.build_variable_scopes(
            global_env_name,
            collection,
            environment_name,
            request_path,
            external_secrets,
        );
        scopes.process_env = std::env::vars().collect();
        scopes.flatten_with_process_env()
    }

    /// Loads the folder chain above a request, outermost folder first.
    /// A request outside a collection has no chain. A `folder.yml` that cannot be read fails
    /// the send, so its settings are never dropped silently.
    pub(crate) fn folder_chain(
        &self,
        collection: Option<&str>,
        request_path: Option<&str>,
    ) -> DomainResult<Vec<FolderSettings>> {
        match (collection, request_path) {
            (Some(_), Some(path)) if path.starts_with(FLOW_INLINE_PATH_PREFIX) => Ok(Vec::new()),
            (Some(col), Some(path)) => self.collection_repo.get_folder_chain_settings(col, path),
            _ => Ok(Vec::new()),
        }
    }

    /// The auth and headers a request sends once collection and folder defaults apply.
    /// Every send path calls this, so a folder setting cannot apply on one path only.
    pub(crate) fn inherited_auth_and_headers(
        &self,
        collection: Option<&str>,
        folders: &[FolderSettings],
        request_auth: Auth,
        request_headers: &[Header],
    ) -> (Auth, Vec<Header>) {
        match collection {
            Some(col) => {
                let settings = self.collection_repo.get_settings(col).unwrap_or_default();
                apply_inherited_defaults(request_auth, request_headers, settings, folders)
            }
            None => (request_auth, request_headers.to_vec()),
        }
    }

    /// Resolves all {{placeholders}} in `input` using the full variable precedence
    /// chain and returns a ready-to-send `HttpRequest`. Called by both `execute` and
    /// `run_load_test` so resolution logic is never duplicated.
    pub(crate) fn resolve_request(
        &self,
        input: &ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> DomainResult<HttpRequest> {
        let folders =
            self.folder_chain(input.collection.as_deref(), input.request_path.as_deref())?;
        self.resolve_request_with_chain(input, external_secrets, &folders)
    }

    /// `resolve_request` with the folder chain already loaded. A caller that also needs the
    /// chain, such as the script phases, reads it once and passes it here.
    pub(crate) fn resolve_request_with_chain(
        &self,
        input: &ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
        folders: &[FolderSettings],
    ) -> DomainResult<HttpRequest> {
        // Build variable map: global_env < collection < env < folder < request.
        let mut vars = self.build_variable_context(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
        vars.extend(input.flow_vars.clone());

        // Merge collection and folder-chain auth and headers with request-level values.
        let (effective_auth, effective_headers) = self.inherited_auth_and_headers(
            input.collection.as_deref(),
            folders,
            input.auth.clone(),
            &input.headers,
        );

        // Resolve {{placeholders}} in auth, URL and headers.
        let effective_auth = resolve_auth(effective_auth, &vars);
        let resolved_url = resolve(&input.url, &vars).output;
        // Path parameter values may hold {{placeholders}}, so they resolve before they are
        // substituted. The substitution runs on the resolved URL, so a placeholder in the URL
        // itself never swallows a parameter.
        let resolved_path_params: Vec<rocket_shared::types::PathParam> = input
            .path_params
            .iter()
            .map(|p| rocket_shared::types::PathParam {
                name: p.name.clone(),
                value: resolve(&p.value, &vars).output,
                description: None,
            })
            .collect();
        let resolved_url =
            rocket_http::substitute_path_params(&resolved_url, &resolved_path_params);
        // Query keys and values resolve like headers do, so a runner step or a flow node sends
        // the same query string as the single send.
        let resolved_query_params: Vec<QueryParam> = input
            .query_params
            .iter()
            .map(|q| QueryParam {
                key: resolve(&q.key, &vars).output,
                value: resolve(&q.value, &vars).output,
                enabled: q.enabled,
                description: q.description.clone(),
            })
            .collect();
        let resolved_headers: Vec<Header> = effective_headers
            .iter()
            .map(|h| Header {
                key: resolve(&h.key, &vars).output,
                value: resolve(&h.value, &vars).output,
                enabled: h.enabled,
                description: None,
            })
            .collect();

        // Resolve {{placeholders}} in the body: raw `content` for text-like modes,
        // and each form-data entry's `value` for multipart. Keys and file paths
        // are left untouched.
        let resolved_body = input.body.clone().map(|mut body| {
            if let Some(content) = &body.content {
                body.content = Some(if body.mode == BodyMode::GraphQl {
                    crate::graphql_request::resolve_json_text(content, |p| resolve(p, &vars).output)
                } else {
                    resolve(content, &vars).output
                });
            }
            // Past resolution it is plain JSON for the rest of the pipeline.
            if body.mode == BodyMode::GraphQl {
                body.mode = BodyMode::Json;
            }
            if let Some(entries) = &body.form_data {
                body.form_data = Some(
                    entries
                        .iter()
                        .map(|entry| {
                            let mut entry = entry.clone();
                            entry.value = resolve(&entry.value, &vars).output;
                            entry
                        })
                        .collect(),
                );
            }
            body
        });

        // Relative upload paths are relative to the collection folder, like certificate paths.
        let collection_dir = self.collection_folder(input.collection.as_deref());
        let resolved_body =
            resolved_body.map(|b| absolutize_upload_paths(b, collection_dir.as_deref()));

        // The selected environment decides which client certificates the executor may present.
        let mut options = input.options.clone();
        options.client_certificates =
            self.environment_client_certificates(input, &vars, external_secrets);

        Ok(HttpRequest {
            method: input.method.clone(),
            url: resolved_url,
            headers: resolved_headers,
            query_params: resolved_query_params,
            body: resolved_body,
            auth: effective_auth,
            options,
        })
    }

    /// Returns the folder of `collection`, when the wiring knows where collections live.
    fn collection_folder(&self, collection: Option<&str>) -> Option<std::path::PathBuf> {
        collection.and_then(|c| self.collection_env_repo_factory.as_ref()?.collection_dir(c))
    }

    /// Returns the selected environment's client certificates, with `{{placeholders}}`, relative
    /// paths and RocketVault references resolved. A missing environment means no certificates, like it means no variables.
    fn environment_client_certificates(
        &self,
        input: &ExecuteRequestInput,
        vars: &std::collections::HashMap<String, String>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> Vec<ResolvedClientCertificate> {
        // Relative file paths are relative to the collection folder, so they work for a
        // collection that is shared through git.
        let base = self.collection_folder(input.collection.as_deref());
        let repo = self.regular_env_repo(input.collection.as_deref());
        crate::client_certificates::environment_client_certificates(
            repo.as_ref(),
            input.environment_name.as_deref(),
            base.as_deref(),
            vars,
            external_secrets,
        )
    }

    /// Applies the persistent and in-memory side effects from a `ScriptResult`.
    ///
    /// - `env_var_writes` → read-modify-write via `env_repo` (persisted unless it holds a vault secret)
    /// - `collection_var_writes` → read-modify-write via `collection_repo.save_settings`
    /// - `global_env_var_writes` → same repo, keyed by `global_env_name`
    /// - `runtime_vars` → merged into `var_ctx.runtime` for the next script phase
    ///
    /// A write whose value holds a RocketVault secret is not persisted in any scope. It goes to
    /// `var_ctx.runtime` instead, with a console warning. This is best-effort: encodings such as
    /// base64 or URL-encoding, slices of a secret and secrets shorter than `MIN_REDACTION_LEN`
    /// are not caught.
    ///
    /// Non-fatal: individual repo errors are logged but do not abort the response.
    fn apply_script_side_effects(
        &self,
        result: &ScriptResult,
        env_name: Option<&str>,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        var_ctx: &mut rocket_environment::VariableContext,
        guard: VaultGuard<'_>,
    ) {
        let VaultGuard {
            forms: vault_forms,
            console,
        } = guard;
        // Apply active-environment writes (always persisted).
        if !result.env_var_writes.is_empty() {
            if let Some(name) = env_name {
                let repo = self.regular_env_repo(collection);
                let writes = hold_back_vault_writes(
                    &result.env_var_writes,
                    "environment",
                    vault_forms,
                    var_ctx,
                    console,
                );
                self.apply_env_writes(repo.as_ref(), name, &writes, true);
            } else {
                tracing::warn!(
                    "rok.setEnvVar write(s) queued but no active environment is selected — write(s) dropped"
                );
            }
        }

        // Apply global-environment writes (always persisted — modifying a shared env).
        if !result.global_env_var_writes.is_empty() {
            if let Some(name) = global_env_name {
                let writes = hold_back_vault_writes(
                    &result.global_env_var_writes,
                    "global environment",
                    vault_forms,
                    var_ctx,
                    console,
                );
                self.apply_env_writes(self.env_repo.as_ref(), name, &writes, true);
            } else {
                tracing::warn!(
                    "rok.setGlobalEnvVar write(s) queued but no global environment is selected — write(s) dropped"
                );
            }
        }

        // Apply collection variable writes.
        if !result.collection_var_writes.is_empty() {
            if let Some(col) = collection {
                for write in &result.collection_var_writes {
                    if write.value.is_null() {
                        if let Err(e) = self.apply_collection_var_delete(col, &write.key) {
                            tracing::warn!(error = %e, key = %write.key, "failed to persist collection var delete");
                        }
                        continue;
                    }
                    let str_val = write
                        .value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| write.value.to_string());
                    if hold_back_if_vault_secret(
                        "collection variable",
                        &write.key,
                        &write.value,
                        vault_forms,
                        var_ctx,
                        console,
                    ) {
                        continue;
                    }
                    if let Err(e) = self.apply_collection_var_write(col, &write.key, &str_val) {
                        tracing::warn!(error = %e, key = %write.key, "failed to persist collection var write");
                    }
                }
            }
        }

        // Merge runtime vars into context for subsequent phases.
        merge_runtime_vars(var_ctx, result);
    }

    /// Read-modify-write helper for a single collection variable.
    ///
    /// Shared by script-side-effect application (`rok.setCollectionVar`) and
    /// the `runtime.actions` set-variable pipeline.
    fn apply_collection_var_write(
        &self,
        collection: &str,
        key: &str,
        value: &str,
    ) -> DomainResult<()> {
        let mut settings = self.collection_repo.get_settings(collection)?;
        upsert_variable(&mut settings.variables, key, value);
        self.collection_repo.save_settings(collection, &settings)?;
        self.events.publish(DomainEvent::CollectionVariableWritten {
            collection: collection.to_string(),
            key: key.to_string(),
        });
        self.events.publish(DomainEvent::ScriptVariableWritten {
            scope: "collection".to_string(),
            environment: None,
            collection: Some(collection.to_string()),
            key: key.to_string(),
        });
        Ok(())
    }

    /// Removes a collection variable and publishes the same events as a write.
    fn apply_collection_var_delete(&self, collection: &str, key: &str) -> DomainResult<()> {
        let mut settings = self.collection_repo.get_settings(collection)?;
        if !remove_variable(&mut settings.variables, key) {
            return Ok(());
        }
        self.collection_repo.save_settings(collection, &settings)?;
        self.events.publish(DomainEvent::CollectionVariableWritten {
            collection: collection.to_string(),
            key: key.to_string(),
        });
        self.events.publish(DomainEvent::ScriptVariableWritten {
            scope: "collection".to_string(),
            environment: None,
            collection: Some(collection.to_string()),
            key: key.to_string(),
        });
        Ok(())
    }

    /// Read-modify-write helper for env var writes against a named environment.
    ///
    /// `EnvVarWrite.persist` is preserved for wire/API compatibility but
    /// currently has no effect — both call sites in `apply_script_side_effects`
    /// and the `runtime.actions` "environment" scope branch always pass
    /// `force_persist: true`, so every write that reaches here is persisted. The callers hold
    /// back writes that contain a RocketVault secret before this point (best-effort, see
    /// `hold_back_text_if_vault_secret`).
    ///
    /// Existing variable metadata (`enabled`, `secret`, `description`, `secret_type`)
    /// is preserved across a script-driven write — only `value` (and, for a
    /// brand-new key, `enabled: true`) is set by the script. `value_variants`
    /// is the one field NOT preserved — it is always cleared to `None` on a
    /// script write, matching this method's pre-existing behavior before this
    /// plan and the spec's explicit choice not to extend preservation to it.
    /// Metadata is looked up against the pre-batch snapshot (`before`), not the
    /// progressively-mutated `env`, so a delete-then-recreate of the same key
    /// within one script still finds the original metadata. A script can never
    /// promote a variable to `secret: true`; only the user can do that via the
    /// environment editor UI. On a successful save, publishes the same
    /// `DomainEvent::EnvironmentSaved` / `AuditEventKind::SecretVariableWritten`
    /// audit trail a manual save produces (via `env_audit::publish_env_write_events`),
    /// plus one `DomainEvent::ScriptVariableWritten` per write actually applied.
    fn apply_env_writes(
        &self,
        repo: &dyn EnvironmentRepository,
        env_name: &str,
        writes: &[rocket_scripting::EnvVarWrite],
        force_persist: bool,
    ) {
        let persist_writes: Vec<&rocket_scripting::EnvVarWrite> = writes
            .iter()
            .filter(|w| force_persist || w.persist)
            .collect();
        if persist_writes.is_empty() {
            return;
        }
        let mut env = match repo.get(env_name) {
            Ok(env) => env,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    environment = %env_name,
                    "rok.setEnvVar/setGlobalEnvVar: environment not found, write dropped"
                );
                return;
            }
        };
        let before = env.clone();

        for write in persist_writes.iter() {
            if write.value.is_null() {
                env.remove_variable(&write.key);
                continue;
            }
            let str_val = write
                .value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| write.value.to_string());
            // Look up metadata from the pre-batch snapshot, not the
            // progressively-mutated `env` — otherwise a delete followed by a
            // re-set of the same key within one script (e.g.
            // rok.deleteEnvVar('K'); rok.setEnvVar('K', v)) would find no
            // "existing" entry in the already-mutated `env`, silently
            // stripping the secret flag (and other metadata) with no audit
            // trail. `before` is the metadata the user actually established
            // before this script ran, which is also the semantically correct
            // source regardless of same-batch reordering.
            let existing = before.variables.iter().find(|v| v.key == write.key);
            let updated = rocket_environment::Variable {
                key: write.key.clone(),
                value: str_val,
                enabled: existing.map(|v| v.enabled).unwrap_or(true),
                secret: existing.map(|v| v.secret).unwrap_or(false),
                description: existing.and_then(|v| v.description.clone()),
                value_variants: None,
                secret_type: existing.and_then(|v| v.secret_type.clone()),
            };
            env.set_variable(updated);
        }

        match repo.save(&env) {
            Ok(()) => {
                env_audit::publish_env_write_events(
                    self.events.as_ref(),
                    self.audit.as_ref(),
                    env_name,
                    &before,
                    &env,
                );
                for write in persist_writes.iter() {
                    self.events.publish(DomainEvent::ScriptVariableWritten {
                        scope: "environment".to_string(),
                        environment: Some(env_name.to_string()),
                        collection: None,
                        key: write.key.clone(),
                    });
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    environment = %env_name,
                    "failed to persist script env var write"
                );
            }
        }
    }

    /// The host for one script run of a request. Script requests reuse the
    /// request's TLS, redirect, cookie and client-certificate options.
    fn script_host(&self, state: &PhaseState) -> script_host::ExecutionScriptHost<'_> {
        script_host::ExecutionScriptHost {
            svc: self,
            options: state.http_request.options.clone(),
        }
    }

    /// Runs one chained script. A failure is published as `ScriptError` and
    /// returned in `error`. Both name the folder when the script came from one.
    async fn run_script_phase(
        &self,
        script: &ChainedScript,
        ctx: ScriptContext,
        host: &dyn ScriptHost,
        request_name: &str,
        phase: &str,
        all_console: &mut Vec<ConsoleEntry>,
    ) -> ScriptResult {
        let engine = match self.script_engine.as_ref() {
            Some(e) => e,
            None => return ScriptResult::default(),
        };
        match engine.execute_with_host(ctx, host).await {
            Ok(mut result) => {
                if let Some(err) = result.error.take() {
                    let message = script.attribute(phase, &err);
                    self.events.publish(DomainEvent::ScriptError {
                        request_name: request_name.to_string(),
                        phase: phase.to_string(),
                        message: message.clone(),
                    });
                    result.error = Some(message);
                }
                all_console.extend(result.console_entries.clone());
                result
            }
            Err(e) => {
                let message = script.attribute(phase, &e.to_string());
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: phase.to_string(),
                    message: message.clone(),
                });
                // Carry the failure in `error` as well. Callers build
                // ExecuteOutput.script_error from this field only, so without
                // it a timed-out script would fire an event but show nothing
                // in the request's own error surface.
                ScriptResult {
                    error: Some(message),
                    ..Default::default()
                }
            }
        }
    }

    /// Applies `Request.actions` (`set-variable`) for the given phase.
    ///
    /// Each enabled action whose `phase` matches `phase_str` evaluates its
    /// `selector.expression` as a JS snippet via the script engine — this is
    /// what "jsonq" means in this codebase (see `apply_actions` in the SP3
    /// completion plan). The result is written to the scope named by
    /// `action.variable.scope`. A bad expression, a missing target scope, or
    /// a repo write failure is logged and the action is skipped — it never
    /// aborts the rest of the request.
    #[allow(clippy::too_many_arguments)]
    async fn apply_actions(
        &self,
        actions: &[rocket_shared::ActionSetVariable],
        phase_str: &str,
        request_name: &str,
        http_request: &HttpRequest,
        response: Option<&HttpResponse>,
        env_name: Option<&str>,
        collection: Option<&str>,
        request_path: Option<&str>,
        var_ctx: &mut VariableContext,
        tags: &[String],
        path_params: &[rocket_shared::types::PathParam],
        guard: VaultGuard<'_>,
    ) {
        let VaultGuard {
            forms: vault_forms,
            console,
        } = guard;
        let Some(engine) = self.script_engine.as_ref() else {
            return;
        };

        for action in actions {
            if action.disabled == Some(true) || action.phase != phase_str {
                continue;
            }

            let code = format!(
                "rok.setVar('__jsonq_result__', (function(){{ return ({}); }})());",
                action.selector.expression
            );
            let ctx = match response {
                Some(res) => ScriptContext::after_response(
                    code,
                    var_ctx.clone(),
                    http_request.clone(),
                    res.clone(),
                    env_name.map(str::to_string),
                    request_name.to_string(),
                    tags.to_vec(),
                    path_params.to_vec(),
                ),
                None => ScriptContext::before_request(
                    code,
                    var_ctx.clone(),
                    http_request.clone(),
                    env_name.map(str::to_string),
                    request_name.to_string(),
                    tags.to_vec(),
                    path_params.to_vec(),
                ),
            };

            let result = match engine.execute(ctx).await {
                Ok(r) => r,
                Err(e) => {
                    self.events.publish(DomainEvent::ScriptError {
                        request_name: request_name.to_string(),
                        phase: format!("action:{phase_str}"),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            if let Some(err) = result.error {
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: format!("action:{phase_str}"),
                    message: err,
                });
                continue;
            }

            let Some(value) = result.runtime_vars.get("__jsonq_result__") else {
                continue;
            };
            let str_val = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            let var_name = &action.variable.name;

            // A value with a vault secret is never saved to any persistent scope. Without an
            // active environment nothing would be saved for that scope, so there is no warning.
            let persistent = match action.variable.scope.as_str() {
                "environment" => env_name.is_some(),
                "collection" | "folder" | "request" => true,
                _ => false,
            };
            if persistent
                && hold_back_text_if_vault_secret(
                    &action.variable.scope,
                    var_name,
                    &str_val,
                    vault_forms,
                    var_ctx,
                    console,
                )
            {
                continue;
            }

            match action.variable.scope.as_str() {
                "runtime" => {
                    var_ctx.runtime.insert(var_name.clone(), str_val);
                }
                "environment" => {
                    if let Some(name) = env_name {
                        let repo = self.regular_env_repo(collection);
                        self.apply_env_writes(
                            repo.as_ref(),
                            name,
                            &[rocket_scripting::EnvVarWrite {
                                key: var_name.clone(),
                                value: serde_json::Value::String(str_val),
                                persist: true,
                            }],
                            true,
                        );
                    } else {
                        tracing::warn!(variable = %var_name, "action scope 'environment' but no active environment");
                    }
                }
                "collection" => {
                    if let Some(col) = collection {
                        if let Err(e) = self.apply_collection_var_write(col, var_name, &str_val) {
                            tracing::warn!(error = %e, variable = %var_name, "failed to persist collection var from action");
                        }
                    }
                }
                "folder" => {
                    if let (Some(col), Some(path)) = (collection, request_path) {
                        let folder_path = std::path::Path::new(path)
                            .parent()
                            .and_then(|p| p.to_str())
                            .unwrap_or("");
                        match self.collection_repo.get_folder_variables(col, folder_path) {
                            Ok(mut vars) => {
                                upsert_variable(&mut vars, var_name, &str_val);
                                if let Err(e) = self.collection_repo.save_folder_variables(
                                    col,
                                    folder_path,
                                    vars,
                                ) {
                                    tracing::warn!(error = %e, variable = %var_name, "failed to persist folder var from action");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, variable = %var_name, "failed to read folder vars for action")
                            }
                        }
                    }
                }
                "request" => {
                    if let (Some(col), Some(path)) = (collection, request_path) {
                        match self.collection_repo.get_request_variables(col, path) {
                            Ok(mut vars) => {
                                upsert_variable(&mut vars, var_name, &str_val);
                                if let Err(e) =
                                    self.collection_repo.save_request_variables(col, path, vars)
                                {
                                    tracing::warn!(error = %e, variable = %var_name, "failed to persist request var from action");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, variable = %var_name, "failed to read request vars for action")
                            }
                        }
                    }
                }
                other => {
                    tracing::warn!(scope = %other, variable = %var_name, "unknown action variable scope, skipping");
                }
            }
        }
    }

    /// Validates a BeforeRequest script's URL mutation against the workspace's
    /// opt-in `RequestGuardPolicy`. Only ever inspects `mutated_url` — the
    /// user's own manually-typed URL never reaches this method (see call site
    /// in `execute()`, which only calls this when a script actually set a new
    /// URL). Compares resolved hosts, not raw URL strings: a script that only
    /// rewrites the path/query of the same host the user already declared is
    /// never blocked, regardless of whether that host happens to be internal.
    fn check_request_guard(
        &self,
        original_url: &str,
        mutated_url: &str,
        policy: &rocket_workspace::RequestGuardPolicy,
    ) -> DomainResult<()> {
        if !policy.block_script_redirects_to_internal_hosts {
            return Ok(());
        }

        let mutated = url::Url::parse(mutated_url).map_err(|e| {
            rocket_shared::error::DomainError::InvalidInput(format!(
                "script produced an invalid URL: {e}"
            ))
        })?;
        let Some(mutated_host) = mutated.host_str() else {
            // No host component (e.g. a relative/opaque URL) — nothing to check.
            return Ok(());
        };

        // If the script only changed the path/query of the host the user
        // already declared, this is not a redirect in the sense the guard
        // cares about.
        if let Ok(original) = url::Url::parse(original_url) {
            if original.host_str() == Some(mutated_host) {
                return Ok(());
            }
        }

        if crate::request_guard::is_blocked_host(mutated_host, policy.also_block_private_ranges) {
            return Err(rocket_shared::error::DomainError::InvalidInput(format!(
                "blocked: script redirected request to internal host '{mutated_host}' \
                 (workspace policy blocks script-driven redirects to internal hosts)"
            )));
        }
        Ok(())
    }

    /// Resolves the request, emits the sensitive-auth audit event, and builds
    /// the scope-separated variable context. Every phase method below assumes
    /// this ran first.
    pub(crate) fn begin_phases(
        &self,
        input: &ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> DomainResult<PhaseState> {
        // One chain read per execution, shared by the request defaults and the
        // script phases. A folder.yml that cannot be read fails the send here,
        // with an error that names the folder.
        let folder_chain =
            self.folder_chain(input.collection.as_deref(), input.request_path.as_deref())?;
        let http_request =
            self.resolve_request_with_chain(input, external_secrets, &folder_chain)?;

        // Emit a sensitive-auth audit event BEFORE dispatch when the resolved
        // request carries a real credential (not None / Inherit). This captures
        // the intent even if the network call itself fails.
        if let Some(auth_type) = sensitive_auth_label(&http_request.auth) {
            self.audit.publish(
                "system".into(),
                None,
                AuditEventKind::SensitiveAuthUsed {
                    auth_type: auth_type.to_string(),
                    collection: input.collection.clone().unwrap_or_default(),
                    request_path: input.request_path.clone().unwrap_or_default(),
                },
            );
        }

        // Build scope-separated variable context for script phases. Scripts read
        // individual scopes via rok.getCollectionVar/getEnvVar/getGlobalEnvVar, so
        // each scope must stay distinct rather than being pre-flattened into one.
        let mut var_ctx = self.build_variable_scopes(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
        // Scripts read the host environment through rok.getProcessEnv.
        var_ctx.process_env = std::env::vars().collect();
        // Flow run variables (e.g. `callback.<name>`) behave like runtime
        // variables. A script that sets the same key later still wins.
        var_ctx.runtime.extend(input.flow_vars.clone());

        let (sandbox_mode, file_scope, script_flow) = match input.collection.as_deref() {
            Some(col) => {
                let settings = self.collection_repo.get_settings(col).unwrap_or_default();
                let mode = match settings.sandbox_mode {
                    CollectionSandboxMode::Safe => SandboxMode::Safe,
                    CollectionSandboxMode::Developer => SandboxMode::Developer,
                };
                // A collection whose directory cannot be resolved just gets no scope.
                let scope = self
                    .collection_repo
                    .collection_root_path(col)
                    .ok()
                    .map(|root| ScriptFileScope {
                        collection_root: root,
                        additional_roots: settings
                            .script_context_roots
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                    });
                (mode, scope, settings.script_flow)
            }
            None => (SandboxMode::Safe, None, ScriptFlow::default()),
        };

        let script_folders: &[FolderSettings] = if input.skip_folder_scripts {
            &[]
        } else {
            &folder_chain
        };
        let labels = folder_labels(
            input.request_path.as_deref().unwrap_or_default(),
            script_folders.len(),
        );
        let scripts = PhaseScripts::assemble(
            script_folders,
            &labels,
            script_flow,
            input.pre_request_script.as_deref(),
            input.post_response_script.as_deref(),
            input.tests_script.as_deref(),
        );

        Ok(PhaseState {
            http_request,
            var_ctx,
            script_error: None,
            console: Vec::new(),
            test_results: Vec::new(),
            next_request: None,
            skip_request: false,
            vault_forms: crate::redaction::secret_forms(external_secrets.values()),
            sandbox_mode,
            file_scope,
            scripts,
            response_body_override: None,
        })
    }

    /// Runs the before-request script (if any), applies its request mutations
    /// and side effects, then runs the `before-request` declarative actions.
    ///
    /// Returns an error only when a script's `req.setUrl()` mutation is
    /// blocked by the workspace's `RequestGuardPolicy` — every other script
    /// problem is recorded on `state.script_error` instead of aborting.
    pub(crate) async fn run_before_request_phase(
        &self,
        input: &ExecuteRequestInput,
        mode: ExecutionMode,
        state: &mut PhaseState,
    ) -> DomainResult<()> {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        // The folder chain and the request's own script, in `chain_scripts`
        // order. Each script is its own engine run, and sees the request and
        // the variables the scripts before it left behind.
        let scripts = state.scripts.pre_request.clone();
        for script in &scripts {
            let ctx = ScriptContext::before_request(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone())
            .with_collection_name(input.collection.clone());
            let had_error = state.script_error.is_some();
            let host = self.script_host(state);
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &host,
                    &request_name,
                    "before-request",
                    &mut state.console,
                )
                .await;

            // Apply request mutations.
            if let Some(ref mutations) = result.request_mutations {
                if let Some(ref url) = mutations.url {
                    let original_url = state.http_request.url.clone();
                    // Deliberate: an early return here also discards any
                    // next_request/runtime_vars this same script set below.
                    // A script whose req.setUrl() just tripped the SSRF
                    // guard does not get to steer the run via
                    // setNextRequest() or leave variables behind either.
                    self.check_request_guard(&original_url, url, &input.request_guard_policy)?;
                    state.http_request.url = url.clone();
                }
                if let Some(ref method_str) = mutations.method {
                    if let Ok(m) = method_str.parse() {
                        state.http_request.method = m;
                    } else {
                        tracing::warn!(
                            method = %method_str,
                            "req.setMethod() called with an unrecognized HTTP method, ignored"
                        );
                        state.script_error.get_or_insert_with(|| {
                            format!(
                            "req.setMethod('{method_str}') is not a valid HTTP method — ignored."
                        )
                        });
                    }
                }
                // Apply header mutations in the order the script issued them —
                // e.g. deleteHeader() then setHeader() on the same name must
                // result in the header being present, not dropped.
                for mutation in &mutations.headers {
                    match mutation {
                        rocket_scripting::HeaderMutation::Set { name, value } => {
                            if let Some(h) = state
                                .http_request
                                .headers
                                .iter_mut()
                                .find(|h| h.key.eq_ignore_ascii_case(name))
                            {
                                h.value = value.clone();
                            } else {
                                state.http_request.headers.push(Header::new(name, value));
                            }
                        }
                        rocket_scripting::HeaderMutation::Delete { name } => {
                            state
                                .http_request
                                .headers
                                .retain(|h| !h.key.eq_ignore_ascii_case(name));
                        }
                    }
                }
                if let Some(ms) = mutations.timeout_ms {
                    state.http_request.options.timeout_ms = ms;
                }
                if let Some(ref body_val) = mutations.body {
                    // A JS object/array is unambiguously meant as JSON. A string
                    // may be non-JSON text (XML, plain text, etc) — respect an
                    // explicit Content-Type header the script already set instead
                    // of forcing JSON, which would mislabel the body on the wire.
                    let mode = if body_val.is_object() || body_val.is_array() {
                        rocket_shared::types::BodyMode::Json
                    } else {
                        body_mode_from_content_type(&state.http_request.headers)
                    };
                    let content = body_val
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| body_val.to_string());
                    state.http_request.body = Some(rocket_shared::types::Body {
                        mode,
                        content: Some(content),
                        form_data: None,
                        file_path: None,
                    });
                }
                if let Some(n) = mutations.max_redirects {
                    state.http_request.options.max_redirects = Some(n);
                }
            }

            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );

            // The first error in phase order is kept. Within one script, a
            // thrown error still replaces its own setMethod warning.
            let failed = result.error.is_some();
            if failed && !had_error {
                state.script_error = result.error;
            }

            // Runner controls. `execute()` never reads these; the runner
            // checks them after every phase that ran (spec §4).
            if result.skip_request {
                state.skip_request = true;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }

            // A failed script ends its phase. In a run, skipRequest() ends it
            // too, because the request will not be sent.
            if failed || (result.skip_request && mode == ExecutionMode::Runner) {
                break;
            }
        }

        // ── Before-request actions (runtime.actions, set-variable) ─────────────
        let http_request = state.http_request.clone();
        self.apply_actions(
            &input.actions,
            "before-request",
            &request_name,
            &http_request,
            None,
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut state.var_ctx,
            &input.tags,
            &input.path_params,
            VaultGuard {
                forms: &state.vault_forms,
                console: &mut state.console,
            },
        )
        .await;

        Ok(())
    }

    /// Dispatches the (possibly script-mutated) request. A RocketVault certificate selected for
    /// its URL, or for its OAuth2 client-credentials token URL, is fetched first.
    pub(crate) async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse> {
        let request = self.with_vault_certificates(&state.http_request).await;
        let response = self.executor.execute(&request).await?;

        tracing::info!(
            status = response.status,
            duration_ms = response.duration_ms,
            size_bytes = response.size_bytes,
            "Request completed"
        );

        Ok(response)
    }

    /// The request with its selected RocketVault certificates fetched. The fetched copy lives
    /// only for this send: `state.http_request`, which history and scripts see, keeps the
    /// names-only form. A request with nothing to fetch is not copied.
    async fn with_vault_certificates<'r>(
        &self,
        request: &'r HttpRequest,
    ) -> std::borrow::Cow<'r, HttpRequest> {
        let urls = crate::vault_certificates::certificate_urls(request);
        if !crate::vault_certificates::needs_fetch(&request.options.client_certificates, &urls) {
            return std::borrow::Cow::Borrowed(request);
        }
        let access = crate::vault_certificates::VaultCertificateAccess {
            connections: self.secret_manager_repo.as_ref(),
            secret_store: self.vault_connection_secret_store.as_ref(),
            fetcher: self.vault_fetcher.as_ref(),
        };
        let mut fetched = request.clone();
        crate::vault_certificates::materialize_selected(
            &mut fetched.options.client_certificates,
            &urls,
            Some(&access),
        )
        .await;
        std::borrow::Cow::Owned(fetched)
    }

    /// Runs the after-response script (if any) and applies its side effects.
    pub(crate) async fn run_after_response_phase(
        &self,
        input: &ExecuteRequestInput,
        mode: ExecutionMode,
        response: &HttpResponse,
        state: &mut PhaseState,
    ) {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        let scripts = state.scripts.post_response.clone();
        for script in &scripts {
            let ctx = ScriptContext::after_response(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                response.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone())
            .with_collection_name(input.collection.clone());
            let host = self.script_host(state);
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &host,
                    &request_name,
                    "after-response",
                    &mut state.console,
                )
                .await;
            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );
            let failed = result.error.is_some();
            if failed && state.script_error.is_none() {
                state.script_error = result.error;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }
            if result.response_body.is_some() {
                state.response_body_override = result.response_body.clone();
            }
            // A failed script ends its phase.
            if failed {
                break;
            }
        }
    }

    /// Runs the tests script (if any) and collects its `rok.test()` results.
    pub(crate) async fn run_tests_phase(
        &self,
        input: &ExecuteRequestInput,
        mode: ExecutionMode,
        response: &HttpResponse,
        state: &mut PhaseState,
    ) {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        let scripts = state.scripts.tests.clone();
        let script_response = with_body_override(response, state.response_body_override.as_deref());
        for script in &scripts {
            let ctx = ScriptContext::tests(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                script_response.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone())
            .with_collection_name(input.collection.clone())
            .with_assertion_results(crate::assertion_evaluator::assertion_outcomes(
                &input.assertions,
                response,
            ));
            let host = self.script_host(state);
            let result = self
                .run_script_phase(script, ctx, &host, &request_name, "tests", &mut state.console)
                .await;
            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );
            state.test_results.extend(result.test_results.clone());
            let failed = result.error.is_some();
            if failed && state.script_error.is_none() {
                state.script_error = result.error;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }
            // A failed script ends its phase.
            if failed {
                break;
            }
        }
    }

    /// Publishes collected console output, if any. Shared by `finish_phases`
    /// and by the runner, which needs it for a step that was skipped before the
    /// send (and therefore never reaches `finish_phases`).
    pub(crate) fn publish_console(&self, request_name: &str, entries: &[ConsoleEntry]) {
        if entries.is_empty() {
            return;
        }
        let entries = entries
            .iter()
            .map(|e| {
                let level = match e.level {
                    ConsoleLevel::Log => "log",
                    ConsoleLevel::Warn => "warn",
                    ConsoleLevel::Error => "error",
                };
                serde_json::json!({ "level": level, "message": e.message })
            })
            .collect();
        self.events.publish(DomainEvent::ConsoleOutput {
            request_name: request_name.to_string(),
            entries,
        });
    }

    /// Runs the after-response actions and declarative assertions, publishes the
    /// console/tests/executed events, saves history, and builds the output.
    pub(crate) async fn finish_phases(
        &self,
        input: &ExecuteRequestInput,
        response: HttpResponse,
        state: &mut PhaseState,
    ) -> ExecuteRequestOutput {
        let request_name = input.request_name.clone().unwrap_or_default();

        // ── After-response actions (runtime.actions, set-variable) ─────────────
        let http_request = state.http_request.clone();
        self.apply_actions(
            &input.actions,
            "after-response",
            &request_name,
            &http_request,
            Some(&response),
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut state.var_ctx,
            &input.tags,
            &input.path_params,
            VaultGuard {
                forms: &state.vault_forms,
                console: &mut state.console,
            },
        )
        .await;

        // ── Declarative assertions ────────────────────────────────────────────
        // Run after tests script so JS test results appear first in TestsPanel.
        let assertion_results =
            crate::assertion_evaluator::evaluate_assertions(&input.assertions, &response);
        state.test_results.extend(assertion_results);

        // ── Emit events ───────────────────────────────────────────────────────
        self.publish_console(&request_name, &state.console);

        if !state.test_results.is_empty() {
            let results = state
                .test_results
                .iter()
                .map(|t| {
                    let status = match t.status {
                        TestStatus::Passed => "passed",
                        TestStatus::Failed => "failed",
                    };
                    serde_json::json!({ "name": t.name, "status": status, "error": t.error })
                })
                .collect();
            self.events.publish(DomainEvent::TestsCompleted {
                request_name: request_name.clone(),
                results,
            });
        }

        // Persist history (non-fatal — a save failure won't cancel the response).
        // Redact secret values from the URL before it reaches rocket-history —
        // the dispatched request and the RequestExecuted event below still
        // carry the real, unredacted URL; only the persisted copy is redacted.
        let redacted_url =
            crate::redaction::redact_secrets(&state.http_request.url, &state.var_ctx.secret_values);
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &redacted_url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
        let deferred_history = if input.skip_history {
            Some(entry)
        } else {
            let _ = self.history_repo.save(&entry);
            None
        };

        // Publish domain event.
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: state.http_request.url.clone(),
            status: response.status,
            duration_ms: response.duration_ms,
        });

        ExecuteRequestOutput {
            response,
            test_results: state.test_results.clone(),
            console_entries: state.console.clone(),
            script_error: state.script_error.clone(),
            deferred_history,
        }
    }

    /// Saves a History entry returned in `deferred_history`. A failure is
    /// ignored, like the save in `finish_phases`.
    pub(crate) fn save_deferred_history(&self, entry: &HistoryEntry) {
        let _ = self.history_repo.save(entry);
    }

    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput> {
        // A configured external secret that fails to resolve live is a hard
        // stop, not a silent empty-string substitution (spec §2/§4.6) — but
        // only when this send refers to it. A binding the request never
        // mentions must not block it, so an unreachable or rejected vault
        // cannot stop unrelated requests in the same environment. An Err
        // returned here leaves execute() before begin_phases (and therefore
        // before send_request) ever runs.
        let (external_secrets, failures) = self
            .resolve_external_secrets_partial(
                input.collection.as_deref(),
                input.environment_name.as_deref(),
            )
            .await;
        for failure in failures {
            if self.references_alias(&input, &failure.alias, &external_secrets) {
                return Err(failure.error);
            }
            tracing::warn!(
                alias = %failure.alias,
                error = %failure.error,
                "vault secrets for an unused binding could not be fetched, sending without them"
            );
        }
        self.execute_with_external_secrets(input, &external_secrets)
            .await
    }

    /// Runs every phase of one send with External Secret values the caller
    /// already resolved. A multi-request orchestrator (the Flow runner) calls
    /// this so it fetches secrets once per run, not once per request.
    #[tracing::instrument(
        name = "http_request",
        skip(self, input, external_secrets),
        fields(
            method = %input.method,
            url = %input.url,
        )
    )]
    pub(crate) async fn execute_with_external_secrets(
        &self,
        input: ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> DomainResult<ExecuteRequestOutput> {
        let mut sent = None;
        self.execute_capturing(input, external_secrets, &mut sent)
            .await
    }

    /// Runs the single-send path and records the request as it was handed to
    /// the executor, after the pre-request script ran. The record survives a
    /// failed send.
    pub(crate) async fn execute_capturing(
        &self,
        input: ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
        sent: &mut Option<HttpRequest>,
    ) -> DomainResult<ExecuteRequestOutput> {
        // Every phase runs unconditionally — this is the single-send path. The
        // Collection Runner calls the same methods one at a time so it can act
        // on skip_request / next_request between them.
        let mut state = self.begin_phases(&input, external_secrets)?;
        self.run_before_request_phase(&input, ExecutionMode::Standalone, &mut state)
            .await?;
        *sent = Some(state.http_request.clone());
        let response = self.send_request(&state).await?;
        self.run_after_response_phase(&input, ExecutionMode::Standalone, &response, &mut state)
            .await;
        self.run_tests_phase(&input, ExecutionMode::Standalone, &response, &mut state)
            .await;
        Ok(self.finish_phases(&input, response, &mut state).await)
    }

    pub async fn run_load_test(
        &self,
        input: ExecuteRequestInput,
        config: LoadTestConfig,
    ) -> DomainResult<LoadTestResult> {
        // Load testing is out of scope for external-secrets resolution and RocketVault
        // certificates, which fail with a clear message when selected.
        let mut resolved = self.resolve_request(&input, &std::collections::HashMap::new())?;
        crate::client_certificates::unavailable_in_load_tests(
            &mut resolved.options.client_certificates,
        );
        // A burst of concurrent requests must not rewrite a jar file for every response.
        resolved.options.use_cookie_jar = false;
        let executor = Arc::clone(&self.executor);
        Ok(http_run_load_test(executor, &resolved, &config).await)
    }

    /// Preview-evaluates a jsonq expression against a captured response, for the
    /// Vars tab's "Test" affordance. Only collection-scope variables are available
    /// (no environment/folder/request scope) — this is a preview tool, separate
    /// from the real `apply_actions` execution pipeline.
    pub async fn evaluate_var_expression(
        &self,
        collection_root: &str,
        expression: &str,
        response_json: &str,
    ) -> DomainResult<serde_json::Value> {
        self.evaluate_expression_with_logs(
            collection_root,
            expression,
            response_json,
            std::collections::HashSet::new(),
        )
        .await
        .0
    }

    /// Like `evaluate_var_expression`, but also returns the script's console
    /// output. The entries include lines logged before a thrown error, and are
    /// empty when the script never ran. `secret_values` are redacted from the
    /// console output by the engine.
    pub async fn evaluate_expression_with_logs(
        &self,
        collection_root: &str,
        expression: &str,
        response_json: &str,
        secret_values: std::collections::HashSet<String>,
    ) -> (DomainResult<serde_json::Value>, Vec<ConsoleEntry>) {
        let Some(engine) = self.script_engine.as_ref() else {
            return (
                Err(rocket_shared::error::DomainError::Internal(
                    "script engine not configured".into(),
                )),
                vec![],
            );
        };

        let response: HttpResponse = match serde_json::from_str(response_json) {
            Ok(r) => r,
            Err(e) => {
                return (
                    Err(rocket_shared::error::DomainError::InvalidInput(format!(
                        "invalid response JSON: {e}"
                    ))),
                    vec![],
                )
            }
        };

        let mut var_ctx = VariableContext {
            secret_values,
            ..Default::default()
        };
        if let Ok(settings) = self.collection_repo.get_settings(collection_root) {
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = if cv.value.is_empty() {
                    cv.initial_value.clone()
                } else {
                    cv.value.clone()
                };
                var_ctx.collection.insert(cv.key.clone(), val);
            }
        }

        let code =
            format!("rok.setVar('__jsonq_result__', (function(){{ return ({expression}); }})());");
        let ctx = ScriptContext::after_response(
            code,
            var_ctx,
            HttpRequest::new(HttpMethod::Get, ""),
            response,
            None,
            String::new(),
            vec![],
            vec![],
        );

        let result = match engine.execute(ctx).await {
            Ok(r) => r,
            Err(e) => return (Err(e), vec![]),
        };
        let logs = result.console_entries;
        if let Some(err) = result.error {
            return (
                Err(rocket_shared::error::DomainError::InvalidInput(err)),
                logs,
            );
        }
        let value = result
            .runtime_vars
            .get("__jsonq_result__")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        (Ok(value), logs)
    }
}

/// Map an `Auth` variant to a short kebab-case label for audit events.
/// Returns `None` for `Auth::None` and `Auth::Inherit` because those are not
/// "sensitive auth used" — no credential is actually being sent on the wire.
pub(crate) fn sensitive_auth_label(auth: &Auth) -> Option<&'static str> {
    match auth {
        Auth::None | Auth::Inherit => None,
        Auth::Basic { .. } => Some("basic"),
        Auth::Bearer { .. } => Some("bearer"),
        Auth::ApiKey { .. } => Some("api-key"),
        Auth::OAuth2(_) => Some("oauth2"),
        Auth::AwsSigV4 { .. } => Some("aws-sig-v4"),
        Auth::Wsse { .. } => Some("wsse"),
        Auth::Digest { .. } => Some("digest"),
        Auth::Ntlm { .. } => Some("ntlm"),
        Auth::OAuth1(_) => Some("oauth1"),
    }
}

/// Upserts a single key/value into a `CollectionVariable` list by key,
/// appending a new enabled, non-secret entry if the key isn't already present.
fn upsert_variable(vars: &mut Vec<rocket_collection::CollectionVariable>, key: &str, value: &str) {
    if let Some(existing) = vars.iter_mut().find(|v| v.key == key) {
        existing.value = value.to_string();
    } else {
        vars.push(rocket_collection::CollectionVariable {
            key: key.to_string(),
            value: value.to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        });
    }
}

/// Removes a collection variable by key. Returns true when one was removed.
fn remove_variable(vars: &mut Vec<rocket_collection::CollectionVariable>, key: &str) -> bool {
    let before = vars.len();
    vars.retain(|v| v.key != key);
    vars.len() != before
}

/// Returns a copy of `response` whose text body is replaced, for later script phases.
fn with_body_override(response: &HttpResponse, body: Option<&str>) -> HttpResponse {
    let mut patched = response.clone();
    if let Some(body) = body {
        patched.body = body.to_string();
        patched.size_bytes = body.len();
        patched.is_binary = false;
        patched.body_base64 = None;
    }
    patched
}

/// Merges a script's runtime writes and deletes into the variable context.
///
/// Non-string values are kept as JSON text so a number or object set with
/// `rok.setVar` survives into the next script phase. A null value is skipped.
fn merge_runtime_vars(var_ctx: &mut rocket_environment::VariableContext, result: &ScriptResult) {
    for (key, value) in &result.runtime_vars {
        let text = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        var_ctx.runtime.insert(key.clone(), text);
    }
    for key in &result.runtime_var_deletes {
        var_ctx.runtime.remove(key);
    }
}

/// Infers a `BodyMode` for a script-set string body from an explicit
/// `Content-Type` header on the request, if the script set one. Falls back to
/// `Json` (the historical default for `req.setBody()`) when no explicit
/// Content-Type is present or it doesn't map to a known text-ish mode.
fn body_mode_from_content_type(headers: &[Header]) -> rocket_shared::types::BodyMode {
    use rocket_shared::types::BodyMode;
    let Some(content_type) = headers
        .iter()
        .find(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
    else {
        return BodyMode::Json;
    };
    if content_type.contains("xml") {
        BodyMode::Xml
    } else if content_type.contains("sparql") {
        BodyMode::Sparql
    } else if content_type.contains("text/plain") {
        BodyMode::Text
    } else {
        BodyMode::Json
    }
}

/// Use the collection auth when the request carries no auth of its own.
/// A request set to inherit also takes the collection auth, since inherit is not sent on the wire.
fn merge_auth(request_auth: Auth, collection_auth: Option<Auth>) -> Auth {
    match request_auth {
        Auth::None | Auth::Inherit => collection_auth.unwrap_or(Auth::None),
        explicit => explicit,
    }
}

/// Resolves `{{placeholders}}` in the credential fields of an auth value.
/// The Request tab resolves auth on the frontend, but the backend-only paths
/// (Flow, collection runner) receive the raw collection auth, so it is done here.
/// OAuth2 flows are left untouched.
fn resolve_auth(auth: Auth, vars: &std::collections::HashMap<String, String>) -> Auth {
    let r = |s: String| resolve(&s, vars).output;
    match auth {
        Auth::Basic { username, password } => Auth::Basic {
            username: r(username),
            password: r(password),
        },
        Auth::Bearer { token } => Auth::Bearer { token: r(token) },
        Auth::ApiKey {
            key,
            value,
            placement,
        } => Auth::ApiKey {
            key: r(key),
            value: r(value),
            placement,
        },
        Auth::Wsse { username, password } => Auth::Wsse {
            username: r(username),
            password: r(password),
        },
        Auth::Digest { username, password } => Auth::Digest {
            username: r(username),
            password: r(password),
        },
        Auth::Ntlm {
            username,
            password,
            domain,
        } => Auth::Ntlm {
            username: r(username),
            password: r(password),
            domain: r(domain),
        },
        Auth::OAuth1(mut a) => {
            a.consumer_key = a.consumer_key.map(&r);
            a.consumer_secret = a.consumer_secret.map(&r);
            a.access_token = a.access_token.map(&r);
            a.access_token_secret = a.access_token_secret.map(&r);
            a.callback_url = a.callback_url.map(&r);
            a.verifier = a.verifier.map(&r);
            Auth::OAuth1(a)
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
            profile_name,
        } => Auth::AwsSigV4 {
            access_key: r(access_key),
            secret_key: r(secret_key),
            region: r(region),
            service: r(service),
            session_token: session_token.map(&r),
            profile_name: profile_name.map(&r),
        },
        other => other,
    }
}

/// Merge collection-level headers with request-level headers.
/// Request headers override collection headers when they share the same key.
fn merge_headers(collection_headers: &[Header], request_headers: &[Header]) -> Vec<Header> {
    let request_keys: std::collections::HashSet<&str> = request_headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| h.key.as_str())
        .collect();
    let mut merged: Vec<Header> = collection_headers
        .iter()
        .filter(|h| !request_keys.contains(h.key.as_str()))
        .cloned()
        .collect();
    merged.extend(request_headers.iter().cloned());
    merged
}

/// Applies the collection and folder-chain defaults to a request's own auth and headers.
/// Headers: collection, then folders from outermost to innermost, then the request. A more
/// specific level replaces a header with the same key, and a disabled header never shadows.
/// Auth: a request auth of `none` or `inherit` takes the nearest folder auth, then the
/// collection auth.
/// When no folder in the chain sets an enabled header, the collection headers merge as they
/// always did, so duplicate and disabled collection headers are kept.
fn apply_inherited_defaults(
    request_auth: Auth,
    request_headers: &[Header],
    settings: CollectionSettings,
    folders: &[FolderSettings],
) -> (Auth, Vec<Header>) {
    let headers = if folders.iter().all(|f| f.headers.iter().all(|h| !h.enabled)) {
        merge_headers(&settings.headers, request_headers)
    } else {
        merge_headers(
            &inherited_headers(&settings.headers, folders),
            request_headers,
        )
    };
    let auth = merge_auth(request_auth, resolve_folder_auth(folders).or(settings.auth));
    (auth, headers)
}

/// Joins the relative file paths of a multipart or binary body onto the collection folder.
///
/// Absolute paths and paths with a `..` stay as written. The executor rejects a path that is
/// still relative, so a stored path can never reach outside the collection through here.
fn absolutize_upload_paths(mut body: Body, base: Option<&std::path::Path>) -> Body {
    if base.is_none() {
        return body;
    }
    if let Some(entries) = body.form_data.as_mut() {
        for entry in entries.iter_mut() {
            if entry.entry_type == rocket_shared::types::FormDataType::File {
                entry.value =
                    crate::client_certificates::absolutize(std::mem::take(&mut entry.value), base);
            }
        }
    }
    if let Some(path) = body.file_path.take() {
        body.file_path = Some(crate::client_certificates::absolutize(path, base));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_certificates::describe_all;
    use async_trait::async_trait;
    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, FolderSettings, Request as CollectionRequest,
    };
    use rocket_environment::{Environment, Variable};
    use rocket_http::{CertificateMaterial, CertificateSource};
    use rocket_http::{CookieJar, HttpResponse};
    use rocket_shared::certificate::ClientCertificate;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    // Fixed-response mock executor that records the last URL it received.
    struct MockExecutor {
        last_url: Mutex<Option<String>>,
        response: HttpResponse,
    }

    impl MockExecutor {
        fn new(status: u16) -> Self {
            Self {
                last_url: Mutex::new(None),
                response: HttpResponse {
                    status,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 50,
                    ttfb_ms: 50,
                    size_bytes: 2,
                    ..Default::default()
                },
            }
        }
    }

    #[async_trait]
    impl HttpExecutor for MockExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.last_url.lock().unwrap() = Some(req.url.clone());
            Ok(self.response.clone())
        }
    }

    /// No connections configured — every lookup misses. Used by every test in
    /// this file that doesn't exercise RocketVault resolution itself.
    struct EmptySecretManagerRepo;

    impl rocket_environment::SecretManagerRepository for EmptySecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
            Ok(vec![])
        }
        fn get(
            &self,
            _id: &str,
        ) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
            Ok(None)
        }
        fn save(
            &self,
            _connection: &rocket_environment::SecretManagerConnection,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _id: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    // Mock environment repo with one pre-loaded environment.
    struct MockEnvRepo {
        env: Option<Environment>,
    }

    impl MockEnvRepo {
        fn with_env(env: Environment) -> Self {
            Self { env: Some(env) }
        }
        fn empty() -> Self {
            Self { env: None }
        }
    }

    impl rocket_environment::EnvironmentRepository for MockEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.env.iter().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.env
                .as_ref()
                .filter(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// Returns the same pre-loaded `MockEnvRepo` for any collection name — used
    /// to prove `resolve_external_secrets` routes through
    /// `regular_env_repo(collection)` and not `self.env_repo` directly.
    struct SingleCollectionEnvRepoFactory {
        env: Environment,
    }

    impl rocket_environment::EnvironmentRepositoryFactory for SingleCollectionEnvRepoFactory {
        fn for_collection(
            &self,
            _collection: &str,
        ) -> Box<dyn rocket_environment::EnvironmentRepository> {
            Box::new(MockEnvRepo::with_env(self.env.clone()))
        }
    }

    /// Like `SingleCollectionEnvRepoFactory`, but it also knows where the collection lives.
    struct DirEnvRepoFactory {
        env: Environment,
        dir: std::path::PathBuf,
    }

    impl rocket_environment::EnvironmentRepositoryFactory for DirEnvRepoFactory {
        fn for_collection(
            &self,
            _collection: &str,
        ) -> Box<dyn rocket_environment::EnvironmentRepository> {
            Box::new(MockEnvRepo::with_env(self.env.clone()))
        }

        fn collection_dir(&self, collection: &str) -> Option<std::path::PathBuf> {
            Some(self.dir.join(collection))
        }
    }

    // In-memory history repo. `entries` is wrapped in an `Arc` so tests can
    // pull out a handle to it (`saved_entries_handle`) before the repo is
    // boxed and moved into `RequestExecutionService::new` as a trait object.
    struct MockHistoryRepo {
        entries: Arc<Mutex<Vec<HistoryEntry>>>,
    }

    impl MockHistoryRepo {
        fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Clones out the shared handle so a test can inspect saved entries
        /// after this repo has been boxed and handed to the service.
        fn saved_entries_handle(&self) -> Arc<Mutex<Vec<HistoryEntry>>> {
            Arc::clone(&self.entries)
        }
    }

    impl HistoryRepository for MockHistoryRepo {
        fn list(&self, _: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            self.entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(id.into()))
        }
        fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            self.entries.lock().unwrap().clear();
            Ok(())
        }
        fn search(&self, _: &rocket_history::HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
    }

    // No-op cookie repo.
    struct NullCookieRepo;

    impl CookieRepository for NullCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(vec![])
        }
        fn get_by_domain(&self, _: &str) -> DomainResult<Option<CookieJar>> {
            Ok(None)
        }
        fn save(&self, _: &CookieJar) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    // Collection repo with configurable per-collection settings, folder, and request variables.
    struct StubCollectionRepo {
        settings: CollectionSettings,
        folder_vars: Vec<CollectionVariable>,
        request_vars: Vec<CollectionVariable>,
        root: Option<std::path::PathBuf>,
        folder_chain: Vec<FolderSettings>,
        folder_chain_error: Option<String>,
    }

    impl StubCollectionRepo {
        fn empty() -> Self {
            Self {
                settings: CollectionSettings::default(),
                folder_vars: vec![],
                request_vars: vec![],
                root: None,
                folder_chain: vec![],
                folder_chain_error: None,
            }
        }

        fn with_settings(settings: CollectionSettings) -> Self {
            Self {
                settings,
                folder_vars: vec![],
                request_vars: vec![],
                root: None,
                folder_chain: vec![],
                folder_chain_error: None,
            }
        }

        fn with_root(mut self, root: &str) -> Self {
            self.root = Some(root.into());
            self
        }

        fn with_folder_vars(mut self, vars: Vec<CollectionVariable>) -> Self {
            self.folder_vars = vars;
            self
        }

        fn with_request_vars(mut self, vars: Vec<CollectionVariable>) -> Self {
            self.request_vars = vars;
            self
        }

        /// Every request path gets `chain` as its folder chain, outermost first.
        fn with_folder_chain(mut self, chain: Vec<FolderSettings>) -> Self {
            self.folder_chain = chain;
            self
        }

        /// Loading the folder chain fails with `message`, like a broken folder.yml.
        fn with_folder_chain_error(mut self, message: &str) -> Self {
            self.folder_chain_error = Some(message.into());
            self
        }
    }

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn get_summaries(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.clone())
        }
        fn collection_root_path(&self, _: &str) -> DomainResult<std::path::PathBuf> {
            self.root
                .clone()
                .ok_or_else(|| DomainError::NotFound("no root".into()))
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(self.folder_vars.clone())
        }
        fn get_folder_chain_settings(&self, _: &str, _: &str) -> DomainResult<Vec<FolderSettings>> {
            match &self.folder_chain_error {
                Some(message) => Err(DomainError::InvalidInput(message.clone())),
                None => Ok(self.folder_chain.clone()),
            }
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(self.request_vars.clone())
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn sample_input(url: &str, env_name: Option<&str>) -> ExecuteRequestInput {
        ExecuteRequestInput {
            skip_history: false,
            flow_vars: std::collections::HashMap::new(),
            skip_folder_scripts: false,
            method: HttpMethod::Get,
            url: url.to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: rocket_shared::types::Auth::None,
            options: RequestOptions::default(),
            environment_name: env_name.map(str::to_string),
            collection: None,
            request_name: None,
            pre_request_script: None,
            post_response_script: None,
            tests_script: None,
            request_path: None,
            global_env_name: None,
            assertions: vec![],
            tags: vec![],
            path_params: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }

    fn test_connection(id: &str) -> rocket_environment::SecretManagerConnection {
        rocket_environment::SecretManagerConnection {
            id: id.to_string(),
            label: "Test".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: Default::default(),
            config: None,
        }
    }

    /// Configurable connection registry for `resolve_external_secrets` tests.
    struct FakeSecretManagerRepo {
        connections: Vec<rocket_environment::SecretManagerConnection>,
    }

    impl FakeSecretManagerRepo {
        fn with_connection(conn: rocket_environment::SecretManagerConnection) -> Self {
            Self {
                connections: vec![conn],
            }
        }
    }

    impl rocket_environment::SecretManagerRepository for FakeSecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
            Ok(self.connections.clone())
        }
        fn get(
            &self,
            id: &str,
        ) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
            Ok(self.connections.iter().find(|c| c.id == id).cloned())
        }
        fn save(
            &self,
            connection: &rocket_environment::SecretManagerConnection,
        ) -> DomainResult<()> {
            let _ = connection;
            Ok(())
        }
        fn delete(&self, _id: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// Always hands back a fixed client secret — the actual value never matters
    /// to these tests, only that the lookup succeeds.
    struct FakeSecretStore;

    impl rocket_environment::SecretStore for FakeSecretStore {
        fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
            Ok(Some("test-client-secret".to_string()))
        }
        fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// Per-secret-id scripted outcome for `FakeVaultFetcher::get_secret_value`.
    #[derive(Clone)]
    enum FakeSecretOutcome {
        Value(String),
        Missing,       // get_secret_value -> Ok(None): deleted on the RocketVault side.
        Error(String), // get_secret_value -> Err(DomainError::Internal(..)).
    }

    /// Records every secret_id it was asked to resolve, in call order, so tests
    /// can assert both the returned value and how many/which calls were made.
    struct FakeVaultFetcher {
        responses: std::collections::HashMap<String, FakeSecretOutcome>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeVaultFetcher {
        fn new(responses: Vec<(&str, FakeSecretOutcome)>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl rocket_environment::VaultSecretFetcher for FakeVaultFetcher {
        async fn list_secrets(
            &self,
            _connection: &rocket_environment::SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<rocket_environment::ExternalSecretRef>> {
            Ok(vec![])
        }

        async fn get_secret_value(
            &self,
            _connection: &rocket_environment::SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            secret_id: &str,
        ) -> DomainResult<Option<String>> {
            self.calls
                .lock()
                .expect("lock FakeVaultFetcher calls")
                .push(secret_id.to_string());
            match self.responses.get(secret_id) {
                Some(FakeSecretOutcome::Value(v)) => Ok(Some(v.clone())),
                Some(FakeSecretOutcome::Missing) | None => Ok(None),
                Some(FakeSecretOutcome::Error(msg)) => Err(DomainError::Internal(msg.clone())),
            }
        }

        async fn test_connection(
            &self,
            _connection: &rocket_environment::SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn binding_with_refs(
        alias: &str,
        refs: Vec<(&str, &str)>, // (name, secret_id)
    ) -> rocket_environment::ExternalSecretBinding {
        rocket_environment::ExternalSecretBinding {
            alias: alias.to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: refs
                .into_iter()
                .map(|(name, secret_id)| rocket_environment::ExternalSecretRef {
                    name: name.to_string(),
                    secret_id: secret_id.to_string(),
                })
                .collect(),
        }
    }

    fn svc_with_vault(
        env: Option<Environment>,
        fetcher: Arc<FakeVaultFetcher>,
    ) -> RequestExecutionService {
        let env_repo: Box<dyn rocket_environment::EnvironmentRepository> = match env {
            Some(e) => Box::new(MockEnvRepo::with_env(e)),
            None => Box::new(MockEnvRepo::empty()),
        };
        RequestExecutionService::new(
            env_repo,
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        )
    }

    #[tokio::test]
    async fn resolve_external_secrets_returns_empty_map_when_no_environment_given() {
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![]));
        let svc = svc_with_vault(None, Arc::clone(&fetcher));

        let result = svc
            .resolve_external_secrets(None, None)
            .await
            .expect("resolve_external_secrets");

        assert!(result.is_empty());
        assert!(
            fetcher.calls.lock().expect("lock calls").is_empty(),
            "no environment name means no lookups at all, not even a miss"
        );
    }

    #[tokio::test]
    async fn resolve_external_secrets_returns_empty_map_for_environment_with_no_bindings() {
        let env = Environment::new("prod");
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![]));
        let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

        let result = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect("resolve_external_secrets");

        assert!(result.is_empty());
        assert!(fetcher.calls.lock().expect("lock calls").is_empty());
    }

    #[tokio::test]
    async fn resolve_external_secrets_resolves_all_secret_names_in_one_binding() {
        let mut env = Environment::new("prod");
        env.external_secrets.push(binding_with_refs(
            "payments",
            vec![("apiKey", "sec-1"), ("webhookSecret", "sec-2")],
        ));
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![
            ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
            ("sec-2", FakeSecretOutcome::Value("whsec-abc".to_string())),
        ]));
        let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

        let result = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect("resolve_external_secrets");

        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get("payments.apiKey"),
            Some(&"sk-live-key".to_string())
        );
        assert_eq!(
            result.get("payments.webhookSecret"),
            Some(&"whsec-abc".to_string())
        );
    }

    #[tokio::test]
    async fn resolve_external_secrets_hard_fails_on_a_fetcher_error() {
        let mut env = Environment::new("prod");
        env.external_secrets.push(binding_with_refs(
            "payments",
            vec![("apiKey", "sec-1"), ("webhookSecret", "sec-2")],
        ));
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![
            ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
            (
                "sec-2",
                FakeSecretOutcome::Error("vault unreachable".to_string()),
            ),
        ]));
        let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

        let err = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect_err("a fetcher error on one ref must fail the whole call");

        assert!(matches!(err, DomainError::Internal(_)));
    }

    #[tokio::test]
    async fn resolve_external_secrets_skips_a_deleted_secret_silently() {
        let mut env = Environment::new("prod");
        env.external_secrets.push(binding_with_refs(
            "payments",
            vec![
                ("apiKey", "sec-1"),
                ("deletedOnVaultSide", "sec-2"),
                ("webhookSecret", "sec-3"),
            ],
        ));
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![
            ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
            ("sec-2", FakeSecretOutcome::Missing),
            ("sec-3", FakeSecretOutcome::Value("whsec-abc".to_string())),
        ]));
        let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

        let result = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect("resolve_external_secrets");

        assert_eq!(
            result.len(),
            2,
            "the deleted secret must be silently omitted, not errored"
        );
        assert!(!result.contains_key("payments.deletedOnVaultSide"));
        assert_eq!(
            result.get("payments.apiKey"),
            Some(&"sk-live-key".to_string())
        );
        assert_eq!(
            result.get("payments.webhookSecret"),
            Some(&"whsec-abc".to_string())
        );
    }

    /// Counts calls instead of just recording the last one, so the failure-path
    /// test below can assert the executor was never reached at all.
    struct CallCountingExecutor {
        calls: Mutex<usize>,
        response: HttpResponse,
    }

    impl CallCountingExecutor {
        fn new(status: u16) -> Self {
            Self {
                calls: Mutex::new(0),
                response: HttpResponse {
                    status,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 1,
                    ttfb_ms: 1,
                    size_bytes: 2,
                    ..Default::default()
                },
            }
        }

        fn call_count(&self) -> usize {
            *self.calls.lock().expect("lock CallCountingExecutor")
        }
    }

    #[async_trait]
    impl HttpExecutor for CallCountingExecutor {
        async fn execute(&self, _req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.calls.lock().expect("lock CallCountingExecutor") += 1;
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn execute_resolves_external_secret_before_dispatch() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("sk-live-test-value".to_string()),
        )]));

        let executor = Arc::new(MockExecutor::new(200));
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let out = svc
            .execute(sample_input(
                "https://api.example.com/{{payments.apiKey}}",
                Some("prod"),
            ))
            .await
            .expect("execute");

        assert_eq!(out.response.status, 200);
        let url = exec_arc
            .last_url
            .lock()
            .expect("lock last_url")
            .clone()
            .expect("executor was called");
        assert_eq!(url, "https://api.example.com/sk-live-test-value");
    }

    #[tokio::test]
    async fn execute_resolves_external_secrets_through_the_collection_scoped_env_repo_not_the_global_one(
    ) {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("sk-live-test-value".to_string()),
        )]));

        let executor = Arc::new(MockExecutor::new(200));
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            // Top-level/global env_repo is deliberately EMPTY — if the code under
            // test regresses to reading self.env_repo directly instead of
            // regular_env_repo(collection), this environment lookup will miss and
            // the test will fail (either as an Err, if the soft-fail regresses
            // too, or as an unresolved `{{payments.apiKey}}` literal reaching the
            // dispatched URL).
            Box::new(MockEnvRepo::empty()),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        )
        .with_collection_env_repo_factory(Box::new(SingleCollectionEnvRepoFactory { env }));

        let mut input = sample_input("https://api.example.com/{{payments.apiKey}}", Some("prod"));
        input.collection = Some("my-collection".to_string());

        let out = svc
            .execute(input)
            .await
            .expect("execute must succeed by routing through the collection-scoped env repo");

        assert_eq!(out.response.status, 200);
        let url = exec_arc
            .last_url
            .lock()
            .expect("lock last_url")
            .clone()
            .expect("executor was called");
        assert_eq!(url, "https://api.example.com/sk-live-test-value");
    }

    #[tokio::test]
    async fn execute_fails_before_dispatch_when_external_secret_fetch_errors() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Error("vault unreachable".to_string()),
        )]));

        let executor = Arc::new(CallCountingExecutor::new(200));
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let result = svc
            .execute(sample_input(
                "https://api.example.com/{{payments.apiKey}}",
                Some("prod"),
            ))
            .await;

        assert!(
            result.is_err(),
            "execute() must fail when external-secret resolution fails"
        );
        assert_eq!(
            exec_arc.call_count(),
            0,
            "HttpExecutor::execute must never run when resolve_external_secrets errors — \
             this is the 'never a silent empty-string substitution' / hard-stop-before-dispatch \
             requirement from spec §2/§4.6"
        );
    }

    #[tokio::test]
    async fn execute_sends_when_the_failing_binding_is_not_referenced() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Error("vault rejected the credentials".to_string()),
        )]));

        let executor = Arc::new(CallCountingExecutor::new(200));
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let result = svc
            .execute(sample_input("https://api.example.com/health", Some("prod")))
            .await;

        assert!(
            result.is_ok(),
            "a request that never mentions the failing binding must still send"
        );
        assert_eq!(exec_arc.call_count(), 1);
    }

    #[tokio::test]
    async fn execute_ignores_a_commented_out_secret_reference_in_a_script() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));
        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Error("vault rejected the credentials".to_string()),
        )]));
        let build = |fetcher: Arc<FakeVaultFetcher>| {
            let executor = Arc::new(CallCountingExecutor::new(200));
            let counter = Arc::clone(&executor);
            let svc = RequestExecutionService::new(
                Box::new(MockEnvRepo::with_env(env.clone())),
                executor,
                Box::new(MockHistoryRepo::new()),
                Box::new(StubCollectionRepo::empty()),
                Box::new(NullCookieRepo),
                Box::new(NullEventPublisher),
                Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                    "conn-1",
                ))),
                Arc::new(FakeSecretStore),
                fetcher,
            );
            (svc, counter)
        };

        let (svc, counter) = build(Arc::clone(&fetcher));
        let mut commented = sample_input("https://api.example.com/health", Some("prod"));
        commented.post_response_script =
            Some("  // rok.getSecretVar(\"payments.apiKey\")\nconsole.log(1)".to_string());
        assert!(svc.execute(commented).await.is_ok());
        assert_eq!(counter.call_count(), 1);

        let (svc, counter) = build(fetcher);
        let mut live = sample_input("https://api.example.com/health", Some("prod"));
        live.post_response_script = Some("rok.getSecretVar(\"payments.apiKey\")".to_string());
        assert!(svc.execute(live).await.is_err());
        assert_eq!(counter.call_count(), 0);
    }

    #[tokio::test]
    async fn resolve_external_secrets_skips_a_binding_whose_connection_was_deleted() {
        // "conn-1" is the connection_id `binding_with_refs` bakes into every
        // binding it builds, but this test's registry is EmptySecretManagerRepo
        // — no connections at all — modeling a connection that was deleted from
        // Settings after the binding was saved (a fully supported action with
        // no cross-check against existing environment bindings).
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("sk-live-test-value".to_string()),
        )]));

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let result = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect(
                "a binding whose connection was deleted must soft-fail that binding, \
                 not abort the whole call",
            );
        assert!(
            result.is_empty(),
            "the binding pointing at a deleted connection must contribute no secrets, got: {result:?}"
        );
        assert!(
            !result.contains_key("payments.apiKey"),
            "the deleted-connection binding's secret must be absent, not present"
        );
    }

    #[tokio::test]
    async fn execute_succeeds_and_still_dispatches_when_a_binding_connection_was_deleted() {
        // Same missing-connection setup as above, but exercised through the
        // full execute() path with a request that doesn't even reference the
        // affected binding's secret — pinning that a deleted connection on an
        // unrelated binding must not abort dispatch of requests that never
        // touch it.
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("sk-live-test-value".to_string()),
        )]));

        let executor = Arc::new(CallCountingExecutor::new(200));
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let out = svc
            .execute(sample_input("https://api.example.com/status", Some("prod")))
            .await
            .expect(
                "execute must succeed even though one binding's connection was deleted from Settings",
            );

        assert_eq!(out.response.status, 200);
        assert_eq!(
            exec_arc.call_count(),
            1,
            "HttpExecutor::execute must still run — a deleted-connection binding must not \
             abort dispatch of a request that never references its secrets"
        );
    }

    #[tokio::test]
    async fn history_entry_redacts_external_secret_value_from_the_url() {
        let mut env = Environment::new("prod");
        env.external_secrets
            .push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("sk-live-test-value".to_string()),
        )]));

        let history_repo = Box::new(MockHistoryRepo::new());
        let history_arc = history_repo.saved_entries_handle();

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            history_repo,
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        svc.execute(sample_input(
            "https://api.example.com/{{payments.apiKey}}",
            Some("prod"),
        ))
        .await
        .expect("execute");

        let saved = history_arc.lock().expect("lock saved entries");
        assert_eq!(saved.len(), 1);
        assert!(
            !saved[0].url.contains("sk-live-test-value"),
            "history entry must not contain the resolved secret value, got: {}",
            saved[0].url
        );
        assert!(
            saved[0].url.contains("••••••"),
            "expected the redaction marker in place of the secret, got: {}",
            saved[0].url
        );
    }

    #[tokio::test]
    async fn service_run_load_test_resolves_variables_before_firing() {
        let mut env = Environment::new("staging");
        env.set_variable(Variable::new("oidc-baseurl", "https://auth.local"));

        let executor = Arc::new(MockExecutor::new(200));

        struct SharedExecLt(Arc<MockExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExecLt {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }

        let exec_arc = Arc::clone(&executor);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(SharedExecLt(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("{{oidc-baseurl}}/api/data", Some("staging"));
        input.collection = None;

        let config = rocket_http::LoadTestConfig {
            concurrency: 1,
            total_requests: 1,
            interval_ms: 0,
            duration_cap_secs: None,
        };
        let result = svc.run_load_test(input, config).await.unwrap();

        assert_eq!(result.total_requests, 1);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);

        // Verify the resolved URL reached the executor.
        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://auth.local/api/data");
    }

    #[tokio::test]
    async fn run_load_test_turns_the_cookie_jar_off() {
        struct JarFlagExecutor(Mutex<Vec<bool>>);
        #[async_trait]
        impl HttpExecutor for JarFlagExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0
                    .lock()
                    .expect("lock")
                    .push(req.options.use_cookie_jar);
                Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 1,
                    ttfb_ms: 1,
                    size_bytes: 2,
                    ..Default::default()
                })
            }
        }
        struct Shared(Arc<JarFlagExecutor>);
        #[async_trait]
        impl HttpExecutor for Shared {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }
        let flags = Arc::new(JarFlagExecutor(Mutex::new(Vec::new())));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(Shared(Arc::clone(&flags))),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let config = rocket_http::LoadTestConfig {
            concurrency: 1,
            total_requests: 2,
            interval_ms: 0,
            duration_cap_secs: None,
        };
        svc.run_load_test(sample_input("https://h.test/x", None), config)
            .await
            .expect("load test");
        let seen = flags.0.lock().expect("lock").clone();
        assert!(!seen.is_empty());
        assert!(
            seen.iter().all(|jar_on| !jar_on),
            "load test requests must not use the jar"
        );
    }

    #[tokio::test]
    async fn resolve_request_handles_hyphenated_variable_names() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("oidc-baseurl", "https://auth.local"));

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let input = sample_input("{{oidc-baseurl}}/api/v1/users", Some("dev"));
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.url, "https://auth.local/api/v1/users");
    }

    #[tokio::test]
    async fn resolve_request_resolves_placeholders_in_query_params() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("token", "abc 123"));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input("https://api.example.com/items", Some("dev"));
        input.query_params = vec![
            QueryParam {
                key: "{{token}}-k".into(),
                value: "{{token}}".into(),
                enabled: true,
                description: None,
            },
            QueryParam {
                key: "off".into(),
                value: "{{token}}".into(),
                enabled: false,
                description: None,
            },
        ];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.query_params[0].key, "abc 123-k");
        assert_eq!(resolved.query_params[0].value, "abc 123");
        assert!(resolved.query_params[0].enabled);
        assert!(
            !resolved.query_params[1].enabled,
            "enabled must be preserved"
        );
        assert_eq!(resolved.query_params[1].value, "abc 123");
    }

    #[tokio::test]
    async fn resolve_request_substitutes_path_params_with_resolved_values() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("userId", "42"));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input(
            "https://api.example.com/users/:id/orders/:id/{kind}",
            Some("dev"),
        );
        input.path_params = vec![
            rocket_shared::types::PathParam {
                name: "id".into(),
                value: "{{userId}}".into(),
                description: None,
            },
            rocket_shared::types::PathParam {
                name: "kind".into(),
                value: "a b".into(),
                description: None,
            },
        ];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(
            resolved.url,
            "https://api.example.com/users/42/orders/42/a%20b"
        );
    }

    #[tokio::test]
    async fn resolve_request_keeps_a_path_param_placeholder_when_the_value_is_empty() {
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input("https://api.example.com/users/:id", None);
        input.path_params = vec![rocket_shared::types::PathParam {
            name: "id".into(),
            value: String::new(),
            description: None,
        }];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.url, "https://api.example.com/users/:id");
    }

    #[tokio::test]
    async fn resolve_request_carries_the_environment_client_certificates() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("certDir", "/certs"));
        env.set_variable(Variable::new("p12Pass", "s3cret"));
        env.client_certificates = vec![ClientCertificate::Pkcs12 {
            domain: "api.example.com".into(),
            pkcs12_file_path: "{{certDir}}/client.p12".into(),
            pkcs12_secret: None,
            passphrase: Some("{{p12Pass}}".into()),
        }];

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        // Certificates sent by the caller are ignored: the environment is the only source.
        let mut input = sample_input("https://api.example.com/x", Some("dev"));
        input.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "evil.example.com",
            CertificateSource::File("/etc/shadow".into()),
            None,
        )];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");

        assert_eq!(
            describe_all(&resolved.options.client_certificates),
            ["pkcs12 api.example.com file:/certs/client.p12 pass:s3cret"]
        );
    }

    fn relative_path_env() -> Environment {
        let mut env = Environment::new("dev");
        env.client_certificates = vec![
            ClientCertificate::Pem {
                domain: "a.example.com".into(),
                certificate_file_path: "certs/client.pem".into(),
                private_key_file_path: "./certs/client-key.pem".into(),
                certificate_secret: None,
                private_key_secret: None,
                passphrase: None,
            },
            ClientCertificate::Pkcs12 {
                domain: "b.example.com".into(),
                pkcs12_file_path: "../outside.p12".into(),
                pkcs12_secret: None,
                passphrase: None,
            },
            ClientCertificate::Pkcs12 {
                domain: "c.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: None,
                passphrase: None,
            },
            ClientCertificate::Pkcs12 {
                domain: "d.example.com".into(),
                pkcs12_file_path: "~/client.p12".into(),
                pkcs12_secret: None,
                passphrase: None,
            },
        ];
        env
    }

    fn cert_paths(certs: &[ResolvedClientCertificate]) -> Vec<String> {
        let path = |s: &CertificateSource| match s {
            CertificateSource::File(p) => p.clone(),
            CertificateSource::Inline(_) => "<inline>".to_string(),
        };
        certs
            .iter()
            .flat_map(|c| match &c.material {
                CertificateMaterial::Pem {
                    certificate,
                    private_key,
                    ..
                } => vec![path(certificate), path(private_key)],
                CertificateMaterial::Pkcs12 { bundle, .. } => vec![path(bundle)],
                CertificateMaterial::Unavailable { .. } => Vec::new(),
                CertificateMaterial::Deferred { .. } => Vec::new(),
            })
            .collect()
    }

    fn service_with(
        env: Environment,
        factory: Option<DirEnvRepoFactory>,
    ) -> RequestExecutionService {
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        match factory {
            Some(f) => svc.with_collection_env_repo_factory(Box::new(f)),
            None => svc,
        }
    }

    #[tokio::test]
    async fn relative_certificate_paths_resolve_against_the_collection_folder() {
        let env = relative_path_env();
        let svc = service_with(
            env.clone(),
            Some(DirEnvRepoFactory {
                env,
                dir: std::path::PathBuf::from("/ws/collections"),
            }),
        );
        let mut input = sample_input("https://a.example.com/x", Some("dev"));
        input.collection = Some("api".into());
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");

        assert_eq!(
            cert_paths(&resolved.options.client_certificates),
            [
                "/ws/collections/api/certs/client.pem",
                "/ws/collections/api/certs/client-key.pem",
                // A `..` is left as written, so the executor rejects it.
                "../outside.p12",
                "/abs/client.p12",
                "~/client.p12",
            ]
        );
    }

    #[tokio::test]
    async fn relative_certificate_paths_stay_as_written_without_a_known_collection_folder() {
        let env = relative_path_env();
        let svc = service_with(env, None);
        let mut input = sample_input("https://a.example.com/x", Some("dev"));
        input.collection = Some("api".into());
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");

        assert_eq!(
            cert_paths(&resolved.options.client_certificates)[0],
            "certs/client.pem"
        );
    }

    fn upload_body() -> rocket_shared::types::Body {
        use rocket_shared::types::{Body, BodyMode, FormDataEntry, FormDataType};
        let file = |key: &str, value: &str, entry_type: FormDataType| FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type,
            enabled: true,
            content_type: None,
            description: None,
        };
        Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![
                file("a", "files/a.txt", FormDataType::File),
                file("b", "./files/b.txt", FormDataType::File),
                file("c", "../outside.txt", FormDataType::File),
                file("d", "/abs/d.txt", FormDataType::File),
                file("e", "files/not-a-file", FormDataType::Text),
            ]),
            file_path: Some("files/blob.bin".into()),
        }
    }

    #[tokio::test]
    async fn relative_upload_paths_resolve_against_the_collection_folder() {
        let env = relative_path_env();
        let svc = service_with(
            env.clone(),
            Some(DirEnvRepoFactory {
                env,
                dir: std::path::PathBuf::from("/ws/collections"),
            }),
        );
        let mut input = sample_input("https://a.example.com/x", Some("dev"));
        input.collection = Some("api".into());
        input.body = Some(upload_body());
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        let body = resolved.body.expect("body");
        let values: Vec<String> = body
            .form_data
            .expect("entries")
            .into_iter()
            .map(|e| e.value)
            .collect();
        assert_eq!(
            values,
            [
                "/ws/collections/api/files/a.txt",
                "/ws/collections/api/files/b.txt",
                // A `..` is left as written, so the executor rejects it.
                "../outside.txt",
                "/abs/d.txt",
                // A text value is never a path.
                "files/not-a-file",
            ]
        );
        assert_eq!(
            body.file_path.as_deref(),
            Some("/ws/collections/api/files/blob.bin")
        );
    }

    #[tokio::test]
    async fn relative_upload_paths_stay_as_written_without_a_known_collection_folder() {
        let svc = service_with(relative_path_env(), None);
        let mut input = sample_input("https://a.example.com/x", Some("dev"));
        input.body = Some(upload_body());
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        let body = resolved.body.expect("body");
        assert_eq!(body.form_data.expect("entries")[0].value, "files/a.txt");
        assert_eq!(body.file_path.as_deref(), Some("files/blob.bin"));
    }

    #[tokio::test]
    async fn resolve_request_has_no_client_certificates_without_an_environment() {
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("dev"))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let input = sample_input("https://api.example.com/x", None);
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert!(resolved.options.client_certificates.is_empty());
    }

    #[tokio::test]
    async fn a_vault_sourced_certificate_without_a_fetched_secret_is_unavailable() {
        let mut env = Environment::new("dev");
        env.client_certificates = vec![ClientCertificate::Pkcs12 {
            domain: "api.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: None,
        }];
        let svc = service_with(env, None);
        let input = sample_input("https://api.example.com/x", Some("dev"));
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(
            describe_all(&resolved.options.client_certificates),
            [
                "unavailable api.example.com Client certificate secret vault.clientBundleB64 \
              was not found. Check the External Secrets binding and fetch the secret names."
            ]
        );
    }

    /// Tests for vault-backed client certificate material (Plan C). The helpers are `pub(super)`
    /// so the hygiene module of Task C2 can reuse them.
    mod vault_certificates {
        use super::*;
        use base64::Engine as _;
        use rocket_http::{CertificateMaterial, CertificateSource, ResolvedClientCertificate};

        pub(super) const CERT_PEM: &str =
            "-----BEGIN CERTIFICATE-----\r\nMIIBcertbody0123\r\n-----END CERTIFICATE-----\r\n";
        pub(super) const KEY_PEM: &str =
            "-----BEGIN PRIVATE KEY-----\nMIIEkeybody0123\n-----END PRIVATE KEY-----\n";

        pub(super) fn vault_pem(
            domain: &str,
            cert_ref: &str,
            key_ref: &str,
            passphrase: Option<&str>,
        ) -> ClientCertificate {
            ClientCertificate::Pem {
                domain: domain.into(),
                certificate_file_path: String::new(),
                private_key_file_path: String::new(),
                certificate_secret: Some(cert_ref.into()),
                private_key_secret: Some(key_ref.into()),
                passphrase: passphrase.map(String::from),
            }
        }

        pub(super) fn vault_p12(domain: &str, reference: &str) -> ClientCertificate {
            ClientCertificate::Pkcs12 {
                domain: domain.into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            }
        }

        pub(super) fn secrets(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }

        /// Resolves `certs` through `resolve_request` for an environment named `dev`.
        pub(super) fn resolve_certificates(
            certs: Vec<ClientCertificate>,
            secrets: &std::collections::HashMap<String, String>,
        ) -> Vec<ResolvedClientCertificate> {
            let mut env = Environment::new("dev");
            env.client_certificates = certs;
            let svc = service_with(env, None);
            svc.resolve_request(
                &sample_input("https://a.example.com/x", Some("dev")),
                secrets,
            )
            .expect("resolve_request")
            .options
            .client_certificates
        }

        pub(super) fn source_bytes(source: &CertificateSource) -> Vec<u8> {
            match source {
                CertificateSource::Inline(bytes) => bytes.to_vec(),
                CertificateSource::File(path) => {
                    panic!("expected inline material, got file {path}")
                }
            }
        }

        pub(super) fn unavailable_reason(cert: &ResolvedClientCertificate) -> String {
            match &cert.material {
                CertificateMaterial::Unavailable { reason } => reason.clone(),
                _ => panic!("expected an unavailable certificate for {}", cert.domain),
            }
        }

        #[tokio::test]
        async fn vault_certificates_pem_references_resolve_to_inline_bytes_unchanged() {
            let certs = resolve_certificates(
                vec![vault_pem(
                    "a.example.com",
                    "vault.certPem",
                    "vault.keyPem",
                    Some("{{vault.keyPass}}"),
                )],
                &secrets(&[
                    ("vault.certPem", CERT_PEM),
                    ("vault.keyPem", KEY_PEM),
                    ("vault.keyPass", "p4ss-word"),
                ]),
            );
            assert_eq!(certs.len(), 1);
            assert_eq!(certs[0].domain, "a.example.com");
            let CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } = &certs[0].material
            else {
                panic!("expected a PEM certificate");
            };
            // Byte for byte, including the CRLF line endings.
            assert_eq!(source_bytes(certificate), CERT_PEM.as_bytes());
            assert_eq!(source_bytes(private_key), KEY_PEM.as_bytes());
            assert_eq!(passphrase.as_ref().map(|p| p.as_str()), Some("p4ss-word"));
        }

        #[tokio::test]
        async fn vault_certificates_pkcs12_secret_is_base64_decoded() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "AQIDBAU=")]),
            );
            let CertificateMaterial::Pkcs12 { bundle, .. } = &certs[0].material else {
                panic!("expected a PKCS12 certificate");
            };
            assert_eq!(source_bytes(bundle), vec![1u8, 2, 3, 4, 5]);
        }

        // Review Focus item 3.
        #[tokio::test]
        async fn vault_certificates_wrapped_base64_pkcs12_secret_decodes() {
            let bundle: Vec<u8> = (0u8..=200).collect();
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bundle);
            // Wrapped at 64 columns with CRLF and an indent on every line, plus a trailing newline.
            let wrapped = format!(
                "{}\r\n",
                encoded
                    .as_bytes()
                    .chunks(64)
                    .map(|c| format!("  {}", std::str::from_utf8(c).expect("ascii")))
                    .collect::<Vec<_>>()
                    .join("\r\n")
            );
            assert!(wrapped.matches("\r\n").count() > 2, "the fixture must wrap");

            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", wrapped.as_str())]),
            );
            let CertificateMaterial::Pkcs12 { bundle: got, .. } = &certs[0].material else {
                panic!("expected a PKCS12 certificate");
            };
            assert_eq!(source_bytes(got), bundle);
        }

        #[tokio::test]
        async fn vault_certificates_bad_base64_is_unavailable_and_never_echoes_the_value() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "this is !!! not base64")]),
            );
            let reason = unavailable_reason(&certs[0]);
            assert_eq!(
                reason,
                "Client certificate secret vault.bundle is not valid base64."
            );
            assert!(!reason.contains("!!!"), "{reason}");
        }

        #[tokio::test]
        async fn vault_certificates_missing_secret_uses_the_spec_message() {
            let certs = resolve_certificates(
                vec![vault_pem(
                    "a.example.com",
                    "vault.clientCertPem",
                    "vault.k",
                    None,
                )],
                &secrets(&[("vault.k", KEY_PEM)]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.clientCertPem was not found. \
                 Check the External Secrets binding and fetch the secret names."
            );
        }

        #[tokio::test]
        async fn vault_certificates_empty_secret_is_unavailable() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "  \r\n")]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.bundle is empty."
            );
        }

        // Review Focus item 1, at the resolution level.
        #[tokio::test]
        async fn vault_certificates_missing_secret_on_another_domain_leaves_the_selected_certificate_usable(
        ) {
            let certs = resolve_certificates(
                vec![
                    vault_p12("a.example.com", "vault.ok"),
                    vault_p12("b.example.com", "vault.missing"),
                ],
                &secrets(&[("vault.ok", "AQIDBAU=")]),
            );
            let first =
                rocket_http::client_cert::find_certificate(&certs, "https://a.example.com/x")
                    .expect("the first certificate is selected");
            assert!(matches!(
                first.material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
            let second =
                rocket_http::client_cert::find_certificate(&certs, "https://b.example.com/x")
                    .expect("the second certificate is selected");
            assert!(unavailable_reason(second).contains("vault.missing"));
            assert!(
                rocket_http::client_cert::find_certificate(&certs, "https://c.example.com/x")
                    .is_none()
            );
        }

        #[tokio::test]
        async fn vault_certificates_a_piece_needs_exactly_one_source() {
            let both = ClientCertificate::Pkcs12 {
                domain: "a.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: Some("vault.bundle".into()),
                passphrase: None,
            };
            let neither = ClientCertificate::Pkcs12 {
                domain: "b.example.com".into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: None,
                passphrase: None,
            };
            let certs = resolve_certificates(
                vec![both, neither],
                &secrets(&[("vault.bundle", "AQIDBAU=")]),
            );
            let both_reason = unavailable_reason(&certs[0]);
            assert!(
                both_reason.contains("a.example.com") && both_reason.contains("only one"),
                "{both_reason}"
            );
            let neither_reason = unavailable_reason(&certs[1]);
            assert!(
                neither_reason.contains("b.example.com") && neither_reason.contains("neither"),
                "{neither_reason}"
            );
        }

        // An empty or whitespace-only reference counts as absent, like in the save validator.
        #[tokio::test]
        async fn vault_certificates_blank_reference_counts_as_absent() {
            let blank = |reference: &str| ClientCertificate::Pkcs12 {
                domain: "a.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            };
            let no_source = |reference: &str| ClientCertificate::Pkcs12 {
                domain: "b.example.com".into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            };
            let certs = resolve_certificates(
                vec![blank(""), blank("  \t"), no_source(" ")],
                &secrets(&[]),
            );
            for cert in &certs[..2] {
                assert!(matches!(
                    &cert.material,
                    CertificateMaterial::Pkcs12 {
                        bundle: CertificateSource::File(p),
                        ..
                    } if p == "/abs/client.p12"
                ));
            }
            assert!(unavailable_reason(&certs[2]).contains("neither"));
        }

        #[tokio::test]
        async fn vault_certificates_file_paths_stay_file_sources() {
            let file = ClientCertificate::Pkcs12 {
                domain: "a.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: None,
                passphrase: Some("changeit".into()),
            };
            let certs = resolve_certificates(vec![file], &secrets(&[]));
            assert!(matches!(
                &certs[0].material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::File(p),
                    ..
                } if p == "/abs/client.p12"
            ));
        }
    }

    /// Hygiene tests for vault-backed certificate material (Plan C, Task C2).
    mod vault_certificate_hygiene {
        use super::vault_certificates::{
            resolve_certificates, secrets, source_bytes, unavailable_reason, vault_p12, vault_pem,
            CERT_PEM, KEY_PEM,
        };
        use super::*;
        use rocket_http::CertificateMaterial;

        #[tokio::test]
        async fn vault_certificate_hygiene_multi_line_values_are_masked_whole_and_by_line() {
            let svc = service_with(Environment::new("dev"), None);
            let values = svc.secret_values(
                None,
                None,
                Some("dev"),
                &secrets(&[("vault.keyPem", KEY_PEM)]),
            );
            assert!(values.contains(KEY_PEM), "the whole value");
            assert!(values.contains("MIIEkeybody0123"), "the body line");
            assert!(
                !values
                    .iter()
                    .any(|v| !v.contains('\n') && v.starts_with("-----")),
                "armor lines on their own are not secret"
            );
            assert_eq!(
                crate::redaction::redact_secrets("log: MIIEkeybody0123", &values),
                "log: ••••••"
            );
            assert_eq!(crate::redaction::redact_secrets(KEY_PEM, &values), "••••••");
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_short_secret_is_still_skipped() {
            let svc = service_with(Environment::new("dev"), None);
            let values =
                svc.secret_values(None, None, Some("dev"), &secrets(&[("vault.short", "abc")]));
            assert!(values.is_empty());
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_inline_material_over_the_cap_is_unavailable() {
            let cap = crate::client_certificates::MAX_INLINE_SECRET_BYTES;
            assert_eq!(cap, 1024 * 1024);

            let too_big = "A".repeat(cap + 1);
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", too_big.as_str())]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.bundle is larger than 1 MiB. \
                 Check that it holds a certificate and not another file."
            );

            // Exactly at the cap is accepted (base64 of zeros, so it decodes).
            let at_cap = "A".repeat(cap);
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", at_cap.as_str())]),
            );
            let CertificateMaterial::Pkcs12 { bundle, .. } = &certs[0].material else {
                panic!("a secret at the cap must resolve");
            };
            assert_eq!(source_bytes(bundle).len(), cap / 4 * 3);
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_resolve_request_keeps_key_text_out_of_debug_and_json() {
            let mut env = Environment::new("dev");
            env.client_certificates = vec![vault_pem(
                "api.example.com",
                "vault.certPem",
                "vault.keyPem",
                Some("{{vault.keyPass}}"),
            )];
            let svc = service_with(env, None);
            let request = svc
                .resolve_request(
                    &sample_input("https://api.example.com/x", Some("dev")),
                    &secrets(&[
                        ("vault.certPem", CERT_PEM),
                        ("vault.keyPem", KEY_PEM),
                        ("vault.keyPass", "p4ss-word"),
                    ]),
                )
                .expect("resolve_request");

            // The resolved request carries inline bytes.
            let CertificateMaterial::Pem {
                certificate,
                private_key,
                ..
            } = &request.options.client_certificates[0].material
            else {
                panic!("expected a PEM certificate");
            };
            assert_eq!(source_bytes(certificate), CERT_PEM.as_bytes());
            assert_eq!(source_bytes(private_key), KEY_PEM.as_bytes());

            // Nothing printable or serializable contains the PEM, its body or the passphrase.
            let needles = [
                "MIIEkeybody0123",
                "MIIBcertbody0123",
                "BEGIN PRIVATE KEY",
                "BEGIN CERTIFICATE",
                "p4ss-word",
                // Decimal bytes, as a derived `Debug` of `Vec<u8>` prints "MIIE" and "-----BEGIN".
                "77, 73, 73, 69",
                "45, 45, 45, 45, 45, 66, 69",
            ];
            let json = serde_json::to_string(&request).expect("serialize");
            for shown in [
                format!("{request:?}"),
                format!("{request:#?}"),
                json.clone(),
            ] {
                for needle in needles {
                    assert!(!shown.contains(needle), "{needle} leaked into {shown}");
                }
            }
            assert!(!json.contains("clientCertificates"), "{json}");
        }
    }

    #[tokio::test]
    async fn resolve_request_resolves_placeholders_in_inherited_collection_auth() {
        let mut env = Environment::new("local");
        env.set_variable(Variable::new("token", "jwt-from-env"));
        let settings = CollectionSettings {
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            ..Default::default()
        };
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::with_settings(settings)),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com/vaults", Some("local"));
        input.collection = Some("my-api".into());
        input.auth = Auth::Inherit;
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(
            resolved.auth,
            Auth::Bearer {
                token: "jwt-from-env".into()
            }
        );
    }

    #[tokio::test]
    async fn resolve_request_folds_in_external_secrets() {
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut external_secrets = std::collections::HashMap::new();
        external_secrets.insert("payments.apiKey".to_string(), "sk-live-key".to_string());

        let input = sample_input("https://api.example.com/{{payments.apiKey}}", None);
        let resolved = svc
            .resolve_request(&input, &external_secrets)
            .expect("resolve_request");

        assert_eq!(resolved.url, "https://api.example.com/sk-live-key");
    }

    /// Folder header and auth inheritance (folder settings plan 05).
    mod folder_inheritance {
        use super::*;

        fn folder_service(
            settings: CollectionSettings,
            repo: StubCollectionRepo,
        ) -> RequestExecutionService {
            let mut env = Environment::new("local");
            env.set_variable(Variable::new("token", "jwt-from-env"));
            let repo = StubCollectionRepo { settings, ..repo };
            RequestExecutionService::new(
                Box::new(MockEnvRepo::with_env(env)),
                Arc::new(MockExecutor::new(200)),
                Box::new(MockHistoryRepo::new()),
                Box::new(repo),
                Box::new(NullCookieRepo),
                Box::new(NullEventPublisher),
                Box::new(EmptySecretManagerRepo),
                Arc::new(rocket_environment::NullSecretStore),
                Arc::new(rocket_environment::NullVaultSecretFetcher),
            )
        }

        fn chain(folders: Vec<FolderSettings>) -> StubCollectionRepo {
            StubCollectionRepo::empty().with_folder_chain(folders)
        }

        fn folder(headers: Vec<Header>, auth: Option<Auth>) -> FolderSettings {
            FolderSettings {
                headers,
                auth,
                ..FolderSettings::default()
            }
        }

        fn input() -> ExecuteRequestInput {
            let mut input = sample_input("https://api.example.com/users", Some("local"));
            input.collection = Some("my-api".into());
            input.request_path = Some("users/admin/get.yml".into());
            input
        }

        fn resolve(svc: &RequestExecutionService, input: &ExecuteRequestInput) -> HttpRequest {
            svc.resolve_request(input, &std::collections::HashMap::new())
                .expect("resolve_request")
        }

        /// Values of the enabled headers named `key`, in send order.
        fn enabled_values(request: &HttpRequest, key: &str) -> Vec<String> {
            request
                .headers
                .iter()
                .filter(|h| h.enabled && h.key == key)
                .map(|h| h.value.clone())
                .collect()
        }

        fn bearer(token: &str) -> Auth {
            Auth::Bearer {
                token: token.into(),
            }
        }

        #[tokio::test]
        async fn folder_header_beats_collection_header_and_request_header_beats_folder() {
            let settings = CollectionSettings {
                headers: vec![
                    Header::new("X-Team", "core"),
                    Header::new("X-Env", "collection"),
                    Header::new("X-Trace", "collection"),
                ],
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![folder(
                    vec![
                        Header::new("X-Env", "folder"),
                        Header::new("X-Trace", "folder"),
                    ],
                    None,
                )]),
            );
            let mut input = input();
            input.headers = vec![Header::new("X-Trace", "request")];

            let resolved = resolve(&svc, &input);

            assert_eq!(enabled_values(&resolved, "X-Team"), vec!["core"]);
            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["folder"]);
            assert_eq!(enabled_values(&resolved, "X-Trace"), vec!["request"]);
        }

        #[tokio::test]
        async fn disabled_headers_never_shadow_an_inherited_header() {
            let settings = CollectionSettings {
                headers: vec![Header::new("X-Env", "collection")],
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![folder(
                    vec![
                        Header::disabled("X-Env", "folder-off"),
                        Header::new("X-Folder", "folder"),
                    ],
                    None,
                )]),
            );
            let mut input = input();
            input.headers = vec![Header::disabled("X-Folder", "request-off")];

            let resolved = resolve(&svc, &input);

            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["collection"]);
            assert_eq!(enabled_values(&resolved, "X-Folder"), vec!["folder"]);
        }

        #[tokio::test]
        async fn inner_folder_header_beats_outer_folder_header() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![
                    folder(
                        vec![
                            Header::new("X-Env", "outer"),
                            Header::new("X-Outer", "only"),
                        ],
                        None,
                    ),
                    folder(vec![Header::new("X-Env", "inner")], None),
                ]),
            );

            let resolved = resolve(&svc, &input());

            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["inner"]);
            assert_eq!(enabled_values(&resolved, "X-Outer"), vec!["only"]);
        }

        #[tokio::test]
        async fn inherit_takes_the_nearest_folder_auth() {
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let outer = Auth::Basic {
                username: "outer".into(),
                password: "secret".into(),
            };
            let svc = folder_service(
                settings,
                chain(vec![
                    folder(vec![], Some(outer)),
                    folder(vec![], Some(bearer("from-inner"))),
                ]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("from-inner"));
        }

        #[tokio::test]
        async fn inherit_skips_folders_without_auth_and_falls_back_to_the_collection() {
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![
                    folder(vec![], None),
                    folder(vec![], Some(Auth::Inherit)),
                    folder(vec![], Some(Auth::None)),
                ]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("from-collection"));
        }

        #[tokio::test]
        async fn an_explicit_request_auth_beats_folder_auth() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(vec![], Some(bearer("from-folder")))]),
            );
            let mut input = input();
            input.auth = bearer("from-request");

            assert_eq!(resolve(&svc, &input).auth, bearer("from-request"));
        }

        #[tokio::test]
        async fn folder_auth_placeholders_resolve_through_resolve_auth() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(vec![], Some(bearer("{{token}}")))]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("jwt-from-env"));
        }

        #[tokio::test]
        async fn oauth2_folder_auth_reaches_the_executor_unchanged() {
            use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
            let oauth = Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: OAuth2ClientCredentials {
                    client_id: "folder-client".into(),
                    client_secret: "folder-secret".into(),
                    placement: None,
                },
                scope: Some("read".into()),
                additional_parameters: None,
                token_config: None,
                settings: None,
            }));
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let svc = folder_service(settings, chain(vec![folder(vec![], Some(oauth.clone()))]));
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, oauth);
        }

        #[tokio::test]
        async fn a_failing_folder_chain_fails_the_request() {
            let svc = folder_service(
                CollectionSettings::default(),
                StubCollectionRepo::empty()
                    .with_folder_chain_error("folder.yml in 'users' is not valid YAML"),
            );

            let err = svc
                .resolve_request(&input(), &std::collections::HashMap::new())
                .expect_err("a broken folder.yml must fail the send");

            assert!(err.to_string().contains("users"), "{err}");
        }

        #[tokio::test]
        async fn without_folders_collection_headers_merge_exactly_as_before() {
            let settings = CollectionSettings {
                headers: vec![
                    Header::new("Accept", "application/json"),
                    Header::new("Accept", "text/plain"),
                    Header::disabled("X-Off", "collection-off"),
                ],
                ..Default::default()
            };
            let svc = folder_service(settings.clone(), StubCollectionRepo::empty());
            let mut input = input();
            input.headers = vec![Header::new("X-Trace", "request")];

            let resolved = resolve(&svc, &input);

            assert_eq!(
                resolved.headers,
                merge_headers(&settings.headers, &input.headers)
            );
        }

        #[tokio::test]
        async fn nested_folders_without_headers_keep_collection_headers_exactly_as_before() {
            let settings = CollectionSettings {
                headers: vec![
                    Header::new("Accept", "application/json"),
                    Header::new("Accept", "text/plain"),
                    Header::disabled("X-Off", "collection-off"),
                ],
                ..Default::default()
            };
            let svc = folder_service(
                settings.clone(),
                chain(vec![FolderSettings::default(), FolderSettings::default()]),
            );
            let mut input = input();
            input.headers = vec![
                Header::new("X-Trace", "request"),
                Header::disabled("Accept", "request-off"),
            ];

            let resolved = resolve(&svc, &input);

            assert_eq!(
                resolved.headers,
                merge_headers(&settings.headers, &input.headers)
            );
        }

        #[tokio::test]
        async fn an_inline_flow_request_inherits_no_folder_settings() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(
                    vec![Header::new("X-Env", "folder")],
                    Some(bearer("from-folder")),
                )]),
            );
            let mut input = input();
            input.request_path = Some(format!("{FLOW_INLINE_PATH_PREFIX}node-1"));
            input.auth = Auth::Inherit;

            let resolved = resolve(&svc, &input);

            assert!(enabled_values(&resolved, "X-Env").is_empty());
            // `merge_auth` turns `inherit` with no collection auth into `none`.
            assert_eq!(resolved.auth, Auth::None);
        }

        #[tokio::test]
        async fn folders_with_only_disabled_headers_keep_collection_headers_exactly_as_before() {
            let settings = CollectionSettings {
                headers: vec![
                    Header::new("Accept", "application/json"),
                    Header::new("Accept", "text/plain"),
                    Header::disabled("X-Off", "collection-off"),
                ],
                ..Default::default()
            };
            let svc = folder_service(
                settings.clone(),
                chain(vec![folder(
                    vec![Header::disabled("X-Folder", "folder-off")],
                    None,
                )]),
            );
            let mut input = input();
            input.headers = vec![Header::new("X-Trace", "request")];

            let resolved = resolve(&svc, &input);

            assert_eq!(
                resolved.headers,
                merge_headers(&settings.headers, &input.headers)
            );
        }

        #[tokio::test]
        async fn nested_folders_without_auth_resolve_auth_exactly_as_before() {
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let svc = folder_service(
                settings.clone(),
                chain(vec![FolderSettings::default(), FolderSettings::default()]),
            );
            for request_auth in [Auth::Inherit, Auth::None, bearer("from-request")] {
                let mut input = input();
                input.auth = request_auth.clone();

                assert_eq!(
                    resolve(&svc, &input).auth,
                    merge_auth(request_auth, settings.auth.clone())
                );
            }
        }
    }

    #[tokio::test]
    async fn execute_resolves_variables_in_url() {
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("BASE_URL", "https://api.example.com"));

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let out = svc
            .execute(sample_input("{{BASE_URL}}/users", Some("prod")))
            .await
            .expect("execute");
        assert_eq!(out.response.status, 200);
    }

    #[tokio::test]
    async fn execute_saves_history() {
        // Share history repo via Arc so we can assert on it after the service runs.
        let history = Arc::new(MockHistoryRepo::new());

        struct SharedHistoryRepo(Arc<MockHistoryRepo>);

        impl HistoryRepository for SharedHistoryRepo {
            fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
                self.0.list(limit)
            }
            fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
                self.0.get(id)
            }
            fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
                self.0.save(entry)
            }
            fn clear(&self) -> DomainResult<()> {
                self.0.clear()
            }
            fn search(
                &self,
                filter: &rocket_history::HistoryFilter,
            ) -> DomainResult<Vec<HistoryEntry>> {
                self.0.search(filter)
            }
        }

        let history_arc = Arc::clone(&history);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(SharedHistoryRepo(history)),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        svc.execute(sample_input("https://example.com", None))
            .await
            .unwrap();

        assert_eq!(history_arc.entries.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn execute_publishes_event() {
        use rocket_shared::events::DomainEvent;

        let publisher = Arc::new(RecordingPublisher {
            events: Mutex::new(vec![]),
        });

        struct SharedPublisher(Arc<RecordingPublisher>);
        impl rocket_shared::events::EventPublisher for SharedPublisher {
            fn publish(&self, event: DomainEvent) {
                self.0.publish(event);
            }
        }

        let pub_arc = Arc::clone(&publisher);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(201)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(publisher)),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        svc.execute(sample_input("https://example.com/items", None))
            .await
            .unwrap();

        let events = pub_arc.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            DomainEvent::RequestExecuted { status: 201, .. }
        ));
    }

    // -------------------------------------------------------------------------
    // merge_headers unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn merge_headers_request_overrides_collection_by_key() {
        let col = vec![
            Header::new("X-Tenant", "acme"),
            Header::new("Accept", "application/json"),
        ];
        let req = vec![Header::new("Accept", "text/plain")];
        let merged = merge_headers(&col, &req);

        // Accept from request wins; X-Tenant from collection is preserved.
        assert_eq!(merged.len(), 2);
        let accept = merged.iter().find(|h| h.key == "Accept").unwrap();
        assert_eq!(accept.value, "text/plain");
        assert!(merged
            .iter()
            .any(|h| h.key == "X-Tenant" && h.value == "acme"));
    }

    #[test]
    fn merge_headers_disabled_request_header_does_not_override_collection() {
        let col = vec![Header::new("Accept", "application/json")];
        // Disabled request header should not shadow the collection header.
        let req = vec![Header::disabled("Accept", "text/plain")];
        let merged = merge_headers(&col, &req);

        // Collection header is kept because request header is disabled.
        let accept = merged.iter().find(|h| h.key == "Accept").unwrap();
        assert_eq!(accept.value, "application/json");
    }

    #[test]
    fn merge_headers_empty_collection_returns_request_headers() {
        let req = vec![Header::new("Authorization", "Bearer tok")];
        let merged = merge_headers(&[], &req);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "Authorization");
    }

    // -------------------------------------------------------------------------
    // merge_auth unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn merge_auth_uses_collection_when_request_is_none() {
        let collection_auth = Some(Auth::Bearer {
            token: "col_tok".into(),
        });
        let result = merge_auth(Auth::None, collection_auth);
        assert_eq!(
            result,
            Auth::Bearer {
                token: "col_tok".into()
            }
        );
    }

    #[test]
    fn merge_auth_request_takes_precedence_over_collection() {
        let collection_auth = Some(Auth::Bearer {
            token: "col_tok".into(),
        });
        let request_auth = Auth::Basic {
            username: "user".into(),
            password: "pass".into(),
        };
        let result = merge_auth(request_auth.clone(), collection_auth);
        assert_eq!(result, request_auth);
    }

    #[test]
    fn merge_auth_none_collection_returns_none() {
        let result = merge_auth(Auth::None, None);
        assert_eq!(result, Auth::None);
    }

    #[test]
    fn merge_auth_uses_collection_when_request_is_inherit() {
        let collection_auth = Some(Auth::Bearer {
            token: "col_tok".into(),
        });
        let result = merge_auth(Auth::Inherit, collection_auth);
        assert_eq!(
            result,
            Auth::Bearer {
                token: "col_tok".into()
            }
        );
    }

    #[test]
    fn merge_auth_inherit_without_collection_auth_returns_none() {
        let result = merge_auth(Auth::Inherit, None);
        assert_eq!(result, Auth::None);
    }

    fn cv(key: &str, value: &str) -> CollectionVariable {
        CollectionVariable {
            key: key.into(),
            value: value.into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }
    }

    #[tokio::test]
    async fn folder_vars_override_collection_vars() {
        let settings = CollectionSettings {
            variables: vec![cv("HOST", "col-host")],
            ..Default::default()
        };
        let repo = StubCollectionRepo::with_settings(settings)
            .with_folder_vars(vec![cv("HOST", "folder-host")]);

        let executor = Arc::new(MockExecutor::new(200));
        struct SharedExec(Arc<MockExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExec {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(SharedExec(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://{{HOST}}/api", None);
        input.collection = Some("my-api".into());
        input.request_path = Some("auth/login.yml".into());
        svc.execute(input).await.unwrap();

        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://folder-host/api");
    }

    #[tokio::test]
    async fn request_vars_override_folder_vars() {
        let repo = StubCollectionRepo::empty()
            .with_folder_vars(vec![cv("TOKEN", "folder-tok")])
            .with_request_vars(vec![cv("TOKEN", "req-tok")]);

        let executor = Arc::new(MockExecutor::new(200));
        struct SharedExec2(Arc<MockExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExec2 {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(SharedExec2(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com/{{TOKEN}}", None);
        input.collection = Some("my-api".into());
        input.request_path = Some("get-users.yml".into());
        svc.execute(input).await.unwrap();

        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://api.example.com/req-tok");
    }

    #[tokio::test]
    async fn full_precedence_collection_lt_env_lt_folder_lt_request() {
        // Same key "V" set at every level — request must win.
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("V", "env-val"));

        let settings = CollectionSettings {
            variables: vec![cv("V", "col-val")],
            ..Default::default()
        };
        let repo = StubCollectionRepo::with_settings(settings)
            .with_folder_vars(vec![cv("V", "folder-val")])
            .with_request_vars(vec![cv("V", "req-val")]);

        let executor = Arc::new(MockExecutor::new(200));
        struct SharedExec3(Arc<MockExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExec3 {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }
        let exec_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(SharedExec3(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com/{{V}}", Some("prod"));
        input.collection = Some("my-api".into());
        input.request_path = Some("items/get.yml".into());
        svc.execute(input).await.unwrap();

        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://api.example.com/req-val");
    }

    // -------------------------------------------------------------------------
    // Integration test: collection settings applied during execute
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn execute_uses_collection_auth_when_request_auth_is_none() {
        use rocket_shared::types::Auth;

        let settings = CollectionSettings {
            docs: None,
            auth: Some(Auth::Bearer {
                token: "col_tok".into(),
            }),
            headers: vec![],
            variables: vec![],
            sandbox_mode: rocket_collection::settings::SandboxMode::Safe,
            ..Default::default()
        };

        // Use a mock executor that captures the request auth.
        struct CapturingExecutor {
            last_auth: Mutex<Option<Auth>>,
        }

        #[async_trait]
        impl HttpExecutor for CapturingExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                *self.last_auth.lock().unwrap() = Some(req.auth.clone());
                Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 1,
                    ttfb_ms: 1,
                    size_bytes: 2,
                    ..Default::default()
                })
            }
        }

        let executor = Arc::new(CapturingExecutor {
            last_auth: Mutex::new(None),
        });

        struct SharedExecutor(Arc<CapturingExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }

        let exec_arc = Arc::clone(&executor);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(SharedExecutor(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::with_settings(settings)),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com", None);
        input.collection = Some("my-api".into());
        svc.execute(input).await.unwrap();

        let captured = exec_arc.last_auth.lock().unwrap().clone().unwrap();
        assert_eq!(
            captured,
            Auth::Bearer {
                token: "col_tok".into()
            }
        );
    }

    struct CapturingAuditPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingAuditPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    struct RecordingPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl rocket_shared::events::EventPublisher for RecordingPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().expect("lock").push(event);
        }
    }

    #[tokio::test]
    async fn execute_emits_security_audit_event_for_sensitive_auth() {
        let publisher = Arc::new(CapturingAuditPublisher {
            captured: Mutex::new(vec![]),
        });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            publisher.clone(),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com/users", None);
        input.auth = Auth::Bearer {
            token: "tok".into(),
        };
        input.collection = Some("my-api".into());
        input.request_path = Some("users.yml".into());
        svc.execute(input).await.unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::SensitiveAuthUsed { auth_type, collection, request_path }
                    if auth_type == "bearer" && collection == "my-api" && request_path == "users.yml"
            )),
            "expected SensitiveAuthUsed bearer event, got {:?}",
            *captured
        );
    }

    #[tokio::test]
    async fn execute_does_not_emit_audit_for_none_or_inherit_auth() {
        let publisher = Arc::new(CapturingAuditPublisher {
            captured: Mutex::new(vec![]),
        });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            publisher.clone(),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        // Auth::None by default in sample_input.
        svc.execute(sample_input("https://api.example.com/public", None))
            .await
            .unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            !captured
                .iter()
                .any(|k| matches!(k, AuditEventKind::SensitiveAuthUsed { .. })),
            "Auth::None must not emit SensitiveAuthUsed, got {:?}",
            *captured
        );
    }

    // -------------------------------------------------------------------------
    // Script side-effect tests (Critical #1 and #2)
    // -------------------------------------------------------------------------

    use rocket_scripting::{
        CollectionVarWrite, EnvVarWrite, ScriptContext, ScriptEngine, ScriptResult,
    };

    struct MockScriptEngine {
        post_response_result: Mutex<ScriptResult>,
    }

    impl MockScriptEngine {
        fn returning_post_response(result: ScriptResult) -> Self {
            Self {
                post_response_result: Mutex::new(result),
            }
        }
    }

    #[async_trait]
    impl ScriptEngine for MockScriptEngine {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            if ctx.phase == ScriptPhase::AfterResponse {
                Ok(self
                    .post_response_result
                    .lock()
                    .expect("lock poisoned")
                    .clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }

    struct CapturingScriptEngine {
        contexts: Mutex<Vec<ScriptContext>>,
        after_response_result: Mutex<ScriptResult>,
    }

    impl CapturingScriptEngine {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                contexts: Mutex::new(Vec::new()),
                after_response_result: Mutex::new(ScriptResult::default()),
            })
        }

        fn with_after_response(result: ScriptResult) -> Arc<Self> {
            Arc::new(Self {
                contexts: Mutex::new(Vec::new()),
                after_response_result: Mutex::new(result),
            })
        }

        fn contexts(&self) -> Vec<ScriptContext> {
            self.contexts.lock().expect("lock poisoned").clone()
        }
    }

    struct SharedCapture(Arc<CapturingScriptEngine>);

    #[async_trait]
    impl ScriptEngine for SharedCapture {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            let phase = ctx.phase.clone();
            self.0.contexts.lock().expect("lock poisoned").push(ctx);
            if phase == ScriptPhase::AfterResponse {
                Ok(self
                    .0
                    .after_response_result
                    .lock()
                    .expect("lock poisoned")
                    .clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }

    struct RecordingEnvRepo {
        initial: Mutex<Option<Environment>>,
        saved: Mutex<Vec<Environment>>,
    }

    impl RecordingEnvRepo {
        fn with_env(env: Environment) -> Arc<Self> {
            Arc::new(Self {
                initial: Mutex::new(Some(env)),
                saved: Mutex::new(vec![]),
            })
        }
        fn last_saved(&self) -> Option<Environment> {
            self.saved.lock().expect("lock poisoned").last().cloned()
        }
    }

    impl rocket_environment::EnvironmentRepository for RecordingEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.initial.lock().expect("lock").iter().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.initial
                .lock()
                .expect("lock")
                .as_ref()
                .filter(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            self.saved.lock().expect("lock").push(env.clone());
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct SharedEnvRepo(Arc<RecordingEnvRepo>);
    impl rocket_environment::EnvironmentRepository for SharedEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            self.0.list()
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0.get(name)
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            self.0.save(env)
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.delete(name)
        }
    }

    struct RecordingCollectionRepo {
        settings: Mutex<CollectionSettings>,
        saved_settings: Mutex<Vec<CollectionSettings>>,
        saved_scoped_vars: Mutex<Vec<(String, Vec<CollectionVariable>)>>,
    }

    impl RecordingCollectionRepo {
        fn with_settings(settings: CollectionSettings) -> Arc<Self> {
            Arc::new(Self {
                settings: Mutex::new(settings),
                saved_settings: Mutex::new(vec![]),
                saved_scoped_vars: Mutex::new(vec![]),
            })
        }
        fn saved_scoped_vars(&self, scope: &str) -> Vec<Vec<CollectionVariable>> {
            self.saved_scoped_vars
                .lock()
                .expect("lock")
                .iter()
                .filter(|(s, _)| s == scope)
                .map(|(_, v)| v.clone())
                .collect()
        }
        fn last_saved_settings(&self) -> Option<CollectionSettings> {
            self.saved_settings.lock().expect("lock").last().cloned()
        }
    }

    impl CollectionRepository for RecordingCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn get_summaries(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.lock().expect("lock").clone())
        }
        fn save_settings(&self, _: &str, settings: &CollectionSettings) -> DomainResult<()> {
            self.saved_settings
                .lock()
                .expect("lock")
                .push(settings.clone());
            Ok(())
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            v: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.saved_scoped_vars
                .lock()
                .expect("lock")
                .push(("folder".into(), v));
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            v: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.saved_scoped_vars
                .lock()
                .expect("lock")
                .push(("request".into(), v));
            Ok(())
        }
    }

    struct SharedCollectionRepo(Arc<RecordingCollectionRepo>);
    impl CollectionRepository for SharedCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            self.0.list()
        }
        fn get(&self, n: &str) -> DomainResult<Collection> {
            self.0.get(n)
        }
        fn get_summaries(&self, n: &str) -> DomainResult<Collection> {
            self.0.get_summaries(n)
        }
        fn create(&self, n: &str) -> DomainResult<Collection> {
            self.0.create(n)
        }
        fn delete(&self, n: &str) -> DomainResult<()> {
            self.0.delete(n)
        }
        fn rename(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.rename(a, b)
        }
        fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> {
            self.0.get_request(a, b)
        }
        fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> {
            self.0.save_request(a, b, c)
        }
        fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> {
            self.0.rename_request(a, b, c)
        }
        fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_request(a, b)
        }
        fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.create_folder(a, b)
        }
        fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_folder(a, b)
        }
        fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> {
            self.0.move_item(a, b, c, d)
        }
        fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> {
            self.0.reorder_items(a, b, c)
        }
        fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> {
            self.0.get_settings(n)
        }
        fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> {
            self.0.save_settings(n, s)
        }
        fn get_folder_chain_variables(
            &self,
            a: &str,
            b: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_chain_variables(a, b)
        }
        fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_variables(a, b)
        }
        fn save_folder_variables(
            &self,
            a: &str,
            b: &str,
            c: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0.save_folder_variables(a, b, c)
        }
        fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_request_variables(a, b)
        }
        fn save_request_variables(
            &self,
            a: &str,
            b: &str,
            c: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0.save_request_variables(a, b, c)
        }
    }

    /// Script engine that returns a custom ScriptResult for the before-request phase.
    struct MockBeforeRequestEngine {
        result: Mutex<ScriptResult>,
    }

    impl MockBeforeRequestEngine {
        fn returning(result: ScriptResult) -> Self {
            Self {
                result: Mutex::new(result),
            }
        }
    }

    #[async_trait]
    impl ScriptEngine for MockBeforeRequestEngine {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            if ctx.phase == ScriptPhase::BeforeRequest {
                Ok(self.result.lock().expect("lock poisoned").clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }

    /// Executor that captures the last request body it received.
    struct BodyCapturingExecutor {
        last_body: Mutex<Option<rocket_shared::types::Body>>,
    }

    impl BodyCapturingExecutor {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                last_body: Mutex::new(None),
            })
        }
        fn last_body(&self) -> Option<rocket_shared::types::Body> {
            self.last_body.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl HttpExecutor for BodyCapturingExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.last_body.lock().expect("lock") = req.body.clone();
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
                ..Default::default()
            })
        }
    }

    fn build_svc_with_script(
        env_repo: Box<dyn rocket_environment::EnvironmentRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        engine: Box<dyn ScriptEngine>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            env_repo,
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            collection_repo,
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    #[tokio::test]
    async fn user_scripts_receive_the_collection_name() {
        let capture = CapturingScriptEngine::new();
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("Payments".into());
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        assert_eq!(contexts.len(), 3);
        assert!(contexts
            .iter()
            .all(|c| c.collection_name.as_deref() == Some("Payments")));
    }

    #[tokio::test]
    async fn user_scripts_receive_the_host_process_env() {
        let capture = CapturingScriptEngine::new();
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        // PATH exists in every test environment, so no env mutation is needed.
        let expected = std::env::var("PATH").expect("PATH should be set");
        let contexts = capture.contexts();
        assert_eq!(contexts.len(), 1);
        assert_eq!(
            contexts[0].variables.process_env.get("PATH"),
            Some(&expected)
        );
    }

    #[test]
    fn with_body_override_replaces_text_body_and_size() {
        let original = HttpResponse {
            status: 200,
            body: "{\"a\":1}".into(),
            size_bytes: 7,
            is_binary: true,
            body_base64: Some("e30=".into()),
            ..Default::default()
        };
        let patched = with_body_override(&original, Some("{\"a\":22}"));
        assert_eq!(patched.body, "{\"a\":22}");
        assert_eq!(patched.size_bytes, 8);
        assert!(!patched.is_binary);
        assert!(patched.body_base64.is_none());
        assert_eq!(patched.status, 200);

        let unchanged = with_body_override(&original, None);
        assert_eq!(unchanged.body, "{\"a\":1}");
    }

    #[tokio::test]
    async fn tests_script_sees_the_body_set_by_the_after_response_script() {
        let capture = CapturingScriptEngine::with_after_response(ScriptResult {
            response_body: Some("patched".into()),
            ..Default::default()
        });
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        let output = svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        let tests_ctx = contexts
            .iter()
            .find(|c| c.phase == rocket_scripting::ScriptPhase::Tests)
            .expect("tests phase ran");
        assert_eq!(
            tests_ctx.response.as_ref().map(|r| r.body.as_str()),
            Some("patched")
        );
        // The response shown to the user is the real one.
        assert_ne!(output.response.body, "patched");
    }

    #[tokio::test]
    async fn tests_script_receives_precomputed_assertion_outcomes() {
        let capture = CapturingScriptEngine::new();
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.tests_script = Some("// tests".into());
        input.assertions = vec![rocket_shared::Assertion::new(
            "res.status",
            "eq",
            Some("200".into()),
        )];
        svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        let tests_ctx = contexts
            .iter()
            .find(|c| c.phase == rocket_scripting::ScriptPhase::Tests)
            .expect("tests phase ran");
        assert_eq!(tests_ctx.assertion_results.len(), 1);
        assert_eq!(tests_ctx.assertion_results[0].lhs, "res.status");
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_persist_calls_env_repo_save() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "TOKEN".into(),
                value: serde_json::json!("new-token"),
                persist: true,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called");
        assert_eq!(saved.get_value("TOKEN"), Some("new-token"));
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_always_calls_env_repo_save() {
        // All active-env writes persist regardless of the per-write persist flag.
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "TOKEN".into(),
                value: serde_json::json!("new-value"),
                persist: false,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() must be called for all active-env writes");
        assert_eq!(saved.get_value("TOKEN"), Some("new-value"));
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_preserves_secret_flag() {
        // A script overwriting a previously-secret variable's value must not
        // silently strip its secret flag.
        let mut env = Environment::new("dev");
        env.set_variable(Variable::secret("API_KEY", "sk-old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "API_KEY".into(),
                value: serde_json::json!("sk-new"),
                persist: true,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called");
        let var = saved
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY present");
        assert_eq!(var.value, "sk-new");
        assert!(
            var.secret,
            "secret flag must be preserved across a script write"
        );
    }

    #[tokio::test]
    async fn post_response_script_env_var_delete_then_set_preserves_secret_flag_and_publishes_audit(
    ) {
        // rok.deleteEnvVar('K') followed by rok.setEnvVar('K', v) in the same
        // script queues a Null write then a value write for the same key in
        // one env_var_writes batch. Metadata lookup must use the pre-batch
        // snapshot, not the progressively-mutated env, or the delete erases
        // "existing" before the re-set can find it — silently downgrading the
        // secret flag to false with no SecretVariableWritten audit event.
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![
                EnvVarWrite {
                    key: "API_KEY".into(),
                    value: serde_json::Value::Null,
                    persist: true,
                },
                EnvVarWrite {
                    key: "API_KEY".into(),
                    value: serde_json::json!("sk-new"),
                    persist: true,
                },
            ],
            ..Default::default()
        };

        let audit_publisher = Arc::new(CapturingAuditPublisher {
            captured: Mutex::new(vec![]),
        });
        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            audit_publisher.clone(),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called");
        let var = saved
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY present");
        assert_eq!(var.value, "sk-new");
        assert!(
            var.secret,
            "secret flag must survive a delete-then-recreate within one script"
        );

        let captured = audit_publisher.captured.lock().expect("lock");
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::SecretVariableWritten { environment, variable_key }
                    if environment == "prod" && variable_key == "API_KEY"
            )),
            "expected SecretVariableWritten even after a delete-then-recreate, got {:?}",
            *captured
        );
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_new_key_defaults_to_non_secret() {
        // A script writing a brand-new key (no pre-existing variable) must not be
        // able to implicitly create a secret — only the user can promote via the UI.
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "NEW_TOKEN".into(),
                value: serde_json::json!("t-123"),
                persist: true,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called");
        let var = saved
            .variables
            .iter()
            .find(|v| v.key == "NEW_TOKEN")
            .expect("NEW_TOKEN present");
        assert!(
            !var.secret,
            "a script must not be able to implicitly create a secret variable"
        );
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_publishes_secret_audit_and_events() {
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "API_KEY".into(),
                value: serde_json::json!("sk-new"),
                persist: true,
            }],
            ..Default::default()
        };

        let event_publisher = Arc::new(RecordingPublisher {
            events: Mutex::new(vec![]),
        });
        struct SharedPub(Arc<RecordingPublisher>);
        impl rocket_shared::events::EventPublisher for SharedPub {
            fn publish(&self, event: DomainEvent) {
                self.0.publish(event);
            }
        }
        let audit_publisher = Arc::new(CapturingAuditPublisher {
            captured: Mutex::new(vec![]),
        });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(SharedPub(Arc::clone(&event_publisher))),
            audit_publisher.clone(),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let published = event_publisher.events.lock().expect("lock");
        assert!(
            published
                .iter()
                .any(|e| matches!(e, DomainEvent::EnvironmentSaved { name } if name == "prod")),
            "expected EnvironmentSaved, got {:?}",
            *published
        );
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ScriptVariableWritten { scope, environment, key, .. }
                    if scope == "environment" && environment.as_deref() == Some("prod") && key == "API_KEY"
            )),
            "expected ScriptVariableWritten, got {:?}", *published
        );

        let captured = audit_publisher.captured.lock().expect("lock");
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::SecretVariableWritten { environment, variable_key }
                    if environment == "prod" && variable_key == "API_KEY"
            )),
            "expected SecretVariableWritten, got {:?}",
            *captured
        );
    }

    #[tokio::test]
    async fn post_response_script_env_var_write_non_secret_does_not_publish_secret_audit() {
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("HOST", "old.example.com"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "HOST".into(),
                value: serde_json::json!("new.example.com"),
                persist: true,
            }],
            ..Default::default()
        };

        let audit_publisher = Arc::new(CapturingAuditPublisher {
            captured: Mutex::new(vec![]),
        });
        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            audit_publisher.clone(),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let captured = audit_publisher.captured.lock().expect("lock");
        assert!(
            !captured
                .iter()
                .any(|k| matches!(k, AuditEventKind::SecretVariableWritten { .. })),
            "a non-secret write must not publish SecretVariableWritten, got {:?}",
            *captured
        );
    }

    #[tokio::test]
    async fn post_response_script_collection_var_write_calls_save_settings() {
        let initial_settings = CollectionSettings {
            variables: vec![CollectionVariable {
                key: "BASE_URL".into(),
                value: "https://old.example.com".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            }],
            ..Default::default()
        };
        let col_repo = RecordingCollectionRepo::with_settings(initial_settings);

        let result = ScriptResult {
            collection_var_writes: vec![CollectionVarWrite {
                key: "BASE_URL".into(),
                value: serde_json::json!("https://new.example.com"),
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = col_repo
            .last_saved_settings()
            .expect("save_settings should have been called");
        let written = saved.variables.iter().find(|v| v.key == "BASE_URL");
        assert_eq!(
            written.map(|v| v.value.as_str()),
            Some("https://new.example.com")
        );
    }

    #[tokio::test]
    async fn post_response_script_collection_var_write_publishes_events() {
        let initial_settings = CollectionSettings {
            variables: vec![CollectionVariable {
                key: "BASE_URL".into(),
                value: "https://old.example.com".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            }],
            ..Default::default()
        };
        let col_repo = RecordingCollectionRepo::with_settings(initial_settings);

        let result = ScriptResult {
            collection_var_writes: vec![CollectionVarWrite {
                key: "BASE_URL".into(),
                value: serde_json::json!("https://new.example.com"),
            }],
            ..Default::default()
        };

        let event_publisher = Arc::new(RecordingPublisher {
            events: Mutex::new(vec![]),
        });
        struct SharedPub(Arc<RecordingPublisher>);
        impl rocket_shared::events::EventPublisher for SharedPub {
            fn publish(&self, event: DomainEvent) {
                self.0.publish(event);
            }
        }

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPub(Arc::clone(&event_publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let published = event_publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionVariableWritten { collection, key }
                    if collection == "my-api" && key == "BASE_URL"
            )),
            "expected CollectionVariableWritten, got {:?}",
            *published
        );
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ScriptVariableWritten { scope, collection, key, .. }
                    if scope == "collection" && collection.as_deref() == Some("my-api") && key == "BASE_URL"
            )),
            "expected ScriptVariableWritten, got {:?}", *published
        );
    }

    #[tokio::test]
    async fn post_response_script_global_env_var_write_calls_env_repo_save() {
        let mut global_env = Environment::new("global-prod");
        global_env.set_variable(Variable::new("API_KEY", "old-key"));
        let env_repo = RecordingEnvRepo::with_env(global_env);

        let result = ScriptResult {
            global_env_var_writes: vec![EnvVarWrite {
                key: "API_KEY".into(),
                value: serde_json::json!("new-key"),
                persist: false,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", None);
        input.global_env_name = Some("global-prod".into());
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called for global env write");
        assert_eq!(saved.get_value("API_KEY"), Some("new-key"));
    }

    // -------------------------------------------------------------------------
    // Vault secrets written by a script are never persisted
    // -------------------------------------------------------------------------

    const VAULT_TOKEN: &str = "vault-token-value-123";

    fn vault_forms_of(values: &[&str]) -> Vec<String> {
        let owned: Vec<String> = values.iter().map(|v| v.to_string()).collect();
        crate::redaction::secret_forms(&owned)
    }

    fn env_write(key: &str, value: &str) -> EnvVarWrite {
        EnvVarWrite {
            key: key.into(),
            value: serde_json::json!(value),
            persist: true,
        }
    }

    fn assert_console_names_key_not_value(console: &[ConsoleEntry], key: &str, secret: &str) {
        assert_eq!(console.len(), 1, "expected one warning, got {console:?}");
        assert_eq!(console[0].level, ConsoleLevel::Warn);
        assert!(console[0].message.contains(key), "{}", console[0].message);
        assert!(console[0].message.contains("not saved"));
        assert!(
            !console[0].message.contains(secret),
            "{}",
            console[0].message
        );
    }

    #[test]
    fn env_write_with_a_vault_value_is_not_saved_but_kept_in_runtime_vars() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        let env_repo = RecordingEnvRepo::with_env(env);
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            env_var_writes: vec![env_write("TOKEN", VAULT_TOKEN)],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        assert!(env_repo.last_saved().is_none(), "vault value was persisted");
        assert_eq!(
            var_ctx.runtime.get("TOKEN").map(String::as_str),
            Some(VAULT_TOKEN)
        );
        assert_console_names_key_not_value(&console, "TOKEN", VAULT_TOKEN);
    }

    #[test]
    fn env_write_with_a_vault_value_keeps_the_other_writes_of_the_batch() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        let env_repo = RecordingEnvRepo::with_env(env);
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            env_var_writes: vec![env_write("TOKEN", VAULT_TOKEN), env_write("PLAIN", "hello")],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        let saved = env_repo.last_saved().expect("plain write must persist");
        assert_eq!(saved.get_value("PLAIN"), Some("hello"));
        assert_eq!(saved.get_value("TOKEN"), Some("old"));
    }

    #[test]
    fn env_write_containing_one_pem_line_is_not_saved() {
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\nhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----\n";
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            env_var_writes: vec![env_write("KEY_LINE", "prefix MIIEvQIBADANBgkq")],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[pem]),
                console: &mut console,
            },
        );
        assert!(env_repo.last_saved().is_none());
        assert!(var_ctx.runtime.contains_key("KEY_LINE"));
        assert_console_names_key_not_value(&console, "KEY_LINE", "MIIEvQIBADANBgkq");
    }

    #[test]
    fn non_vault_env_write_still_persists_without_a_warning() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            env_var_writes: vec![env_write("TOKEN", "plain-value")],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        let saved = env_repo.last_saved().expect("must persist");
        assert_eq!(saved.get_value("TOKEN"), Some("plain-value"));
        assert!(console.is_empty());
        assert!(!var_ctx.runtime.contains_key("TOKEN"));
    }

    #[test]
    fn short_vault_value_never_triggers_the_guard() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            env_var_writes: vec![env_write("PIN", "1234")],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&["1234"]),
                console: &mut console,
            },
        );
        assert_eq!(
            env_repo
                .last_saved()
                .expect("must persist")
                .get_value("PIN"),
            Some("1234")
        );
        assert!(console.is_empty());
    }

    #[test]
    fn global_env_write_with_a_vault_value_is_not_saved() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("global-prod"));
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            global_env_var_writes: vec![env_write("API_KEY", VAULT_TOKEN)],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            None,
            Some("global-prod"),
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        assert!(env_repo.last_saved().is_none());
        assert_eq!(
            var_ctx.runtime.get("API_KEY").map(String::as_str),
            Some(VAULT_TOKEN)
        );
        assert_console_names_key_not_value(&console, "API_KEY", VAULT_TOKEN);
    }

    #[test]
    fn collection_var_write_with_a_vault_value_is_not_saved() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(MockScriptEngine::returning_post_response(
                ScriptResult::default(),
            )),
        );
        let result = ScriptResult {
            collection_var_writes: vec![
                CollectionVarWrite {
                    key: "SECRET".into(),
                    value: serde_json::json!(VAULT_TOKEN),
                },
                CollectionVarWrite {
                    key: "PLAIN".into(),
                    value: serde_json::json!("hello"),
                },
            ],
            ..Default::default()
        };
        let mut var_ctx = rocket_environment::VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            None,
            None,
            Some("my-api"),
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        let saved = col_repo
            .last_saved_settings()
            .expect("plain write must persist");
        assert!(saved.variables.iter().all(|v| v.key != "SECRET"));
        assert!(saved.variables.iter().any(|v| v.key == "PLAIN"));
        assert_eq!(
            var_ctx.runtime.get("SECRET").map(String::as_str),
            Some(VAULT_TOKEN)
        );
        assert_console_names_key_not_value(&console, "SECRET", VAULT_TOKEN);
    }

    #[tokio::test]
    async fn execute_reports_the_vault_write_warning_without_the_value() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let result = ScriptResult {
            env_var_writes: vec![env_write("TOKEN", VAULT_TOKEN)],
            ..Default::default()
        };
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );
        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        let secrets =
            std::collections::HashMap::from([("vault.token".to_string(), VAULT_TOKEN.to_string())]);
        let out = svc
            .execute_with_external_secrets(input, &secrets)
            .await
            .expect("execute failed");
        assert!(env_repo.last_saved().is_none());
        assert_console_names_key_not_value(&out.console_entries, "TOKEN", VAULT_TOKEN);
    }

    #[tokio::test]
    async fn before_request_script_invalid_method_is_surfaced_as_script_error() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_shared::types::HttpMethod;

        // req.setMethod('PACTH ME') — text that is not a valid method token.
        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                method: Some("PACTH ME".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.method = HttpMethod::Get;
        input.pre_request_script = Some("// pre".into());
        let output = svc.execute(input).await.expect("execute failed");

        assert_eq!(
            output.response.status, 200,
            "the original method must still be used, unmodified"
        );
        let err = output
            .script_error
            .expect("an invalid setMethod() must surface a script_error");
        assert!(
            err.contains("PACTH ME"),
            "error should name the invalid method: {err}"
        );
    }

    #[tokio::test]
    async fn ac1_policy_disabled_sends_redirect_unmodified() {
        use rocket_scripting::{RequestMutations, ScriptResult};

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://169.254.169.254/latest/meta-data/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        // request_guard_policy left at its Default — fully permissive.
        let output = svc
            .execute(input)
            .await
            .expect("execute should succeed — policy is off");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac2_policy_enabled_blocks_redirect_to_metadata_endpoint() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://169.254.169.254/latest/meta-data/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let err = svc.execute(input).await.expect_err("must be blocked");
        assert!(err.to_string().contains("169.254.169.254"));
    }

    #[tokio::test]
    async fn ac3_policy_enabled_without_private_flag_allows_private_redirect() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://192.168.1.1/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let output = svc
            .execute(input)
            .await
            .expect("private ranges must be allowed by default");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac4_both_flags_enabled_blocks_private_redirect() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://192.168.1.1/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let err = svc
            .execute(input)
            .await
            .expect_err("must be blocked with both flags on");
        assert!(err.to_string().contains("192.168.1.1"));
    }

    #[tokio::test]
    async fn ac5_manual_loopback_url_never_blocked_regardless_of_policy() {
        use rocket_workspace::RequestGuardPolicy;

        // No pre_request_script at all — this is exactly what a user manually
        // typing http://localhost:8080/ into the URL bar looks like to the
        // service. The guard must never inspect input.url itself.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("http://localhost:8080/", None);
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let output = svc
            .execute(input)
            .await
            .expect("manual URLs are never checked");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac6_script_without_seturl_is_unaffected_by_policy() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_shared::types::HttpMethod;
        use rocket_workspace::RequestGuardPolicy;

        // Script only calls req.setMethod — no URL mutation at all.
        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                method: Some("POST".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.method = HttpMethod::Get;
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let output = svc
            .execute(input)
            .await
            .expect("no URL mutation means nothing to check");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn before_request_script_body_mutation_reaches_executor() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_shared::types::BodyMode;

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                body: Some(serde_json::json!(r#"{"injected":true}"#)),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        assert_eq!(body.mode, BodyMode::Json);
        assert_eq!(body.content.as_deref(), Some(r#"{"injected":true}"#));
    }

    /// Executor whose send always fails.
    struct FailingExecutor;

    #[async_trait]
    impl HttpExecutor for FailingExecutor {
        async fn execute(&self, _req: &HttpRequest) -> DomainResult<HttpResponse> {
            Err(DomainError::Internal("connection refused".into()))
        }
    }

    fn capturing_svc(
        executor: Arc<dyn HttpExecutor>,
        engine: Box<dyn ScriptEngine>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn history_svc() -> (RequestExecutionService, Arc<Mutex<Vec<HistoryEntry>>>) {
        let history_repo = Box::new(MockHistoryRepo::new());
        let saved = history_repo.saved_entries_handle();
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("dev"))),
            Arc::new(MockExecutor::new(200)),
            history_repo,
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher::new(vec![])),
        );
        (svc, saved)
    }

    #[tokio::test]
    async fn execute_capturing_saves_history_by_default() {
        let (svc, saved) = history_svc();
        let mut sent = None;
        let out = svc
            .execute_capturing(
                sample_input("https://example.com/a", None),
                &std::collections::HashMap::new(),
                &mut sent,
            )
            .await
            .expect("execute");
        assert_eq!(saved.lock().expect("lock").len(), 1);
        assert!(out.deferred_history.is_none());
    }

    #[tokio::test]
    async fn execute_capturing_with_skip_history_defers_the_entry() {
        let (svc, saved) = history_svc();
        let mut input = sample_input("https://example.com/a", None);
        input.skip_history = true;
        let mut sent = None;
        let out = svc
            .execute_capturing(input, &std::collections::HashMap::new(), &mut sent)
            .await
            .expect("execute");

        assert_eq!(saved.lock().expect("lock").len(), 0, "nothing saved yet");
        let entry = out.deferred_history.expect("the entry is handed back");
        assert_eq!(entry.status, 200);

        svc.save_deferred_history(&entry);
        assert_eq!(saved.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn execute_capturing_records_the_request_after_the_pre_request_script() {
        use rocket_scripting::{HeaderMutation, RequestMutations, ScriptResult};

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                headers: vec![HeaderMutation::Set {
                    name: "X-Trace".into(),
                    value: "from-script".into(),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let svc = capturing_svc(
            Arc::new(MockExecutor::new(200)),
            Box::new(MockBeforeRequestEngine::returning(result)),
        );
        let mut input = sample_input("https://example.com/a", None);
        input.pre_request_script = Some("// pre".into());

        let mut sent = None;
        svc.execute_capturing(input, &std::collections::HashMap::new(), &mut sent)
            .await
            .expect("execute failed");

        let sent = sent.expect("request should be captured");
        assert_eq!(sent.url, "https://example.com/a");
        assert!(sent
            .headers
            .iter()
            .any(|h| h.key == "X-Trace" && h.value == "from-script"));
    }

    #[tokio::test]
    async fn execute_capturing_keeps_the_request_when_the_send_fails() {
        let svc = capturing_svc(
            Arc::new(FailingExecutor),
            Box::new(MockBeforeRequestEngine::returning(Default::default())),
        );
        let mut sent = None;
        let result = svc
            .execute_capturing(
                sample_input("https://example.com/b", None),
                &std::collections::HashMap::new(),
                &mut sent,
            )
            .await;

        assert!(result.is_err());
        assert_eq!(sent.expect("captured").url, "https://example.com/b");
    }

    #[tokio::test]
    async fn before_request_script_string_body_respects_explicit_content_type() {
        use rocket_scripting::{HeaderMutation, RequestMutations, ScriptResult};
        use rocket_shared::types::BodyMode;

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        // req.setHeader('Content-Type', 'application/xml'); req.setBody('<a/>');
        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                headers: vec![HeaderMutation::Set {
                    name: "Content-Type".into(),
                    value: "application/xml".into(),
                }],
                body: Some(serde_json::json!("<a/>")),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        assert_eq!(
            body.mode,
            BodyMode::Xml,
            "a string body should respect the script's explicit Content-Type instead of being forced to JSON"
        );
        assert_eq!(body.content.as_deref(), Some("<a/>"));
    }

    #[tokio::test]
    async fn resolve_request_resolves_plain_variable_in_body_content() {
        use rocket_shared::types::{Body, BodyMode};

        let settings = CollectionSettings {
            variables: vec![cv("PASSWORD", "hunter2")],
            ..Default::default()
        };
        let repo = StubCollectionRepo::with_settings(settings);

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://example.com/login", None);
        input.collection = Some("my-api".into());
        input.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"password":"{{PASSWORD}}"}"#.into()),
            form_data: None,
            file_path: None,
        });
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        assert_eq!(body.content.as_deref(), Some(r#"{"password":"hunter2"}"#));
    }

    #[tokio::test]
    async fn resolve_request_resolves_vault_secret_in_body_content() {
        use rocket_shared::types::{Body, BodyMode};

        let mut env = Environment::new("prod");
        env.external_secrets.push(binding_with_refs(
            "password",
            vec![("rocket-admin", "sec-1")],
        ));

        let fetcher = Arc::new(FakeVaultFetcher::new(vec![(
            "sec-1",
            FakeSecretOutcome::Value("s3cr3t-pw".to_string()),
        )]));

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection(
                "conn-1",
            ))),
            Arc::new(FakeSecretStore),
            fetcher,
        );

        let mut input = sample_input("https://example.com/login", Some("prod"));
        input.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"password":"{{password.rocket-admin}}"}"#.into()),
            form_data: None,
            file_path: None,
        });
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        assert_eq!(body.content.as_deref(), Some(r#"{"password":"s3cr3t-pw"}"#));
    }

    #[tokio::test]
    async fn resolve_request_resolves_formdata_value_but_not_key() {
        use rocket_shared::types::{Body, BodyMode, FormDataEntry, FormDataType};

        let settings = CollectionSettings {
            variables: vec![cv("TOKEN", "tok-123")],
            ..Default::default()
        };
        let repo = StubCollectionRepo::with_settings(settings);

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://example.com/upload", None);
        input.collection = Some("my-api".into());
        input.body = Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "{{TOKEN}}".into(),
                value: "{{TOKEN}}".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        let entries = body.form_data.expect("form_data should be present");
        assert_eq!(entries[0].value, "tok-123", "value should be resolved");
        assert_eq!(entries[0].key, "{{TOKEN}}", "key should NOT be resolved");
    }

    #[tokio::test]
    async fn resolve_request_handles_body_with_no_content_without_panicking() {
        use rocket_shared::types::{Body, BodyMode};

        let body_capturing = BodyCapturingExecutor::new();
        let executor_arc = Arc::clone(&body_capturing);

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://example.com", None);
        input.body = Some(Body {
            mode: BodyMode::None,
            content: None,
            form_data: None,
            file_path: None,
        });
        let result = svc.execute(input).await;
        assert!(result.is_ok());

        let body = body_capturing
            .last_body()
            .expect("executor should have received a body");
        assert_eq!(body.content, None);
    }

    #[test]
    fn check_request_guard_noop_when_policy_disabled() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy::default();
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://169.254.169.254/latest/meta-data/",
            &policy,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_blocks_metadata_endpoint_when_enabled() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://169.254.169.254/latest/meta-data/",
            &policy,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("169.254.169.254"),
            "error should name the blocked host: {msg}"
        );
    }

    #[test]
    fn check_request_guard_allows_private_range_when_flag_off() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result =
            svc.check_request_guard("https://example.com/", "http://192.168.1.1/", &policy);
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_blocks_private_range_when_both_flags_on() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result =
            svc.check_request_guard("https://example.com/", "http://192.168.1.1/", &policy);
        assert!(result.is_err());
    }

    #[test]
    fn check_request_guard_ignores_same_host_path_only_rewrite() {
        use rocket_workspace::RequestGuardPolicy;
        // A script that only rewrites the path/query of a host the user already
        // declared themselves (even an internal one) must never be blocked —
        // only a host *change* introduced by the script is in scope.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result =
            svc.check_request_guard("http://192.168.1.1/foo", "http://192.168.1.1/bar", &policy);
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_errors_on_unparseable_mutated_url() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "not a url", &policy);
        assert!(result.is_err());
    }

    #[test]
    fn check_request_guard_blocks_bracketed_ipv6_loopback_through_url_parse() {
        use rocket_workspace::RequestGuardPolicy;
        // Regression test for a real bug found by review: check_request_guard
        // extracts the host via url::Url::host_str(), which returns an IPv6
        // literal wrapped in brackets (e.g. "[::1]"), not a bare address.
        // is_blocked_host's own unit tests passed bare strings that never went
        // through this parsing step, so this must exercise the real call path.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "http://[::1]/", &policy);
        assert!(
            result.is_err(),
            "a bracketed IPv6 loopback URL must be blocked"
        );
    }

    #[test]
    fn check_request_guard_blocks_ipv4_mapped_metadata_endpoint_through_url_parse() {
        use rocket_workspace::RequestGuardPolicy;
        // The IPv4-mapped IPv6 form of the cloud metadata endpoint must be
        // blocked exactly like its plain IPv4 form is.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://[::ffff:169.254.169.254]/latest/meta-data/",
            &policy,
        );
        assert!(
            result.is_err(),
            "the IPv4-mapped metadata endpoint must be blocked"
        );
    }

    #[test]
    fn check_request_guard_blocks_unspecified_address_shorthand_through_url_parse() {
        use rocket_workspace::RequestGuardPolicy;
        // Regression test for a second bypass found by re-review: "0" and
        // "0.0.0.0" both resolve to the unspecified IPv4 address, which url
        // normalizes to "0.0.0.0" -- and 0.0.0.0 reaches loopback-bound
        // services on Linux/macOS, so it must be blocked even though it is
        // not itself loopback, link-local, or private.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "http://0/", &policy);
        assert!(
            result.is_err(),
            "the unspecified-address shorthand '0' must be blocked"
        );
    }

    #[test]
    fn check_request_guard_blocks_localhost_with_trailing_dot_through_url_parse() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "http://localhost./", &policy);
        assert!(
            result.is_err(),
            "'localhost.' must be blocked exactly like 'localhost'"
        );
    }

    /// Executor that captures the RequestOptions it received.
    struct OptionsCapturingExecutor {
        last_options: Mutex<Option<RequestOptions>>,
    }

    impl OptionsCapturingExecutor {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                last_options: Mutex::new(None),
            })
        }
        fn last_options(&self) -> Option<RequestOptions> {
            self.last_options.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl HttpExecutor for OptionsCapturingExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.last_options.lock().expect("lock") = Some(req.options.clone());
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn assertions_run_after_tests_script_and_appear_in_results() {
        use rocket_scripting::TestStatus;
        use rocket_shared::Assertion;

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://example.com", None);
        // One passing and one failing assertion.
        input.assertions = vec![
            Assertion::new("res.status", "eq", Some("200".into())),
            Assertion::new("res.status", "eq", Some("404".into())),
        ];

        let output = svc.execute(input).await.expect("execute");
        assert_eq!(output.test_results.len(), 2);
        assert_eq!(output.test_results[0].status, TestStatus::Passed);
        assert_eq!(output.test_results[1].status, TestStatus::Failed);
    }

    #[tokio::test]
    async fn before_request_script_max_redirects_mutation_reaches_executor() {
        use rocket_scripting::{RequestMutations, ScriptResult};

        let options_capturing = OptionsCapturingExecutor::new();
        let executor_arc = Arc::clone(&options_capturing);

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                max_redirects: Some(3),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            executor_arc,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let opts = options_capturing
            .last_options()
            .expect("executor should have received options");
        assert_eq!(opts.max_redirects, Some(3));
    }

    // -------------------------------------------------------------------------
    // apply_actions (runtime.actions set-variable pipeline) tests
    // -------------------------------------------------------------------------

    /// Script engine stub for `apply_actions` tests — always resolves the jsonq
    /// snippet to a fixed value, regardless of what the expression text says.
    struct FixedJsonqEngine {
        value: serde_json::Value,
    }

    #[async_trait]
    impl ScriptEngine for FixedJsonqEngine {
        async fn execute(
            &self,
            _ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            let mut vars = std::collections::HashMap::new();
            vars.insert("__jsonq_result__".to_string(), self.value.clone());
            Ok(ScriptResult {
                runtime_vars: vars,
                ..Default::default()
            })
        }
    }

    /// Script engine stub that simulates a jsonq expression throwing.
    struct ErrorJsonqEngine;

    #[async_trait]
    impl ScriptEngine for ErrorJsonqEngine {
        async fn execute(
            &self,
            _ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            Ok(ScriptResult {
                error: Some("ReferenceError: nope".into()),
                ..Default::default()
            })
        }
    }

    fn stub_action(scope: &str, phase: &str, disabled: bool) -> rocket_shared::ActionSetVariable {
        rocket_shared::ActionSetVariable {
            phase: phase.into(),
            selector: rocket_shared::ActionSelector {
                expression: "res.body".into(),
                method: "jsonq".into(),
            },
            variable: rocket_shared::ActionVariable {
                name: "extracted".into(),
                scope: scope.into(),
            },
            disabled: if disabled { Some(true) } else { None },
            description: None,
        }
    }

    fn stub_action_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: "{}".into(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 2,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn after_response_action_writes_collection_variable() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("extracted-value"),
            }),
        );

        let actions = vec![stub_action("collection", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            None,
            Some("my-api"),
            None,
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms: &[],
                console: &mut Vec::new(),
            },
        )
        .await;

        let saved = col_repo
            .last_saved_settings()
            .expect("save_settings should have been called");
        let written = saved.variables.iter().find(|v| v.key == "extracted");
        assert_eq!(written.map(|v| v.value.as_str()), Some("extracted-value"));
    }

    #[tokio::test]
    async fn after_response_action_writes_environment_variable_when_scope_environment() {
        let env = Environment::new("dev");
        let env_repo = RecordingEnvRepo::with_env(env);
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("token-123"),
            }),
        );

        let actions = vec![stub_action("environment", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms: &[],
                console: &mut Vec::new(),
            },
        )
        .await;

        let saved = env_repo
            .last_saved()
            .expect("env_repo.save() should have been called");
        assert_eq!(saved.get_value("extracted"), Some("token-123"));
    }

    #[tokio::test]
    async fn after_response_action_writes_runtime_variable_without_persisting() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("in-memory-value"),
            }),
        );

        let actions = vec![stub_action("runtime", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            None,
            None,
            None,
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms: &[],
                console: &mut Vec::new(),
            },
        )
        .await;

        assert_eq!(
            var_ctx.runtime.get("extracted"),
            Some(&"in-memory-value".to_string())
        );
        assert!(
            col_repo.last_saved_settings().is_none(),
            "runtime scope must never persist"
        );
    }

    #[tokio::test]
    async fn disabled_action_is_skipped() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("should-not-be-written"),
            }),
        );

        let actions = vec![stub_action("collection", "after-response", true)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            None,
            Some("my-api"),
            None,
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms: &[],
                console: &mut Vec::new(),
            },
        )
        .await;

        assert!(
            col_repo.last_saved_settings().is_none(),
            "disabled action must not run"
        );
    }

    #[tokio::test]
    async fn action_wrong_phase_is_skipped() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("should-not-be-written"),
            }),
        );

        // A before-request action must not fire during the after-response pass.
        let actions = vec![stub_action("collection", "before-request", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            None,
            Some("my-api"),
            None,
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms: &[],
                console: &mut Vec::new(),
            },
        )
        .await;

        assert!(
            col_repo.last_saved_settings().is_none(),
            "wrong-phase action must not run"
        );
    }

    // Vault secrets in values written by declarative actions or object values.

    async fn run_vault_action(
        svc: &RequestExecutionService,
        scope: &str,
        forms: &[String],
        env: Option<&str>,
    ) -> (VariableContext, Vec<ConsoleEntry>) {
        let actions = vec![stub_action(scope, "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();
        let mut console = Vec::new();
        svc.apply_actions(
            &actions,
            "after-response",
            "Get User",
            &http_request,
            Some(&response),
            env,
            Some("my-api"),
            Some("folder/req.yml"),
            &mut var_ctx,
            &[],
            &[],
            VaultGuard {
                forms,
                console: &mut console,
            },
        )
        .await;
        (var_ctx, console)
    }

    fn action_svc(
        env_repo: &Arc<RecordingEnvRepo>,
        col_repo: &Arc<RecordingCollectionRepo>,
        value: serde_json::Value,
    ) -> RequestExecutionService {
        build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(env_repo))),
            Box::new(SharedCollectionRepo(Arc::clone(col_repo))),
            Box::new(FixedJsonqEngine { value }),
        )
    }

    #[tokio::test]
    async fn action_with_a_vault_value_is_not_persisted_in_any_scope() {
        for scope in ["environment", "collection", "folder", "request"] {
            let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
            let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
            let value = serde_json::json!(format!("Bearer {VAULT_TOKEN}"));
            let svc = action_svc(&env_repo, &col_repo, value.clone());
            let (var_ctx, console) =
                run_vault_action(&svc, scope, &vault_forms_of(&[VAULT_TOKEN]), Some("dev")).await;
            assert!(env_repo.last_saved().is_none(), "{scope}: env persisted");
            assert!(col_repo.last_saved_settings().is_none(), "{scope}");
            assert!(col_repo.saved_scoped_vars("folder").is_empty(), "{scope}");
            assert!(col_repo.saved_scoped_vars("request").is_empty(), "{scope}");
            assert_eq!(
                var_ctx.runtime.get("extracted").map(String::as_str),
                Some(format!("Bearer {VAULT_TOKEN}").as_str()),
                "{scope}"
            );
            assert_console_names_key_not_value(&console, "extracted", VAULT_TOKEN);
        }
    }

    #[tokio::test]
    async fn action_with_a_vault_value_and_no_active_environment_does_not_warn() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let value = serde_json::json!(format!("Bearer {VAULT_TOKEN}"));
        let svc = action_svc(&env_repo, &col_repo, value);
        let (var_ctx, console) =
            run_vault_action(&svc, "environment", &vault_forms_of(&[VAULT_TOKEN]), None).await;
        assert!(console.is_empty(), "{console:?}");
        assert!(env_repo.last_saved().is_none());
        assert!(!var_ctx.runtime.contains_key("extracted"));
    }

    #[tokio::test]
    async fn action_with_a_plain_value_still_persists_in_every_scope() {
        for scope in ["environment", "collection", "folder", "request"] {
            let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
            let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
            let value = serde_json::json!("plain-value");
            let svc = action_svc(&env_repo, &col_repo, value.clone());
            let (var_ctx, console) =
                run_vault_action(&svc, scope, &vault_forms_of(&[VAULT_TOKEN]), Some("dev")).await;
            let persisted = match scope {
                "environment" => env_repo.last_saved().is_some(),
                "collection" => col_repo.last_saved_settings().is_some(),
                other => !col_repo.saved_scoped_vars(other).is_empty(),
            };
            assert!(persisted, "{scope}: plain value must persist");
            assert!(console.is_empty());
            assert!(!var_ctx.runtime.contains_key("extracted"));
        }
    }

    #[test]
    fn object_value_containing_a_vault_secret_is_not_persisted() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = action_svc(&env_repo, &col_repo, serde_json::Value::Null);
        let object = serde_json::json!({ "token": VAULT_TOKEN });
        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "AUTH".into(),
                value: object.clone(),
                persist: true,
            }],
            collection_var_writes: vec![CollectionVarWrite {
                key: "AUTH".into(),
                value: object.clone(),
            }],
            ..Default::default()
        };
        let mut var_ctx = VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            Some("my-api"),
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        assert!(env_repo.last_saved().is_none());
        assert!(col_repo.last_saved_settings().is_none());
        assert_eq!(
            var_ctx.runtime.get("AUTH").map(String::as_str),
            Some(object.to_string().as_str())
        );
        assert_eq!(console.len(), 2);
        assert!(console.iter().all(|c| !c.message.contains(VAULT_TOKEN)));
    }

    #[test]
    fn a_null_write_is_never_held_back() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = action_svc(&env_repo, &col_repo, serde_json::Value::Null);
        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "GONE".into(),
                value: serde_json::Value::Null,
                persist: true,
            }],
            ..Default::default()
        };
        let mut var_ctx = VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        assert!(env_repo.last_saved().is_some());
        assert!(console.is_empty());
    }

    #[test]
    fn a_key_that_matches_a_vault_value_is_not_echoed_in_the_warning() {
        let env_repo = RecordingEnvRepo::with_env(Environment::new("dev"));
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = action_svc(&env_repo, &col_repo, serde_json::Value::Null);
        let result = ScriptResult {
            env_var_writes: vec![env_write(VAULT_TOKEN, VAULT_TOKEN)],
            ..Default::default()
        };
        let mut var_ctx = VariableContext::default();
        let mut console = Vec::new();
        svc.apply_script_side_effects(
            &result,
            Some("dev"),
            None,
            None,
            &mut var_ctx,
            VaultGuard {
                forms: &vault_forms_of(&[VAULT_TOKEN]),
                console: &mut console,
            },
        );
        assert_eq!(console.len(), 1);
        assert!(!console[0].message.contains(VAULT_TOKEN));
        assert!(console[0].message.contains("<redacted key>"));
    }

    // Environment repo backed by a map, so a test can look up both the active
    // and global environments by name.
    struct MultiEnvRepo {
        envs: std::collections::HashMap<String, Environment>,
    }

    impl MultiEnvRepo {
        fn new(envs: Vec<Environment>) -> Self {
            Self {
                envs: envs.into_iter().map(|e| (e.name.clone(), e)).collect(),
            }
        }
    }

    impl rocket_environment::EnvironmentRepository for MultiEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.envs.values().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.envs
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    // Script engine that records the VariableContext it was invoked with.
    struct CapturingEngine {
        captured: Mutex<Option<VariableContext>>,
    }

    #[async_trait]
    impl ScriptEngine for CapturingEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            *self.captured.lock().expect("lock") = Some(ctx.variables);
            Ok(ScriptResult::default())
        }
    }

    #[tokio::test]
    async fn before_request_script_sees_scope_separated_variables() {
        let settings = CollectionSettings {
            variables: vec![cv("API_KEY", "col-secret")],
            ..Default::default()
        };
        let collection_repo = StubCollectionRepo::with_settings(settings);

        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::new("BASE_URL", "https://dev.local"));
        let mut global_env = Environment::new("shared-global");
        global_env.set_variable(Variable::new("ORG_ID", "acme"));
        let env_repo = MultiEnvRepo::new(vec![active_env, global_env]);

        let engine = Arc::new(CapturingEngine {
            captured: Mutex::new(None),
        });
        struct SharedCapturingEngine(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngine {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(collection_repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedCapturingEngine(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("my-api".into());
        input.global_env_name = Some("shared-global".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc
            .captured
            .lock()
            .expect("lock")
            .clone()
            .expect("engine was called");
        assert_eq!(
            captured.collection.get("API_KEY"),
            Some(&"col-secret".to_string()),
            "collection scope must stay separate, not be flattened into env"
        );
        assert_eq!(
            captured.env.get("BASE_URL"),
            Some(&"https://dev.local".to_string())
        );
        assert_eq!(
            captured.global_env.get("ORG_ID"),
            Some(&"acme".to_string()),
            "global env scope must be populated from global_env_name"
        );
        assert!(
            !captured.env.contains_key("API_KEY"),
            "env scope must not contain the collection variable (would indicate the old flattening bug)"
        );
    }

    #[tokio::test]
    async fn resolve_request_populates_global_env_scope_for_url_resolution() {
        let mut global_env = Environment::new("shared-global");
        global_env.set_variable(Variable::new("ORG_ID", "acme"));
        let env_repo = MultiEnvRepo::new(vec![global_env]);

        let executor = Arc::new(MockExecutor::new(200));
        let executor_arc = Arc::clone(&executor);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            executor,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://example.com/{{ORG_ID}}", None);
        input.global_env_name = Some("shared-global".into());

        svc.execute(input).await.expect("execute should succeed");

        let last_url = executor_arc.last_url.lock().expect("lock").clone();
        assert_eq!(
            last_url,
            Some("https://example.com/acme".to_string()),
            "a global env var must resolve in the sent request URL, not just script scope"
        );
    }

    /// Records the whole request the executor was handed.
    struct CapturingExecutor {
        sent: Mutex<Option<HttpRequest>>,
    }

    #[async_trait]
    impl HttpExecutor for CapturingExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.sent.lock().expect("lock") = Some(req.clone());
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
                ..Default::default()
            })
        }
    }

    fn callback_vars() -> std::collections::HashMap<String, String> {
        std::collections::HashMap::from([(
            "callback.payment".to_string(),
            "http://10.0.0.5:4000/cb/abc".to_string(),
        )])
    }

    #[tokio::test]
    async fn flow_vars_resolve_in_url_header_and_body() {
        let executor = Arc::new(CapturingExecutor {
            sent: Mutex::new(None),
        });
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("unused"))),
            Arc::clone(&executor) as Arc<dyn HttpExecutor>,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input(
            "https://api.example.com/register?cb={{callback.payment}}",
            None,
        );
        input.headers = vec![rocket_shared::types::Header {
            key: "X-Callback".to_string(),
            value: "{{callback.payment}}".to_string(),
            enabled: true,
            description: None,
        }];
        input.body = Some(rocket_shared::types::Body {
            mode: rocket_shared::types::BodyMode::Json,
            content: Some(r#"{"callbackUrl":"{{callback.payment}}"}"#.to_string()),
            form_data: None,
            file_path: None,
        });
        input.flow_vars = callback_vars();

        svc.execute(input).await.expect("execute");

        let sent = executor
            .sent
            .lock()
            .expect("lock")
            .clone()
            .expect("a sent request");
        assert_eq!(
            sent.url,
            "https://api.example.com/register?cb=http://10.0.0.5:4000/cb/abc"
        );
        assert_eq!(sent.headers[0].value, "http://10.0.0.5:4000/cb/abc");
        assert_eq!(
            sent.body.and_then(|b| b.content).as_deref(),
            Some(r#"{"callbackUrl":"http://10.0.0.5:4000/cb/abc"}"#)
        );
    }

    /// Records the runtime scope the pre-request script was given.
    struct RuntimeCapturingEngine {
        runtime: Mutex<Option<std::collections::HashMap<String, String>>>,
    }

    #[async_trait]
    impl ScriptEngine for RuntimeCapturingEngine {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            if ctx.phase == rocket_scripting::ScriptPhase::BeforeRequest {
                *self.runtime.lock().expect("lock") = Some(ctx.variables.runtime.clone());
            }
            Ok(ScriptResult::default())
        }
    }

    #[tokio::test]
    async fn a_pre_request_script_sees_flow_vars_as_runtime_vars() {
        let engine = Arc::new(RuntimeCapturingEngine {
            runtime: Mutex::new(None),
        });
        struct SharedEngine(Arc<RuntimeCapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedEngine {
            async fn execute(
                &self,
                ctx: ScriptContext,
            ) -> rocket_shared::error::DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("unused"))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));
        let mut input = sample_input("https://api.example.com", None);
        input.pre_request_script = Some("console.log(1)".to_string());
        input.flow_vars = callback_vars();

        svc.execute(input).await.expect("execute");

        let runtime = engine
            .runtime
            .lock()
            .expect("lock")
            .clone()
            .expect("pre-request ran");
        assert_eq!(
            runtime.get("callback.payment").map(String::as_str),
            Some("http://10.0.0.5:4000/cb/abc")
        );
    }

    #[tokio::test]
    async fn secret_env_and_collection_vars_populate_secret_values() {
        let settings = CollectionSettings {
            variables: vec![
                CollectionVariable {
                    key: "COL_SECRET".into(),
                    value: "col-secret-val".into(),
                    initial_value: String::new(),
                    enabled: true,
                    secret: true,
                },
                cv("COL_PLAIN", "col-plain-val"),
            ],
            ..Default::default()
        };
        let collection_repo = StubCollectionRepo::with_settings(settings);

        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::secret("API_KEY", "sk-live-abcdef123"));
        active_env.set_variable(Variable::new("PLAIN", "plain-not-secret"));
        let env_repo = MockEnvRepo::with_env(active_env);

        let engine = Arc::new(CapturingEngine {
            captured: Mutex::new(None),
        });
        struct SharedCapturingEngineSecrets(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineSecrets {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(collection_repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineSecrets(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc
            .captured
            .lock()
            .expect("lock")
            .clone()
            .expect("engine was called");
        assert!(
            captured.secret_values.contains("sk-live-abcdef123"),
            "secret env var value must be in secret_values"
        );
        assert!(
            captured.secret_values.contains("col-secret-val"),
            "secret collection var value must be in secret_values"
        );
        assert!(
            !captured.secret_values.contains("plain-not-secret"),
            "non-secret env var value must not be in secret_values"
        );
        assert!(
            !captured.secret_values.contains("col-plain-val"),
            "non-secret collection var value must not be in secret_values"
        );
    }

    #[tokio::test]
    async fn short_secret_value_is_not_added_to_secret_values() {
        // Documented limitation (MIN_REDACTION_LEN = 6): secrets shorter
        // than this are not added to secret_values, so they are never
        // redacted. Asserted explicitly so this doesn't get "fixed"
        // accidentally later without revisiting the trade-off.
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::secret("SHORT", "abc")); // 3 chars < MIN_REDACTION_LEN
        let env_repo = MockEnvRepo::with_env(active_env);

        let engine = Arc::new(CapturingEngine {
            captured: Mutex::new(None),
        });
        struct SharedCapturingEngineShort(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineShort {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineShort(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc
            .captured
            .lock()
            .expect("lock")
            .clone()
            .expect("engine was called");
        assert!(
            !captured.secret_values.contains("abc"),
            "secrets shorter than MIN_REDACTION_LEN must not be added to secret_values"
        );
    }

    #[tokio::test]
    async fn secret_value_exactly_at_min_redaction_len_is_added_to_secret_values() {
        // MIN_REDACTION_LEN = 6 is an inclusive floor ("len >= MIN_REDACTION_LEN"):
        // a secret exactly 6 characters long must still be added and redacted,
        // not excluded. Complements short_secret_value_is_not_added_to_secret_values,
        // which only covers the too-short (3-char) side of the boundary.
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::secret("EXACT", "abcdef")); // 6 chars == MIN_REDACTION_LEN
        let env_repo = MockEnvRepo::with_env(active_env);

        let engine = Arc::new(CapturingEngine {
            captured: Mutex::new(None),
        });
        struct SharedCapturingEngineExact(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineExact {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineExact(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc
            .captured
            .lock()
            .expect("lock")
            .clone()
            .expect("engine was called");
        assert!(
            captured.secret_values.contains("abcdef"),
            "a secret exactly MIN_REDACTION_LEN characters long must be added to secret_values"
        );
    }

    #[tokio::test]
    async fn global_env_secret_populates_secret_values() {
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::new("BASE_URL", "https://dev.local"));
        let mut global_env = Environment::new("shared-global");
        global_env.set_variable(Variable::secret("GLOBAL_TOKEN", "glbl-secret-999"));
        global_env.set_variable(Variable::new("GLOBAL_PLAIN", "glbl-plain-val"));
        let env_repo = MultiEnvRepo::new(vec![active_env, global_env]);

        let engine = Arc::new(CapturingEngine {
            captured: Mutex::new(None),
        });
        struct SharedCapturingEngineGlobal(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineGlobal {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineGlobal(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.global_env_name = Some("shared-global".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc
            .captured
            .lock()
            .expect("lock")
            .clone()
            .expect("engine was called");
        assert!(
            captured.secret_values.contains("glbl-secret-999"),
            "secret global env var value must be in secret_values"
        );
        assert!(
            !captured.secret_values.contains("glbl-plain-val"),
            "non-secret global env var value must not be in secret_values"
        );
    }

    #[tokio::test]
    async fn action_jsonq_error_does_not_abort_request() {
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(ErrorJsonqEngine),
        );

        let mut input = sample_input("https://example.com", None);
        input.actions = vec![stub_action("runtime", "after-response", false)];

        let output = svc
            .execute(input)
            .await
            .expect("execute must succeed despite a bad jsonq expression");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn script_engine_error_surfaces_in_output_script_error() {
        // Stands in for a timed-out script: the engine returns Err, not an
        // Ok(ScriptResult) that carries an error.
        struct FailingEngine;

        #[async_trait]
        impl ScriptEngine for FailingEngine {
            async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
                Err(DomainError::Internal(
                    "script execution timed out after 5s".into(),
                ))
            }
        }

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(FailingEngine));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("while (true) {}".into());
        let output = svc.execute(input).await.expect("execute failed");

        // The request itself must still complete normally.
        assert_eq!(output.response.status, 200);

        let err = output
            .script_error
            .expect("a timed-out script must populate script_error, not just fire an event");
        assert!(err.contains("timed out"), "unexpected message: {err}");
    }

    /// Script engine that reports the execution mode string it was given and
    /// returns a fixed result for the before-request phase.
    struct ModeProbeEngine {
        seen_modes: Mutex<Vec<String>>,
        before_request_result: ScriptResult,
    }

    #[async_trait]
    impl ScriptEngine for ModeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            self.seen_modes
                .lock()
                .expect("lock")
                .push(ctx.execution_mode.clone());
            if ctx.phase == ScriptPhase::BeforeRequest {
                Ok(self.before_request_result.clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }

    struct SharedModeProbe(Arc<ModeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedModeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn execute_always_reports_standalone_execution_mode() {
        let engine = Arc::new(ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult::default(),
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute");

        let modes = engine.seen_modes.lock().expect("lock").clone();
        assert_eq!(modes, vec!["standalone", "standalone", "standalone"]);
    }

    struct SandboxModeProbeEngine {
        seen_modes: Mutex<Vec<rocket_scripting::context::SandboxMode>>,
    }

    #[async_trait]
    impl ScriptEngine for SandboxModeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.seen_modes.lock().expect("lock").push(ctx.sandbox_mode);
            Ok(ScriptResult::default())
        }
    }

    struct SharedSandboxModeProbe(Arc<SandboxModeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedSandboxModeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn before_request_script_receives_collection_sandbox_mode() {
        let engine = Arc::new(SandboxModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
        });
        let collection_repo = StubCollectionRepo::with_settings(CollectionSettings {
            sandbox_mode: rocket_collection::settings::SandboxMode::Developer,
            ..Default::default()
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(collection_repo),
            Box::new(SharedSandboxModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute");

        let modes = engine.seen_modes.lock().expect("lock").clone();
        assert_eq!(
            modes,
            vec![rocket_scripting::context::SandboxMode::Developer]
        );
    }

    struct ScopeProbeEngine {
        seen: Mutex<Vec<Option<rocket_scripting::ScriptFileScope>>>,
    }

    #[async_trait]
    impl ScriptEngine for ScopeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.seen.lock().expect("lock").push(ctx.file_scope.clone());
            Ok(ScriptResult::default())
        }
    }

    struct SharedScopeProbe(Arc<ScopeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedScopeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn scripts_receive_the_collection_file_scope_in_every_phase() {
        let engine = Arc::new(ScopeProbeEngine {
            seen: Mutex::new(vec![]),
        });
        let repo = StubCollectionRepo::with_settings(CollectionSettings {
            script_context_roots: vec!["../shared".into()],
            ..Default::default()
        })
        .with_root("/work/my-api");
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(repo),
            Box::new(SharedScopeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute");

        let expected = Some(rocket_scripting::ScriptFileScope {
            collection_root: "/work/my-api".into(),
            additional_roots: vec!["../shared".into()],
        });
        let seen = engine.seen.lock().expect("lock").clone();
        assert_eq!(seen, vec![expected.clone(), expected.clone(), expected]);
    }

    #[tokio::test]
    async fn scripts_get_no_file_scope_without_a_collection() {
        let engine = Arc::new(ScopeProbeEngine {
            seen: Mutex::new(vec![]),
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedScopeProbe(Arc::clone(&engine))),
        );
        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute");
        assert_eq!(engine.seen.lock().expect("lock").clone(), vec![None]);
    }

    #[tokio::test]
    async fn execute_ignores_skip_request_and_still_sends() {
        // skipRequest() is a runner-only control. The single-send path must not
        // start honouring it.
        let engine = Arc::new(ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult {
                skip_request: true,
                ..Default::default()
            },
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        let out = svc.execute(input).await.expect("execute");
        assert_eq!(
            out.response.status, 200,
            "single send must ignore skipRequest()"
        );
    }

    #[tokio::test]
    async fn before_request_phase_records_skip_and_next_request() {
        let engine = ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult {
                skip_request: true,
                next_request: Some(rocket_scripting::NextRequest::Name("Poll Status".into())),
                ..Default::default()
            },
        };
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(engine),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        let mut state = svc
            .begin_phases(&input, &std::collections::HashMap::new())
            .expect("begin");
        svc.run_before_request_phase(&input, rocket_scripting::ExecutionMode::Runner, &mut state)
            .await
            .expect("run_before_request_phase");

        assert!(state.skip_request);
        assert!(matches!(
            state.next_request,
            Some(rocket_scripting::NextRequest::Name(ref n)) if n == "Poll Status"
        ));
    }

    #[tokio::test]
    async fn seed_runtime_puts_carried_vars_in_the_runtime_scope() {
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(ErrorJsonqEngine),
        );
        let input = sample_input("https://example.com", None);
        let mut state = svc
            .begin_phases(&input, &std::collections::HashMap::new())
            .expect("begin");

        let mut carried = std::collections::HashMap::new();
        carried.insert("TOKEN".to_string(), "from-step-1".to_string());
        state.seed_runtime(&carried);

        assert_eq!(
            state.var_ctx.runtime.get("TOKEN"),
            Some(&"from-step-1".to_string())
        );
    }

    #[test]
    fn resolve_auth_resolves_the_aws_profile_name() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("profile".to_string(), "prod".to_string());
        let out = resolve_auth(
            Auth::AwsSigV4 {
                access_key: String::new(),
                secret_key: String::new(),
                region: "us-east-1".into(),
                service: "s3".into(),
                session_token: None,
                profile_name: Some("{{profile}}".into()),
            },
            &vars,
        );
        assert!(matches!(
            out,
            Auth::AwsSigV4 { profile_name: Some(ref n), .. } if n == "prod"
        ));
    }

    #[test]
    fn merge_runtime_vars_keeps_non_string_values_as_json_text() {
        let mut ctx = rocket_environment::VariableContext::default();
        let mut result = ScriptResult::default();
        result.runtime_vars.insert("n".into(), serde_json::json!(0));
        result.runtime_vars.insert("s".into(), serde_json::json!("text"));
        result
            .runtime_vars
            .insert("o".into(), serde_json::json!({ "a": 1 }));
        result.runtime_vars.insert("nil".into(), serde_json::Value::Null);
        merge_runtime_vars(&mut ctx, &result);
        assert_eq!(ctx.runtime.get("n").map(String::as_str), Some("0"));
        assert_eq!(ctx.runtime.get("s").map(String::as_str), Some("text"));
        assert_eq!(ctx.runtime.get("o").map(String::as_str), Some("{\"a\":1}"));
        assert!(!ctx.runtime.contains_key("nil"));
    }

    #[test]
    fn merge_runtime_vars_applies_deletes_after_sets() {
        let mut ctx = rocket_environment::VariableContext::default();
        ctx.runtime.insert("old".into(), "1".into());
        let result = ScriptResult {
            runtime_var_deletes: vec!["old".into()],
            ..Default::default()
        };
        merge_runtime_vars(&mut ctx, &result);
        assert!(!ctx.runtime.contains_key("old"));
    }

    #[test]
    fn remove_variable_drops_the_named_variable_only() {
        let mut vars = vec![
            rocket_collection::CollectionVariable {
                key: "keep".into(),
                value: "1".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            rocket_collection::CollectionVariable {
                key: "drop".into(),
                value: "2".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
        ];
        assert!(remove_variable(&mut vars, "drop"));
        assert!(!remove_variable(&mut vars, "drop"));
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "keep");
    }

    #[tokio::test]
    async fn post_response_script_env_var_null_write_removes_the_variable() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        env.set_variable(Variable::new("KEEP", "1"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "TOKEN".into(),
                value: serde_json::Value::Null,
                persist: true,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo.last_saved().expect("env_repo.save() was called");
        assert_eq!(saved.get_value("TOKEN"), None);
        assert_eq!(saved.get_value("KEEP"), Some("1"));
    }
}
