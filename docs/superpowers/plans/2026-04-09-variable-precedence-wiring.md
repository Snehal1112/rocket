# Variable Precedence Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the already-implemented `VariableContext` 7-scope precedence into the backend `execution_service.rs` so folder-chain and request-level variables are resolved during HTTP execution.

**Architecture:** `VariableContext` (in `rocket-environment`) already models all 7 scopes with correct precedence via `flatten()`. The backend execution service currently builds a plain `HashMap` with only collection + env vars. We add `request_path` to the input DTO, populate all 4 backend-accessible scopes of `VariableContext`, and call `ctx.flatten()`. The frontend already resolves fully before sending, so this change makes direct (non-UI) execution consistent.

**Tech Stack:** Rust (`rocket-app`, `rocket-environment`), TypeScript (Tauri frontend)

---

## Scope

Bruno's 7 scopes and where each lives today:

| Scope | Bruno precedence | Backend source | Status |
|---|---|---|---|
| Runtime | Highest (1) | Future (scripts) | Out of scope |
| Request | 2 | `get_request_variables(col, path)` | Needs wiring |
| Folder chain | 3 | `get_folder_chain_variables(col, path)` | Needs wiring |
| Environment | 4 | `env_repo.get(name)` | Already wired |
| Collection | 5 | `collection_repo.get_settings(col)` | Already wired |
| Global env | 6 | Not passed to backend yet | Out of scope |
| Process env | Lowest (7) | Not passed to backend | Out of scope |

This plan wires scopes 2–5 on the backend. Scopes 1, 6, 7 are already handled by the frontend's `buildVariableContext` / `resolveWithContext` before values reach the backend.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-app/src/execution_service.rs` | Add `request_path` to `ExecuteRequestInput`; replace manual HashMap with `VariableContext`; add folder/request var loading |
| `src/lib/tauri-api.ts` | Add `requestPath?: string` to `ExecuteRequestInput` interface |
| `src/lib/execute-request.ts` | Pass `requestPath` to `executeRequest()` |

---

## Task 1: Add `request_path` to `ExecuteRequestInput` and update tests

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Add `request_path` field to the struct**

In `execution_service.rs`, update `ExecuteRequestInput`:

```rust
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
}
```

- [ ] **Step 2: Update `sample_input` helper in the test module**

Find `fn sample_input` in the `#[cfg(test)]` block and add the new field:

```rust
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
        request_path: None,
    }
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p rocket-app
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(execution): add request_path field to ExecuteRequestInput"
```

---

## Task 2: Wire `VariableContext` into `execute()` — with tests first

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Extend `StubCollectionRepo` to support configurable folder/request vars**

In the `#[cfg(test)]` block, replace the existing `StubCollectionRepo` struct and its `impl` with this version:

```rust
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
```

- [ ] **Step 2: Add the three new failing tests**

Add these tests at the end of the `mod tests` block, before the closing `}`:

```rust
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
        Box::new(SharedExec(executor)),
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
        Box::new(SharedExec2(executor)),
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
        Box::new(SharedExec3(executor)),
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
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cargo test -p rocket-app folder_vars_override_collection_vars 2>&1 | tail -5
cargo test -p rocket-app request_vars_override_folder_vars 2>&1 | tail -5
cargo test -p rocket-app full_precedence_collection_lt_env_lt_folder_lt_request 2>&1 | tail -5
```

Expected: all 3 FAIL (URL will be unresolved because folder/request vars aren't loaded yet).

- [ ] **Step 4: Replace the manual HashMap build in `execute()` with `VariableContext`**

Add the import at the top of the file (after the existing `use rocket_environment::{resolve, EnvironmentRepository};`):

```rust
use rocket_environment::{resolve, EnvironmentRepository, VariableContext};
```

Replace the entire "Step 1: Build variable map" block in `execute()` (lines ~51–70) with:

```rust
// Step 1: Build variable map using full Bruno precedence (collection < env < folder < request).
let mut ctx = VariableContext::default();

// Scope: collection variables (lowest of the 4 backend-accessible scopes).
if let Some(col) = &input.collection {
    let settings = self.collection_repo.get_settings(col).unwrap_or_default();
    for cv in settings.variables.iter().filter(|v| v.enabled) {
        let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
        ctx.collection.insert(cv.key.clone(), val);
    }
}

// Scope: environment variables override collection.
if let Some(name) = &input.environment_name {
    if let Ok(env) = self.env_repo.get(name) {
        for (k, v) in env.enabled_variables() {
            ctx.env.insert(k.to_string(), v.to_string());
        }
    }
}

// Scope: folder-chain variables (repo walks ancestors; inner folder wins).
if let (Some(col), Some(path)) = (&input.collection, &input.request_path) {
    if let Ok(folder_vars) = self.collection_repo.get_folder_chain_variables(col, path) {
        for cv in folder_vars.iter().filter(|v| v.enabled) {
            let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
            ctx.folder.insert(cv.key.clone(), val);
        }
    }
}

// Scope: request-level variables (highest priority on the backend).
if let (Some(col), Some(path)) = (&input.collection, &input.request_path) {
    if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
        for cv in request_vars.iter().filter(|v| v.enabled) {
            let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
            ctx.request.insert(cv.key.clone(), val);
        }
    }
}

let vars = ctx.flatten();
```

Note: the variable `vars` is used in Step 3 of the function (`resolve(&input.url, &vars)`) — leave those lines unchanged.

- [ ] **Step 5: Run all three new tests — they must pass**

```bash
cargo test -p rocket-app folder_vars_override_collection_vars
cargo test -p rocket-app request_vars_override_folder_vars
cargo test -p rocket-app full_precedence_collection_lt_env_lt_folder_lt_request
```

Expected: all 3 PASS.

- [ ] **Step 6: Run the full test suite for the crate**

```bash
cargo test -p rocket-app
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(execution): wire folder and request variable scopes via VariableContext"
```

---

## Task 3: Propagate `request_path` from the frontend

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/execute-request.ts`

- [ ] **Step 1: Add `requestPath` to the TypeScript interface**

In `src/lib/tauri-api.ts`, update the `ExecuteRequestInput` interface (currently around line 174):

```typescript
export interface ExecuteRequestInput {
  method: HttpMethod;
  url: string;
  headers: Header[];
  queryParams: QueryParam[];
  body?: Body;
  auth: Auth;
  options: RequestOptions;
  environmentName?: string;
  collection?: string;
  requestName?: string;
  /** Path of the request file relative to the collection root (e.g. "auth/login.yml"). */
  requestPath?: string;
}
```

- [ ] **Step 2: Pass `requestPath` in `sendRequest()`**

In `src/lib/execute-request.ts`, the `executeRequest` call currently (around line 164) is:

```typescript
const result = await executeRequest({
  method: request.method,
  url: resolvedUrl,
  headers: effectiveHeaders,
  queryParams: resolvedQueryParams,
  body: resolvedBody,
  auth: resolvedAuth,
  options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
  collection: collection ?? undefined,
  environmentName: envStore.activeEnvId ?? undefined,
});
```

Change it to:

```typescript
const result = await executeRequest({
  method: request.method,
  url: resolvedUrl,
  headers: effectiveHeaders,
  queryParams: resolvedQueryParams,
  body: resolvedBody,
  auth: resolvedAuth,
  options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
  collection: collection ?? undefined,
  environmentName: envStore.activeEnvId ?? undefined,
  requestPath: requestPath ?? undefined,
});
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Lint check**

```bash
yarn check
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/execute-request.ts
git commit -m "feat(frontend): pass requestPath to executeRequest for backend variable resolution"
```

---

## Self-Review

**Spec coverage:**
- Runtime vars: out of scope (scripts not implemented) — documented in scope table.
- Request vars: wired via `get_request_variables` + `ctx.request`.
- Folder-chain vars: wired via `get_folder_chain_variables` + `ctx.folder`.
- Env vars: already wired, now via `ctx.env`.
- Collection vars: already wired, now via `ctx.collection`.
- Global vars: handled by frontend only — out of scope, documented.
- Process env: handled by frontend only — out of scope, documented.
- `request_path` field is `#[serde(default)]` so old callers without it won't break.

**Placeholder scan:** No TODOs, no "similar to task N", all code is complete.

**Type consistency:**
- `CollectionVariable` is imported in the test from `rocket_collection` — matches existing imports.
- `cv()` helper produces `CollectionVariable` with all required fields.
- `VariableContext` is re-exported from `rocket_environment::lib.rs` as `pub use context::VariableContext` — import path `rocket_environment::VariableContext` is correct.
