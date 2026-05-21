# SP3-06 — `rocket-app`: `HttpService` 3-Phase Script Pipeline

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `Box<dyn ScriptEngine>` into `HttpService` via constructor injection, add the 3-phase execution pipeline (before-request → HTTP → after-response → tests), apply all `ScriptResult` mutations in the correct order, and add the three new `DomainEvent` variants.

**Architecture:** `HttpService` already exists in `rocket-app`. We add a `script_engine: Box<dyn ScriptEngine>` field and three helper methods — `run_before_request`, `run_after_response`, `run_tests` — each building a `ScriptContext`, calling `engine.execute()`, and applying mutations. Mutation application order follows spec §8.

**Tech Stack:** Rust, `async-trait`, `rocket-scripting`, `rocket-shared`

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §5, §8

**Depends on:** SP3-01 merged. (SP3-03/04/05 can run in parallel; not required for this plan.)

---

## Task 1: New `DomainEvent` variants + `rocket-app` dependency

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `crates/rocket-app/Cargo.toml`

- [ ] **Step 1: Add `rocket-scripting` dep to `rocket-app/Cargo.toml`**

```toml
rocket-scripting = { path = "../rocket-scripting" }
```

- [ ] **Step 2: Add three new variants to `DomainEvent` in `crates/rocket-shared/src/events.rs`**

Open `crates/rocket-shared/src/events.rs`. Find the `DomainEvent` enum and append:

```rust
/// Emitted after all script phases complete for a request.
/// Carries all console.log/warn/error entries from all three phases combined.
ConsoleOutput {
    request_name: String,
    entries: Vec<rocket_scripting::ConsoleEntry>,
},

/// Emitted after the tests phase completes.
TestsCompleted {
    request_name: String,
    results: Vec<rocket_scripting::TestResult>,
},

/// Emitted when a script throws an uncaught exception.
ScriptError {
    request_name: String,
    /// "before-request" | "after-response" | "tests"
    phase: String,
    message: String,
},
```

> **Note to subagent:** `rocket-shared/Cargo.toml` needs `rocket-scripting` as a dependency for these variants. Add it:
> ```toml
> rocket-scripting = { path = "../rocket-scripting" }
> ```
> Check whether `rocket-shared` already imports from `rocket-scripting` before adding.

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-shared -p rocket-app 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/ crates/rocket-app/Cargo.toml
git commit -m "feat(rocket-shared): ConsoleOutput, TestsCompleted, ScriptError DomainEvent variants"
```

---

## Task 2: Inject `ScriptEngine` into `HttpService`

**Files:**
- Modify: `crates/rocket-app/src/http_service.rs`

- [ ] **Step 1: Add `script_engine` field and update constructor**

Open `crates/rocket-app/src/http_service.rs`. Find the `HttpService` struct definition and add the field:

```rust
use rocket_scripting::{ScriptEngine, ScriptContext, ScriptPhase};

pub struct HttpService {
    // ... existing fields ...
    pub script_engine: Box<dyn ScriptEngine>,
}
```

Update the `HttpService::new(...)` constructor to accept `script_engine: Box<dyn ScriptEngine>` and store it.

- [ ] **Step 2: Add `run_before_request` helper**

Add this private method to `HttpService`:

```rust
async fn run_before_request(
    &self,
    request: &mut HttpRequest,
    variables: &VariableContext,
    env_name: Option<&str>,
) -> DomainResult<(ScriptResult, Vec<ConsoleEntry>)> {
    use rocket_scripting::ConsoleEntry;

    // Find before-request script code, if any
    let code = request.pre_request_script.clone().unwrap_or_default();
    if code.trim().is_empty() {
        return Ok((ScriptResult::default(), vec![]));
    }

    let ctx = ScriptContext::before_request(
        code,
        variables.clone(),
        request.clone(),
        env_name.map(String::from),
    );

    let result = self.script_engine.execute(ctx).await?;

    // Apply request mutations immediately (before HTTP send)
    if let Some(ref mutations) = result.request_mutations {
        if let Some(ref url) = mutations.url {
            request.url = url.clone();
        }
        if let Some(ref method) = mutations.method {
            request.method = method.parse().unwrap_or(request.method.clone());
        }
        for (name, value) in &mutations.headers_set {
            // Upsert header
            if let Some(h) = request.headers.iter_mut().find(|h| h.name.eq_ignore_ascii_case(name)) {
                h.value = value.clone();
            } else {
                request.headers.push(rocket_http::HttpHeader { name: name.clone(), value: value.clone(), ..Default::default() });
            }
        }
        for name in &mutations.headers_deleted {
            request.headers.retain(|h| !h.name.eq_ignore_ascii_case(name));
        }
        if let Some(ref body) = mutations.body {
            request.body = Some(rocket_http::RequestBody::from_json_value(body.clone()));
        }
        if let Some(ms) = mutations.timeout_ms {
            if let Some(ref mut settings) = request.settings {
                settings.timeout = Some(ms);
            }
        }
    }

    let console_entries = result.console_entries.clone();
    Ok((result, console_entries))
}
```

> **Note to subagent:** Field names on `HttpRequest`, `HttpHeader`, `RequestBody` — check `crates/rocket-http/src/` and adjust. The intent is clear: apply URL, method, headers, body, timeout mutations to the mutable `request`.

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-app 2>&1 | grep "^error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/http_service.rs
git commit -m "feat(rocket-app): inject ScriptEngine, add run_before_request helper"
```

---

## Task 3: After-response + tests phases + mutation application + event emission

**Files:**
- Modify: `crates/rocket-app/src/http_service.rs`

- [ ] **Step 1: Add `run_after_response` and `run_tests` helpers**

```rust
async fn run_after_response(
    &self,
    request: &HttpRequest,
    response: &HttpResponse,
    variables: &VariableContext,
    env_name: Option<&str>,
) -> DomainResult<ScriptResult> {
    let code = request.post_response_script.clone().unwrap_or_default();
    if code.trim().is_empty() {
        return Ok(ScriptResult::default());
    }
    let ctx = ScriptContext::after_response(
        code,
        variables.clone(),
        request.clone(),
        response.clone(),
        env_name.map(String::from),
    );
    self.script_engine.execute(ctx).await
}

async fn run_tests(
    &self,
    request: &HttpRequest,
    response: &HttpResponse,
    variables: &VariableContext,
    env_name: Option<&str>,
) -> DomainResult<ScriptResult> {
    let code = request.tests.clone().unwrap_or_default();
    if code.trim().is_empty() {
        return Ok(ScriptResult::default());
    }
    let ctx = ScriptContext::tests(
        code,
        variables.clone(),
        request.clone(),
        response.clone(),
        env_name.map(String::from),
    );
    self.script_engine.execute(ctx).await
}
```

- [ ] **Step 2: Add `apply_script_mutations` helper**

```rust
async fn apply_script_mutations(
    &self,
    result: &ScriptResult,
    collection_root: &str,
) -> DomainResult<()> {
    // 1. env_var_writes (persist=false) — update in-memory env
    // 2. env_var_writes (persist=true) — also write .yml
    for write in &result.env_var_writes {
        if write.persist {
            // Write to environment .yml via FsEnvironmentRepo
            // self.env_repo.set_var(...)?;
        }
        // In-memory update handled by the caller updating their VariableContext
    }

    // 3. collection_var_writes — write to opencollection.yml
    for write in &result.collection_var_writes {
        // self.collection_repo.set_var(collection_root, &write.key, &write.value)?;
        let _ = write; // Remove when wired to real repos
    }

    // 4. global_env_var_writes
    for write in &result.global_env_var_writes {
        if write.persist {
            // self.global_env_repo.set_var(...)?;
        }
        let _ = write;
    }

    Ok(())
}
```

> **Note to subagent:** The `// self.env_repo...` lines are intentional placeholders — wire to the actual repo methods that exist on `HttpService` for env/collection persistence. Check what repo fields currently exist on `HttpService` and use their real method names.

- [ ] **Step 3: Update the main `execute` method to call all three phases**

Find the existing `execute` method in `HttpService` (the one that sends the HTTP request). Add the script pipeline calls around the HTTP send:

```rust
// ── Before-request script ──────────────────────────────────────────────────
let (before_result, mut all_console) = self
    .run_before_request(&mut request, &variables, env_name.as_deref())
    .await
    .unwrap_or_else(|e| {
        // Script errors never abort the request — emit event and continue
        self.event_bus.publish(DomainEvent::ScriptError {
            request_name: request.name.clone(),
            phase: "before-request".into(),
            message: e.to_string(),
        });
        (ScriptResult::default(), vec![])
    });

// Apply runtime vars from before-request to VariableContext
for (k, v) in &before_result.runtime_vars {
    if let Some(s) = v.as_str() {
        variables.runtime.insert(k.clone(), s.to_string());
    }
}

// ── HTTP execution (existing code — unchanged) ─────────────────────────────
let response = self.executor.execute(&request).await?;

// ── After-response script ──────────────────────────────────────────────────
let after_result = self
    .run_after_response(&request, &response, &variables, env_name.as_deref())
    .await
    .unwrap_or_else(|e| {
        self.event_bus.publish(DomainEvent::ScriptError {
            request_name: request.name.clone(),
            phase: "after-response".into(),
            message: e.to_string(),
        });
        ScriptResult::default()
    });

all_console.extend(after_result.console_entries.clone());
self.apply_script_mutations(&after_result, collection_root).await.ok();

// ── Tests script ───────────────────────────────────────────────────────────
let tests_result = self
    .run_tests(&request, &response, &variables, env_name.as_deref())
    .await
    .unwrap_or_else(|e| {
        self.event_bus.publish(DomainEvent::ScriptError {
            request_name: request.name.clone(),
            phase: "tests".into(),
            message: e.to_string(),
        });
        ScriptResult::default()
    });

all_console.extend(tests_result.console_entries.clone());

// ── Emit events ────────────────────────────────────────────────────────────
if !all_console.is_empty() {
    self.event_bus.publish(DomainEvent::ConsoleOutput {
        request_name: request.name.clone(),
        entries: all_console,
    });
}

if !tests_result.test_results.is_empty() {
    self.event_bus.publish(DomainEvent::TestsCompleted {
        request_name: request.name.clone(),
        results: tests_result.test_results.clone(),
    });
}
```

> **Note to subagent:** Locate the actual `execute` or `send` method on `HttpService`. The variable names (`request`, `response`, `variables`, `env_name`, `collection_root`, `self.event_bus`) may differ — adjust to match the existing code. The pipeline order is the non-negotiable part.

- [ ] **Step 4: Compile check**

```bash
cargo check -p rocket-app 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 5: Run existing tests to ensure no regressions**

```bash
cargo test -p rocket-app 2>&1 | tail -20
```

Expected: all existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/http_service.rs
git commit -m "feat(rocket-app): 3-phase script pipeline, mutation application, event emission"
```
