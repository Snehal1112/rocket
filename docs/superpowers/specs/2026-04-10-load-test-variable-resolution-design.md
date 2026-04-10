# Load Test Variable Resolution Fix

**Date:** 2026-04-10
**Status:** Approved

## Problem

Load tests fail silently when request URLs or fields contain `{{variable}}` placeholders.
Two root causes compound each other:

1. **Frontend regex excludes hyphens.** `variable-context.ts` uses `[\w.]+` which does not
   match variable names containing `-` (e.g. `oidc-baseurl`). The env-store already uses
   `[\w.-]+`. `resolveWithContext` leaves hyphenated placeholders unchanged.

2. **No backend variable resolution in the load test path.** `run_load_test_command`
   receives an `HttpRequest` directly and has no environment context. Regular requests
   work because `execute_request` passes `environmentName` and `collection` to
   `RequestExecutionService`, which resolves placeholders server-side. Load test has no
   equivalent, so unresolved placeholders reach `reqwest::Url::parse`, fail immediately,
   and every request reports 0 ms latency and `failed`.

## Design

### Layer responsibilities (must not be violated)

```
src-tauri commands   →  thin adapters only, no business logic
rocket-app           →  use-case orchestration (resolution, merging, execution)
rocket-http          →  HTTP primitives and load test runner
rocket-infra         →  concrete I/O implementations
```

### Change 1 — Fix frontend regex (variable-context.ts)

Change the variable regex from `[\w.]+` to `[\w.-]+`.

This is a one-character fix that makes `resolveWithContext` consistent with
`env-store.ts` and supports all variable names the backend accepts.

**File:** `src/lib/variable-context.ts`, line 3.

### Change 2 — Add `run_load_test` to `RequestExecutionService` (rocket-app)

Extract the variable resolution steps from `execute()` into a private helper
`resolve_request(&self, input: &ExecuteRequestInput) -> DomainResult<HttpRequest>`.
The existing `execute()` calls it internally — no behaviour change.

Add a new public method:

```rust
pub async fn run_load_test(
    &self,
    input: ExecuteRequestInput,
    config: LoadTestConfig,
) -> DomainResult<LoadTestResult>
```

It calls `self.resolve_request(&input)?` then delegates to
`rocket_http::run_load_test(executor, &resolved, &config)`.

The service already holds `Box<dyn HttpExecutor>`, so no new dependencies are
introduced. `rocket-app` already depends on `rocket-http`.

**File:** `crates/rocket-app/src/execution_service.rs`.

### Change 3 — Slim down the Tauri command (src-tauri)

`run_load_test_command` changes to mirror `execute_request`:

```rust
#[tauri::command]
pub async fn run_load_test_command(
    input: ExecuteRequestInput,
    config: LoadTestConfig,
    svc: State<'_, RequestExecutionService>,
) -> Result<LoadTestResult, DomainError> {
    svc.run_load_test(input, config).await
}
```

No orchestration logic in the command itself.

**File:** `src-tauri/src/commands/load_test.rs`.

### Change 4 — Frontend passes resolution context (tauri-api.ts + LoadTestDialog.tsx)

`runLoadTest` in `tauri-api.ts` changes its request shape from `HttpRequest`-shaped
to `ExecuteRequestInput`-shaped (adds `collection?: string`, `environmentName?: string`,
`requestPath?: string`).

`LoadTestDialog.handleRun` already calls `resolveRequestFields(tabId, request)`, which
returns `collection`, `environmentName`, and `requestPath`. These are forwarded in the
`runLoadTest` call.

**Files:** `src/lib/tauri-api.ts`, `src/components/request/LoadTestDialog.tsx`.

## Data flow after fix

```
User clicks Run in LoadTestDialog
  → resolveRequestFields(tabId, request)          [frontend, best-effort]
  → runLoadTest({ ...resolved, collection, environmentName })
  → run_load_test_command(input, config, svc)      [Tauri command]
  → svc.run_load_test(input, config)               [rocket-app]
      → resolve_request(&input)                    [variable resolution]
      → run_load_test(executor, &resolved, &config) [rocket-http]
```

Frontend resolution handles process env, global, and in-memory env vars.
Backend resolution handles collection, environment file, folder-chain, and
request-level variables — the same scopes `execute_request` uses.

## Testing

- **Unit:** `resolve_request` gets a dedicated test verifying hyphenated variable names
  resolve correctly (e.g. `{{oidc-baseurl}}`).
- **Unit:** `run_load_test` on `RequestExecutionService` verifies variables are resolved
  before requests are fired (mock executor asserts on the resolved URL).
- **Frontend:** `variable-context.test.ts` adds a test case for hyphenated variable names.

## Out of scope

- Query params are not resolved on the backend (consistent with existing `execute_request`
  behaviour).
- Error surfacing for individual failed requests within a load test run (separate feature).
