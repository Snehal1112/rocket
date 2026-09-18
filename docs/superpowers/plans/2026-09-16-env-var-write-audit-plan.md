# Env-Var-Write Audit Trail and Secret Preservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make script-driven variable writes (`rok.setEnvVar`, `rok.setGlobalEnvVar`, `rok.setCollectionVar`, and the `runtime.actions` set-variable pipeline) preserve existing variable metadata (especially the `secret` flag) and route through the same `DomainEvent`/`SecurityAuditPublisher` plumbing that manual environment saves already use, so a script can no longer silently downgrade a secret to plaintext or persist with zero audit trail.

**Architecture:** Extract the diff-and-publish logic already living in `EnvironmentService::save` into a new shared free function `rocket_app::env_audit::publish_env_write_events`, call it from both `EnvironmentService::save` (refactor, no behavior change) and the rewritten `RequestExecutionService::apply_env_writes` (which now merges into existing variable metadata instead of overwriting it). Add two new `DomainEvent` variants (`ScriptVariableWritten`, `CollectionVariableWritten`) so script-driven writes are visible to the event bus the same way manual saves are. On the frontend, `src/lib/execute-request.ts` invalidates the relevant React Query caches after `executeRequest()` resolves, and `RequestPanel.tsx` gets a `window` custom-event listener so its locally-fetched collection variables refresh after a script writes one — following the exact patterns these two files already use (React Query invalidation in `environment-queries.ts`, `window.dispatchEvent`/`addEventListener('rocket:...')` in `useKeyboardShortcuts.ts`/`RequestPanel.tsx`).

**Tech Stack:** Rust (Cargo workspace: `rocket-shared`, `rocket-app`, `rocket-environment`, `rocket-audit`, `src-tauri`), TypeScript/React (Vite, Zustand, TanStack Query).

**Spec:** `docs/superpowers/specs/2026-09-16-env-var-write-audit-spec.md`

**Note on the prior "secret storage hardening" spec dependency:** The spec declares a dependency on `docs/superpowers/specs/2026-09-16-secret-storage-hardening-spec.md`, whose implementation plan (`docs/superpowers/plans/2026-09-16-secret-storage-hardening-plan.md`) **does not exist yet** (checked at plan-writing time). Per the task brief, this plan is written directly against the CURRENT source of `Variable`, `Environment::set_variable`, and `EnvironmentRepository` — their public shape is not expected to change from that other work, only internal behavior (e.g. how `secret: true` values are stored, keyring vs. plaintext). If that work lands first and changes these signatures, re-check Task 3 and Task 4 against the new shape before executing them.

**Note on a spec premise that did not hold on re-verification:** The spec's §3.3 assumes `src/lib/execute-request.ts` "currently listens for/handles `ScriptError`, `ConsoleOutput`, `TestsCompleted`, `RequestExecuted`" via Tauri event listeners. On reading the actual file, this is not the case — `sendRequest()` reads `result.consoleEntries` / `result.testResults` / `result.scriptError` directly off the synchronous return value of the `executeRequest()` Tauri **command** (an IPC call/response, not an event subscription). There is no `listen()` call anywhere in `execute-request.ts`, and grepping the whole frontend shows **nothing currently listens** for the `environment-changed` Tauri event that `DomainEvent::EnvironmentSaved` already maps to in `TauriEventBus` — environment data freshness today is handled entirely through React Query cache invalidation on the mutating hook's own `onSuccess` (see `useSaveEnvironment` in `src/lib/queries/environment-queries.ts`), not through event subscriptions. Task 7 below follows that real, verified pattern (direct `invalidateQueries` calls after a successful `executeRequest()`, plus a `window.dispatchEvent`/`addEventListener('rocket:...')` pair for the one piece of state — `RequestPanel`'s locally-fetched collection variables — that isn't in React Query at all) instead of inventing a new event-listener pattern that doesn't otherwise exist in this file. The two new Rust `DomainEvent` variants are still added (Task 1) so other/future windows and consumers can observe script-driven writes via the Tauri event bus, matching the spec's backend design intent.

## Global Constraints

- Persist-by-default for `rok.setEnvVar`/`rok.setGlobalEnvVar` stays exactly as shipped in commits `3e116d6`/`1d67d3d` — this plan does not touch that decision.
- A brand-new key a script introduces (no pre-existing variable of that key) must default to `secret: false` — only the user, via the environment editor UI, may promote a variable to secret.
- `rok.setGlobalEnvVar`'s existing `persist: false` hardcode at `ops/rok.rs:114` is untouched — it is already force-persisted by its caller and gets the same event/audit plumbing as active-env writes via the shared `apply_env_writes` fix.
- No confirmation dialog before a script persists a write — out of scope, flagged as a future consideration only.
- Rust: never `unwrap()` in production paths (test code may use `.unwrap()`/`.expect()` per existing convention in this file).
- Serde: `#[serde(rename_all = "camelCase")]` stays on `DomainEvent` (already present at the enum level) — do not add it again per-variant.
- Every task that touches `.yml`-adjacent infra (collection/environment crates, `FsEnvironmentRepo`-backed repos) must read `docs/superpowers/specs/opencollection-spec-reference.md` first, per this repo's `.claude/rules/rust-ddd-boundaries.md` and `.claude/rules/tauri-ipc-boundaries.md`.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-shared/src/events.rs` | Add `DomainEvent::ScriptVariableWritten` and `DomainEvent::CollectionVariableWritten` variants + serialization tests. |
| `src-tauri/src/tauri_event_bus.rs` | Map the two new variants to Tauri event names. |
| `crates/rocket-app/src/env_audit.rs` | **New file.** Shared `publish_env_write_events` helper + its own unit tests. |
| `crates/rocket-app/src/lib.rs` | Register the new `env_audit` module. |
| `crates/rocket-app/src/environment_service.rs` | Refactor `EnvironmentService::save` to call the shared helper (behavior-preserving). |
| `crates/rocket-app/src/execution_service.rs` | Rewrite `apply_env_writes` to preserve variable metadata and emit events/audit; add events to `apply_collection_var_write`; fix stale doc comment. |
| `crates/rocket-scripting/src/result.rs` | Fix stale doc comment on `EnvVarWrite.persist`. |
| `src/lib/execute-request.ts` | Invalidate environment React Query caches and dispatch a `rocket:collection-vars-written` custom event after a successful `executeRequest()` call. |
| `src/components/request/RequestPanel.tsx` | Listen for `rocket:collection-vars-written` and re-fetch collection variables when it matches the open tab's collection. |

---

### Task 1: Add `ScriptVariableWritten` and `CollectionVariableWritten` domain events

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `src-tauri/src/tauri_event_bus.rs`

**Interfaces:**
- Produces: `DomainEvent::ScriptVariableWritten { scope: String, environment: Option<String>, collection: Option<String>, key: String }` — `scope` is `"environment"` for `rok.setEnvVar`/`rok.setGlobalEnvVar`-driven writes (and the `runtime.actions` `"environment"` scope), `"collection"` for `rok.setCollectionVar`-driven writes (and the `runtime.actions` `"collection"` scope). Exactly one of `environment`/`collection` is `Some` depending on `scope`.
- Produces: `DomainEvent::CollectionVariableWritten { collection: String, key: String }`.
- Consumed by: Task 3 (`apply_env_writes`) and Task 5 (`apply_collection_var_write`) publish these; Task 1 only defines and wires them.

I confirmed by reading the full `DomainEvent` enum in `crates/rocket-shared/src/events.rs` that no existing variant covers "a collection variable was written" — the closest is `RequestSaved`/`ItemMoved`, neither of which fits. Both new variants are additive.

- [ ] **Step 1: Write failing serialization tests for the two new variants**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-shared/src/events.rs` (after `workspace_description_updated_serializes`):

```rust
    #[test]
    fn script_variable_written_serializes() {
        let event = DomainEvent::ScriptVariableWritten {
            scope: "environment".into(),
            environment: Some("staging".into()),
            collection: None,
            key: "API_KEY".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("scriptVariableWritten") || json.contains("ScriptVariableWritten"));
        assert!(json.contains("staging"));
        assert!(json.contains("API_KEY"));
    }

    #[test]
    fn collection_variable_written_serializes() {
        let event = DomainEvent::CollectionVariableWritten {
            collection: "my-api".into(),
            key: "BASE_URL".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("collectionVariableWritten") || json.contains("CollectionVariableWritten"));
        assert!(json.contains("my-api"));
        assert!(json.contains("BASE_URL"));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail to compile**

Run: `cargo test -p rocket-shared script_variable_written_serializes collection_variable_written_serializes`
Expected: FAIL — `no variant named ScriptVariableWritten found for enum DomainEvent` (compile error).

- [ ] **Step 3: Add the two variants to `DomainEvent`**

In `crates/rocket-shared/src/events.rs`, add after the existing `ScriptError` variant (still inside the `// Script events` group) and before the enum's closing `}`:

```rust
    /// Emitted when a script (`rok.setEnvVar`/`setGlobalEnvVar`/`setCollectionVar`)
    /// or a declarative `runtime.actions` set-variable write persists a variable.
    /// Distinct from `EnvironmentSaved`/`CollectionVariableWritten`, which fire
    /// alongside it — this variant exists so the frontend can distinguish an
    /// automated script write from a manual user edit without correlating
    /// timestamps.
    ScriptVariableWritten {
        /// "environment" | "collection"
        scope: String,
        environment: Option<String>,
        collection: Option<String>,
        key: String,
    },
    /// Emitted when a collection-scoped variable is written (currently only via
    /// `rok.setCollectionVar` / `runtime.actions` collection-scope writes; manual
    /// collection-settings saves do not yet publish any event).
    CollectionVariableWritten { collection: String, key: String },
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p rocket-shared script_variable_written_serializes collection_variable_written_serializes`
Expected: PASS (2 tests).

- [ ] **Step 5: Map the new variants in `TauriEventBus`**

In `src-tauri/src/tauri_event_bus.rs`, the `match &event` is exhaustive, so the compiler will already force this — but do it explicitly and intentionally. Add `DomainEvent::CollectionVariableWritten { .. }` to the existing collection-changed arm group, and a new arm for `ScriptVariableWritten` next to the other script-event arms:

```rust
            DomainEvent::RequestSaved { .. } | DomainEvent::RequestDeleted { .. } => {
                "collection-changed"
            }
            DomainEvent::ItemMoved { .. } => "collection-changed",
            DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
```

and:

```rust
            // Script events
            DomainEvent::ConsoleOutput { .. } => "script-console",
            DomainEvent::TestsCompleted { .. } => "script-tests",
            DomainEvent::ScriptError { .. } => "script-error",
            DomainEvent::ScriptVariableWritten { .. } => "script-variable-written",
```

- [ ] **Step 6: Verify the workspace still compiles**

Run: `cargo check -p rocket-shared -p rocket-app`
Expected: no errors. (`src-tauri` requires the Tauri build toolchain; a plain `cargo check -p src-tauri` is fine to run too if available in this environment, but is not required to pass this task — the match-exhaustiveness check is what matters and that crate is checked again in Task 8.)

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs
git commit -m "feat: add ScriptVariableWritten and CollectionVariableWritten domain events"
```

---

### Task 2: Extract shared `publish_env_write_events` helper

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/env_audit.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `crates/rocket-app/src/env_audit.rs`

**Interfaces:**
- Consumes: `rocket_environment::Environment` (from Task-independent existing crate — `crates/rocket-environment/src/environment.rs`, unchanged), `rocket_shared::events::{DomainEvent, EventPublisher}` (Task 1 — unchanged by Task 1 for `EnvironmentSaved`, which already existed), `rocket_audit::{event::AuditEventKind, publisher::SecurityAuditPublisher}` (unchanged, read at `crates/rocket-audit/src/event.rs` and `crates/rocket-audit/src/publisher.rs`).
- Produces: `pub fn rocket_app::env_audit::publish_env_write_events(events: &dyn EventPublisher, audit: &dyn SecurityAuditPublisher, env_name: &str, before: &Environment, after: &Environment)`. Consumed by Task 3 (`environment_service.rs::save`) and Task 4 (`execution_service.rs::apply_env_writes`).

This is a pure extraction of the diff-and-publish loop already in `EnvironmentService::save` (`crates/rocket-app/src/environment_service.rs:41-74`, read in full before starting — reproduced here for reference):

```rust
pub fn save(&self, env: &Environment) -> DomainResult<()> {
    let previous = self.repo.get(&env.name).ok();
    self.repo.save(env)?;
    self.events.publish(DomainEvent::EnvironmentSaved { name: env.name.clone() });
    for var in &env.variables {
        if !var.secret || var.value.is_empty() { continue; }
        let changed = match &previous {
            Some(prev) => prev.variables.iter().find(|v| v.key == var.key)
                .map(|v| v.value != var.value || !v.secret).unwrap_or(true),
            None => true,
        };
        if changed {
            self.audit.publish("system".into(), None, AuditEventKind::SecretVariableWritten {
                environment: env.name.clone(),
                variable_key: var.key.clone(),
            });
        }
    }
    Ok(())
}
```

- [ ] **Step 1: Write the failing tests for the new module**

Create `crates/rocket-app/src/env_audit.rs` with just the test module first (function body is `todo!()` so the crate compiles enough to attempt the test, but the test will fail):

```rust
//! Shared audit/event-publishing logic for environment writes. Extracted so that
//! both a manual environment save (`EnvironmentService::save`) and a script-driven
//! write (`RequestExecutionService::apply_env_writes`) produce an identical audit
//! trail — see docs/superpowers/specs/2026-09-16-env-var-write-audit-spec.md §3.2.

use rocket_audit::{event::AuditEventKind, publisher::SecurityAuditPublisher};
use rocket_environment::Environment;
use rocket_shared::events::{DomainEvent, EventPublisher};

/// Publishes `DomainEvent::EnvironmentSaved` unconditionally, then one
/// `AuditEventKind::SecretVariableWritten` per secret variable in `after` whose
/// value changed (or is new) relative to `before`. Non-secret variables and
/// empty-valued secrets never emit the audit event.
pub fn publish_env_write_events(
    events: &dyn EventPublisher,
    audit: &dyn SecurityAuditPublisher,
    env_name: &str,
    before: &Environment,
    after: &Environment,
) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::Variable;
    use std::sync::Mutex;

    struct RecordingPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    struct CapturingAuditPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingAuditPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    #[test]
    fn publishes_environment_saved_unconditionally() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let after = Environment::new("prod");

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let published = events.events.lock().unwrap();
        assert!(published
            .iter()
            .any(|e| matches!(e, DomainEvent::EnvironmentSaved { name } if name == "prod")));
    }

    #[test]
    fn publishes_secret_variable_written_for_new_secret() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let mut after = Environment::new("prod");
        after.set_variable(Variable::secret("API_KEY", "sk-12345"));

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().unwrap();
        assert!(captured.iter().any(|k| matches!(
            k,
            AuditEventKind::SecretVariableWritten { environment, variable_key }
                if environment == "prod" && variable_key == "API_KEY"
        )));
    }

    #[test]
    fn does_not_publish_secret_variable_written_for_non_secret() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let mut after = Environment::new("prod");
        after.set_variable(Variable::new("HOST", "api.example.com"));

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().unwrap();
        assert!(captured.is_empty());
    }

    #[test]
    fn does_not_publish_secret_variable_written_when_value_unchanged() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let mut before = Environment::new("prod");
        before.set_variable(Variable::secret("API_KEY", "sk-12345"));
        let after = before.clone();

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().unwrap();
        assert!(captured.is_empty());
    }
}
```

Also add `pub mod env_audit;` to `crates/rocket-app/src/lib.rs` (alphabetically, after `pub mod environment_service;` and before `pub mod export_service;`):

```rust
pub mod env_audit;
pub mod environment_service;
```

(Keep the existing line ordering for the rest of the file as-is — only insert this one line.)

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p rocket-app --lib env_audit::`
Expected: FAIL — panics with `not yet implemented` (from `todo!()`) on all 4 tests.

- [ ] **Step 3: Implement `publish_env_write_events`**

Replace the `todo!()` body in `crates/rocket-app/src/env_audit.rs`:

```rust
pub fn publish_env_write_events(
    events: &dyn EventPublisher,
    audit: &dyn SecurityAuditPublisher,
    env_name: &str,
    before: &Environment,
    after: &Environment,
) {
    events.publish(DomainEvent::EnvironmentSaved { name: env_name.to_string() });

    for var in &after.variables {
        if !var.secret || var.value.is_empty() {
            continue;
        }
        let changed = before
            .variables
            .iter()
            .find(|v| v.key == var.key)
            .map(|v| v.value != var.value || !v.secret)
            .unwrap_or(true);
        if changed {
            audit.publish(
                "system".into(),
                None,
                AuditEventKind::SecretVariableWritten {
                    environment: env_name.to_string(),
                    variable_key: var.key.clone(),
                },
            );
        }
    }
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p rocket-app --lib env_audit::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/env_audit.rs crates/rocket-app/src/lib.rs
git commit -m "feat: extract shared env-write audit/event publishing helper"
```

---

### Task 3: Refactor `EnvironmentService::save` to use the shared helper

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/environment_service.rs`

**Interfaces:**
- Consumes: `rocket_app::env_audit::publish_env_write_events` (Task 2).
- Produces: `EnvironmentService::save` public signature unchanged (`pub fn save(&self, env: &Environment) -> DomainResult<()>`); internal behavior identical to before (this is a pure refactor, verified by the existing `save_emits_security_audit_event` test continuing to pass unmodified).

This task is a **behavior-preserving refactor** — no new test is needed beyond confirming the existing test suite for this file still passes, because the extraction in Task 2 was designed to reproduce this exact logic. Treat "existing tests still green" as the pass/fail signal.

- [ ] **Step 1: Confirm the existing test passes before the refactor (baseline)**

Run: `cargo test -p rocket-app --lib environment_service::tests::save_emits_security_audit_event`
Expected: PASS (this establishes the baseline you must not break).

- [ ] **Step 2: Replace the inline diff-and-publish loop with a call to the shared helper**

In `crates/rocket-app/src/environment_service.rs`, replace the body of `save` (currently lines 41-74):

```rust
    pub fn save(&self, env: &Environment) -> DomainResult<()> {
        // Snapshot previous state so we can detect which secret values actually changed.
        let previous = self.repo.get(&env.name).ok();
        self.repo.save(env)?;

        // `before` is an empty environment of the same name when there was no prior
        // save — `publish_env_write_events` treats every secret in `after` as "changed"
        // in that case, matching the pre-refactor behavior (`previous: None` branch).
        let before = previous.unwrap_or_else(|| Environment::new(env.name.as_str()));
        crate::env_audit::publish_env_write_events(
            self.events.as_ref(),
            self.audit.as_ref(),
            &env.name,
            &before,
            env,
        );

        Ok(())
    }
```

- [ ] **Step 3: Run the full test file to confirm no regression**

Run: `cargo test -p rocket-app --lib environment_service::`
Expected: PASS — all tests in this module (`save_and_list`, `get_by_name`, `delete_removes_environment`, `save_emits_security_audit_event`) pass unchanged.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/environment_service.rs
git commit -m "refactor: route EnvironmentService::save through shared env-write audit helper"
```

---

### Task 4: Rewrite `apply_env_writes` to preserve variable metadata and emit audit/events

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`
- Test: same file, `#[cfg(test)] mod tests` block (reuses `RecordingEnvRepo`, `SharedEnvRepo`, `MockScriptEngine`, `StubCollectionRepo`, `sample_input`, `CapturingAuditPublisher` — all already defined in this file's test module, read them in full before starting at lines ~1615-1904 and ~1032-1063).

**Interfaces:**
- Consumes: `rocket_app::env_audit::publish_env_write_events` (Task 2), `DomainEvent::ScriptVariableWritten` (Task 1), `rocket_environment::Variable` fields (`key, value, enabled, secret, description, value_variants, secret_type` — read in full at `crates/rocket-environment/src/variable.rs:5-20` first, unchanged shape).
- Produces: `apply_env_writes` signature unchanged (`fn apply_env_writes(&self, env_name: &str, writes: &[rocket_scripting::EnvVarWrite], force_persist: bool)`), internals rewritten. Called by `apply_script_side_effects` (env + global-env write sites, unchanged call sites) and `apply_actions`'s `"environment"` scope branch (unchanged call site) — both automatically inherit the fix.

The current buggy implementation (`crates/rocket-app/src/execution_service.rs:320-349`, confirmed by reading the file):

```rust
fn apply_env_writes(&self, env_name: &str, writes: &[rocket_scripting::EnvVarWrite], force_persist: bool) {
    let persist_writes: Vec<_> = writes.iter().filter(|w| force_persist || w.persist).collect();
    if persist_writes.is_empty() { return; }
    if let Ok(mut env) = self.env_repo.get(env_name) {
        for write in persist_writes {
            if write.value.is_null() {
                env.remove_variable(&write.key);
            } else {
                let str_val = write.value.as_str().map(str::to_owned).unwrap_or_else(|| write.value.to_string());
                env.set_variable(rocket_environment::Variable::new(write.key.clone(), str_val)); // <-- always secret:false, drops description/secret_type
            }
        }
        let _ = self.env_repo.save(&env); // <-- no event, no audit
    }
}
```

- [ ] **Step 1: Write the failing test for secret-flag preservation**

Add to the `#[cfg(test)] mod tests` block in `crates/rocket-app/src/execution_service.rs`, near the other `post_response_script_env_var_write_*` tests (after `post_response_script_env_var_write_always_calls_env_repo_save`, around line 1965):

```rust
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
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p rocket-app --lib execution_service::tests::post_response_script_env_var_write_preserves_secret_flag execution_service::tests::post_response_script_env_var_write_new_key_defaults_to_non_secret`
Expected: FAIL on the first test — `assertion failed: var.secret` (currently `false` because `Variable::new` always sets `secret: false`). The second test currently passes already (no regression risk there, but it locks in the deliberate-default behavior before the rewrite touches this code path).

- [ ] **Step 3: Rewrite `apply_env_writes`**

Replace the method body in `crates/rocket-app/src/execution_service.rs` (lines 320-349):

```rust
    /// Read-modify-write helper for env var writes against a named environment.
    ///
    /// Both active-environment and global-environment writes are currently always
    /// persisted (`force_persist: true` at both call sites in `apply_script_side_effects`,
    /// and always `true` from the `runtime.actions` "environment" scope branch of
    /// `apply_actions`) regardless of the script-supplied `persist` flag on each
    /// `EnvVarWrite`. `EnvVarWrite.persist` is preserved for wire/API compatibility
    /// but has no effect today.
    ///
    /// Existing variable metadata (`enabled`, `secret`, `description`, `secret_type`)
    /// is preserved across a script-driven write — only `value` (and, for a brand-new
    /// key, `enabled: true`) is set by the script. A script can never promote a
    /// variable to `secret: true`; only the user can do that via the environment
    /// editor UI. On a successful save, publishes the same `DomainEvent::EnvironmentSaved`
    /// / `AuditEventKind::SecretVariableWritten` audit trail a manual save produces
    /// (via `env_audit::publish_env_write_events`), plus one
    /// `DomainEvent::ScriptVariableWritten` per write actually applied.
    fn apply_env_writes(
        &self,
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
        let Ok(mut env) = self.env_repo.get(env_name) else {
            return;
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
            let existing = env.variables.iter().find(|v| v.key == write.key);
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

        if self.env_repo.save(&env).is_ok() {
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
    }
```

Add `use crate::env_audit;` to the top-level `use` block of `crates/rocket-app/src/execution_service.rs` (alongside the existing `use rocket_audit::{...}` import at the top of the file).

- [ ] **Step 4: Run the two new tests to confirm they pass**

Run: `cargo test -p rocket-app --lib execution_service::tests::post_response_script_env_var_write_preserves_secret_flag execution_service::tests::post_response_script_env_var_write_new_key_defaults_to_non_secret`
Expected: PASS (2 tests).

- [ ] **Step 5: Write the failing test for audit-event publication on a secret write**

Add a new test in the same block. This one needs both audit and event capturing, so it constructs the service manually rather than via `build_svc_with_script` (which hardcodes `NullEventPublisher` and no audit) — follow the exact pattern already used by `execute_publishes_event` (lines ~1328-1367) and `execute_emits_audit_for_bearer_auth` for the `Arc`/`SharedPublisher` wiring, combined with `.with_script_engine(...)`:

```rust
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

        let published = event_publisher.events.lock().unwrap();
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

        let captured = audit_publisher.captured.lock().unwrap();
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

        let captured = audit_publisher.captured.lock().unwrap();
        assert!(
            !captured.iter().any(|k| matches!(k, AuditEventKind::SecretVariableWritten { .. })),
            "a non-secret write must not publish SecretVariableWritten, got {:?}", *captured
        );
    }
```

`RecordingPublisher` (a struct with `events: Mutex<Vec<DomainEvent>>` implementing `EventPublisher`) does not yet exist in this file's test module — the closest existing thing is the inline one defined inside `execute_publishes_event`'s test body. Promote it to a module-level struct next to `CapturingAuditPublisher` (around line 1615) so both new tests above can share it:

```rust
    struct RecordingPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl rocket_shared::events::EventPublisher for RecordingPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().unwrap().push(event);
        }
    }
```

Then simplify the existing `execute_publishes_event` test to use this shared struct instead of its own inline `RecordingPublisher` definition (delete the duplicate `struct RecordingPublisher { events: Mutex<Vec<DomainEvent>> }` and its `impl` block currently inside that test body, at lines ~1333-1341) — keep its `SharedPublisher` wrapper as-is since that one is specific to that test's `Arc` plumbing pattern (the new tests above define their own `SharedPub` inline for the same reason: each test's `Arc` wrapper needs a distinct type name to avoid `E0119` conflicting-impl errors if two tests are in the same module and both `impl EventPublisher for SharedPublisher` — keeping the wrapper-struct definitions test-local, only hoisting the plain recording struct, avoids that).

- [ ] **Step 6: Run the tests to confirm they fail, then pass**

Run: `cargo test -p rocket-app --lib execution_service::tests::post_response_script_env_var_write_publishes_secret_audit_and_events execution_service::tests::post_response_script_env_var_write_non_secret_does_not_publish_secret_audit`
Expected first: FAIL to compile (`RecordingPublisher` not yet hoisted / duplicate impl) or fail assertions if it does compile against the old `apply_env_writes` (no events published at all). After Step 3's rewrite is in place (it already is, from Step 3 above — Steps 5-6 are verifying the event/audit half of the same rewritten method), re-run:
Expected: PASS (2 tests).

- [ ] **Step 7: Run the full `execution_service` test module to confirm no regressions**

Run: `cargo test -p rocket-app --lib execution_service::`
Expected: PASS — all tests, including the pre-existing `post_response_script_env_var_write_persist_calls_env_repo_save`, `post_response_script_env_var_write_always_calls_env_repo_save`, `post_response_script_global_env_var_write_calls_env_repo_save`, `after_response_action_writes_environment_variable_when_scope_environment`, and every other test in this file.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "fix: preserve variable metadata and emit audit trail on script-driven env writes"
```

---

### Task 5: Emit events from `apply_collection_var_write`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

**Interfaces:**
- Consumes: `DomainEvent::CollectionVariableWritten`, `DomainEvent::ScriptVariableWritten` (Task 1).
- Produces: `apply_collection_var_write` signature unchanged (`fn apply_collection_var_write(&self, collection: &str, key: &str, value: &str) -> DomainResult<()>`). This method is shared by `apply_script_side_effects` (the `rok.setCollectionVar` path) and `apply_actions`'s `"collection"` scope branch (the declarative `runtime.actions` path) — both automatically get the new events, which is intentional: both are automated, non-interactive writes and the audit-trail rationale in the spec applies equally to both.

Current implementation (`crates/rocket-app/src/execution_service.rs:309-313`):

```rust
fn apply_collection_var_write(&self, collection: &str, key: &str, value: &str) -> DomainResult<()> {
    let mut settings = self.collection_repo.get_settings(collection)?;
    upsert_variable(&mut settings.variables, key, value);
    self.collection_repo.save_settings(collection, &settings)
}
```

- [ ] **Step 1: Write the failing test**

Add near `post_response_script_collection_var_write_calls_save_settings` (after it, around line 2005):

```rust
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

        let published = event_publisher.events.lock().unwrap();
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
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p rocket-app --lib execution_service::tests::post_response_script_collection_var_write_publishes_events`
Expected: FAIL — the `published` vector is empty (no events currently emitted).

- [ ] **Step 3: Add event publishing to `apply_collection_var_write`**

```rust
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
```

- [ ] **Step 4: Run the test to confirm it passes**

Run: `cargo test -p rocket-app --lib execution_service::tests::post_response_script_collection_var_write_publishes_events`
Expected: PASS.

- [ ] **Step 5: Run the full test module to confirm no regressions**

Run: `cargo test -p rocket-app --lib execution_service::`
Expected: PASS — all tests, including `post_response_script_collection_var_write_calls_save_settings` and `after_response_action_writes_collection_variable`, which now also emit events but don't assert on them (no false failures).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat: publish CollectionVariableWritten/ScriptVariableWritten on collection var writes"
```

---

### Task 6: Fix stale documentation comments

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (doc comment already rewritten as part of Task 4, Step 3 — this task only covers `rocket-scripting`)
- Modify: `crates/rocket-scripting/src/result.rs`

**Interfaces:** None — comment-only change, no compiled interface affected.

Note: the `execution_service.rs:317-319` doc comment the spec calls out as stale is the same doc comment replaced in Task 4 Step 3 (the new doc comment on `apply_env_writes` already states the corrected behavior). This task only needs to fix the second stale comment.

- [ ] **Step 1: Fix the `EnvVarWrite.persist` doc comment**

In `crates/rocket-scripting/src/result.rs`, the current comment (line 99):

```rust
/// A single variable write to an environment scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
    /// When `true`, write is persisted to the environment `.yml` file.
    pub persist: bool,
}
```

Change the `persist` field's doc comment:

```rust
/// A single variable write to an environment scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
    /// Historically gated whether this write reached disk. As of `rocket-app`
    /// commits `3e116d6`/`1d67d3d`, both active-environment and
    /// global-environment writes are always persisted regardless of this flag
    /// (`force_persist: true` at both `RequestExecutionService::apply_env_writes`
    /// call sites). Preserved for wire/API compatibility; currently inert.
    pub persist: bool,
}
```

- [ ] **Step 2: Verify the crate still compiles**

Run: `cargo check -p rocket-scripting`
Expected: no errors (comment-only change).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-scripting/src/result.rs
git commit -m "docs: correct stale EnvVarWrite.persist doc comment"
```

---

### Task 7: Frontend — refresh environment and collection variable state after a script write

**Files:**
- Modify: `src/lib/execute-request.ts`
- Modify: `src/components/request/RequestPanel.tsx`
- Test: `src/lib/__tests__/execute-request.test.ts` (new file, for the extracted pure helper only — see Step 1)

**Interfaces:**
- Produces (new, exported from `execute-request.ts`): `export function getEnvInvalidationKeys(environmentName: string | undefined, globalEnvName: string | undefined): readonly (readonly unknown[])[]` — a pure function returning the React Query keys to invalidate, unit-testable without mocking Tauri IPC.
- Consumes: `environmentKeys` from `@/lib/queries/environment-queries` (already imported in `execute-request.ts`), `getQueryClient` from `@/lib/query-client` (already imported), `window.dispatchEvent`/`CustomEvent` (browser built-in, matching the existing `rocket:save-to-collection` pattern in `src/hooks/useKeyboardShortcuts.ts`/`src/components/request/RequestPanel.tsx`).

**Why this shape:** `sendRequest()` in `execute-request.ts` already has direct access to `collection`, `environmentName`, and `globalEnvName` right after `executeRequest()` resolves (see the existing destructure at lines 440-448 and the `globalEnvName` computed at line 451-453). Environment data lives in React Query (`environmentKeys.collection(...)`, `environmentKeys.global(...)`) — confirmed by reading `src/lib/queries/environment-queries.ts`, where `useSaveEnvironment`'s `onSuccess` already calls `qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) })` for the equivalent manual-save case. Collection variables, by contrast, are **not** in React Query at all — `RequestPanel.tsx` fetches them with a plain `useEffect`/`useState` keyed only on `tab.source?.collection` (confirmed by reading `RequestPanel.tsx:171-184`), so there is no cache to invalidate; the fix there is a same-tab-refresh signal via the `window.dispatchEvent`/`addEventListener('rocket:...')` pattern this codebase already uses for `rocket:save-to-collection`.

- [ ] **Step 1: Write the failing test for the pure key-selection helper**

Create `src/lib/__tests__/execute-request.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { getEnvInvalidationKeys } from '@/lib/execute-request';
import { environmentKeys } from '@/lib/queries/environment-queries';

describe('getEnvInvalidationKeys', () => {
  it('returns the collection-scoped key when environmentName is set', () => {
    const keys = getEnvInvalidationKeys('staging', undefined);
    expect(keys).toContainEqual(environmentKeys.collection('staging'));
  });

  it('returns the global-scoped key when globalEnvName is set', () => {
    const keys = getEnvInvalidationKeys(undefined, 'global-prod');
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns both keys when both are set', () => {
    const keys = getEnvInvalidationKeys('staging', 'global-prod');
    expect(keys).toContainEqual(environmentKeys.collection('staging'));
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns an empty list when neither is set', () => {
    const keys = getEnvInvalidationKeys(undefined, undefined);
    expect(keys).toHaveLength(0);
  });
});
```

Wait — `environmentKeys.collection('staging')` takes a *collection name*, not an environment name; re-check the actual key shape before writing the assertion. Read `src/lib/queries/environment-queries.ts` again: `collection: (collectionName: string) => ['environments', collectionName] as const`. This key is keyed by **collection name**, not environment name — the query for a collection's environments returns the whole list, and an env var write needs to invalidate the collection's environment list (so the specific env re-fetches), not something keyed by environment name. Fix the test (and the eventual implementation) to take `collection` instead of `environmentName`:

```typescript
import { describe, expect, it } from 'vitest';
import { getEnvInvalidationKeys } from '@/lib/execute-request';
import { environmentKeys } from '@/lib/queries/environment-queries';

describe('getEnvInvalidationKeys', () => {
  it('returns the collection environments key when collection is set', () => {
    const keys = getEnvInvalidationKeys('my-api', undefined);
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
  });

  it('returns the global environment key when globalEnvName is set', () => {
    const keys = getEnvInvalidationKeys(undefined, 'global-prod');
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns both keys when both are set', () => {
    const keys = getEnvInvalidationKeys('my-api', 'global-prod');
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns an empty list when neither is set', () => {
    const keys = getEnvInvalidationKeys(undefined, undefined);
    expect(keys).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `yarn test execute-request`
Expected: FAIL — `getEnvInvalidationKeys is not a function` / not exported.

- [ ] **Step 3: Add `getEnvInvalidationKeys` and wire it into `sendRequest`**

In `src/lib/execute-request.ts`, add the new exported function near the other exported helpers (after `resolveRequestFields`, before `maybeAutoRefreshOrFetchToken`):

```typescript
// Returns the React Query keys to invalidate after a script-driven variable
// write (rok.setEnvVar / rok.setGlobalEnvVar), so the environment editor and
// any open request tabs stop showing a stale value. `collection` is the
// collection whose per-collection environments should be re-fetched (env
// data is keyed by collection name, not environment name — see
// `environmentKeys.collection` in `@/lib/queries/environment-queries`).
export function getEnvInvalidationKeys(
  collection: string | undefined,
  globalEnvName: string | undefined,
): readonly (readonly unknown[])[] {
  const keys: (readonly unknown[])[] = [];
  if (collection) keys.push(environmentKeys.collection(collection));
  if (globalEnvName) keys.push(environmentKeys.global(globalEnvName));
  return keys;
}
```

Then, in `sendRequest`, after the `usePaneStore.getState().setResponse(tabId, responseState);` call (inside the `try` block, after the response has been written — around line 504), add the cache invalidation and the collection-vars signal. `collection` and `globalEnvName` are already in scope at this point in the function:

```typescript
    usePaneStore.getState().setResponse(tabId, responseState);

    // A pre/post-response or tests script may have written env or collection
    // variables via rok.setEnvVar/setGlobalEnvVar/setCollectionVar. Refresh the
    // relevant caches so the environment editor and this tab's variable context
    // don't keep showing the stale pre-write value. Cheap even when nothing
    // changed — React Query dedupes a no-op invalidate against unchanged data.
    const qc = getQueryClient();
    for (const key of getEnvInvalidationKeys(collection, globalEnvName)) {
      qc.invalidateQueries({ queryKey: key });
    }
    if (collection) {
      window.dispatchEvent(
        new CustomEvent('rocket:collection-vars-written', { detail: { collection } }),
      );
    }
```

- [ ] **Step 4: Run the test to confirm it passes**

Run: `yarn test execute-request`
Expected: PASS (4 tests).

- [ ] **Step 5: Add the `RequestPanel.tsx` listener for `rocket:collection-vars-written`**

In `src/components/request/RequestPanel.tsx`, extract the existing collection-variables fetch (currently inline in the `useEffect` at lines 172-184) into a `useCallback` so both the mount/collection-change effect and the new event listener can call it:

```typescript
  // Fetch collection variables for the scoped variable context.
  const refetchCollectionVariables = useCallback(() => {
    if (!tab.source?.collection) {
      setCollectionVariables([]);
      return;
    }
    getCollectionSettings(tab.source.collection)
      .then((s) => {
        setCollectionVariables(s.variables);
      })
      .catch(() => {
        setCollectionVariables([]);
      });
  }, [tab.source?.collection]);

  useEffect(() => {
    refetchCollectionVariables();
  }, [refetchCollectionVariables]);

  // A script-driven rok.setCollectionVar write (execute-request.ts) dispatches
  // this event after a successful execute() call — refresh so the Vars tab and
  // variable-resolution context for this tab don't show a stale value.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ collection: string }>).detail;
      if (detail?.collection === tab.source?.collection) {
        refetchCollectionVariables();
      }
    };
    window.addEventListener('rocket:collection-vars-written', handler);
    return () => window.removeEventListener('rocket:collection-vars-written', handler);
  }, [tab.source?.collection, refetchCollectionVariables]);
```

This replaces the existing `useEffect` block at lines 172-184 (delete the old inline version — `refetchCollectionVariables` plus its own trigger-`useEffect` replaces it 1:1) and adds the new listener effect immediately after it.

- [ ] **Step 6: Run TypeScript and lint checks**

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no new lint errors introduced by this change (pre-existing warnings elsewhere in the repo, if any, are not this task's concern).

- [ ] **Step 7: Commit**

```bash
git add src/lib/execute-request.ts src/lib/__tests__/execute-request.test.ts src/components/request/RequestPanel.tsx
git commit -m "fix: refresh environment/collection variable caches after a script writes a variable"
```

---

### Task 8: Full verification pass

**Files:** None modified — verification only.

- [ ] **Step 1: Run the full Rust test suite for every crate touched by this plan**

Run: `cargo test -p rocket-shared -p rocket-app -p rocket-environment -p rocket-scripting`
Expected: PASS, zero failures.

- [ ] **Step 2: Run `cargo check` across the whole workspace, including `src-tauri`**

Run: `cargo check --workspace`
Expected: no errors (this is the first point the `DomainEvent` match-exhaustiveness change in `src-tauri/src/tauri_event_bus.rs` from Task 1 gets checked against a real Tauri build context, if one is available in this environment).

- [ ] **Step 3: Run the full frontend TypeScript check**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Run the full frontend test suite**

Run: `yarn test`
Expected: PASS, zero failures, including the new `src/lib/__tests__/execute-request.test.ts`.

- [ ] **Step 5: Run Biome lint/format check**

Run: `yarn check`
Expected: no new violations introduced by this plan's changes.

- [ ] **Step 6: Manually cross-check each acceptance criterion from the spec**

Go through `docs/superpowers/specs/2026-09-16-env-var-write-audit-spec.md` §6 (Acceptance criteria) one by one and confirm which test in this plan covers it:

1. Secret flag preserved after script write → `post_response_script_env_var_write_preserves_secret_flag` (Task 4).
2. Exactly one `SecretVariableWritten` published → `post_response_script_env_var_write_publishes_secret_audit_and_events` (Task 4).
3. `EnvironmentSaved` + `ScriptVariableWritten` published → same test (Task 4).
4. Non-secret write does not publish `SecretVariableWritten` → `post_response_script_env_var_write_non_secret_does_not_publish_secret_audit` (Task 4).
5. `rok.setCollectionVar` publishes a collection-scoped event → `post_response_script_collection_var_write_publishes_events` (Task 5).
6. Pre-existing persist-behavior tests still pass → verified in Task 4 Step 7 and this task's Step 1.
7. `cargo test -p rocket-app -p rocket-environment` passes; `yarn tsc --noEmit` passes → this task's Steps 1 and 3.

No gaps — do not proceed to declare the plan complete if any of the above is unaccounted for.
