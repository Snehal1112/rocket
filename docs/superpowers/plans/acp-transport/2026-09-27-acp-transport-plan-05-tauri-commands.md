# ACP Transport Plan 05: Tauri Commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `AcpSessionService` over Tauri IPC (`start_agent_session`, `send_agent_prompt`, `end_agent_session`), map Plan 02's `DomainEvent` variants to named frontend events in `TauriEventBus`, and wire everything into `src-tauri/src/lib.rs`. This is the last plan in subproject B — once done, the ACP transport capability is independently exercisable via devtools, ahead of subproject C's chat UI.

**Architecture:** Thin command wrappers over `AcpSessionService`, following the exact structure `src-tauri/src/commands/agent_configs.rs` already uses. `TauriEventBus`'s match statement gains four new arms, in the same style the just-landed Flow events used. `lib.rs` gets a dedicated `AgentConfigService`/`SecretManagerService` instance pair for `AcpSessionService`'s own use, mirroring the existing pattern where `agent_config_svc` already got its own dedicated `SecretManagerService` instance sharing the common `vault_connection_secret_store`/`vault_fetcher` Arcs — this avoids needing to retrofit `Arc`-sharing into the already-committed subproject A wiring code.

**Tech Stack:** Rust, Tauri 2 commands.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Tauri IPC surface section). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md`.

## Global Constraints

- No new DTO types are needed — all three commands pass plain `String` parameters/returns, matching the spec's explicit decision to keep `stop_reason` a plain string rather than a modeled enum.
- `start_agent_session`/`send_agent_prompt`/`end_agent_session` are all `async` commands (mirroring `AcpSessionService`'s own async methods), unlike `agent_configs.rs`'s mostly-sync commands.
- This plan's commands and the `TauriEventBus` match-arm addition have no new dedicated unit tests, matching the existing precedent for this exact kind of thin-wrapper/wiring code in this codebase (`agent_configs.rs`'s simplest pass-through commands, and `tauri_event_bus.rs` generally, have none either — correctness here is enforced by the compiler's exhaustive-match check on `DomainEvent` and by `cargo check`/`cargo test` across the whole app).

## Review Focus

- The new dedicated `SecretManagerService`/`AgentConfigService` instance pair for `AcpSessionService` must share the *same* `vault_connection_secret_store`/`vault_fetcher` `Arc`s as every other instance in `lib.rs` (via `Arc::clone`), not construct fresh ones — otherwise a connection's client secret would be invisible to this new instance's credential resolution, and `ReqwestVaultSecretFetcher`'s internal token cache would needlessly duplicate.
- Registering the three new commands in `tauri::generate_handler!` without also adding `pub mod acp_sessions;` to `src-tauri/src/commands/mod.rs` is a common one-line omission that fails compilation — confirm both are present.
- `AcpAgentClient::new()` (Plan 03) must be constructed exactly once and passed into `AcpSessionService::new` as the boxed trait object — do not construct a second one anywhere, since the session map lives on that single instance.

---

## Task 1: `TauriEventBus` mapping

**Files:**
- Modify: `src-tauri/src/tauri_event_bus.rs`

**Interfaces:**
- Consumes: the four `DomainEvent::AcpSession*` variants (Plan 02).
- Produces: named frontend events `agent-session-started`, `agent-session-chunk`, `agent-session-finished`, `agent-session-failed` — consumed by subproject C's chat UI (or, for this subproject's own end-to-end verification, a devtools `listen(...)` call).

- [ ] **Step 1: Add the match arms**

In `src-tauri/src/tauri_event_bus.rs`, add alongside the existing `// Flow events` block, following it:

```rust
            // ACP AI-assist session events — mirrors the Flow/Collection
            // Runner events above: each variant gets its own frontend channel.
            DomainEvent::AcpSessionStarted { .. } => "agent-session-started",
            DomainEvent::AcpSessionChunk { .. } => "agent-session-chunk",
            DomainEvent::AcpSessionFinished { .. } => "agent-session-finished",
            DomainEvent::AcpSessionFailed { .. } => "agent-session-failed",
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rocket -j4`
Expected: succeeds. `DomainEvent`'s `match` in this file is exhaustive (no wildcard arm), so if any arm were missing this step would fail to compile with "non-exhaustive patterns" naming the missing variant — that compile error is this task's correctness proof, in place of a dedicated unit test (this file has none currently, for any event; `AppHandle`/`Emitter` aren't easily unit-testable without a running Tauri app).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tauri_event_bus.rs
git commit -m "feat(tauri): map ACP session events to frontend channels"
```

---

## Task 2: Tauri commands

**Files:**
- Create: `src-tauri/src/commands/acp_sessions.rs`
- Modify: `src-tauri/src/commands/mod.rs`

**Interfaces:**
- Consumes: `AcpSessionService` (Plan 04).
- Produces: `start_agent_session`, `send_agent_prompt`, `end_agent_session` — consumed by Task 3 of this plan (handler registration) and subproject C's frontend `invoke` calls.

- [ ] **Step 1: Write the commands**

```rust
// src-tauri/src/commands/acp_sessions.rs
use rocket_app::AcpSessionService;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.start_session(&agent_config_id, &cwd).await
}

#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.send_prompt(&session_id, prompt).await
}

#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.end_session(&session_id).await
}
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/commands/mod.rs`, add alongside the existing `pub mod agent_configs;`:

```rust
pub mod acp_sessions;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p rocket -j4`
Expected: succeeds. Per this plan's Global Constraints, no dedicated unit test is added for these commands — they are thin, direct pass-throughs to already-tested `AcpSessionService` methods, matching the precedent set by `agent_configs.rs`'s simplest commands (`delete_agent_config`, etc.), which also have none.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/acp_sessions.rs src-tauri/src/commands/mod.rs
git commit -m "feat(tauri): add ACP agent session commands"
```

---

## Task 3: Wire `AcpSessionService` into `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AcpAgentClient` (Plan 03), `AcpSessionService` (Plan 04), the existing `vault_connection_secret_store`/`vault_fetcher` `Arc`s and `data_dir` already in scope (`src-tauri/src/lib.rs`, same closure as `agent_config_svc`'s construction).
- Produces: `acp_session_svc` registered as managed state, the three new commands registered in `tauri::generate_handler!`.

- [ ] **Step 1: Construct the dedicated service instances**

In `src-tauri/src/lib.rs`, immediately after the existing `agent_config_svc` construction, add:

```rust
// A dedicated SecretManagerService + AgentConfigService pair for
// AcpSessionService's own use, mirroring the pattern already used for
// agent_config_svc above — sharing the same
// vault_connection_secret_store/vault_fetcher Arcs, per this plan's Global
// Constraints.
let acp_agent_config_secret_manager = Arc::new(rocket_app::SecretManagerService::new(
    Box::new(rocket_infra::FsSecretManagerRepo::new(
        data_dir.join("secret_managers.yml"),
    )),
    Arc::clone(&vault_connection_secret_store),
    Arc::clone(&vault_fetcher),
));
let acp_agent_config_svc = Arc::new(rocket_app::AgentConfigService::new(
    Box::new(rocket_infra::FsAgentConfigRepo::new(
        data_dir.join("agent_configs.yml"),
    )),
    acp_agent_config_secret_manager,
));

let acp_session_svc = rocket_app::AcpSessionService::new(
    Box::new(rocket_infra::AcpAgentClient::new()),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
    acp_agent_config_svc,
);
```

- [ ] **Step 2: Register as managed state**

Add, alongside the existing `app.manage(agent_config_svc);`:

```rust
app.manage(acp_session_svc);
```

- [ ] **Step 3: Register the commands**

In the `tauri::generate_handler!` list, add alongside the existing `commands::agent_configs::test_agent_config,` entry:

```rust
commands::acp_sessions::start_agent_session,
commands::acp_sessions::send_agent_prompt,
commands::acp_sessions::end_agent_session,
```

- [ ] **Step 4: Verify the full app builds and tests pass**

Run: `cargo check -p rocket -j4`
Expected: succeeds — confirms the new service wiring and command registration compile against the real `AcpSessionService`/`AcpAgentClient` types, and that no existing command or service construction was disturbed.

Run: `cargo test -p rocket -j4`
Expected: PASS — every existing test in this crate unaffected (no new tests were added in this plan's Tasks 1/2 per its Global Constraints).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire AcpSessionService into app state"
```

---

## Next Plan

None — this is the last plan in subproject B. The overall ACP AI-assist feature's next subprojects are **C (AI Assist Chat Panel, frontend)** and **D (MCP Tool Server + Rocket Tools)**, which can proceed in parallel per the project memory `project_acp_ai_assist_feature.md`. Both need their own brainstorming/spec/plan cycle; do not start implementing either from this plan file alone.

## Post-Implementation Review

Before considering subproject B complete, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `src-tauri/src/tauri_event_bus.rs`, `src-tauri/src/commands/acp_sessions.rs`,
> `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do the three commands and
>    the four new event-bus match arms match exactly what the plan index's
>    locked interface contract specifies?
> 2. Code quality versus this plan's Review Focus section — the dedicated
>    `SecretManagerService`/`AgentConfigService` pair shares the existing
>    `vault_connection_secret_store`/`vault_fetcher` Arcs rather than
>    constructing fresh ones, both the module registration and handler
>    registration for the new commands are present, and `AcpAgentClient` is
>    constructed exactly once.
> 3. DDD/IPC boundary conformance per `.claude/rules/tauri-ipc-boundaries.md`
>    — commands stay thin (no domain logic), and this is also a natural
>    point to sanity-check the whole subproject B series end-to-end: re-read
>    the spec's Credential Handling & Security section and confirm nothing
>    across Plans 03-05 ever logs or error-messages a raw credential value or
>    full `env` list, now that all the pieces are wired together.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket -j4` and `cargo check -p rocket -j4`,
> and confirm they still pass. Report what you found and fixed.

Once this review comes back clean (or its fixes are applied and re-verified), subproject B is complete. Update the project memory `project_acp_ai_assist_feature.md` to mark subproject B's status as done before starting subproject C or D's brainstorming.
