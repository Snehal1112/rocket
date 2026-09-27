# ACP Transport Plan 06: App-Exit Session Cleanup — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every running ACP agent process is explicitly killed when the Tauri app itself quits, closing a credential-persistence gap found during Plan 05's Post-Implementation Review: Tauri does not drop managed state on exit, so nothing currently calls `end_session` for sessions still open when the user quits Rocket — leaving an agent process (with a real API key in its environment) running orphaned indefinitely.

**Architecture:** Add one new `AcpSessionClient` trait method, `end_all_sessions`, mirroring `end_session`'s per-session kill logic but sweeping the whole session map. `AcpAgentClient` (Plan 03) already has a reusable `terminate_session(&RunningSession) -> std::io::Result<()>` helper — `end_all_sessions` drains the session map and calls it on every entry, swallowing individual failures (this is best-effort shutdown cleanup; nothing observes a partial failure meaningfully once the app is already exiting). `AcpSessionService` (Plan 04) gets a thin delegating wrapper. `src-tauri/src/lib.rs`'s `.run(...)` call becomes `.build(...)` + `.run(|app_handle, event| ...)`, checking for `RunEvent::Exit` and calling the service synchronously via `tauri::async_runtime::block_on`.

**Tech Stack:** Rust, Tauri 2 `RunEvent`.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Session lifecycle & data flow, Credential handling & security). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md`.

## Global Constraints

- `end_all_sessions` is best-effort: it must never panic and should always return `Ok(())` from `AcpAgentClient`'s implementation — an individual session's kill failure (e.g. the process already exited) must not prevent cleanup of the rest. This mirrors `fail_and_remove`'s existing `let _ = terminate_session(&running);` pattern in the same file.
- The `RunEvent::Exit` handler must not log or format the `AcpSessionService`/`AcpAgentClient` state — this is purely a "kill everything" call, no new logging surface is being added.
- Test code uses `.expect("message")` for fallible calls, never the bare panicking shorthand; production code never reaches for that shorthand either.

## Review Focus

- `end_all_sessions` must actually empty the session map, not just iterate a snapshot and leave entries behind — a session started after the sweep begins is out of scope (app is exiting), but every session present when the call starts must be gone after it returns.
- The `RunEvent::Exit` wiring must retrieve `AcpSessionService` from `app_handle`'s managed state correctly — Tauri 2's `AppHandle::try_state::<T>()` returns `None` if the type was never `.manage()`d; the plan's own Plan 05 already does `app.manage(acp_session_svc)`, so this should succeed, but the handler must not panic if it somehow didn't (defensive `if let Some(...)`, not `.expect(...)`).
- Calling async code from a sync `RunEvent` closure needs `tauri::async_runtime::block_on` (or equivalent) — verify this is the correct, real API for the installed Tauri version, not guessed.

---

## Task 1: `AcpSessionClient::end_all_sessions` (trait + `AcpAgentClient` impl)

**Files:**
- Modify: `crates/rocket-acp/src/session.rs`
- Modify: `crates/rocket-infra/src/acp_agent_client.rs`

**Interfaces:**
- Produces: `AcpSessionClient::end_all_sessions(&self) -> DomainResult<()>`, implemented by `AcpAgentClient` — consumed by Task 2 of this plan (`AcpSessionService::end_all_sessions`).

- [ ] **Step 1: Add the trait method**

In `crates/rocket-acp/src/session.rs`, add to the `AcpSessionClient` trait, after `end_session`:

```rust
    /// Kills every currently-tracked session's process, for use when the
    /// whole application is shutting down (Tauri does not drop managed
    /// state on exit, so nothing else calls `end_session` for sessions
    /// still open at quit time). Best-effort: an individual session's kill
    /// failure must not prevent cleanup of the rest.
    async fn end_all_sessions(&self) -> DomainResult<()>;
```

Also add a matching implementation to the trait's own test module's `FakeSessionClient`/`FailingClient` (both existing structs in this file's `#[cfg(test)]` block) so the crate still compiles — `Ok(())` is sufficient for both, since neither test exercises this new method.

- [ ] **Step 2: Write the failing test in `rocket-infra`**

```rust
// crates/rocket-infra/tests/acp_agent_client.rs (add to the existing file)

#[tokio::test]
async fn acp_agent_client_end_all_sessions_kills_every_running_session() {
    let client = AcpAgentClient::new();
    let session_a = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session a");
    let session_b = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session b");

    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions should succeed");

    let (tx_a, _rx_a) = tokio::sync::mpsc::unbounded_channel();
    let err_a = client
        .send_prompt(&session_a, "hi".to_string(), tx_a)
        .await
        .expect_err("session a must be gone after end_all_sessions");
    assert!(matches!(err_a, DomainError::NotFound(_)));

    let (tx_b, _rx_b) = tokio::sync::mpsc::unbounded_channel();
    let err_b = client
        .send_prompt(&session_b, "hi".to_string(), tx_b)
        .await
        .expect_err("session b must be gone after end_all_sessions");
    assert!(matches!(err_b, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_all_sessions_on_empty_map_succeeds() {
    let client = AcpAgentClient::new();
    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions on an empty session map must succeed, not error");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: FAIL with "no method named `end_all_sessions`" (compile error — the trait method exists after Step 1, but `AcpAgentClient` doesn't implement it yet).

- [ ] **Step 4: Implement `end_all_sessions` in `AcpAgentClient`**

```rust
// crates/rocket-infra/src/acp_agent_client.rs — add inside
// `impl AcpSessionClient for AcpAgentClient`, after `end_session`

async fn end_all_sessions(&self) -> DomainResult<()> {
    // Drain the whole map (rather than iterating a snapshot and removing
    // one-by-one) so a session that finishes naturally mid-sweep can't be
    // double-terminated, and so the lock is held only for the swap itself.
    let sessions = std::mem::take(&mut *self.sessions.lock().await);
    for running in sessions.values() {
        let _ = terminate_session(running);
    }
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-infra -j4`
Expected: PASS — the 2 new tests plus every existing `rocket-infra` test unaffected (run the full crate suite, not just the filtered subset, since this task touches a shared trait implementation).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-acp/src/session.rs crates/rocket-infra/src/acp_agent_client.rs crates/rocket-infra/tests/acp_agent_client.rs
git commit -m "feat(acp): add AcpSessionClient::end_all_sessions"
```

---

## Task 2: `AcpSessionService::end_all_sessions` + `RunEvent::Exit` wiring

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `AcpSessionService::end_all_sessions(&self) -> DomainResult<()>` and the app-exit wiring — nothing downstream in this subproject consumes it further; this is the plan's terminal integration point.

- [ ] **Step 1: Write the failing test for `AcpSessionService`**

```rust
// crates/rocket-app/src/acp_session_service.rs (add to the existing tests module)

// Add a field to the existing FakeSessionClient struct in this file's test
// module: `end_all_sessions_called: Arc<AtomicBool>` (defaulting to
// `Arc::new(AtomicBool::new(false))` in its `Default` impl), and implement
// the trait method on it:
//
//     async fn end_all_sessions(&self) -> DomainResult<()> {
//         self.end_all_sessions_called.store(true, Ordering::SeqCst);
//         Ok(())
//     }

#[tokio::test]
async fn end_all_sessions_delegates_to_session_client() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let end_all_sessions_called = Arc::new(AtomicBool::new(false));
    let client = FakeSessionClient {
        end_all_sessions_called: Arc::clone(&end_all_sessions_called),
        ..Default::default()
    };
    let service = AcpSessionService::new(
        Box::new(client),
        Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        agent_config_service(),
    );

    service
        .end_all_sessions()
        .await
        .expect("end_all_sessions should succeed");
    assert!(end_all_sessions_called.load(Ordering::SeqCst));
}
```

(Use whatever the file's existing `EventPublisher`-wrapping pattern actually is — check the current file for the exact wrapper name before writing this, since Task 2 of Plan 04 introduced a `SharedEventPublisher` newtype that differs from this plan's own sketch above; match the real one.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: FAIL — `FakeSessionClient` doesn't implement `end_all_sessions` yet (compile error), and `AcpSessionService` has no such method.

- [ ] **Step 3: Implement `AcpSessionService::end_all_sessions`**

```rust
// crates/rocket-app/src/acp_session_service.rs — add inside `impl AcpSessionService`

pub async fn end_all_sessions(&self) -> DomainResult<()> {
    self.session_client.end_all_sessions().await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: PASS — 1 new test, plus every existing test in this module unaffected.

- [ ] **Step 5: Wire the `RunEvent::Exit` handler in `lib.rs`**

Find the existing `.run(tauri::generate_context!())` call near the end of the `run()` function in `src-tauri/src/lib.rs` (immediately followed by `.expect("error while running tauri application");`). Replace it with:

```rust
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(acp_session_svc) = app_handle.try_state::<rocket_app::AcpSessionService>() {
                    tauri::async_runtime::block_on(acp_session_svc.end_all_sessions());
                }
            }
        });
```

Before writing this, confirm `tauri::async_runtime::block_on` and `AppHandle::try_state` are the real, correct APIs for the installed Tauri version (check `Cargo.toml`'s pinned `tauri` version and, if in doubt, grep the vendored crate source) — do not guess if the names differ.

- [ ] **Step 6: Verify the full app builds and tests pass**

Run: `cargo check -p rocket -j4`
Expected: succeeds.

Run: `cargo test -p rocket-app -j4 && cargo test -p rocket-infra -j4 && cargo test -p rocket -j4`
Expected: PASS across all three crates, no regressions.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/acp_session_service.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): kill all agent sessions on app exit"
```

---

## Next Plan

None — this is the last plan in subproject B, closing the gap found during Plan 05's review. The overall ACP AI-assist feature's next subprojects are **C (AI Assist Chat Panel, frontend)** and **D (MCP Tool Server + Rocket Tools)**, which can proceed in parallel per the project memory `project_acp_ai_assist_feature.md`. Both need their own brainstorming/spec/plan cycle; do not start implementing either from this plan file alone.

## Post-Implementation Review

Before considering subproject B complete, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-acp/src/session.rs`, `crates/rocket-infra/src/acp_agent_client.rs`,
> `crates/rocket-infra/tests/acp_agent_client.rs`, `crates/rocket-app/src/acp_session_service.rs`,
> `src-tauri/src/lib.rs`.
>
> Check for:
> 1. Correctness — does `end_all_sessions` actually terminate every session that
>    was in the map when it was called, with no partial-failure leaving a
>    session both un-killed and still absent from the map (a stuck orphan)?
>    Does the `RunEvent::Exit` handler correctly retrieve `AcpSessionService`
>    from managed state and block on the async call without panicking if the
>    state were ever absent?
> 2. Manually verify (reasoning from the code, since this can't be exercised by
>    the automated test suite) that quitting the Tauri app with an active
>    session would actually invoke this path — trace `RunEvent::Exit`'s real
>    firing conditions for the installed Tauri version to confirm it fires on
>    a normal user-initiated quit, not just some narrower case.
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` — no
>    new dependency added to `rocket-acp`, no bare panicking shorthand in
>    production code paths.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-app -j4`, `cargo test -p rocket-infra -j4`,
> and `cargo check -p rocket -j4`, and confirm they still pass. Report what
> you found and fixed.

Once this review comes back clean (or its fixes are applied and re-verified), subproject B is complete. Update the project memory `project_acp_ai_assist_feature.md` to mark subproject B's status as done before starting subproject C or D's brainstorming.
