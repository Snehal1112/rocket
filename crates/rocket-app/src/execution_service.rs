use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::CollectionRepository;
use rocket_environment::{resolve, EnvironmentRepository, VariableContext};
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
    env_repo: Box<dyn EnvironmentRepository>,
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
            executor,
            history_repo,
            collection_repo,
            cookie_repo,
            events,
            audit,
            script_engine: None,
        }
    }

    /// Attach a script engine. Call this after construction in the DI layer.
    pub fn with_script_engine(mut self, engine: Box<dyn ScriptEngine>) -> Self {
        self.script_engine = Some(engine);
        self
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
                ctx.collection.insert(cv.key.clone(), effective_val(cv));
            }
        }

        if let Some(name) = environment_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    ctx.env.insert(k.to_string(), v.to_string());
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

        ctx.flatten()
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
    /// - `env_var_writes` with `persist: true` → read-modify-write via `env_repo`
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
        // Apply active-environment writes.
        if !result.env_var_writes.is_empty() {
            if let Some(name) = env_name {
                self.apply_env_writes(name, &result.env_var_writes, false);
            }
        }

        // Apply global-environment writes (always persisted — modifying a shared env).
        if !result.global_env_var_writes.is_empty() {
            if let Some(name) = global_env_name {
                self.apply_env_writes(name, &result.global_env_var_writes, true);
            }
        }

        // Apply collection variable writes.
        if !result.collection_var_writes.is_empty() {
            if let Some(col) = collection {
                if let Ok(mut settings) = self.collection_repo.get_settings(col) {
                    for write in &result.collection_var_writes {
                        let str_val = write.value.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| write.value.to_string());
                        if let Some(existing) = settings.variables.iter_mut()
                            .find(|v| v.key == write.key)
                        {
                            existing.value = str_val;
                        } else {
                            settings.variables.push(rocket_collection::CollectionVariable {
                                key: write.key.clone(),
                                value: str_val,
                                initial_value: String::new(),
                                enabled: true,
                                secret: false,
                            });
                        }
                    }
                    let _ = self.collection_repo.save_settings(col, &settings);
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

    /// Read-modify-write helper for env var writes against a named environment.
    ///
    /// When `force_persist` is true (used for global env writes), all writes go
    /// to disk regardless of the individual `persist` flag. For active-environment
    /// writes, only entries with `persist: true` are saved.
    fn apply_env_writes(
        &self,
        env_name: &str,
        writes: &[rocket_scripting::EnvVarWrite],
        force_persist: bool,
    ) {
        let persist_writes: Vec<_> = writes
            .iter()
            .filter(|w| force_persist || w.persist)
            .collect();
        if persist_writes.is_empty() {
            return;
        }
        if let Ok(mut env) = self.env_repo.get(env_name) {
            for write in persist_writes {
                if write.value.is_null() {
                    env.remove_variable(&write.key);
                } else {
                    let str_val = write.value.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| write.value.to_string());
                    env.set_variable(rocket_environment::Variable::new(
                        write.key.clone(),
                        str_val,
                    ));
                }
            }
            let _ = self.env_repo.save(&env);
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
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: phase.to_string(),
                    message: e.to_string(),
                });
                ScriptResult::default()
            }
        }
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

        // Build variable context for script phases.
        let var_flat = self.build_variable_context(
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
        );
        let mut var_ctx = VariableContext::default();
        // Populate env scope from flattened map (best-effort; full scope separation not needed here).
        for (k, v) in &var_flat {
            var_ctx.env.insert(k.clone(), v.clone());
        }

        // ── Before-request script ─────────────────────────────────────────────
        if let Some(code) = &input.pre_request_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::before_request(
                    code.clone(),
                    var_ctx.clone(),
                    http_request.clone(),
                    env_name.clone(),
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
                        }
                    }
                    for (name, value) in &mutations.headers_set {
                        if let Some(h) = http_request.headers.iter_mut()
                            .find(|h| h.key.eq_ignore_ascii_case(name))
                        {
                            h.value = value.clone();
                        } else {
                            http_request.headers.push(Header::new(name, value));
                        }
                    }
                    http_request.headers.retain(|h| {
                        !mutations.headers_deleted.iter().any(|d| d.eq_ignore_ascii_case(&h.key))
                    });
                    if let Some(ms) = mutations.timeout_ms {
                        http_request.options.timeout_ms = ms;
                    }
                    if let Some(ref body_val) = mutations.body {
                        let content = body_val.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| body_val.to_string());
                        http_request.body = Some(rocket_shared::types::Body {
                            mode: rocket_shared::types::BodyMode::Json,
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
        use std::sync::Mutex;

        struct RecordingPublisher {
            events: Mutex<Vec<DomainEvent>>,
        }

        impl rocket_shared::events::EventPublisher for RecordingPublisher {
            fn publish(&self, event: DomainEvent) {
                self.events.lock().unwrap().push(event);
            }
        }

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
    async fn post_response_script_env_var_no_persist_skips_env_repo_save() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "TOKEN".into(),
                value: serde_json::json!("runtime-only"),
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

        assert!(
            env_repo.last_saved().is_none(),
            "env_repo.save() must NOT be called for non-persist writes"
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
}
