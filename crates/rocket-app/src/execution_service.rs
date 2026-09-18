use crate::env_audit;
use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::CollectionRepository;
use rocket_environment::{
    resolve, Environment, EnvironmentRepository, EnvironmentRepositoryFactory, VariableContext,
};
use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_http::{
    run_load_test as http_run_load_test, CookieRepository, HttpExecutor, HttpRequest, HttpResponse,
    LoadTestConfig, LoadTestResult, RequestOptions,
};
use rocket_scripting::{
    ConsoleEntry, ConsoleLevel, ScriptContext, ScriptEngine, ScriptResult, TestResult, TestStatus,
};
use rocket_shared::error::DomainResult;
use std::sync::Arc;
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

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
}

/// Secrets shorter than this are not added to `VariableContext.secret_values`
/// and are therefore never redacted in console/test-failure output. A
/// documented, deliberate trade-off (see
/// docs/superpowers/specs/2026-09-16-secret-aware-variable-context-spec.md
/// §3.4) — redacting every occurrence of a very short string risks
/// over-redacting unrelated output.
const MIN_REDACTION_LEN: usize = 6;

impl RequestExecutionService {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Arc<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
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
        }
    }

    pub fn new_with_audit(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Arc<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
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
    fn regular_env_repo<'a>(&'a self, collection: Option<&str>) -> Box<dyn EnvironmentRepository + 'a> {
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

    /// Builds a scope-separated `VariableContext` from all backend-accessible
    /// scopes (collection, environment, folder-chain, request-level). Does NOT
    /// populate `global_env` — callers that need it (script execution) load it
    /// separately via `global_env_name`, since it's a different named environment.
    ///
    /// Reused by `build_variable_context()` and `execute()`.
    fn build_variable_scopes(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
    ) -> VariableContext {
        // Precedence (lowest → highest): collection < env < folder < request.
        let mut ctx = VariableContext::default();

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

        ctx
    }

    /// Builds a flattened variable map from all backend-accessible scopes
    /// (collection, environment, folder-chain, request-level).
    ///
    /// Reused by `resolve_request()`, `run_load_test()`, and OAuth2 commands.
    pub fn build_variable_context(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
    ) -> std::collections::HashMap<String, String> {
        self.build_variable_scopes(collection, environment_name, request_path).flatten()
    }

    /// Resolves all {{placeholders}} in `input` using the full variable precedence
    /// chain and returns a ready-to-send `HttpRequest`. Called by both `execute` and
    /// `run_load_test` so resolution logic is never duplicated.
    pub(crate) fn resolve_request(&self, input: &ExecuteRequestInput) -> DomainResult<HttpRequest> {
        // Build variable map: collection < env < folder < request.
        let vars = self.build_variable_context(
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
        );

        // Merge collection auth and headers with request-level values.
        let (effective_auth, effective_headers) = if let Some(col) = &input.collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            let auth = merge_auth(input.auth.clone(), settings.auth);
            let headers = merge_headers(&settings.headers, &input.headers);
            (auth, headers)
        } else {
            (input.auth.clone(), input.headers.clone())
        };

        // Resolve {{placeholders}} in URL and headers.
        let resolved_url = resolve(&input.url, &vars).output;
        let resolved_headers: Vec<Header> = effective_headers
            .iter()
            .map(|h| Header {
                key: resolve(&h.key, &vars).output,
                value: resolve(&h.value, &vars).output,
                enabled: h.enabled,
                description: None,
            })
            .collect();

        Ok(HttpRequest {
            method: input.method.clone(),
            url: resolved_url,
            headers: resolved_headers,
            query_params: input.query_params.clone(),
            body: input.body.clone(),
            auth: effective_auth,
            options: input.options.clone(),
        })
    }

    /// Applies the persistent and in-memory side effects from a `ScriptResult`.
    ///
    /// - `env_var_writes` → always read-modify-write via `env_repo` (always persisted)
    /// - `collection_var_writes` → read-modify-write via `collection_repo.save_settings`
    /// - `global_env_var_writes` → same repo, keyed by `global_env_name`
    /// - `runtime_vars` → merged into `var_ctx.runtime` for the next script phase
    ///
    /// Non-fatal: individual repo errors are logged but do not abort the response.
    fn apply_script_side_effects(
        &self,
        result: &ScriptResult,
        env_name: Option<&str>,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        var_ctx: &mut rocket_environment::VariableContext,
    ) {
        // Apply active-environment writes (always persisted).
        if !result.env_var_writes.is_empty() {
            if let Some(name) = env_name {
                let repo = self.regular_env_repo(collection);
                self.apply_env_writes(repo.as_ref(), name, &result.env_var_writes, true);
            } else {
                tracing::warn!(
                    "rok.setEnvVar write(s) queued but no active environment is selected — write(s) dropped"
                );
            }
        }

        // Apply global-environment writes (always persisted — modifying a shared env).
        if !result.global_env_var_writes.is_empty() {
            if let Some(name) = global_env_name {
                self.apply_env_writes(self.env_repo.as_ref(), name, &result.global_env_var_writes, true);
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
                    let str_val = write.value.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| write.value.to_string());
                    if let Err(e) = self.apply_collection_var_write(col, &write.key, &str_val) {
                        tracing::warn!(error = %e, key = %write.key, "failed to persist collection var write");
                    }
                }
            }
        }

        // Merge runtime vars into context for subsequent phases.
        for (k, v) in &result.runtime_vars {
            if let Some(s) = v.as_str() {
                var_ctx.runtime.insert(k.clone(), s.to_owned());
            }
        }
    }

    /// Read-modify-write helper for a single collection variable.
    ///
    /// Shared by script-side-effect application (`rok.setCollectionVar`) and
    /// the `runtime.actions` set-variable pipeline.
    fn apply_collection_var_write(&self, collection: &str, key: &str, value: &str) -> DomainResult<()> {
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

    /// Read-modify-write helper for env var writes against a named environment.
    ///
    /// `EnvVarWrite.persist` is preserved for wire/API compatibility but
    /// currently has no effect — both call sites in `apply_script_side_effects`
    /// and the `runtime.actions` "environment" scope branch always pass
    /// `force_persist: true`, so every write here is unconditionally persisted.
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

    async fn run_script_phase(
        &self,
        _code: &str,
        ctx: ScriptContext,
        request_name: &str,
        phase: &str,
        all_console: &mut Vec<ConsoleEntry>,
    ) -> ScriptResult {
        let engine = match self.script_engine.as_ref() {
            Some(e) => e,
            None => return ScriptResult::default(),
        };
        match engine.execute(ctx).await {
            Ok(result) => {
                if let Some(ref err) = result.error {
                    self.events.publish(DomainEvent::ScriptError {
                        request_name: request_name.to_string(),
                        phase: phase.to_string(),
                        message: err.clone(),
                    });
                }
                all_console.extend(result.console_entries.clone());
                result
            }
            Err(e) => {
                let message = e.to_string();
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
    ) {
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
            let str_val = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
            let var_name = &action.variable.name;

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
                                if let Err(e) = self.collection_repo.save_folder_variables(col, folder_path, vars) {
                                    tracing::warn!(error = %e, variable = %var_name, "failed to persist folder var from action");
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, variable = %var_name, "failed to read folder vars for action"),
                        }
                    }
                }
                "request" => {
                    if let (Some(col), Some(path)) = (collection, request_path) {
                        match self.collection_repo.get_request_variables(col, path) {
                            Ok(mut vars) => {
                                upsert_variable(&mut vars, var_name, &str_val);
                                if let Err(e) = self.collection_repo.save_request_variables(col, path, vars) {
                                    tracing::warn!(error = %e, variable = %var_name, "failed to persist request var from action");
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, variable = %var_name, "failed to read request vars for action"),
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

    #[tracing::instrument(
        name = "http_request",
        skip(self, input),
        fields(
            method = %input.method,
            url = %input.url,
        )
    )]
    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput> {
        let mut http_request = self.resolve_request(&input)?;

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

        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();
        let mut all_console: Vec<ConsoleEntry> = Vec::new();
        let mut all_test_results: Vec<TestResult> = Vec::new();
        let mut script_error: Option<String> = None;

        // Build scope-separated variable context for script phases. Scripts read
        // individual scopes via rok.getCollectionVar/getEnvVar/getGlobalEnvVar, so
        // each scope must stay distinct rather than being pre-flattened into one.
        let mut var_ctx = self.build_variable_scopes(
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
        );
        if let Some(name) = input.global_env_name.as_deref() {
            if let Ok(global_env) = self.env_repo.get(name) {
                for var in global_env.variables.iter().filter(|v| v.enabled) {
                    var_ctx.global_env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        var_ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }

        // ── Before-request script ─────────────────────────────────────────────
        if let Some(code) = &input.pre_request_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::before_request(
                    code.clone(),
                    var_ctx.clone(),
                    http_request.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "before-request", &mut all_console,
                ).await;

                // Apply request mutations.
                if let Some(ref mutations) = result.request_mutations {
                    if let Some(ref url) = mutations.url {
                        http_request.url = url.clone();
                    }
                    if let Some(ref method_str) = mutations.method {
                        if let Ok(m) = method_str.parse() {
                            http_request.method = m;
                        } else {
                            tracing::warn!(
                                method = %method_str,
                                "req.setMethod() called with an unrecognized HTTP method, ignored"
                            );
                            script_error.get_or_insert_with(|| format!(
                                "req.setMethod('{method_str}') is not a valid HTTP method — ignored."
                            ));
                        }
                    }
                    // Apply header mutations in the order the script issued them —
                    // e.g. deleteHeader() then setHeader() on the same name must
                    // result in the header being present, not dropped.
                    for mutation in &mutations.headers {
                        match mutation {
                            rocket_scripting::HeaderMutation::Set { name, value } => {
                                if let Some(h) = http_request.headers.iter_mut()
                                    .find(|h| h.key.eq_ignore_ascii_case(name))
                                {
                                    h.value = value.clone();
                                } else {
                                    http_request.headers.push(Header::new(name, value));
                                }
                            }
                            rocket_scripting::HeaderMutation::Delete { name } => {
                                http_request.headers.retain(|h| !h.key.eq_ignore_ascii_case(name));
                            }
                        }
                    }
                    if let Some(ms) = mutations.timeout_ms {
                        http_request.options.timeout_ms = ms;
                    }
                    if let Some(ref body_val) = mutations.body {
                        // A JS object/array is unambiguously meant as JSON. A string
                        // may be non-JSON text (XML, plain text, etc) — respect an
                        // explicit Content-Type header the script already set instead
                        // of forcing JSON, which would mislabel the body on the wire.
                        let mode = if body_val.is_object() || body_val.is_array() {
                            rocket_shared::types::BodyMode::Json
                        } else {
                            body_mode_from_content_type(&http_request.headers)
                        };
                        let content = body_val.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| body_val.to_string());
                        http_request.body = Some(rocket_shared::types::Body {
                            mode,
                            content: Some(content),
                            form_data: None,
                            file_path: None,
                        });
                    }
                    if let Some(n) = mutations.max_redirects {
                        http_request.options.max_redirects = Some(n);
                    }
                }

                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut var_ctx,
                );

                if result.error.is_some() {
                    script_error = result.error;
                }
            }
        }

        // ── Before-request actions (runtime.actions, set-variable) ─────────────
        self.apply_actions(
            &input.actions,
            "before-request",
            &request_name,
            &http_request,
            None,
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut var_ctx,
            &input.tags,
            &input.path_params,
        ).await;

        // ── HTTP execution ────────────────────────────────────────────────────
        let response = self.executor.execute(&http_request).await?;

        tracing::info!(
            status = response.status,
            duration_ms = response.duration_ms,
            size_bytes = response.size_bytes,
            "Request completed"
        );

        // ── After-response script ─────────────────────────────────────────────
        if let Some(code) = &input.post_response_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::after_response(
                    code.clone(),
                    var_ctx.clone(),
                    http_request.clone(),
                    response.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "after-response", &mut all_console,
                ).await;
                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut var_ctx,
                );
                if result.error.is_some() && script_error.is_none() {
                    script_error = result.error;
                }
            }
        }

        // ── Tests script ──────────────────────────────────────────────────────
        if let Some(code) = &input.tests_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::tests(
                    code.clone(),
                    var_ctx.clone(),
                    http_request.clone(),
                    response.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "tests", &mut all_console,
                ).await;
                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut var_ctx,
                );
                all_test_results.extend(result.test_results.clone());
                if result.error.is_some() && script_error.is_none() {
                    script_error = result.error;
                }
            }
        }

        // ── After-response actions (runtime.actions, set-variable) ─────────────
        self.apply_actions(
            &input.actions,
            "after-response",
            &request_name,
            &http_request,
            Some(&response),
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut var_ctx,
            &input.tags,
            &input.path_params,
        ).await;

        // ── Declarative assertions ────────────────────────────────────────────
        // Run after tests script so JS test results appear first in TestsPanel.
        let assertion_results = crate::assertion_evaluator::evaluate_assertions(
            &input.assertions,
            &response,
        );
        all_test_results.extend(assertion_results);

        // ── Emit events ───────────────────────────────────────────────────────
        if !all_console.is_empty() {
            let entries = all_console.iter().map(|e| {
                let level = match e.level {
                    ConsoleLevel::Log => "log",
                    ConsoleLevel::Warn => "warn",
                    ConsoleLevel::Error => "error",
                };
                serde_json::json!({ "level": level, "message": e.message })
            }).collect();
            self.events.publish(DomainEvent::ConsoleOutput {
                request_name: request_name.clone(),
                entries,
            });
        }

        if !all_test_results.is_empty() {
            let results = all_test_results.iter().map(|t| {
                let status = match t.status {
                    TestStatus::Passed => "passed",
                    TestStatus::Failed => "failed",
                };
                serde_json::json!({ "name": t.name, "status": status, "error": t.error })
            }).collect();
            self.events.publish(DomainEvent::TestsCompleted {
                request_name: request_name.clone(),
                results,
            });
        }

        // Persist history (non-fatal — a save failure won't cancel the response).
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &http_request.url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
        let _ = self.history_repo.save(&entry);

        // Publish domain event.
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: http_request.url.clone(),
            status: response.status,
            duration_ms: response.duration_ms,
        });

        Ok(ExecuteRequestOutput {
            response,
            test_results: all_test_results,
            console_entries: all_console,
            script_error,
        })
    }

    pub async fn run_load_test(
        &self,
        input: ExecuteRequestInput,
        config: LoadTestConfig,
    ) -> DomainResult<LoadTestResult> {
        let resolved = self.resolve_request(&input)?;
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
        let engine = self.script_engine.as_ref().ok_or_else(|| {
            rocket_shared::error::DomainError::Internal("script engine not configured".into())
        })?;

        let response: HttpResponse = serde_json::from_str(response_json).map_err(|e| {
            rocket_shared::error::DomainError::InvalidInput(format!("invalid response JSON: {e}"))
        })?;

        let mut var_ctx = VariableContext::default();
        if let Ok(settings) = self.collection_repo.get_settings(collection_root) {
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                var_ctx.collection.insert(cv.key.clone(), val);
            }
        }

        let code = format!(
            "rok.setVar('__jsonq_result__', (function(){{ return ({expression}); }})());"
        );
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

        let result = engine.execute(ctx).await?;
        if let Some(err) = result.error {
            return Err(rocket_shared::error::DomainError::InvalidInput(err));
        }
        Ok(result
            .runtime_vars
            .get("__jsonq_result__")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

}

/// Map an `Auth` variant to a short kebab-case label for audit events.
/// Returns `None` for `Auth::None` and `Auth::Inherit` because those are not
/// "sensitive auth used" — no credential is actually being sent on the wire.
fn sensitive_auth_label(auth: &Auth) -> Option<&'static str> {
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
fn merge_auth(request_auth: Auth, collection_auth: Option<Auth>) -> Auth {
    match request_auth {
        Auth::None => collection_auth.unwrap_or(Auth::None),
        explicit => explicit,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_collection::{Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable, Request as CollectionRequest};
    use rocket_environment::{Environment, Variable};
    use rocket_http::{CookieJar, HttpResponse};
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

    // In-memory history repo.
    struct MockHistoryRepo {
        entries: Mutex<Vec<HistoryEntry>>,
    }

    impl MockHistoryRepo {
        fn new() -> Self {
            Self { entries: Mutex::new(Vec::new()) }
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
    }

    impl StubCollectionRepo {
        fn empty() -> Self {
            Self { settings: CollectionSettings::default(), folder_vars: vec![], request_vars: vec![] }
        }

        fn with_settings(settings: CollectionSettings) -> Self {
            Self { settings, folder_vars: vec![], request_vars: vec![] }
        }

        fn with_folder_vars(mut self, vars: Vec<CollectionVariable>) -> Self {
            self.folder_vars = vars;
            self
        }

        fn with_request_vars(mut self, vars: Vec<CollectionVariable>) -> Self {
            self.request_vars = vars;
            self
        }
    }

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> { Ok(vec![]) }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn get_summaries(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> { Ok(path.to_string()) }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.clone())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> { Ok(()) }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(self.folder_vars.clone())
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(self.request_vars.clone())
        }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
    }

    fn sample_input(url: &str, env_name: Option<&str>) -> ExecuteRequestInput {
        ExecuteRequestInput {
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
        }
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
        );

        let mut input = sample_input("{{oidc-baseurl}}/api/data", Some("staging"));
        input.collection = None;

        let config = rocket_http::LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0, duration_cap_secs: None };
        let result = svc.run_load_test(input, config).await.unwrap();

        assert_eq!(result.total_requests, 1);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);

        // Verify the resolved URL reached the executor.
        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://auth.local/api/data");
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
        );

        let input = sample_input("{{oidc-baseurl}}/api/v1/users", Some("dev"));
        let resolved = svc.resolve_request(&input).unwrap();
        assert_eq!(resolved.url, "https://auth.local/api/v1/users");
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
            fn search(&self, filter: &rocket_history::HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
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
        );

        svc.execute(sample_input("https://example.com", None)).await.unwrap();

        assert_eq!(history_arc.entries.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn execute_publishes_event() {
        use rocket_shared::events::DomainEvent;

        let publisher = Arc::new(RecordingPublisher { events: Mutex::new(vec![]) });

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
        );

        svc.execute(sample_input("https://example.com/items", None)).await.unwrap();

        let events = pub_arc.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::RequestExecuted { status: 201, .. }));
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
        assert!(merged.iter().any(|h| h.key == "X-Tenant" && h.value == "acme"));
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
        let collection_auth = Some(Auth::Bearer { token: "col_tok".into() });
        let result = merge_auth(Auth::None, collection_auth);
        assert_eq!(result, Auth::Bearer { token: "col_tok".into() });
    }

    #[test]
    fn merge_auth_request_takes_precedence_over_collection() {
        let collection_auth = Some(Auth::Bearer { token: "col_tok".into() });
        let request_auth = Auth::Basic { username: "user".into(), password: "pass".into() };
        let result = merge_auth(request_auth.clone(), collection_auth);
        assert_eq!(result, request_auth);
    }

    #[test]
    fn merge_auth_none_collection_returns_none() {
        let result = merge_auth(Auth::None, None);
        assert_eq!(result, Auth::None);
    }

    fn cv(key: &str, value: &str) -> CollectionVariable {
        CollectionVariable { key: key.into(), value: value.into(), initial_value: String::new(), enabled: true, secret: false }
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
            auth: Some(Auth::Bearer { token: "col_tok".into() }),
            headers: vec![],
            variables: vec![],
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
                })
            }
        }

        let executor = Arc::new(CapturingExecutor { last_auth: Mutex::new(None) });

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
        );

        let mut input = sample_input("https://api.example.com", None);
        input.collection = Some("my-api".into());
        svc.execute(input).await.unwrap();

        let captured = exec_arc.last_auth.lock().unwrap().clone().unwrap();
        assert_eq!(captured, Auth::Bearer { token: "col_tok".into() });
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
        let publisher = Arc::new(CapturingAuditPublisher { captured: Mutex::new(vec![]) });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );

        let mut input = sample_input("https://api.example.com/users", None);
        input.auth = Auth::Bearer { token: "tok".into() };
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
        let publisher = Arc::new(CapturingAuditPublisher { captured: Mutex::new(vec![]) });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            publisher.clone(),
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
            Self { post_response_result: Mutex::new(result) }
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
                Ok(self.post_response_result.lock().expect("lock poisoned").clone())
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
        fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
    }

    struct SharedEnvRepo(Arc<RecordingEnvRepo>);
    impl rocket_environment::EnvironmentRepository for SharedEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> { self.0.list() }
        fn get(&self, name: &str) -> DomainResult<Environment> { self.0.get(name) }
        fn save(&self, env: &Environment) -> DomainResult<()> { self.0.save(env) }
        fn delete(&self, name: &str) -> DomainResult<()> { self.0.delete(name) }
    }

    struct RecordingCollectionRepo {
        settings: Mutex<CollectionSettings>,
        saved_settings: Mutex<Vec<CollectionSettings>>,
    }

    impl RecordingCollectionRepo {
        fn with_settings(settings: CollectionSettings) -> Arc<Self> {
            Arc::new(Self {
                settings: Mutex::new(settings),
                saved_settings: Mutex::new(vec![]),
            })
        }
        fn last_saved_settings(&self) -> Option<CollectionSettings> {
            self.saved_settings.lock().expect("lock").last().cloned()
        }
    }

    impl CollectionRepository for RecordingCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> { Ok(vec![]) }
        fn get(&self, _: &str) -> DomainResult<Collection> { Err(DomainError::NotFound("stub".into())) }
        fn get_summaries(&self, _: &str) -> DomainResult<Collection> { Err(DomainError::NotFound("stub".into())) }
        fn create(&self, _: &str) -> DomainResult<Collection> { Err(DomainError::NotFound("stub".into())) }
        fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> { Err(DomainError::NotFound("stub".into())) }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> { Ok(path.to_string()) }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.lock().expect("lock").clone())
        }
        fn save_settings(&self, _: &str, settings: &CollectionSettings) -> DomainResult<()> {
            self.saved_settings.lock().expect("lock").push(settings.clone());
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
    }

    struct SharedCollectionRepo(Arc<RecordingCollectionRepo>);
    impl CollectionRepository for SharedCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> { self.0.list() }
        fn get(&self, n: &str) -> DomainResult<Collection> { self.0.get(n) }
        fn get_summaries(&self, n: &str) -> DomainResult<Collection> { self.0.get_summaries(n) }
        fn create(&self, n: &str) -> DomainResult<Collection> { self.0.create(n) }
        fn delete(&self, n: &str) -> DomainResult<()> { self.0.delete(n) }
        fn rename(&self, a: &str, b: &str) -> DomainResult<()> { self.0.rename(a, b) }
        fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> { self.0.get_request(a, b) }
        fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> { self.0.save_request(a, b, c) }
        fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> { self.0.rename_request(a, b, c) }
        fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> { self.0.delete_request(a, b) }
        fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> { self.0.create_folder(a, b) }
        fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> { self.0.delete_folder(a, b) }
        fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> { self.0.move_item(a, b, c, d) }
        fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> { self.0.reorder_items(a, b, c) }
        fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> { self.0.get_settings(n) }
        fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> { self.0.save_settings(n, s) }
        fn get_folder_chain_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_folder_chain_variables(a, b) }
        fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_folder_variables(a, b) }
        fn save_folder_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> { self.0.save_folder_variables(a, b, c) }
        fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_request_variables(a, b) }
        fn save_request_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> { self.0.save_request_variables(a, b, c) }
    }

    /// Script engine that returns a custom ScriptResult for the before-request phase.
    struct MockBeforeRequestEngine {
        result: Mutex<ScriptResult>,
    }

    impl MockBeforeRequestEngine {
        fn returning(result: ScriptResult) -> Self {
            Self { result: Mutex::new(result) }
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
            Arc::new(Self { last_body: Mutex::new(None) })
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
        )
        .with_script_engine(engine)
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

        let saved = env_repo.last_saved()
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

        let saved = env_repo.last_saved()
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

        let saved = env_repo.last_saved().expect("env_repo.save() should have been called");
        let var = saved.variables.iter().find(|v| v.key == "API_KEY").expect("API_KEY present");
        assert_eq!(var.value, "sk-new");
        assert!(var.secret, "secret flag must be preserved across a script write");
    }

    #[tokio::test]
    async fn post_response_script_env_var_delete_then_set_preserves_secret_flag_and_publishes_audit() {
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

        let audit_publisher = Arc::new(CapturingAuditPublisher { captured: Mutex::new(vec![]) });
        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            audit_publisher.clone(),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo.last_saved().expect("env_repo.save() should have been called");
        let var = saved.variables.iter().find(|v| v.key == "API_KEY").expect("API_KEY present");
        assert_eq!(var.value, "sk-new");
        assert!(var.secret, "secret flag must survive a delete-then-recreate within one script");

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

        let saved = env_repo.last_saved().expect("env_repo.save() should have been called");
        let var = saved.variables.iter().find(|v| v.key == "NEW_TOKEN").expect("NEW_TOKEN present");
        assert!(!var.secret, "a script must not be able to implicitly create a secret variable");
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

        let event_publisher = Arc::new(RecordingPublisher { events: Mutex::new(vec![]) });
        struct SharedPub(Arc<RecordingPublisher>);
        impl rocket_shared::events::EventPublisher for SharedPub {
            fn publish(&self, event: DomainEvent) {
                self.0.publish(event);
            }
        }
        let audit_publisher = Arc::new(CapturingAuditPublisher { captured: Mutex::new(vec![]) });

        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(SharedPub(Arc::clone(&event_publisher))),
            audit_publisher.clone(),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let published = event_publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(e, DomainEvent::EnvironmentSaved { name } if name == "prod")),
            "expected EnvironmentSaved, got {:?}", *published
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
            "expected SecretVariableWritten, got {:?}", *captured
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

        let audit_publisher = Arc::new(CapturingAuditPublisher { captured: Mutex::new(vec![]) });
        let svc = RequestExecutionService::new_with_audit(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            audit_publisher.clone(),
        )
        .with_script_engine(Box::new(MockScriptEngine::returning_post_response(result)));

        let mut input = sample_input("https://example.com", Some("prod"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let captured = audit_publisher.captured.lock().expect("lock");
        assert!(
            !captured.iter().any(|k| matches!(k, AuditEventKind::SecretVariableWritten { .. })),
            "a non-secret write must not publish SecretVariableWritten, got {:?}", *captured
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

        let saved = col_repo.last_saved_settings()
            .expect("save_settings should have been called");
        let written = saved.variables.iter().find(|v| v.key == "BASE_URL");
        assert_eq!(written.map(|v| v.value.as_str()), Some("https://new.example.com"));
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

        let event_publisher = Arc::new(RecordingPublisher { events: Mutex::new(vec![]) });
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
            "expected CollectionVariableWritten, got {:?}", *published
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

        let saved = env_repo.last_saved()
            .expect("env_repo.save() should have been called for global env write");
        assert_eq!(saved.get_value("API_KEY"), Some("new-key"));
    }

    #[tokio::test]
    async fn before_request_script_invalid_method_is_surfaced_as_script_error() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_shared::types::HttpMethod;

        // req.setMethod('PACTH') — a typo that doesn't parse as a valid method.
        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                method: Some("PACTH".into()),
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
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.method = HttpMethod::Get;
        input.pre_request_script = Some("// pre".into());
        let output = svc.execute(input).await.expect("execute failed");

        assert_eq!(output.response.status, 200, "the original method must still be used, unmodified");
        let err = output.script_error.expect("an invalid setMethod() must surface a script_error");
        assert!(err.contains("PACTH"), "error should name the invalid method: {err}");
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
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing.last_body().expect("executor should have received a body");
        assert_eq!(body.mode, BodyMode::Json);
        assert_eq!(body.content.as_deref(), Some(r#"{"injected":true}"#));
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
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let body = body_capturing.last_body().expect("executor should have received a body");
        assert_eq!(
            body.mode,
            BodyMode::Xml,
            "a string body should respect the script's explicit Content-Type instead of being forced to JSON"
        );
        assert_eq!(body.content.as_deref(), Some("<a/>"));
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
        assert!(msg.contains("169.254.169.254"), "error should name the blocked host: {msg}");
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
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://192.168.1.1/",
            &policy,
        );
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
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://192.168.1.1/",
            &policy,
        );
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
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result = svc.check_request_guard(
            "http://192.168.1.1/foo",
            "http://192.168.1.1/bar",
            &policy,
        );
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
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "not a url", &policy);
        assert!(result.is_err());
    }

    /// Executor that captures the RequestOptions it received.
    struct OptionsCapturingExecutor {
        last_options: Mutex<Option<RequestOptions>>,
    }

    impl OptionsCapturingExecutor {
        fn new() -> Arc<Self> {
            Arc::new(Self { last_options: Mutex::new(None) })
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
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute failed");

        let opts = options_capturing.last_options().expect("executor should have received options");
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
        async fn execute(&self, _ctx: ScriptContext) -> rocket_shared::error::DomainResult<ScriptResult> {
            let mut vars = std::collections::HashMap::new();
            vars.insert("__jsonq_result__".to_string(), self.value.clone());
            Ok(ScriptResult { runtime_vars: vars, ..Default::default() })
        }
    }

    /// Script engine stub that simulates a jsonq expression throwing.
    struct ErrorJsonqEngine;

    #[async_trait]
    impl ScriptEngine for ErrorJsonqEngine {
        async fn execute(&self, _ctx: ScriptContext) -> rocket_shared::error::DomainResult<ScriptResult> {
            Ok(ScriptResult { error: Some("ReferenceError: nope".into()), ..Default::default() })
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
        }
    }

    #[tokio::test]
    async fn after_response_action_writes_collection_variable() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine { value: serde_json::json!("extracted-value") }),
        );

        let actions = vec![stub_action("collection", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions, "after-response", "Get User", &http_request, Some(&response),
            None, Some("my-api"), None, &mut var_ctx, &[], &[],
        ).await;

        let saved = col_repo.last_saved_settings().expect("save_settings should have been called");
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
            Box::new(FixedJsonqEngine { value: serde_json::json!("token-123") }),
        );

        let actions = vec![stub_action("environment", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions, "after-response", "Get User", &http_request, Some(&response),
            Some("dev"), None, None, &mut var_ctx, &[], &[],
        ).await;

        let saved = env_repo.last_saved().expect("env_repo.save() should have been called");
        assert_eq!(saved.get_value("extracted"), Some("token-123"));
    }

    #[tokio::test]
    async fn after_response_action_writes_runtime_variable_without_persisting() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine { value: serde_json::json!("in-memory-value") }),
        );

        let actions = vec![stub_action("runtime", "after-response", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions, "after-response", "Get User", &http_request, Some(&response),
            None, None, None, &mut var_ctx, &[], &[],
        ).await;

        assert_eq!(var_ctx.runtime.get("extracted"), Some(&"in-memory-value".to_string()));
        assert!(col_repo.last_saved_settings().is_none(), "runtime scope must never persist");
    }

    #[tokio::test]
    async fn disabled_action_is_skipped() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine { value: serde_json::json!("should-not-be-written") }),
        );

        let actions = vec![stub_action("collection", "after-response", true)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions, "after-response", "Get User", &http_request, Some(&response),
            None, Some("my-api"), None, &mut var_ctx, &[], &[],
        ).await;

        assert!(col_repo.last_saved_settings().is_none(), "disabled action must not run");
    }

    #[tokio::test]
    async fn action_wrong_phase_is_skipped() {
        let col_repo = RecordingCollectionRepo::with_settings(CollectionSettings::default());
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedCollectionRepo(Arc::clone(&col_repo))),
            Box::new(FixedJsonqEngine { value: serde_json::json!("should-not-be-written") }),
        );

        // A before-request action must not fire during the after-response pass.
        let actions = vec![stub_action("collection", "before-request", false)];
        let http_request = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let response = stub_action_response();
        let mut var_ctx = VariableContext::default();

        svc.apply_actions(
            &actions, "after-response", "Get User", &http_request, Some(&response),
            None, Some("my-api"), None, &mut var_ctx, &[], &[],
        ).await;

        assert!(col_repo.last_saved_settings().is_none(), "wrong-phase action must not run");
    }

    // Environment repo backed by a map, so a test can look up both the active
    // and global environments by name.
    struct MultiEnvRepo {
        envs: std::collections::HashMap<String, Environment>,
    }

    impl MultiEnvRepo {
        fn new(envs: Vec<Environment>) -> Self {
            Self { envs: envs.into_iter().map(|e| (e.name.clone(), e)).collect() }
        }
    }

    impl rocket_environment::EnvironmentRepository for MultiEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.envs.values().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.envs.get(name).cloned().ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> { Ok(()) }
        fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
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

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
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
        assert_eq!(captured.env.get("BASE_URL"), Some(&"https://dev.local".to_string()));
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

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
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
        )
        .with_script_engine(Box::new(SharedCapturingEngineSecrets(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(captured.secret_values.contains("sk-live-abcdef123"), "secret env var value must be in secret_values");
        assert!(captured.secret_values.contains("col-secret-val"), "secret collection var value must be in secret_values");
        assert!(!captured.secret_values.contains("plain-not-secret"), "non-secret env var value must not be in secret_values");
        assert!(!captured.secret_values.contains("col-plain-val"), "non-secret collection var value must not be in secret_values");
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

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
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
        )
        .with_script_engine(Box::new(SharedCapturingEngineShort(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(!captured.secret_values.contains("abc"), "secrets shorter than MIN_REDACTION_LEN must not be added to secret_values");
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

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
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
        )
        .with_script_engine(Box::new(SharedCapturingEngineExact(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(captured.secret_values.contains("abcdef"), "a secret exactly MIN_REDACTION_LEN characters long must be added to secret_values");
    }

    #[tokio::test]
    async fn global_env_secret_populates_secret_values() {
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::new("BASE_URL", "https://dev.local"));
        let mut global_env = Environment::new("shared-global");
        global_env.set_variable(Variable::secret("GLOBAL_TOKEN", "glbl-secret-999"));
        global_env.set_variable(Variable::new("GLOBAL_PLAIN", "glbl-plain-val"));
        let env_repo = MultiEnvRepo::new(vec![active_env, global_env]);

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
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
        )
        .with_script_engine(Box::new(SharedCapturingEngineGlobal(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.global_env_name = Some("shared-global".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(captured.secret_values.contains("glbl-secret-999"), "secret global env var value must be in secret_values");
        assert!(!captured.secret_values.contains("glbl-plain-val"), "non-secret global env var value must not be in secret_values");
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

        let output = svc.execute(input).await.expect("execute must succeed despite a bad jsonq expression");
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
}
