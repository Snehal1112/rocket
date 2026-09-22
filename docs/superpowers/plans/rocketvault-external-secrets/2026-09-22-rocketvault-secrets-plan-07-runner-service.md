# RocketVault Secrets Plan 07: CollectionRunnerService Once-Per-Run Wiring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `CollectionRunnerService::run` fetch every External Secret value
exactly **once per run**, and reuse that same resolved map for every step's
`begin_phases` call — not re-fetch per step. RocketVault secret resolution is
a real network round-trip per secret name; the active environment's
`external_secrets` bindings do not change mid-run, so re-resolving per
request would multiply RocketVault round-trips by the number of requests in
the run. This is spec acceptance criterion 7.

**Architecture:** `CollectionRunnerService` holds no `RequestExecutionService`
of its own — `run()` takes `exec: &RequestExecutionService` as a parameter and
threads it down to the per-step `run_step` helper (confirmed by reading
`crates/rocket-app/src/collection_runner_service.rs:177-219`; the struct
itself only stores `collection_repo`/`events`/`cancelled`/`in_flight`). This
plan adds one `exec.resolve_external_secrets(...)` call at the top of `run()`,
before the step loop starts, and threads the resulting
`HashMap<String, String>` as a new parameter through `run_step` into its one
`begin_phases` call site (`collection_runner_service.rs:362`). `begin_phases`
itself stays synchronous (Plan 06) — this is the only new `.await` point this
plan introduces; the loop body's existing structure and its own async phase
calls (`run_before_request_phase`, `send_request`, `run_after_response_phase`,
`run_tests_phase`, `finish_phases`) are otherwise unchanged.

**Task count:** This plan is 2 tasks, not the usual 3-task budget for this
series (see `00-plan-index.md`'s breakdown table, row 07). The work genuinely
only needs two: one to make the call-site change (Task 1), one to prove it
with a call-count assertion (Task 2). There is no third slice to carve out —
padding to 3 would mean splitting either of those into an artificially small
half.

**Tech Stack:** Rust, `tokio` (async test), `std::sync::atomic::AtomicUsize`
(call-count fake).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
§4.6 (design) and acceptance criterion 7 ("Running a Collection Runner run
against a folder that uses external secrets issues exactly one
`list`/value-resolution pass per run, not one per request in the run —
verified via a call-count assertion against the fake `RocketVaultClient` in
tests."). Plan index: `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`
(locked interface contract). Previous plan: [Plan 06](2026-09-22-rocketvault-secrets-plan-06-execution-service.md)
(produces `RequestExecutionService::resolve_external_secrets` and the
`begin_phases(&self, input: &ExecuteRequestInput, external_secrets: &HashMap<String, String>)`
signature this plan calls into).

## Global Constraints

- **Plan 06's plan file did not exist yet when this plan was written** — only
  Plans 01, 02, 03, and the index existed on disk at the time
  (`docs/superpowers/plans/rocketvault-external-secrets/` was checked
  directly). Every Plan-06-owned signature used below is taken verbatim from
  `00-plan-index.md`'s "Locked interface contract" §"`rocket-app` (modified,
  Plan 06/07)" section:
  ```rust
  impl RequestExecutionService {
      pub async fn resolve_external_secrets(
          &self,
          collection: Option<&str>,
          environment_name: Option<&str>,
      ) -> DomainResult<std::collections::HashMap<String, String>>;
  }
  ```
  and `begin_phases` gaining an added `external_secrets: &HashMap<String, String>`
  parameter. **Correction (post-Plan-06 whole-branch review):** Plan 06 as
  originally written and initially implemented omitted the `collection`
  parameter, which routed the lookup through the wrong (app-level, not
  collection-scoped) environment repository — fixed before Plan 07 started,
  in Plan 06's own final-review fix round. The signature above is the
  corrected, actually-shipped one. This plan's Task 1 call site (below) must
  pass `Some(&input.collection)` as the first argument — `RunCollectionInput.collection`
  is a plain `String`, not `Option<String>`, so it is always present for a
  runner call, unlike `execute()`'s `input.collection: Option<String>`. If
  Plan 06 has since been implemented with a still-different exact shape,
  reconcile this plan's Task 1 diff against that signature before running it
  — the call sites below assume the signatures exactly as quoted.
- **No stored `RequestExecutionService` field on `CollectionRunnerService`.**
  Confirmed by reading the struct definition
  (`crates/rocket-app/src/collection_runner_service.rs:177-188`): it holds
  `collection_repo`, `events`, `cancelled`, `in_flight` — nothing else.
  `exec: &RequestExecutionService` is a parameter of `run()` and is passed by
  reference into `run_step`. Do not add a field for it; thread it as a
  parameter, matching how `exec` already flows today.
- **`begin_phases` has exactly one call site in this file** — inside
  `run_step`, at `collection_runner_service.rs:362`
  (`exec.begin_phases(&step_input)`). Confirmed by reading the whole file;
  there is no other place in `collection_runner_service.rs` that calls it.
- **The run-level environment name is `input.environment_name`** — a field of
  `RunCollectionInput` (`Option<String>`), already read at
  `collection_runner_service.rs:357` inside `run_step`
  (`input.environment_name.as_deref()`, passed into `build_step_input`).
  Task 1 reads the exact same field at the top of `run()`, once, before the
  loop.
- **`environment_name: None` must stay a zero-I/O no-op.** Every existing test
  in `collection_runner_service.rs` builds its input via `sample_run_input()`,
  which sets `environment_name: None`. `execution_service.rs:287`
  (`if let Some(name) = environment_name { ... }`) shows this file's
  established convention: an environment lookup only happens when a name is
  actually given. `resolve_external_secrets(None)` is expected (per that same
  convention, inherited unmodified from Plan 06 — not reimplemented here) to
  short-circuit to `Ok(HashMap::new())` without touching any environment
  repo, secret-manager repo, secret store, or vault fetcher. This is what
  lets every pre-existing `collection_runner_service.rs` test keep compiling
  and passing with zero fixture changes after Task 1 lands — none of them
  need updating in this plan.
- Test code in this plan uses `.expect("message")` for fallible calls, not the
  bare panicking shorthand, matching this repository's stricter Rust safety
  convention even in test paths.

---

## Task 1: Fetch once, reuse across the loop

**Files:**
- Modify: `crates/rocket-app/src/collection_runner_service.rs`

**Interfaces:**
- Consumes: `RequestExecutionService::resolve_external_secrets` and the
  updated `begin_phases` signature (Plan 06).
- Produces: `CollectionRunnerService::run` now issues exactly one
  `resolve_external_secrets` call per run — consumed by Task 2's test, and by
  Plan 08's Tauri command wiring (which calls `run()` unchanged).

Unlike Plans 01–03's per-task TDD cycle (new pure types with no existing call
site to break), this task is a call-site update to code that already has
extensive test coverage (18 tests currently in this file's `#[cfg(test)] mod
tests`). Its own steps below are implement → compile-check → run the existing
suite for regressions, rather than write-a-failing-test-first — there is no
new isolated unit of behavior to red/green here in isolation; Task 2 owns the
test that actually proves the once-per-run property.

- [ ] **Step 1: Change `run()` to resolve external secrets once, before the loop**

Current code (`collection_runner_service.rs:215-254`):

```rust
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunCollectionInput,
    ) -> DomainResult<RunSummary> {
        let collection = self.collection_repo.get(&input.collection)?;
        let items = flatten_run_set(&collection, input.folder_path.as_deref())?;
        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }

        self.events.publish(DomainEvent::RunnerStarted {
            run_id: run_id.clone(),
            collection: input.collection.clone(),
            folder_path: input.folder_path.clone(),
            total_steps: items.len(),
        });

        let mut steps: Vec<RunStepResult> = Vec::new();
        let mut carried_runtime: HashMap<String, String> = HashMap::new();
        let mut cursor = 0usize;
        let mut stopped_reason = StoppedReason::Completed;

        while cursor < items.len() {
            if self.is_cancelled(&run_id) {
                stopped_reason = StoppedReason::Cancelled;
                break;
            }
            if steps.len() >= MAX_RUN_STEPS {
                stopped_reason = StoppedReason::StepLimitReached {
                    limit: MAX_RUN_STEPS,
                };
                break;
            }

            let item = &items[cursor];
            let outcome = self
                .run_step(exec, &input, item, steps.len(), &mut carried_runtime)
                .await;
            let mut result = outcome.result;
```

New code:

```rust
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunCollectionInput,
    ) -> DomainResult<RunSummary> {
        let collection = self.collection_repo.get(&input.collection)?;
        let items = flatten_run_set(&collection, input.folder_path.as_deref())?;

        // Fetch every External Secret value exactly once, up front, and reuse
        // this same map for every step below. RocketVault value resolution is
        // a real network round-trip per secret name; the active environment's
        // external_secrets bindings do not change mid-run, so re-fetching per
        // step would multiply RocketVault round-trips by the step count
        // (spec acceptance criterion 7). A failure here fails the whole run
        // before any step executes and before RunnerStarted is even
        // published — no step, and no "this run started" signal, for a run
        // whose secrets could not be resolved.
        let external_secrets = exec
            .resolve_external_secrets(Some(&input.collection), input.environment_name.as_deref())
            .await?;

        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }

        self.events.publish(DomainEvent::RunnerStarted {
            run_id: run_id.clone(),
            collection: input.collection.clone(),
            folder_path: input.folder_path.clone(),
            total_steps: items.len(),
        });

        let mut steps: Vec<RunStepResult> = Vec::new();
        let mut carried_runtime: HashMap<String, String> = HashMap::new();
        let mut cursor = 0usize;
        let mut stopped_reason = StoppedReason::Completed;

        while cursor < items.len() {
            if self.is_cancelled(&run_id) {
                stopped_reason = StoppedReason::Cancelled;
                break;
            }
            if steps.len() >= MAX_RUN_STEPS {
                stopped_reason = StoppedReason::StepLimitReached {
                    limit: MAX_RUN_STEPS,
                };
                break;
            }

            let item = &items[cursor];
            let outcome = self
                .run_step(
                    exec,
                    &input,
                    item,
                    steps.len(),
                    &mut carried_runtime,
                    &external_secrets,
                )
                .await;
            let mut result = outcome.result;
```

(Everything from `let mut result = outcome.result;` onward, through the end of
`run()`, is unchanged — the jump/stop/publish/cancel logic never touches
`external_secrets`.)

- [ ] **Step 2: Thread `external_secrets` through `run_step` into `begin_phases`**

Current code (`collection_runner_service.rs:346-370`):

```rust
    async fn run_step(
        &self,
        exec: &RequestExecutionService,
        input: &RunCollectionInput,
        item: &RunItem,
        index: usize,
        carried_runtime: &mut HashMap<String, String>,
    ) -> StepOutcome {
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
            input.request_guard_policy.clone(),
        );

        let mut state = match exec.begin_phases(&step_input) {
            Ok(state) => state,
            Err(e) => {
                return StepOutcome {
                    result: error_step(index, item, e.to_string()),
                    next_request: None,
                }
            }
        };
```

New code:

```rust
    async fn run_step(
        &self,
        exec: &RequestExecutionService,
        input: &RunCollectionInput,
        item: &RunItem,
        index: usize,
        carried_runtime: &mut HashMap<String, String>,
        external_secrets: &HashMap<String, String>,
    ) -> StepOutcome {
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
            input.request_guard_policy.clone(),
        );

        let mut state = match exec.begin_phases(&step_input, external_secrets) {
            Ok(state) => state,
            Err(e) => {
                return StepOutcome {
                    result: error_step(index, item, e.to_string()),
                    next_request: None,
                }
            }
        };
```

The rest of `run_step` (seeding runtime, before-request phase, skip handling,
send, after-response, tests, finish) is unchanged — none of it touches
`external_secrets` directly; it was already captured into `state.var_ctx` by
`begin_phases`.

- [ ] **Step 3: Compile-check**

Run: `cargo check -p rocket-app`
Expected: no errors. If Plan 06's actual `begin_phases`/`resolve_external_secrets`
signatures differ from the Global Constraints quote above, this is where a
mismatch surfaces — fix this file's call sites to match Plan 06's real
signatures, not the other way around.

- [ ] **Step 4: Run the existing suite for regressions**

Run: `cargo test -p rocket-app collection_runner_service::`
Expected: PASS — all pre-existing tests in this module unchanged (see Global
Constraints: `environment_name: None` keeps `resolve_external_secrets` a
zero-I/O no-op, so none of them observe any behavior change).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/collection_runner_service.rs
git commit -m "perf(runner): resolve external secrets once per run, not per step"
```

---

## Task 2: Call-count test proving exactly one fetch per run

**Files:**
- Modify: `crates/rocket-app/src/test_doubles.rs`
- Modify: `crates/rocket-app/src/collection_runner_service.rs`

**Interfaces:**
- Consumes: `rocket_environment::VaultSecretFetcher`,
  `SecretManagerConnection`, `SecretManagerRepository`, `SecretStore`,
  `ExternalSecretBinding`, `ExternalSecretRef` (Plans 01/02), and Task 1's
  once-per-run wiring.
- Produces: `FakeVaultSecretFetcher`, `FakeSecretManagerRepo`,
  `FakeSecretStore`, `StaticEnvRepo` in `test_doubles.rs` — reusable by any
  later test *within the `rocket-app` crate* that needs a runnable
  External-Secrets fixture. **Correction (post-Plan-07 whole-branch
  review):** `test_doubles` is declared `pub(crate) mod test_doubles;` in
  `crates/rocket-app/src/lib.rs:22`, so these fixtures are NOT visible
  outside this crate — Plan 08's `src-tauri` command tests cannot import
  them and must build their own equivalents (or Plan 08 could promote this
  module's visibility if broader reuse is wanted later).

This task does not follow the write-test-then-implement order the rest of
this series uses, because there is nothing left to implement — Task 1 already
made the once-per-run change. Instead: write the test, run it, and confirm it
passes *because* of Task 1's change (not by accident) by reasoning through
what it would report if the call were still inside the loop (call count 3,
not 1) — this is exactly the assertion that would catch a future regression
that moved the call back inside the loop.

- [ ] **Step 1: Add the fakes to `test_doubles.rs`**

Add near the top of `crates/rocket-app/src/test_doubles.rs`, alongside the
existing `use` block:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
```

and extend the existing `rocket_environment` import:

```rust
use rocket_environment::{
    Environment, EnvironmentRepository, ExternalSecretRef, SecretManagerConnection,
    SecretManagerRepository, SecretStore, VaultSecretFetcher,
};
```

Then add a new section (matching this file's existing `// --- Section --- `
comment-banner convention):

```rust
// ---------------------------------------------------------------------------
// External Secrets: environment, vault fetcher, secret manager repo, store
// ---------------------------------------------------------------------------

/// Environment repo that always returns one fixed `Environment`, regardless
/// of the name asked for. Enough for a test that only needs one environment
/// with a known `external_secrets` binding — mirrors `NullEnvRepo` above but
/// answers instead of always erroring.
pub struct StaticEnvRepo(pub Environment);

impl EnvironmentRepository for StaticEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, _name: &str) -> DomainResult<Environment> {
        Ok(self.0.clone())
    }
    fn save(&self, _: &Environment) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Secret Manager connection repo that always answers one fixed connection.
pub struct FakeSecretManagerRepo(pub SecretManagerConnection);

impl SecretManagerRepository for FakeSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok(if id == self.0.id {
            Some(self.0.clone())
        } else {
            None
        })
    }
    fn save(&self, _: &SecretManagerConnection) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Secret store that always answers one fixed client secret, regardless of
/// scope/key. Enough for a test that only needs the vault-connection client
/// secret lookup to succeed.
pub struct FakeSecretStore(pub String);

impl SecretStore for FakeSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
        Ok(Some(self.0.clone()))
    }
    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Vault fetcher that answers one canned value per secret id and counts how
/// many times `get_secret_value` was called. That count is the assertion
/// this plan's test makes to enforce spec acceptance criterion 7 (one
/// resolution pass per run, not one per request).
pub struct FakeVaultSecretFetcher {
    values: HashMap<String, String>, // secret_id -> value
    get_secret_value_calls: AtomicUsize,
}

impl FakeVaultSecretFetcher {
    pub fn new(values: HashMap<String, String>) -> Arc<Self> {
        Arc::new(Self {
            values,
            get_secret_value_calls: AtomicUsize::new(0),
        })
    }

    pub fn call_count(&self) -> usize {
        self.get_secret_value_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl VaultSecretFetcher for FakeVaultSecretFetcher {
    async fn list_secrets(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        Ok(Vec::new())
    }

    async fn get_secret_value(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        self.get_secret_value_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.values.get(secret_id).cloned())
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Ok(())
    }
}
```

- [ ] **Step 2: Add the test to `collection_runner_service.rs`**

First, extend the existing `test_doubles` import at the top of `mod tests`
(`collection_runner_service.rs:508-511`):

```rust
    use crate::test_doubles::{
        FakeSecretManagerRepo, FakeSecretStore, FakeVaultSecretFetcher, InMemoryCollectionRepo,
        InMemoryHistoryRepo, NullCookieRepo, ProgrammableEngine, RecordingExecutor,
        RecordingPublisher, SharedCollectionRepo, SharedEngine, SharedExecutor, SharedHistoryRepo,
        SharedPublisher, StaticEnvRepo,
    };
```

(this drops `NullEnvRepo` from the imported set only if nothing else in the
file still uses it — check first; every other existing test in this module
uses `NullEnvRepo` via `harness()`, so in practice it stays imported and this
edit only adds the four new names alongside it.)

Then add, alongside the other fixture builders (near `three_step_collection`):

```rust
    use rocket_environment::{Environment, ExternalSecretBinding, ExternalSecretRef, SecretManagerConnection};

    /// Same 3-request shape as `three_step_collection`, but each request's
    /// URL references `{{payments.apiKey}}` — an External Secret — in its
    /// query string, so a resolved run must substitute the real value into
    /// every one of the 3 requests actually sent.
    fn req_with_secret_ref(name: &str, file: &str) -> Request {
        let mut r = Request::new(
            name,
            HttpMethod::Get,
            format!("https://api.test/{file}?key=") + "{{payments.apiKey}}",
        );
        r.file_name = Some(file.to_string());
        r
    }

    fn three_step_collection_with_secret_refs() -> Collection {
        let mut collection = Collection::new("my-api");
        collection
            .root
            .add_request(req_with_secret_ref("First", "first.yml"));
        collection
            .root
            .add_request(req_with_secret_ref("Second", "second.yml"));
        collection
            .root
            .add_request(req_with_secret_ref("Third", "third.yml"));
        collection
    }

    fn environment_with_one_external_secret_binding() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets.push(ExternalSecretBinding {
            alias: "payments".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: vec![ExternalSecretRef {
                name: "apiKey".to_string(),
                secret_id: "sec-1".to_string(),
            }],
        });
        env
    }

    fn fake_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    #[tokio::test]
    async fn resolves_external_secrets_exactly_once_per_run_not_once_per_step() {
        // Spec acceptance criterion 7: a Collection Runner run over N
        // requests that reference an External Secret must issue exactly one
        // resolution pass, not N -- RocketVault's get_secret_value is a real
        // network round-trip per secret name.
        let collection = three_step_collection_with_secret_refs();
        let repo = InMemoryCollectionRepo::new(collection);
        let executor = RecordingExecutor::new();
        let engine = ProgrammableEngine::new();

        let mut values = HashMap::new();
        values.insert("sec-1".to_string(), "sk-test-secret-value".to_string());
        let fetcher = FakeVaultSecretFetcher::new(values);

        // Plan 06 (now written — see 2026-09-22-rocketvault-secrets-plan-06-execution-service.md)
        // finalized `secret_manager_repo`/`vault_connection_secret_store`/
        // `vault_fetcher` as three trailing POSITIONAL arguments appended to
        // `RequestExecutionService::new`/`new_with_audit` — not builder
        // methods. `with_script_engine` is unaffected (Plan 06 left it as
        // the pre-existing opt-in builder it already was).
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(environment_with_one_external_secret_binding())),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(FakeSecretManagerRepo(fake_connection())),
            Arc::new(FakeSecretStore("client-secret-xyz".to_string())),
            Arc::clone(&fetcher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));

        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(rocket_shared::events::NullEventPublisher),
        );

        let mut input = sample_run_input();
        input.environment_name = Some("prod".to_string());

        let summary = runner.run(&exec, input).await.expect("run");

        assert_eq!(
            fetcher.call_count(),
            1,
            "expected exactly one get_secret_value call for the whole 3-step run, got {}",
            fetcher.call_count()
        );

        assert_eq!(summary.steps.len(), 3, "all 3 steps must still run");
        assert!(
            summary
                .steps
                .iter()
                .all(|s| s.status == RunStepStatus::Completed),
            "every step must complete, got {:?}",
            summary.steps
        );

        let sent = executor.sent_urls();
        assert_eq!(sent.len(), 3);
        for (i, url) in sent.iter().enumerate() {
            assert!(
                url.ends_with("?key=sk-test-secret-value"),
                "step {i} url {url} did not resolve {{{{payments.apiKey}}}} to the fetched value"
            );
        }
    }
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p rocket-app resolves_external_secrets_exactly_once_per_run_not_once_per_step`
Expected: PASS. This proves Task 1's change: `fetcher.call_count()` is `1`,
not `3`. If Task 1's `resolve_external_secrets` call were moved back inside
the loop (once per step instead of once per run), this same test would fail
with `call_count() == 3` — that failure mode is exactly what this test exists
to catch.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/collection_runner_service.rs
git commit -m "test(runner): assert external secrets resolve once per run, not per step"
```

---

## Milestone Checklist — Plan 07

- [ ] `CollectionRunnerService::run` calls `exec.resolve_external_secrets(...)` exactly once, before the step loop, before `run_id`/`in_flight`/`RunnerStarted`
- [ ] A failed resolution fails the whole run before any step executes
- [ ] `run_step` threads the resolved `external_secrets: &HashMap<String, String>` into `exec.begin_phases(&step_input, external_secrets)`
- [ ] Every pre-existing `collection_runner_service.rs` test still passes unchanged (`environment_name: None` stays a zero-I/O no-op)
- [ ] New call-count test: `FakeVaultSecretFetcher::get_secret_value` called exactly once across a 3-request run
- [ ] New call-count test: all 3 steps complete and each resolves `{{payments.apiKey}}` to the correct fetched value
- [ ] `cargo test -p rocket-app` — all pass

## Next Plan

[Plan 08: Tauri commands + service wiring](2026-09-22-rocketvault-secrets-plan-08-tauri-commands.md) —
wires `SecretManagerService`, the updated `RequestExecutionService`
constructor, and `CollectionRunnerService` into `src-tauri/src/lib.rs`'s
managed state.
