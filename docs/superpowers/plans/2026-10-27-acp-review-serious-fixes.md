# ACP Review: Serious Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the two confirmed serious findings of the 2026-10-27 architecture review of the ACP implementation: unmasked tool and permission text, and turn state that survives a dropped `send_prompt`.

**Architecture:** Task 1 masks tool call and permission text in `AcpSessionService::publish_update` with the same per-turn `forms` set that chunks and errors already use. Task 2 adds a drop guard in `AcpAgentClient::send_prompt` so `end_turn` runs on every exit path, including a dropped future.

**Tech Stack:** Rust, tokio, `agent-client-protocol`, `async-process`.

**Spec:** The review report of this session (findings 1 and 2, no spec file). Context: `crates/rocket-app/CLAUDE.md` (`AcpSessionService` row), `crates/rocket-infra/CLAUDE.md`.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- No panicking unwrap calls in production paths. Comments are short, full sentences that end with a punctuation mark.
- Do not rename `DomainEvent` keys (wire format is pinned by contract tests in `rocket-shared`).
- Commit through the `dev-workflow-skills:1-git-commit` skill, with explicit paths only (no `git add -A`).
- `rocket-app` stays free of I/O. The guard in Task 2 lives in `rocket-infra`.

## Finding 3 (pid reuse in `terminate()`): dropped from this plan

Not a defect as far as the code shows. `async-process` 2.5.0 reaps a child only when the `Child` is dropped (`reaper::ChildGuard` pushes it to the zombie list in `Drop`), or when `status`/`try_status`/`wait` is polled. `acp_agent_client.rs` never polls those, and `AgentProcess::drop` calls `terminate()` before the `Child` is dropped. So the leader pid stays reserved while `terminate()` signals its group. Adding a `try_status()` guard would make things worse: it would skip the group kill for a dead `npx` wrapper whose real agent is still alive in the group. Leave the code; no task here changes it.

## Review Focus

- A tool call title, a `ToolCallUpdate` title, a permission title and a permission summary that contain a workspace secret value: all four must reach events masked.
- A tool call title that is masked, then updated with no new title: the merged title must stay masked (store the masked form).
- Workspace with no secrets (`forms` empty): titles pass through unchanged and `cap_title` still applies.
- `send_prompt` dropped while a permission card is open: the card's waiter gets `None` and the slot is empty.
- `send_prompt` that finishes normally: `end_turn` runs before `fail_and_remove`, as today.

---

### Task 1: Mask tool and permission text

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs` (`send_prompt` call at ~366, `publish_update` at ~490, helpers near `mask_error` at ~709)
- Test: same file, `#[cfg(test)]` module (uses `masked_service`, `FakeSessionClient`, `FakeEventPublisher`)

**Interfaces:**
- Consumes: `redact_secrets(text: &str, secret_values: &HashSet<String>) -> String` (`redaction.rs`), `cap_title(String) -> String`.
- Produces: `fn mask_text(text: String, forms: &HashSet<String>) -> String` (private, returns the input untouched when `forms` is empty) and `publish_update(&self, session_id: &str, update: AcpUpdate, tool_calls: &mut ToolCallStates, forms: &HashSet<String>)`.

- [ ] **Step 1: Write the failing tests**

Add four tests next to `chunks_are_flushed_before_a_tool_call_and_keep_their_order`, each built with `masked_service(client, &publisher, &["s3cr3t-value"])`:
  - `tool_call_title_is_masked`: an `AcpUpdate::ToolCall` titled `curl -H 'Bearer s3cr3t-value'`. Assert the published `AcpToolActivity.title` has no `s3cr3t-value`.
  - `tool_call_update_title_is_masked_and_stays_masked`: a `ToolCall` with a clean title, then a `ToolCallUpdate` with title `curl s3cr3t-value`, then a second `ToolCallUpdate` with `title: None`. Assert no event contains the secret, including the third one (merged title).
  - `permission_title_and_summary_are_masked`: an `AcpUpdate::PermissionRequest` with the secret in `title` and `summary`. Assert neither field of `AcpPermissionRequested` contains it.
  - `titles_pass_through_without_secrets`: `service_with(...)` (no secrets), title `Read GET /orders`. Assert it is unchanged.

- [ ] **Step 2: Run to verify the three masking tests fail**

Run: `cargo test -j4 -p rocket-app acp_session_service::tests::tool_call_title_is_masked` (and the other three names)
Expected: the three masking tests FAIL with the secret present in the event, the pass-through test passes.

- [ ] **Step 3: Implement**

Add `mask_text` beside `mask_error`, using `redact_secrets`. Change `publish_update` to take `forms: &HashSet<String>` and pass `&forms` from `send_prompt`. Mask `title` in `ToolCall` before inserting it into `tool_calls` and before the event, mask the new `title` in `ToolCallUpdate` before storing it, and mask `title` (then `cap_title`) and `summary` in `PermissionRequest`. Mask before capping so a cut cannot split a secret into an unmaskable half. Update the doc comment of `publish_update` to say titles and summaries are masked.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-app acp_session_service`
Expected: PASS, including the existing `title.chars().count() == 201` cap test.

- [ ] **Step 5: Commit**

Use `dev-workflow-skills:1-git-commit`. Path: `crates/rocket-app/src/acp_session_service.rs`. Suggested type: `fix(acp):`.

---

### Task 2: End the turn on every exit path

**Files:**
- Modify: `crates/rocket-infra/src/acp_agent_client.rs` (`end_turn` at ~107, `send_prompt` at ~537)
- Test: same file, test module next to the existing `end_turn` permission test (~1425)

**Interfaces:**
- Consumes: `UpdateSlot`, `PermissionBroker`, `end_turn(slot: &UpdateSlot, permissions: &PermissionBroker)`, `set_update_sender`.
- Produces: `struct TurnGuard { slot: UpdateSlot, permissions: Arc<PermissionBroker> }` with `fn new(slot: UpdateSlot, permissions: Arc<PermissionBroker>) -> Self` and a `Drop` impl that calls `end_turn`. `end_turn` stays idempotent, so an explicit `drop(guard)` and the `Drop` never conflict.

- [ ] **Step 1: Write the failing tests**

  - `dropping_the_turn_guard_clears_the_slot_and_cancels_open_permissions`: build a slot and a broker as the existing `end_turn` test does, install a sender, open one permission request, drop the guard. Assert the waiter resolves to `None`, `current_update_sender(&slot)` is `None`, and `broker.resolve(...)` is `NotFound`.
  - `turn_guard_is_a_no_op_after_end_turn`: call `end_turn` then drop the guard. Assert no panic and the slot stays empty.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra turn_guard`
Expected: FAIL to compile (`TurnGuard` not defined).

- [ ] **Step 3: Implement**

Add `TurnGuard` below `end_turn`. In `send_prompt`, create the guard right after `set_update_sender(...)` (clone the slot `Arc` and the permissions `Arc` from `running`), and replace the explicit `end_turn(...)` after the request with `drop(guard)` at the same place, so the slot is cleared before `fail_and_remove` runs as today. A dropped future now runs the guard's `Drop`. Update the comment above it to say so.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra acp_agent_client` and `cargo check -j4`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

Use `dev-workflow-skills:1-git-commit`. Path: `crates/rocket-infra/src/acp_agent_client.rs`. Suggested type: `fix(acp):`.

---

## Self-Review

- Coverage: finding 1 is Task 1, finding 2 is Task 2, finding 3 is dropped with a reason. Finding 6 (`PermissionResolved` after `end_turn`) is a separate lifecycle decision and is not in this plan.
- Types: `publish_update` takes `&HashSet<String>`, the same type as `forms` in `send_prompt`. `TurnGuard` holds `UpdateSlot` and `Arc<PermissionBroker>`, matching `RunningSession.permissions`.
- Docs: add one sentence to the `AcpSessionService` row of `crates/rocket-app/CLAUDE.md` ("tool and permission titles and summaries are masked too") when Task 1 lands.
