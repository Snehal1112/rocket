# Load Test Variable Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix load test failures caused by unresolved `{{variable}}` placeholders by repairing the frontend regex and adding DDD-aligned backend variable resolution to `RequestExecutionService`.

**Architecture:** The frontend regex fix (`[\w.]+` → `[\w.-]+`) enables hyphenated variable names. The backend fix extracts a `resolve_request` helper from `RequestExecutionService::execute`, adds a `run_load_test` method on the service, and slims the Tauri command to a one-liner — mirroring the existing `execute_request` pattern. The executor field changes from `Box` to `Arc` so it can be shared across concurrent tasks without violating the `'static` bound required by `tokio::spawn`.

**Tech Stack:** TypeScript + Vitest (frontend), Rust + Tokio + `cargo test` (backend), Tauri IPC.

---

## File Map

| File | Change |
|---|---|
| `src/lib/variable-context.ts` | Fix regex: `[\w.]+` → `[\w.-]+` |
| `src/lib/__tests__/variable-context.test.ts` | Add hyphenated variable name test |
| `crates/rocket-app/src/execution_service.rs` | Extract `resolve_request`; add `run_load_test`; change executor to `Arc` |
| `src-tauri/src/commands/load_test.rs` | Accept `ExecuteRequestInput + State<RequestExecutionService>` |
| `src-tauri/src/lib.rs` | Wrap `ReqwestExecutor` in `Arc` at construction |
| `src/lib/tauri-api.ts` | Add `collection?`, `environmentName?`, `requestPath?` to `runLoadTest` |
| `src/components/request/LoadTestDialog.tsx` | Forward the three new fields from `resolveRequestFields` result |

---

## Task 1: Fix frontend regex and add test

**Files:**
- Modify: `src/lib/variable-context.ts:3`
- Modify: `src/lib/__tests__/variable-context.test.ts`

- [ ] **Step 1: Write the failing test**

Add this test case inside the existing `describe('resolveWithContext', ...)` block in `src/lib/__tests__/variable-context.test.ts`:

```typescript
it('resolves hyphenated variable names', () =>
  expect(resolveWithContext('{{oidc-baseurl}}/api', { 'oidc-baseurl': 'https://auth.local' })).toBe(
    'https://auth.local/api',
  ));
```

- [ ] **Step 2: Run to confirm it fails**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn test src/lib/__tests__/variable-context.test.ts --run
```

Expected: FAIL — `resolves hyphenated variable names` — output is `{{oidc-baseurl}}/api`.

- [ ] **Step 3: Fix the regex**

In `src/lib/variable-context.ts`, line 3, change:

```typescript
const VAR_REGEX = /\{\{\s*([\w.]+)\s*\}\}/g;
```

to:

```typescript
const VAR_REGEX = /\{\{\s*([\w.-]+)\s*\}\}/g;
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
yarn test src/lib/__tests__/variable-context.test.ts --run
```

Expected: all tests PASS including the new one.

- [ ] **Step 5: Commit**

```bash
git add src/lib/variable-context.ts src/lib/__tests__/variable-context.test.ts
git commit -m "fix(frontend): support hyphenated variable names in resolveWithContext"
```

---

## Task 2: Extract `resolve_request` from `execution_service.rs`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

The goal is to extract the variable-building and placeholder-resolution steps (steps 1–3 of `execute`) into a private `resolve_request` method. `execute` then calls it. No behaviour changes.

- [ ] **Step 1: Write the failing test**

Add this test at the bottom of the `#[cfg(test)] mod tests` block in `execution_service.rs`:

```rust
#[tokio::test]
async fn resolve_request_handles_hyphenated_variable_names() {
    let mut env = Environment::new("dev");
    env.set_variable(Variable::new("oidc-baseurl", "https://auth.local"));

    let svc = RequestExecutionService::new(
        Box::new(MockEnvRepo::with_env(env)),
        Box::new(MockExecutor::new(200)),
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
    );

    let input = sample_input("{{oidc-baseurl}}/api/v1/users", Some("dev"));
    let resolved = svc.resolve_request(&input).unwrap();
    assert_eq!(resolved.url, "https://auth.local/api/v1/users");
}
```

- [ ] **Step 2: Run to confirm it fails**

```bash
cargo test -p rocket-app resolve_request_handles_hyphenated 2>&1 | tail -20
```

Expected: compile error — `resolve_request` does not exist yet.

- [ ] **Step 3: Extract `resolve_request` and update `execute`**

Replace the body of `execution_service.rs` `impl RequestExecutionService` with the following. The `execute` method delegates to `resolve_request`; the logic is identical to before, just split:

```rust
impl RequestExecutionService {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Box<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self { env_repo, executor, history_repo, collection_repo, cookie_repo, events }
    }

    /// Resolves all {{placeholders}} in `input` using the full variable precedence
    /// chain and returns a ready-to-send `HttpRequest`. Called by both `execute` and
    /// `run_load_test` so resolution logic is never duplicated.
    fn resolve_request(&self, input: &ExecuteRequestInput) -> DomainResult<HttpRequest> {
        // Build variable map: collection < env < folder < request.
        let mut ctx = VariableContext::default();

        if let Some(col) = &input.collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                ctx.collection.insert(cv.key.clone(), val);
            }
        }

        if let Some(name) = &input.environment_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    ctx.env.insert(k.to_string(), v.to_string());
                }
            }
        }

        if let (Some(col), Some(path)) = (&input.collection, &input.request_path) {
            if let Ok(folder_vars) = self.collection_repo.get_folder_chain_variables(col, path) {
                for cv in folder_vars.iter().filter(|v| v.enabled) {
                    let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    ctx.folder.insert(cv.key.clone(), val);
                }
            }
        }

        if let (Some(col), Some(path)) = (&input.collection, &input.request_path) {
            if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
                for cv in request_vars.iter().filter(|v| v.enabled) {
                    let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    ctx.request.insert(cv.key.clone(), val);
                }
            }
        }

        let vars = ctx.flatten();

        // Merge collection auth and headers with request-level values.
        let (effective_auth, effective_headers) = if let Some(col) = &input.collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            let auth = merge_auth(input.auth.clone(), settings.auth);
            let headers = merge_headers(&settings.headers, &input.headers);
            (auth, headers)
        } else {
            (input.auth.clone(), input.headers.clone())
        };

        // Resolve placeholders in URL and headers.
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

    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
        let http_request = self.resolve_request(&input)?;
        let response = self.executor.execute(&http_request).await?;

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

        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: http_request.url.clone(),
            status: response.status,
            duration_ms: response.duration_ms,
        });

        Ok(response)
    }
}
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p rocket-app 2>&1 | tail -20
```

Expected: all existing tests PASS, new `resolve_request_handles_hyphenated_variable_names` test PASSES.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "refactor(rocket-app): extract resolve_request helper from execute"
```

---

## Task 3: Add `run_load_test` to `RequestExecutionService`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

`run_load_test` in `rocket-http` calls `tokio::spawn`, which requires `Arc<dyn HttpExecutor>` (needs `'static`). The executor field changes from `Box` to `Arc` to support cloning into tasks.

- [ ] **Step 1: Write the failing test**

Add this test at the bottom of the `#[cfg(test)] mod tests` block:

```rust
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

    let config = rocket_http::LoadTestConfig { concurrency: 1, total_requests: 1 };
    let result = svc.run_load_test(input, config).await.unwrap();

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.failed, 0);

    // Verify the resolved URL reached the executor.
    let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
    assert_eq!(url, "https://auth.local/api/data");
}
```

- [ ] **Step 2: Run to confirm it fails**

```bash
cargo test -p rocket-app service_run_load_test 2>&1 | tail -20
```

Expected: compile error — constructor second parameter is `Box`, not `Arc`.

- [ ] **Step 3: Change executor field from `Box` to `Arc` and add `run_load_test`**

At the top of `execution_service.rs`, add `Arc` to the imports:

```rust
use std::sync::Arc;
```

Also add `LoadTestConfig, LoadTestResult, run_load_test as http_run_load_test` to the `rocket_http` import:

```rust
use rocket_http::{CookieRepository, HttpExecutor, HttpRequest, HttpResponse, LoadTestConfig, LoadTestResult, RequestOptions};
```

And add `run_load_test as http_run_load_test` — update that import line to:

```rust
use rocket_http::{
    run_load_test as http_run_load_test, CookieRepository, HttpExecutor, HttpRequest,
    HttpResponse, LoadTestConfig, LoadTestResult, RequestOptions,
};
```

Change the `executor` field in the struct:

```rust
pub struct RequestExecutionService {
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Arc<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    #[allow(dead_code)]
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
}
```

Update the constructor signature:

```rust
pub fn new(
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Arc<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
) -> Self {
    Self { env_repo, executor, history_repo, collection_repo, cookie_repo, events }
}
```

Add `run_load_test` after `execute`:

```rust
pub async fn run_load_test(
    &self,
    input: ExecuteRequestInput,
    config: LoadTestConfig,
) -> DomainResult<LoadTestResult> {
    let resolved = self.resolve_request(&input)?;
    let executor = Arc::clone(&self.executor);
    Ok(http_run_load_test(executor, &resolved, &config).await)
}
```

- [ ] **Step 4: Fix the test helper and all existing test constructors**

In the `tests` module, update `sample_input` (no change needed) and every `RequestExecutionService::new(...)` call — the second argument changes from `Box::new(...)` to `Arc::new(...)`.

Update these tests (second constructor argument shown):

**`execute_resolves_variables_in_url`** — change:
```rust
Box::new(MockExecutor::new(200)),
```
to:
```rust
Arc::new(MockExecutor::new(200)),
```

**`execute_saves_history`** — change:
```rust
Box::new(MockExecutor::new(200)),
```
to:
```rust
Arc::new(MockExecutor::new(200)),
```

**`execute_publishes_event`** — change:
```rust
Box::new(MockExecutor::new(201)),
```
to:
```rust
Arc::new(MockExecutor::new(201)),
```

**`folder_vars_override_collection_vars`** — change:
```rust
Box::new(SharedExec(executor)),
```
to:
```rust
Arc::new(SharedExec(executor)),
```

**`request_vars_override_folder_vars`** — change:
```rust
Box::new(SharedExec2(executor)),
```
to:
```rust
Arc::new(SharedExec2(executor)),
```

**`full_precedence_collection_lt_env_lt_folder_lt_request`** — change:
```rust
Box::new(SharedExec3(executor)),
```
to:
```rust
Arc::new(SharedExec3(executor)),
```

**`execute_uses_collection_auth_when_request_auth_is_none`** — change:
```rust
Box::new(SharedExecutor(executor)),
```
to:
```rust
Arc::new(SharedExecutor(executor)),
```

Also add `use std::sync::Arc;` inside the `#[cfg(test)] mod tests` block at the top.

- [ ] **Step 5: Run all tests**

```bash
cargo test -p rocket-app 2>&1 | tail -25
```

Expected: all tests PASS including the new `service_run_load_test_resolves_variables_before_firing`.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(rocket-app): add run_load_test to RequestExecutionService with variable resolution"
```

---

## Task 4: Update Tauri command and construction site

**Files:**
- Modify: `src-tauri/src/commands/load_test.rs`
- Modify: `src-tauri/src/lib.rs:108-115`

- [ ] **Step 1: Replace `load_test.rs`**

Replace the entire content of `src-tauri/src/commands/load_test.rs` with:

```rust
use rocket_app::{ExecuteRequestInput, RequestExecutionService};
use rocket_http::{LoadTestConfig, LoadTestResult};
use rocket_shared::error::DomainError;
use tauri::State;

/// Runs a load test against the given request and returns aggregated statistics.
/// Variable resolution is handled by RequestExecutionService using the same
/// scopes as execute_request (collection < env < folder < request).
#[tauri::command]
pub async fn run_load_test_command(
    input: ExecuteRequestInput,
    config: LoadTestConfig,
    svc: State<'_, RequestExecutionService>,
) -> Result<LoadTestResult, DomainError> {
    svc.run_load_test(input, config).await
}
```

- [ ] **Step 2: Update the executor construction in `lib.rs`**

In `src-tauri/src/lib.rs`, find the `exec_svc` construction block (around line 108) and change `Box::new(ReqwestExecutor::new())` to `Arc::new(ReqwestExecutor::new())`:

```rust
let exec_svc = RequestExecutionService::new(
    Box::new(FsEnvironmentRepo::new(environments_dir)),
    Arc::new(ReqwestExecutor::new()),       // was Box::new(...)
    Box::new(FsHistoryRepo::new(history_dir)),
    Box::new(FsCollectionRepo::new(collections_dir.clone())),
    Box::new(FsCookieRepo::new(cookies_dir)),
    Box::new(NullEventPublisher),
);
```

`Arc` is already imported at line 5 (`use std::sync::{Arc, Mutex};`).

- [ ] **Step 3: Check compilation**

```bash
cargo check -p src-tauri 2>&1 | tail -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/load_test.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): route run_load_test_command through RequestExecutionService"
```

---

## Task 5: Update frontend API and dialog

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/request/LoadTestDialog.tsx`

- [ ] **Step 1: Update `runLoadTest` in `tauri-api.ts`**

Find the `runLoadTest` function (around line 449) and replace it:

```typescript
export const runLoadTest = (
  request: {
    method: HttpMethod;
    url: string;
    headers: Header[];
    queryParams: QueryParam[];
    body?: Body | null;
    auth: Auth;
    options: RequestOptions;
    collection?: string;
    environmentName?: string;
    requestPath?: string;
  },
  config: LoadTestConfig,
) => invoke<LoadTestResult>('run_load_test_command', { input: request, config });
```

Note: the Tauri command now expects `input` (not `request`) as the parameter name — the invoke key changes from `{ request, config }` to `{ input: request, config }`.

- [ ] **Step 2: Update `LoadTestDialog.tsx`**

In `handleRun`, add the three new fields after `auth`:

```typescript
const res = await runLoadTest(
  {
    method: request.method,
    url: resolved.url,
    headers: resolved.headers,
    queryParams: resolved.queryParams,
    body: resolved.body ?? null,
    auth: resolved.auth,
    options: {
      followRedirects: request.settings.followRedirects,
      timeoutMs: request.settings.timeoutMs,
      verifySsl: request.settings.verifySsl,
    },
    collection: resolved.collection,
    environmentName: resolved.environmentName,
    requestPath: resolved.requestPath,
  },
  {
    concurrency: parseInt(concurrency, 10),
    totalRequests: parseInt(totalRequests, 10),
  },
);
```

- [ ] **Step 3: TypeScript check and lint**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors, only the pre-existing warning in `CollectionOverviewTab.tsx`.

- [ ] **Step 4: Run frontend tests**

```bash
yarn test --run 2>&1 | tail -10
```

Expected: 200 tests pass (199 existing + 1 new from Task 1).

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/components/request/LoadTestDialog.tsx
git commit -m "feat(frontend): pass collection and environment context to run_load_test_command"
```

---

## Task 6: Final verification

- [ ] **Step 1: Full Rust build and test**

```bash
cargo check && cargo test -p rocket-app -p rocket-http 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 2: Full frontend test suite**

```bash
yarn tsc --noEmit && yarn check && yarn test --run 2>&1 | tail -10
```

Expected: all 200 tests pass, no TypeScript or lint errors.
