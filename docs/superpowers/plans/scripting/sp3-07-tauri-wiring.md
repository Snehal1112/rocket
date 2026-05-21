# SP3-07 — `src-tauri`: DI Wiring + IPC Response Extension

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `DenoScriptEngine` into `HttpService` in `lib.rs` DI setup. Extend `ExecuteRequestResponse` IPC type with `testResults`, `consoleEntries`, and `scriptError` fields. Add the `evaluate_var_expression` Tauri command. Update `tauri-api.ts` with the new TypeScript types.

**Architecture:** `src-tauri/src/lib.rs` is the only place `DenoScriptEngine` is constructed — exactly following the existing DI pattern. `src-tauri/src/commands/http.rs` extends the response DTO (IPC-only, `#[serde(rename_all = "camelCase")]`). `frontend/src/lib/tauri-api.ts` mirrors the new types.

**Tech Stack:** Rust (Tauri), TypeScript

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §6

**Depends on:** SP3-06 merged (SP3-03/04/05 must also be merged for `DenoScriptEngine` to be fully functional, but wiring compiles without them).

---

## Task 1: Wire `DenoScriptEngine` into DI in `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `rocket-infra` scripting import to `src-tauri/src/lib.rs`**

Near the top of `lib.rs` where other infra types are imported, add:

```rust
use rocket_infra::scripting::DenoScriptEngine;
```

- [ ] **Step 2: Pass `DenoScriptEngine` to `HttpService::new()`**

Find the section in `lib.rs` where `HttpService` is constructed (look for `HttpService::new(` or similar). Add `DenoScriptEngine::new()` as the `script_engine` argument:

```rust
let http_service = HttpService::new(
    // ... existing args ...
    Box::new(DenoScriptEngine::new()),
);
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(src-tauri): wire DenoScriptEngine into HttpService DI"
```

---

## Task 2: Extend `ExecuteRequestResponse` IPC type + new Tauri command

**Files:**
- Modify: `src-tauri/src/commands/http.rs`
- Modify: `src-tauri/src/lib.rs` (register new command)

- [ ] **Step 1: Extend `ExecuteRequestResponse` in `commands/http.rs`**

Find the `ExecuteRequestResponse` struct (IPC-only DTO). Add the new fields — these use `#[serde(rename_all = "camelCase")]` since the struct is IPC-only:

```rust
/// IPC DTO for script test results. camelCase for frontend.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcTestResult {
    pub name: String,
    pub status: String,  // "passed" | "failed"
    pub error: Option<String>,
}

/// IPC DTO for console entries.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcConsoleEntry {
    pub level: String,   // "log" | "warn" | "error"
    pub message: String,
}

// On ExecuteRequestResponse, add:
// (find the struct and append these fields)
pub test_results: Vec<IpcTestResult>,
pub console_entries: Vec<IpcConsoleEntry>,
pub script_error: Option<String>,
```

- [ ] **Step 2: Populate the new fields in the `execute_request` command handler**

Find where `ExecuteRequestResponse` is constructed (inside `execute_request` command). After the `HttpService` call returns, populate the new fields by converting from `ScriptResult` / the domain events:

```rust
// The execute pipeline on HttpService now returns test_results, console_entries
// as part of the response or via the returned value — check what HttpService.execute()
// now returns after SP3-06 and map accordingly.

test_results: response.test_results.iter().map(|t| IpcTestResult {
    name: t.name.clone(),
    status: match t.status {
        rocket_scripting::TestStatus::Passed => "passed".into(),
        rocket_scripting::TestStatus::Failed => "failed".into(),
    },
    error: t.error.clone(),
}).collect(),

console_entries: response.console_entries.iter().map(|e| IpcConsoleEntry {
    level: match e.level {
        rocket_scripting::ConsoleLevel::Log   => "log".into(),
        rocket_scripting::ConsoleLevel::Warn  => "warn".into(),
        rocket_scripting::ConsoleLevel::Error => "error".into(),
    },
    message: e.message.clone(),
}).collect(),

script_error: response.script_error.clone(),
```

> **Note to subagent:** Check what `HttpService.execute()` returns after SP3-06 modifications. If test/console data is carried on the return value, map from there. If it's emitted only as events, you may need to collect them via the event bus or add them to the return type — choose the approach consistent with how other data flows from `HttpService` to commands today.

- [ ] **Step 3: Add `evaluate_var_expression` command**

Append to `commands/http.rs`:

```rust
/// Evaluates a single JS expression against a response for the Vars tab post-response section.
#[tauri::command]
pub async fn evaluate_var_expression(
    collection_root: String,
    expression: String,
    response_json: String,
    svc: State<'_, HttpService>,
) -> Result<serde_json::Value, DomainError> {
    svc.evaluate_var_expression(&collection_root, &expression, &response_json).await
}
```

Add the corresponding method stub to `HttpService` in `rocket-app/src/http_service.rs`:

```rust
pub async fn evaluate_var_expression(
    &self,
    _collection_root: &str,
    expression: &str,
    response_json: &str,
) -> DomainResult<serde_json::Value> {
    // Wrap expression in a minimal script that sets a result variable
    let code = format!(
        "const res = {{ getBody: () => JSON.parse('{}'), getStatus: () => 200 }};\n\
         rok.setVar('__result__', JSON.stringify({}));",
        response_json.replace('\'', "\\'"),
        expression
    );
    let ctx = ScriptContext::after_response(
        code,
        VariableContext::default(),
        HttpRequest::default(),
        HttpResponse { status: 200, body: Some(response_json.to_string()), ..Default::default() },
        None,
    );
    let result = self.script_engine.execute(ctx).await?;
    let val = result.runtime_vars.get("__result__")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    Ok(val)
}
```

- [ ] **Step 4: Register `evaluate_var_expression` in `invoke_handler!` in `lib.rs`**

Add to the command list:

```rust
commands::http::evaluate_var_expression,
```

- [ ] **Step 5: Compile check**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/
git commit -m "feat(src-tauri): extend ExecuteRequestResponse, add evaluate_var_expression command"
```

---

## Task 3: TypeScript IPC types in `tauri-api.ts`

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts`
- Create: `frontend/src/types/scripting.ts`

- [ ] **Step 1: Create `frontend/src/types/scripting.ts`**

```typescript
export interface TestResult {
  name: string
  status: 'passed' | 'failed'
  error: string | null
}

export interface ConsoleEntry {
  level: 'log' | 'warn' | 'error'
  message: string
}
```

- [ ] **Step 2: Update `ExecuteRequestResponse` in `tauri-api.ts`**

Find the existing `ExecuteRequestResponse` interface and add the new fields:

```typescript
import type { TestResult, ConsoleEntry } from '../types/scripting'

// On ExecuteRequestResponse, add:
testResults: TestResult[]
consoleEntries: ConsoleEntry[]
scriptError: string | null
```

- [ ] **Step 3: Add `evaluateVarExpression` wrapper**

Append to `tauri-api.ts`:

```typescript
export async function evaluateVarExpression(
  collectionRoot: string,
  expression: string,
  responseJson: string,
): Promise<unknown> {
  return invoke('evaluate_var_expression', { collectionRoot, expression, responseJson })
}
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/types/scripting.ts frontend/src/lib/tauri-api.ts
git commit -m "feat(frontend): TestResult, ConsoleEntry types + evaluateVarExpression IPC wrapper"
```
