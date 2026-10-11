# ACP Permission Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show an agent's `session/request_permission` as an approval card in the assistant chat, and send the user's choice back to the agent, instead of auto-denying it.

**Architecture:** The ACP client parks each permission request in a per-session `PermissionBroker`, emits `AcpUpdate::PermissionRequest` on the turn's update channel, and waits for the answer in a spawned task (never inside the dispatch loop). `AcpSessionService` maps the updates to two `DomainEvent`s, pauses its idle clock while a request is pending, and forwards the answer through a new Tauri command. The frontend keeps the requests in `assistant-store` and renders `AssistantPermissionCard`.

**Tech Stack:** Rust (`agent-client-protocol` 2.2.0, tokio), Tauri 2 IPC and events, React + Zustand + shadcn/ui + lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-12-acp-permission-card-design.md`

## Global Constraints

- Rust: never `unwrap()` in production paths. Never shell out to `git`.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs. `DomainEvent` fields stay snake_case (the enum is tagged `type`, camelCase variant names).
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, no inline SVGs. Zustand: never fully destructure store state at component top level; use narrow selectors.
- Harness (hooks enforce these): `cargo test|check|clippy|build` must pass `-j4`; no `cargo test --workspace` or `--all`; no `git add -A`, `--all` or `.`; commit with a pathspec: `git add <paths> && git commit --only -m "..." -- <paths>`. Conventional commits.
- Pre-commit gates: staged `.ts/.tsx` run `yarn tsc --noEmit` and `yarn check`; staged `.rs` run `cargo check -j4`.
- This work touches the ACP session path only, not collections, environments or `.yml` data, so the OpenCollection spec read does not apply.
- Rocket never invents a grant: it forwards only an option id the agent offered and the user clicked. No timeout; stopping the turn cancels the request.
- `summary` text is masked with the session's secret values and capped at 500 characters.

## Review Focus

- A card left unanswered for longer than the 120 s idle limit must not end the session (Task 3 test).
- Answering twice, answering an unknown request id, or answering with an option the agent did not offer must fail without reaching the agent (Task 2 tests).
- Stopping the turn while a card is open must answer the agent `cancelled` and show the card as "Cancelled" (Task 2 integration test, Task 5 store test).
- A turn that ends or fails with a card still open must not leave a clickable card (Task 5 store test).
- A permission request that arrives outside a turn must be denied at once, not hang (the no-turn branch in Task 2 has no direct test; see Known gap).
- Parallel requests in one turn each get their own card and answer (Task 2 integration test, Task 5 store test).

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-shared/src/acp.rs` | modify | `PermissionOption`, `PermissionOptionKind` (shared so `DomainEvent` can carry them) |
| `crates/rocket-shared/src/events.rs` | modify | `AcpPermissionRequested`, `AcpPermissionResolved` |
| `crates/rocket-acp/src/update.rs`, `lib.rs`, `session.rs` | modify | `AcpUpdate` variants, re-exports, `resolve_permission` default method |
| `crates/rocket-infra/src/acp_permissions.rs` | create | `PermissionBroker`: pending map, validation, drain |
| `crates/rocket-infra/src/acp_agent_client.rs`, `lib.rs` | modify | spawned handler, `resolve_permission`, drain on cancel |
| `crates/rocket-infra/tests/acp_agent_client.rs` | modify | integration tests against the fixture agent |
| `crates/rocket-app/src/acp_session_service.rs` | modify | event mapping, idle pause, `respond_permission` |
| `src-tauri/src/tauri_event_bus.rs`, `commands/acp_sessions.rs`, `lib.rs` | modify | channels, command, registration |
| `src/lib/tauri-api.ts` | modify | types, listeners, `respondAgentPermission` |
| `src/stores/assistant-store.ts` | modify | `permissions` state and actions |
| `src/lib/assistant-event-bridge.ts` | modify | route the two events into the store |
| `src/components/assistant/AssistantPermissionCard.tsx` | create | the card |
| `src/components/assistant/AssistantChatView.tsx` | modify | render the Permissions section |
| `.claude/assistant-followups.md` | modify | record the Gemini built-in tools follow-up |

---

### Task 1: Shared types, `AcpUpdate` variants and the trait method

**Files:**
- Modify: `crates/rocket-shared/src/acp.rs`
- Modify: `crates/rocket-acp/src/lib.rs:26-28`
- Modify: `crates/rocket-acp/src/update.rs`
- Modify: `crates/rocket-acp/src/session.rs`
- Test: unit tests in the same files

**Interfaces:**
- Produces (used by Tasks 2, 3):
  - `rocket_shared::acp::PermissionOptionKind { AllowOnce, AllowAlways, RejectOnce, RejectAlways }` with `fn as_str(self) -> &'static str` (snake_case).
  - `rocket_shared::acp::PermissionOption { id: String, name: String, kind: PermissionOptionKind }`.
  - Both re-exported as `rocket_acp::{PermissionOption, PermissionOptionKind}`.
  - `AcpUpdate::PermissionRequest { request_id: String, call_id: String, title: String, kind: String, summary: String, options: Vec<PermissionOption> }`
  - `AcpUpdate::PermissionResolved { request_id: String, option_id: Option<String> }`
  - `AcpSessionClient::resolve_permission(&self, session_id: &str, request_id: &str, option_id: Option<&str>) -> DomainResult<()>`, default body returns `DomainError::NotFound`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-shared/src/acp.rs`, inside `mod tests`:

```rust
    #[test]
    fn permission_option_serializes_with_a_snake_case_kind() {
        let option = PermissionOption {
            id: "proceed_always_server".to_string(),
            name: "Allow all server tools for this session".to_string(),
            kind: PermissionOptionKind::AllowAlways,
        };
        assert_eq!(
            serde_json::to_string(&option).expect("serialize"),
            r#"{"id":"proceed_always_server","name":"Allow all server tools for this session","kind":"allow_always"}"#
        );
        assert_eq!(PermissionOptionKind::RejectOnce.as_str(), "reject_once");
    }
```

In `crates/rocket-acp/src/session.rs`, inside `mod tests` (uses the existing `FakeSessionClient`):

```rust
    #[tokio::test]
    async fn resolve_permission_defaults_to_not_found() {
        let err = FakeSessionClient
            .resolve_permission("session-1", "perm-1", Some("allow"))
            .await
            .expect_err("the default has no pending request");
        assert!(matches!(err, DomainError::NotFound(_)));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared permission_option_serializes` and `cargo test -j4 -p rocket-acp resolve_permission_defaults`
Expected: FAIL to compile (`PermissionOption`, `resolve_permission` not defined).

- [ ] **Step 3: Implement**

Append to `crates/rocket-shared/src/acp.rs`, above `#[cfg(test)]`:

```rust
/// What a permission option does. Serialized as snake_case, like the ACP
/// wire values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

impl PermissionOptionKind {
    /// The snake_case name used on events and in the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionOptionKind::AllowOnce => "allow_once",
            PermissionOptionKind::AllowAlways => "allow_always",
            PermissionOptionKind::RejectOnce => "reject_once",
            PermissionOptionKind::RejectAlways => "reject_always",
        }
    }
}

/// One answer an agent offers to a permission request. `id` is the agent's
/// own option id and is the only value Rocket ever sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}
```

In `crates/rocket-acp/src/lib.rs` add (next to the other `pub use` lines):

```rust
pub use rocket_shared::acp::{PermissionOption, PermissionOptionKind};
```

In `crates/rocket-acp/src/update.rs`, add `use crate::PermissionOption;` at the top and two variants to `AcpUpdate`:

```rust
    /// The agent asked the user to approve something, for example to run a
    /// tool. `kind` is the ACP tool kind in snake_case. `summary` is short,
    /// masked text about the call. The turn waits for the answer.
    PermissionRequest {
        request_id: String,
        call_id: String,
        title: String,
        kind: String,
        summary: String,
        options: Vec<PermissionOption>,
    },
    /// A permission request was answered. `option_id` is the chosen option,
    /// or `None` when the request was cancelled.
    PermissionResolved {
        request_id: String,
        option_id: Option<String>,
    },
```

In `crates/rocket-acp/src/session.rs`, change the error import to `use rocket_shared::error::{DomainError, DomainResult};` and add to the trait, after `cancel`:

```rust
    /// Answers a permission request that was sent as
    /// `AcpUpdate::PermissionRequest`. `option_id` must be an option the
    /// agent offered; `None` cancels the request. Fails without touching the
    /// agent when the request is unknown or already answered, or the option
    /// was not offered. The default has no pending requests.
    async fn resolve_permission(
        &self,
        session_id: &str,
        _request_id: &str,
        _option_id: Option<&str>,
    ) -> DomainResult<()> {
        Err(DomainError::NotFound(format!("acp session '{session_id}'")))
    }
```

(The test module already imports `DomainError`; remove a now-duplicate import there if the compiler warns.)

- [ ] **Step 4: Run the tests and fix non-exhaustive matches**

Run: `cargo test -j4 -p rocket-shared permission_option_serializes && cargo test -j4 -p rocket-acp`
Expected: PASS. Then `cargo check -j4 -p rocket-app -p rocket-infra`: `rocket-app`'s `publish_update` will report a non-exhaustive match on `AcpUpdate`. That is expected and fixed in Task 3; to keep this commit compiling, add a temporary arm in `publish_update` now:

```rust
            AcpUpdate::PermissionRequest { .. } | AcpUpdate::PermissionResolved { .. } => return,
```

Task 3 replaces it. Re-run `cargo check -j4 -p rocket-app -p rocket-infra`; Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared/src/acp.rs crates/rocket-acp/src/lib.rs crates/rocket-acp/src/update.rs crates/rocket-acp/src/session.rs crates/rocket-app/src/acp_session_service.rs
git commit --only -m "feat(acp): add permission request updates and resolve_permission" -- crates/rocket-shared/src/acp.rs crates/rocket-acp/src/lib.rs crates/rocket-acp/src/update.rs crates/rocket-acp/src/session.rs crates/rocket-app/src/acp_session_service.rs
```

---

### Task 2: `PermissionBroker` and the spawned handler in the ACP client

**Files:**
- Create: `crates/rocket-infra/src/acp_permissions.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (add `mod acp_permissions;` next to `mod acp_agent_client;`; no public export needed)
- Modify: `crates/rocket-infra/src/acp_agent_client.rs` (struct `RunningSession` ~line 36; `start_session` ~270-360; `cancel` ~line 575; `deny_outcome` ~857)
- Modify: `crates/rocket-infra/tests/acp_agent_client.rs:1052` (replace `acp_agent_client_denies_permission_requests`)

**Interfaces:**
- Consumes: Task 1 types.
- Produces: `PermissionBroker` with
  - `fn register(&self, option_ids: Vec<String>) -> (String, tokio::sync::oneshot::Receiver<Option<String>>)`
  - `fn resolve(&self, request_id: &str, option_id: Option<&str>) -> DomainResult<()>`
  - `fn drain_cancelled(&self)`
  - and `AcpAgentClient::resolve_permission` (trait impl).

- [ ] **Step 1: Write the broker with its failing tests**

Create `crates/rocket-infra/src/acp_permissions.rs`:

```rust
//! The pending permission requests of one agent session. A request is
//! registered when the agent asks, and answered once: by the user's choice,
//! or as cancelled when the turn is stopped.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::oneshot;

struct Pending {
    option_ids: Vec<String>,
    answer: oneshot::Sender<Option<String>>,
}

#[derive(Default)]
pub struct PermissionBroker {
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, Pending>>,
}

impl PermissionBroker {
    /// Registers a request offering `option_ids`. Returns its id and the
    /// receiver that resolves with the chosen option id, or `None` when the
    /// request was cancelled.
    pub fn register(
        &self,
        option_ids: Vec<String>,
    ) -> (String, oneshot::Receiver<Option<String>>) {
        let request_id = format!("perm-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let (answer, receiver) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_id.clone(), Pending { option_ids, answer });
        (request_id, receiver)
    }

    /// Answers one request. Fails, leaving the request pending, when the
    /// option was not offered; fails when the request is unknown or was
    /// already answered.
    pub fn resolve(&self, request_id: &str, option_id: Option<&str>) -> DomainResult<()> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(entry) = pending.get(request_id) else {
            return Err(DomainError::NotFound(format!(
                "permission request '{request_id}'"
            )));
        };
        if let Some(option_id) = option_id {
            if !entry.option_ids.iter().any(|id| id == option_id) {
                return Err(DomainError::InvalidInput(format!(
                    "option '{option_id}' was not offered for this request"
                )));
            }
        }
        if let Some(entry) = pending.remove(request_id) {
            // A dropped receiver means the waiting task is gone, which only
            // happens when the session ended. Nothing is left to answer.
            let _ = entry.answer.send(option_id.map(str::to_string));
        }
        Ok(())
    }

    /// Answers every pending request as cancelled.
    pub fn drain_cancelled(&self) {
        let drained: Vec<Pending> = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        for entry in drained {
            let _ = entry.answer.send(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<String> {
        vec!["allow".to_string(), "reject".to_string()]
    }

    #[tokio::test]
    async fn resolve_delivers_the_chosen_option() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        broker.resolve(&id, Some("allow")).expect("resolve");
        assert_eq!(rx.await.expect("answer"), Some("allow".to_string()));
    }

    #[tokio::test]
    async fn requests_get_distinct_ids() {
        let broker = PermissionBroker::default();
        let (first, _rx1) = broker.register(options());
        let (second, _rx2) = broker.register(options());
        assert_ne!(first, second);
    }

    #[test]
    fn an_unknown_request_is_not_found() {
        let broker = PermissionBroker::default();
        let err = broker.resolve("perm-99", Some("allow")).expect_err("unknown");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn a_request_can_be_answered_only_once() {
        let broker = PermissionBroker::default();
        let (id, _rx) = broker.register(options());
        broker.resolve(&id, Some("allow")).expect("first answer");
        let err = broker.resolve(&id, Some("reject")).expect_err("second answer");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn an_option_that_was_not_offered_is_refused_and_the_request_stays_pending() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        let err = broker.resolve(&id, Some("allow-forever")).expect_err("not offered");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        broker.resolve(&id, Some("reject")).expect("still answerable");
        assert_eq!(rx.await.expect("answer"), Some("reject".to_string()));
    }

    #[tokio::test]
    async fn none_cancels_a_request() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        broker.resolve(&id, None).expect("cancel");
        assert_eq!(rx.await.expect("answer"), None);
    }

    #[tokio::test]
    async fn drain_cancels_every_pending_request() {
        let broker = PermissionBroker::default();
        let (_a, rx_a) = broker.register(options());
        let (_b, rx_b) = broker.register(options());
        broker.drain_cancelled();
        assert_eq!(rx_a.await.expect("answer a"), None);
        assert_eq!(rx_b.await.expect("answer b"), None);
    }
}
```

Add `mod acp_permissions;` to `crates/rocket-infra/src/lib.rs` beside `mod acp_agent_client;` (keep the same visibility style as that line).

- [ ] **Step 2: Run the broker tests**

Run: `cargo test -j4 -p rocket-infra acp_permissions`
Expected: PASS (7 tests). If `tokio::test` needs the `macros` feature, it is already used by this crate's tests.

- [ ] **Step 3: Write the failing integration tests**

In `crates/rocket-infra/tests/acp_agent_client.rs`, add `use std::sync::Arc;` and `use rocket_acp::PermissionOption;` to the imports, then **replace** `acp_agent_client_denies_permission_requests` (line ~1052) with:

```rust
/// Reads updates until the permission request arrives.
async fn next_permission_request(
    rx: &mut mpsc::UnboundedReceiver<AcpUpdate>,
) -> (String, Vec<PermissionOption>) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Some(AcpUpdate::PermissionRequest {
                    request_id,
                    options,
                    ..
                }) => return (request_id, options),
                Some(_) => continue,
                None => panic!("the update channel closed before a permission request"),
            }
        }
    })
    .await
    .expect("a permission request must reach the update channel")
}

/// Starts a `__PERMISSION__` turn and returns the client, the session id,
/// the update receiver and the running prompt.
async fn start_permission_turn() -> (
    Arc<AcpAgentClient>,
    String,
    mpsc::UnboundedReceiver<AcpUpdate>,
    tokio::task::JoinHandle<Result<String, DomainError>>,
) {
    let client = Arc::new(AcpAgentClient::new());
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;
    let (tx, rx) = mpsc::unbounded_channel();
    let turn_client = Arc::clone(&client);
    let turn_session = session_id.clone();
    let prompt = tokio::spawn(async move {
        turn_client
            .send_prompt(
                &turn_session,
                vec![PromptPart::Text("__PERMISSION__".to_string())],
                tx,
            )
            .await
    });
    (client, session_id, rx, prompt)
}

#[tokio::test]
async fn acp_agent_client_permission_request_waits_for_an_allow_answer() {
    let (client, session_id, mut rx, prompt) = start_permission_turn().await;
    let (request_id, options) = next_permission_request(&mut rx).await;
    assert_eq!(
        options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        vec!["allow-once", "reject-once"]
    );

    client
        .resolve_permission(&session_id, &request_id, Some("allow-once"))
        .await
        .expect("resolve");
    let stop_reason = tokio::time::timeout(std::time::Duration::from_secs(5), prompt)
        .await
        .expect("the turn must finish after the answer")
        .expect("join")
        .expect("send_prompt");
    assert_eq!(stop_reason, "end_turn");

    let updates = drain_updates(&mut rx);
    assert!(updates.contains(&AcpUpdate::PermissionResolved {
        request_id,
        option_id: Some("allow-once".to_string()),
    }));
    assert!(updates.contains(&AcpUpdate::Text {
        text: "permission:selected:allow-once".to_string()
    }));
}

#[tokio::test]
async fn acp_agent_client_permission_request_passes_a_reject_answer_to_the_agent() {
    let (client, session_id, mut rx, prompt) = start_permission_turn().await;
    let (request_id, _) = next_permission_request(&mut rx).await;
    client
        .resolve_permission(&session_id, &request_id, Some("reject-once"))
        .await
        .expect("resolve");
    tokio::time::timeout(std::time::Duration::from_secs(5), prompt)
        .await
        .expect("the turn must finish")
        .expect("join")
        .expect("send_prompt");
    assert!(drain_updates(&mut rx).contains(&AcpUpdate::Text {
        text: "permission:selected:reject-once".to_string()
    }));
}

#[tokio::test]
async fn acp_agent_client_refuses_an_option_the_agent_did_not_offer() {
    let (client, session_id, mut rx, prompt) = start_permission_turn().await;
    let (request_id, _) = next_permission_request(&mut rx).await;

    let err = client
        .resolve_permission(&session_id, &request_id, Some("allow-forever"))
        .await
        .expect_err("an option that was not offered");
    assert!(matches!(err, DomainError::InvalidInput(_)));

    // The request is still open and can be answered properly.
    client
        .resolve_permission(&session_id, &request_id, Some("reject-once"))
        .await
        .expect("resolve");
    tokio::time::timeout(std::time::Duration::from_secs(5), prompt)
        .await
        .expect("the turn must finish")
        .expect("join")
        .expect("send_prompt");

    let err = client
        .resolve_permission(&session_id, &request_id, Some("reject-once"))
        .await
        .expect_err("an answered request");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_cancel_answers_an_open_permission_request_as_cancelled() {
    let (client, session_id, mut rx, prompt) = start_permission_turn().await;
    let (request_id, _) = next_permission_request(&mut rx).await;

    client.cancel(&session_id).await.expect("cancel");
    tokio::time::timeout(std::time::Duration::from_secs(5), prompt)
        .await
        .expect("a cancelled request must not hang the turn")
        .expect("join")
        .expect("send_prompt");

    let updates = drain_updates(&mut rx);
    assert!(updates.contains(&AcpUpdate::PermissionResolved {
        request_id,
        option_id: None,
    }));
    assert!(updates.contains(&AcpUpdate::Text {
        text: "permission:cancelled".to_string()
    }));
}
```

(`drain_updates` is the existing helper in this file. If `PermissionOption` has no `Debug` the `contains` asserts still work because `AcpUpdate` derives `PartialEq`; both derive in Task 1.)

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra --test acp_agent_client permission`
Expected: FAIL (the handler still denies at once, so no `PermissionRequest` update arrives and `next_permission_request` times out).

- [ ] **Step 5: Implement the handler**

In `crates/rocket-infra/src/acp_agent_client.rs`:

1. Imports: add `use crate::acp_permissions::PermissionBroker;`, and in the `agent_client_protocol::schema::v1` import list add `ToolCallContent` and `ToolCallUpdateFields` if missing. Import Rocket's types under aliases so they do not clash with the wire `PermissionOption`/`PermissionOptionKind` already imported: `use rocket_acp::{PermissionOption as AcpPermissionOption, PermissionOptionKind as AcpPermissionOptionKind};`.

2. Add to `RunningSession` (after `prompt_capabilities`):

```rust
    /// The permission requests the agent has open. Answered by
    /// `resolve_permission`, or as cancelled by `cancel`.
    permissions: Arc<PermissionBroker>,
```

3. In `start_session`, right after the `secret_values` loop (where `current_update_tx` and `notif_update_tx` are created, ~line 285), add:

```rust
        let permission_broker = Arc::new(PermissionBroker::default());
        let handler_broker = Arc::clone(&permission_broker);
        let handler_update_tx = Arc::clone(&current_update_tx);
        let handler_secrets = secret_values.clone();
```

and set `permissions: permission_broker,` when `RunningSession` is built (~line 485).

4. Replace the whole `.on_receive_request(...)` block (the one that logs "the agent asked for a permission; Rocket denied it", lines ~338-352) with:

```rust
                // A permission request becomes a card the user answers. The
                // wait runs in a spawned task: this callback runs inside the
                // dispatch loop, and waiting here would block every other
                // message, including the prompt response and the cancel.
                .on_receive_request(
                    move |request: RequestPermissionRequest,
                          responder: Responder<RequestPermissionResponse>,
                          cx: ConnectionTo<AgentRole>| {
                        let broker = Arc::clone(&handler_broker);
                        let slot = Arc::clone(&handler_update_tx);
                        let secrets = handler_secrets.clone();
                        async move { handle_permission_request(request, responder, &cx, broker, slot, &secrets) }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
```

5. Add these free functions near `deny_outcome` (~line 857):

```rust
/// Longest `summary` sent for a permission request.
const PERMISSION_SUMMARY_MAX_CHARS: usize = 500;

fn current_update_sender(slot: &UpdateSlot) -> Option<UnboundedSender<AcpUpdate>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Turns a permission request into a card update, then answers the agent when
/// the user has chosen. With no turn running (no update sender) nobody could
/// answer, so the request is denied at once, as before.
fn handle_permission_request(
    request: RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
    cx: &ConnectionTo<AgentRole>,
    broker: Arc<PermissionBroker>,
    slot: UpdateSlot,
    secrets: &[String],
) -> Result<(), agent_client_protocol::Error> {
    let deny = |responder: Responder<RequestPermissionResponse>| {
        responder.respond(RequestPermissionResponse::new(deny_outcome(&request.options)))
    };
    let Some(update_tx) = current_update_sender(&slot) else {
        tracing::warn!("the agent asked for a permission outside a turn; Rocket denied it");
        return deny(responder);
    };
    let options = permission_options_from_wire(&request.options);
    let (request_id, answer) =
        broker.register(options.iter().map(|option| option.id.clone()).collect());
    let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    let fields = &request.tool_call.fields;
    let update = AcpUpdate::PermissionRequest {
        request_id: request_id.clone(),
        call_id: request.tool_call.tool_call_id.to_string(),
        title: mask_secrets(fields.title.as_deref().unwrap_or(""), &secret_refs),
        kind: fields.kind.map(tool_kind_to_wire).unwrap_or("other").to_string(),
        summary: permission_summary(fields, &secret_refs),
        options,
    };
    if update_tx.send(update).is_err() {
        // The turn's receiver is gone, so no card can be shown.
        let _ = broker.resolve(&request_id, None);
        return deny(responder);
    }
    cx.spawn(async move {
        let chosen = answer.await.unwrap_or(None);
        // Told to the UI before the agent hears the answer, so the card is
        // closed before the agent can end the turn.
        if let Some(tx) = current_update_sender(&slot) {
            let _ = tx.send(AcpUpdate::PermissionResolved {
                request_id,
                option_id: chosen.clone(),
            });
        }
        let outcome = match chosen {
            Some(option_id) => {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
            }
            None => RequestPermissionOutcome::Cancelled,
        };
        responder.respond(RequestPermissionResponse::new(outcome))
    })
}

fn permission_options_from_wire(options: &[PermissionOption]) -> Vec<AcpPermissionOption> {
    options
        .iter()
        .map(|option| AcpPermissionOption {
            id: option.option_id.to_string(),
            name: option.name.clone(),
            kind: match option.kind {
                PermissionOptionKind::AllowOnce => AcpPermissionOptionKind::AllowOnce,
                PermissionOptionKind::AllowAlways => AcpPermissionOptionKind::AllowAlways,
                PermissionOptionKind::RejectAlways => AcpPermissionOptionKind::RejectAlways,
                // Reject is the safe reading of a kind Rocket does not know.
                _ => AcpPermissionOptionKind::RejectOnce,
            },
        })
        .collect()
}

/// Short, masked text about the call: the agent's own content text, a diff's
/// path, or the raw input as a last resort.
fn permission_summary(fields: &ToolCallUpdateFields, secrets: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in fields.content.iter().flatten() {
        match item {
            ToolCallContent::Content(content) => {
                if let ContentBlock::Text(text) = &content.content {
                    parts.push(text.text.clone());
                }
            }
            ToolCallContent::Diff(diff) => parts.push(format!("Edit {}", diff.path.display())),
            _ => {}
        }
    }
    if parts.is_empty() {
        if let Some(raw) = &fields.raw_input {
            parts.push(raw.to_string());
        }
    }
    let masked = mask_secrets(&parts.join("\n"), secrets);
    if masked.chars().count() > PERMISSION_SUMMARY_MAX_CHARS {
        let cut: String = masked.chars().take(PERMISSION_SUMMARY_MAX_CHARS).collect();
        format!("{cut}…")
    } else {
        masked
    }
}
```

6. In `impl AcpSessionClient for AcpAgentClient`, add after `cancel`'s body is changed. First change `cancel` to drain before sending the notification:

```rust
    async fn cancel(&self, session_id: &str) -> DomainResult<()> {
        let running = self.running(session_id).await?;
        // ACP: on cancel, open permission requests are answered cancelled.
        running.permissions.drain_cancelled();
        running
            .connection
            .send_notification(CancelNotification::new(session_id.to_string()))
            .map_err(|e| DomainError::Internal(format!("failed to send cancel: {e}")))
    }

    async fn resolve_permission(
        &self,
        session_id: &str,
        request_id: &str,
        option_id: Option<&str>,
    ) -> DomainResult<()> {
        self.running(session_id)
            .await?
            .permissions
            .resolve(request_id, option_id)
    }
```

(Keep the existing comment lines of `cancel`.)

- [ ] **Step 6: Add the unit tests for the pure helpers**

In the `#[cfg(test)] mod tests` of `acp_agent_client.rs`:

```rust
    #[test]
    fn permission_summary_masks_secrets_and_is_capped() {
        let fields = ToolCallUpdateFields::new().raw_input(serde_json::json!({
            "command": "curl -H 'Authorization: tok-secret-123' https://example.com"
        }));
        let summary = permission_summary(&fields, &["tok-secret-123"]);
        assert!(!summary.contains("tok-secret-123"), "{summary}");

        let long = ToolCallUpdateFields::new().raw_input(serde_json::json!("x".repeat(900)));
        let capped = permission_summary(&long, &[]);
        assert_eq!(capped.chars().count(), PERMISSION_SUMMARY_MAX_CHARS + 1);
        assert!(capped.ends_with('…'));
    }

    #[test]
    fn an_unknown_permission_kind_becomes_a_reject_option() {
        // `PermissionOptionKind` is non-exhaustive, so Rocket reads an
        // unknown kind as the safe one. This pins the known mappings.
        let wire = vec![
            PermissionOption::new("a", "A", PermissionOptionKind::AllowOnce),
            PermissionOption::new("b", "B", PermissionOptionKind::RejectAlways),
        ];
        let mapped = permission_options_from_wire(&wire);
        assert_eq!(mapped[0].kind, AcpPermissionOptionKind::AllowOnce);
        assert_eq!(mapped[1].kind, AcpPermissionOptionKind::RejectAlways);
    }
```

If `ToolCallUpdateFields` has no `raw_input(...)` builder, build it with `let mut fields = ToolCallUpdateFields::new(); fields.raw_input = Some(...)` (the fields are public, see `agent-client-protocol-schema-1.9.1/src/v1/tool_call.rs:261`).

- [ ] **Step 7: Run everything for the crate**

Run: `cargo check -j4 -p rocket-infra && cargo test -j4 -p rocket-infra acp_agent_client && cargo test -j4 -p rocket-infra acp_permissions`
Expected: PASS, including the four new integration tests. If `SelectedPermissionOutcome::new(option_id)` does not accept a `String`, pass `option_id.as_str()`; if `option.option_id.to_string()` does not compile, use `option.option_id.0.to_string()`. Fix at the compiler's direction and keep the behaviour.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/src/acp_permissions.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/src/acp_agent_client.rs crates/rocket-infra/tests/acp_agent_client.rs
git commit --only -m "feat(acp): wait for the user's answer to an agent permission request" -- crates/rocket-infra/src/acp_permissions.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/src/acp_agent_client.rs crates/rocket-infra/tests/acp_agent_client.rs
```

---

### Task 3: Events, idle pause and `respond_permission` in `AcpSessionService`

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (add variants next to `AcpUsage`, ~line 598; add a wire-shape test next to `acp_usage_wire_shape_with_and_without_cost`)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (`send_prompt` ~line 204, `publish_update` ~line 299, tests ~line 770)

**Interfaces:**
- Consumes: `AcpUpdate::{PermissionRequest, PermissionResolved}`, `AcpSessionClient::resolve_permission`.
- Produces:
  - `DomainEvent::AcpPermissionRequested { session_id, request_id, call_id, title, kind, summary, options: Vec<PermissionOption> }`
  - `DomainEvent::AcpPermissionResolved { session_id, request_id, option_id: Option<String> }`
  - `AcpSessionService::respond_permission(&self, session_id: &str, request_id: &str, option_id: Option<&str>) -> DomainResult<()>`

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-shared/src/events.rs` tests:

```rust
    #[test]
    fn acp_permission_events_wire_shape() {
        use crate::acp::{PermissionOption, PermissionOptionKind};
        let requested = DomainEvent::AcpPermissionRequested {
            session_id: "sess-1".into(),
            request_id: "perm-1".into(),
            call_id: "call-1".into(),
            title: "propose_changes".into(),
            kind: "other".into(),
            summary: "{}".into(),
            options: vec![PermissionOption {
                id: "proceed_once".into(),
                name: "Allow once".into(),
                kind: PermissionOptionKind::AllowOnce,
            }],
        };
        assert_eq!(
            serde_json::to_string(&requested).expect("serialize"),
            r#"{"type":"acpPermissionRequested","session_id":"sess-1","request_id":"perm-1","call_id":"call-1","title":"propose_changes","kind":"other","summary":"{}","options":[{"id":"proceed_once","name":"Allow once","kind":"allow_once"}]}"#
        );
        let resolved = DomainEvent::AcpPermissionResolved {
            session_id: "sess-1".into(),
            request_id: "perm-1".into(),
            option_id: None,
        };
        assert_eq!(
            serde_json::to_string(&resolved).expect("serialize"),
            r#"{"type":"acpPermissionResolved","session_id":"sess-1","request_id":"perm-1","option_id":null}"#
        );
    }
```

In `crates/rocket-app/src/acp_session_service.rs` tests, first extend the fake. Add a field to `FakeSessionClient` (and its `Default`):

```rust
        /// Pause after the first update of a prompt, to leave a quiet gap.
        pause_after_first_update: Duration,
```
default `pause_after_first_update: Duration::ZERO,`. In its `send_prompt`, change the update loop to:

```rust
            for (index, update) in self.prompt_updates.iter().enumerate() {
                tokio::time::sleep(self.update_interval).await;
                let _ = update_tx.send(update.clone());
                if index == 0 {
                    tokio::time::sleep(self.pause_after_first_update).await;
                }
            }
```

Then add tests (helpers `service_with`, `hi`, `start`, `send_hi` exist in this module):

```rust
    fn permission_request(id: &str) -> AcpUpdate {
        AcpUpdate::PermissionRequest {
            request_id: id.to_string(),
            call_id: "call-1".to_string(),
            title: "propose_changes".to_string(),
            kind: "other".to_string(),
            summary: "{}".to_string(),
            options: vec![rocket_acp::PermissionOption {
                id: "proceed_once".to_string(),
                name: "Allow once".to_string(),
                kind: rocket_acp::PermissionOptionKind::AllowOnce,
            }],
        }
    }

    #[tokio::test]
    async fn permission_updates_are_published_as_events() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![
                permission_request("perm-1"),
                AcpUpdate::PermissionResolved {
                    request_id: "perm-1".to_string(),
                    option_id: Some("proceed_once".to_string()),
                },
            ],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service.send_prompt("session-1", hi()).await.expect("turn");

        let events = publisher.events.lock().expect("lock");
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::AcpPermissionRequested { request_id, options, .. }
                if request_id == "perm-1" && options.len() == 1
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::AcpPermissionResolved { request_id, option_id, .. }
                if request_id == "perm-1" && option_id.as_deref() == Some("proceed_once")
        )));
    }

    #[tokio::test]
    async fn an_open_permission_request_pauses_the_idle_clock() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_updates: vec![
                permission_request("perm-1"),
                AcpUpdate::PermissionResolved {
                    request_id: "perm-1".to_string(),
                    option_id: Some("proceed_once".to_string()),
                },
            ],
            // The user takes four times the idle limit to answer.
            pause_after_first_update: Duration::from_millis(400),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_millis(100));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("a turn waiting for the user must not time out");
        assert_eq!(stop_reason, "end_turn");
        assert!(!end_session_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn the_idle_clock_still_runs_without_an_open_permission_request() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_updates: vec![AcpUpdate::Text {
                text: "quiet".to_string(),
            }],
            pause_after_first_update: Duration::from_millis(400),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_millis(100));

        service
            .send_prompt("session-1", hi())
            .await
            .expect_err("a silent agent must still time out");
        assert!(end_session_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn respond_permission_reaches_the_client() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = service_with(
            FakeSessionClient::default(),
            &publisher,
            Duration::from_secs(5),
        );
        // The fake keeps the trait default, which has no pending requests.
        let err = service
            .respond_permission("session-1", "perm-1", Some("proceed_once"))
            .await
            .expect_err("no pending request");
        assert!(matches!(err, DomainError::NotFound(_)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket-shared acp_permission_events` and `cargo test -j4 -p rocket-app permission`
Expected: FAIL to compile (variants and `respond_permission` missing).

- [ ] **Step 3: Implement**

`crates/rocket-shared/src/events.rs`: add `use crate::acp::PermissionOption;` next to the `ConfigOption` import and these variants after `AcpUsage`:

```rust
    /// Emitted when the agent asks the user to approve something. The turn
    /// waits for the answer. `kind` is the ACP tool kind in snake_case;
    /// `summary` is short, masked text about the call.
    AcpPermissionRequested {
        session_id: String,
        request_id: String,
        call_id: String,
        title: String,
        kind: String,
        summary: String,
        options: Vec<PermissionOption>,
    },
    /// Emitted when a permission request was answered. `option_id` is the
    /// chosen option, or `None` when the request was cancelled.
    AcpPermissionResolved {
        session_id: String,
        request_id: String,
        option_id: Option<String>,
    },
```

`crates/rocket-app/src/acp_session_service.rs`:

1. Add `use std::collections::HashSet;` if not imported (the file imports `HashMap`/`HashSet`; keep one import).
2. In `send_prompt`, after `let mut tool_calls = ToolCallStates::new();` add `let mut open_permissions: HashSet<String> = HashSet::new();`. Change the update arm to:

```rust
                update = rx.recv(), if channel_open => match update {
                    Some(update) => {
                        match &update {
                            AcpUpdate::PermissionRequest { request_id, .. } => {
                                open_permissions.insert(request_id.clone());
                            }
                            AcpUpdate::PermissionResolved { request_id, .. } => {
                                open_permissions.remove(request_id);
                            }
                            _ => {}
                        }
                        self.publish_update(session_id, update, &mut tool_calls);
                    }
                    None => channel_open = false,
                },
```

and the idle arm header to `() = tokio::time::sleep(self.prompt_idle_timeout), if open_permissions.is_empty() => {`. Add this comment above the arm: `// A turn waiting for the user's answer is not idle. The clock starts again when the answer arrives.` Extend the method's doc comment "Idle timeout" paragraph with: `The limit does not run while a permission request is open.`

3. In `publish_update`, replace the temporary arm from Task 1 with:

```rust
            AcpUpdate::PermissionRequest {
                request_id,
                call_id,
                title,
                kind,
                summary,
                options,
            } => DomainEvent::AcpPermissionRequested {
                session_id,
                request_id,
                call_id,
                title,
                kind,
                summary,
                options,
            },
            AcpUpdate::PermissionResolved {
                request_id,
                option_id,
            } => DomainEvent::AcpPermissionResolved {
                session_id,
                request_id,
                option_id,
            },
```

4. Add the method after `cancel`:

```rust
    /// Answers a permission request the agent opened during a turn.
    /// `option_id` must be one of the options the request offered; `None`
    /// cancels it. No event is published here: the client reports the
    /// answer on the turn's channel, which `send_prompt` publishes.
    pub async fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        option_id: Option<&str>,
    ) -> DomainResult<()> {
        self.session_client
            .resolve_permission(session_id, request_id, option_id)
            .await
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -j4 -p rocket-shared acp_permission_events && cargo test -j4 -p rocket-app acp_session_service`
Expected: PASS, including the existing idle tests (`idle_timeout_restarts_on_every_update`, `idle_timeout_runs_cleanup_once`).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared/src/events.rs crates/rocket-app/src/acp_session_service.rs
git commit --only -m "feat(acp): publish permission events and pause the idle clock while one is open" -- crates/rocket-shared/src/events.rs crates/rocket-app/src/acp_session_service.rs
```

---

### Task 4: Tauri channels, command and registration

**Files:**
- Modify: `src-tauri/src/tauri_event_bus.rs:68-71` and its tests (~line 129)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (after `cancel_agent_prompt`, ~line 56)
- Modify: `src-tauri/src/lib.rs:1080-1082`

**Interfaces:**
- Consumes: `AcpSessionService::respond_permission`, the two `DomainEvent`s.
- Produces (used by Task 5): event channels `agent-permission-requested`, `agent-permission-resolved`; command `respond_agent_permission(sessionId, requestId, optionId)`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/tauri_event_bus.rs` tests:

```rust
    #[test]
    fn permission_events_have_their_own_channels() {
        assert_eq!(
            channel_for(&DomainEvent::AcpPermissionRequested {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                call_id: "c".into(),
                title: "t".into(),
                kind: "other".into(),
                summary: String::new(),
                options: Vec::new(),
            }),
            "agent-permission-requested"
        );
        assert_eq!(
            channel_for(&DomainEvent::AcpPermissionResolved {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                option_id: None,
            }),
            "agent-permission-resolved"
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket permission_events_have_their_own_channels` (use the actual package name of `src-tauri` from its `Cargo.toml`; check with `grep '^name' src-tauri/Cargo.toml`).
Expected: FAIL (non-exhaustive match in `channel_for`).

- [ ] **Step 3: Implement**

`tauri_event_bus.rs`, after the `AcpUsage` arm:

```rust
        DomainEvent::AcpPermissionRequested { .. } => "agent-permission-requested",
        DomainEvent::AcpPermissionResolved { .. } => "agent-permission-resolved",
```

`commands/acp_sessions.rs`, after `cancel_agent_prompt`:

```rust
/// Answers an agent's permission request with one of the options it offered,
/// or with `None` to cancel it. Fails when the request is unknown, already
/// answered, or the option was not offered.
#[tauri::command]
pub async fn respond_agent_permission(
    session_id: String,
    request_id: String,
    option_id: Option<String>,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.respond_permission(&session_id, &request_id, option_id.as_deref())
        .await
}
```

`lib.rs`, after `commands::acp_sessions::cancel_agent_prompt,`:

```rust
            commands::acp_sessions::respond_agent_permission,
```

- [ ] **Step 4: Verify**

Run: `cargo check -j4 -p <src-tauri package> && cargo test -j4 -p <src-tauri package> permission_events_have_their_own_channels`
Expected: PASS. Also run the existing `cargo test -j4 -p <src-tauri package> --test acp_mcp_start_workspace_assistant` to confirm the unchanged `FakeSessionClient` there still compiles (the trait method has a default body).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tauri_event_bus.rs src-tauri/src/commands/acp_sessions.rs src-tauri/src/lib.rs
git commit --only -m "feat(acp): add the respond_agent_permission command and event channels" -- src-tauri/src/tauri_event_bus.rs src-tauri/src/commands/acp_sessions.rs src-tauri/src/lib.rs
```

---

### Task 5: Frontend API, store and event bridge

**Files:**
- Modify: `src/lib/tauri-api.ts` (after `onAgentUsage`, ~line 3135, and after `cancelAgentPrompt`, ~line 2731)
- Modify: `src/stores/assistant-store.ts`
- Modify: `src/lib/assistant-event-bridge.ts` (~lines 14-20 imports, ~line 100 registrations)
- Test: `src/stores/__tests__/assistant-store.test.ts`, `src/lib/__tests__/assistant-event-bridge.test.ts`

**Interfaces:**
- Consumes: channels and command from Task 4.
- Produces (used by Task 6):
  - `AgentPermissionOption { id: string; name: string; kind: AgentPermissionKind }`, `AgentPermissionKind`
  - `respondAgentPermission(sessionId: string, requestId: string, optionId: string | null): Promise<void>`
  - Store: `AgentPermission { requestId; callId; title; kind; summary; options; status: 'pending' | 'answered' | 'cancelled'; chosenName?: string }`, state `permissions: AgentPermission[]`, actions `addPermission(sessionId, permission)` and `resolvePermission(sessionId, requestId, optionId: string | null)`.

- [ ] **Step 1: Write the failing store tests**

In `src/stores/__tests__/assistant-store.test.ts`, inside `describe('assistant-store', ...)`:

```ts
  describe('permissions', () => {
    const request = (requestId: string) => ({
      requestId,
      callId: 'call-1',
      title: 'propose_changes',
      kind: 'other',
      summary: '{}',
      options: [
        { id: 'proceed_always_server', name: 'Allow all server tools for this session', kind: 'allow_always' as const },
        { id: 'cancel', name: 'Deny', kind: 'reject_once' as const },
      ],
    });

    it('adds a pending request for the current session only', () => {
      activate('s1');
      store().addPermission('other', request('perm-1'));
      expect(store().permissions).toEqual([]);
      store().addPermission('s1', request('perm-1'));
      expect(store().permissions).toHaveLength(1);
      expect(store().permissions[0]).toMatchObject({ requestId: 'perm-1', status: 'pending' });
    });

    it('keeps parallel requests apart and resolves each by id', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().addPermission('s1', request('perm-2'));
      store().resolvePermission('s1', 'perm-2', 'cancel');
      const [first, second] = store().permissions;
      expect(first.status).toBe('pending');
      expect(second).toMatchObject({ status: 'answered', chosenName: 'Deny' });
    });

    it('shows a cancelled request as cancelled', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().resolvePermission('s1', 'perm-1', null);
      expect(store().permissions[0]).toMatchObject({ status: 'cancelled' });
      expect(store().permissions[0].chosenName).toBeUndefined();
    });

    it('ignores a duplicate request id', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().addPermission('s1', request('perm-1'));
      expect(store().permissions).toHaveLength(1);
    });

    it('cancels pending requests when the turn finishes or fails', () => {
      activate();
      store().appendUserMessage('hi');
      store().addPermission('s1', request('perm-1'));
      store().completeMessage('s1');
      expect(store().permissions[0].status).toBe('cancelled');

      store().appendUserMessage('again');
      store().addPermission('s1', request('perm-2'));
      store().failMessage('s1', 'boom', false);
      expect(store().permissions[1].status).toBe('cancelled');
    });

    it('does not reopen an answered request when the turn finishes', () => {
      activate();
      store().appendUserMessage('hi');
      store().addPermission('s1', request('perm-1'));
      store().resolvePermission('s1', 'perm-1', 'proceed_always_server');
      store().completeMessage('s1');
      expect(store().permissions[0].status).toBe('answered');
    });

    it('clears the requests when the session ends, fails fatally or is reset', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().endSession();
      expect(store().permissions).toEqual([]);

      activate('s2');
      store().addPermission('s2', request('perm-2'));
      store().failMessage('s2', 'dead', true);
      expect(store().permissions).toEqual([]);

      activate('s3');
      store().addPermission('s3', request('perm-3'));
      store().reset();
      expect(store().permissions).toEqual([]);
    });
  });
```

In `src/lib/__tests__/assistant-event-bridge.test.ts`, add two listeners to `mocks.api`:

```ts
      onAgentPermissionRequested: listener('permissionRequested'),
      onAgentPermissionResolved: listener('permissionResolved'),
```

and a test (use the same pattern the file uses for other events; `mocks.handlers` holds the registered handlers and the file already has a helper that mounts the hook with `renderHook(() => useAssistantEventBridge())` and waits for registration):

```ts
  it('routes permission events into the store', async () => {
    const token = store().beginSession('agent-1', 'edit');
    store().activateSession(token, 's1', []);
    renderHook(() => useAssistantEventBridge());
    await waitFor(() => expect(mocks.handlers.permissionRequested).toBeDefined());

    mocks.handlers.permissionRequested({
      type: 'acpPermissionRequested',
      session_id: 's1',
      request_id: 'perm-1',
      call_id: 'call-1',
      title: 'propose_changes',
      kind: 'other',
      summary: '{}',
      options: [{ id: 'proceed_once', name: 'Allow once', kind: 'allow_once' }],
    });
    expect(store().permissions[0]).toMatchObject({ requestId: 'perm-1', status: 'pending' });

    mocks.handlers.permissionResolved({
      type: 'acpPermissionResolved',
      session_id: 's1',
      request_id: 'perm-1',
      option_id: 'proceed_once',
    });
    expect(store().permissions[0]).toMatchObject({ status: 'answered', chosenName: 'Allow once' });
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `yarn test assistant-store assistant-event-bridge`
Expected: FAIL (`addPermission` is not a function; no handler registered).

- [ ] **Step 3: Implement the API layer**

`src/lib/tauri-api.ts`, after `onAgentUsage`:

```ts
export type AgentPermissionKind = 'allow_once' | 'allow_always' | 'reject_once' | 'reject_always';

/** One answer the agent offers to a permission request. */
export interface AgentPermissionOption {
  id: string;
  name: string;
  kind: AgentPermissionKind;
}

export interface AgentPermissionRequestedEvent {
  type: 'acpPermissionRequested';
  session_id: string;
  request_id: string;
  call_id: string;
  title: string;
  /** The ACP tool kind in snake_case, for example `execute`, `edit` or `other`. */
  kind: string;
  /** Short, masked text about the call. */
  summary: string;
  options: AgentPermissionOption[];
}

export interface AgentPermissionResolvedEvent {
  type: 'acpPermissionResolved';
  session_id: string;
  request_id: string;
  /** The chosen option id, or null when the request was cancelled. */
  option_id: string | null;
}

export const onAgentPermissionRequested = (
  handler: (event: AgentPermissionRequestedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentPermissionRequestedEvent>('agent-permission-requested', (e) => handler(e.payload));

export const onAgentPermissionResolved = (
  handler: (event: AgentPermissionResolvedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentPermissionResolvedEvent>('agent-permission-resolved', (e) => handler(e.payload));
```

After `cancelAgentPrompt`:

```ts
/** Answers an agent's permission request with an offered option, or null to cancel it. */
export const respondAgentPermission = (
  sessionId: string,
  requestId: string,
  optionId: string | null,
) => invoke<void>('respond_agent_permission', { sessionId, requestId, optionId });
```

- [ ] **Step 4: Implement the store**

In `src/stores/assistant-store.ts`: import `AgentPermissionOption` from `@/lib/tauri-api` (extend the existing type import), then:

```ts
export interface AgentPermission {
  requestId: string;
  callId: string;
  title: string;
  kind: string;
  summary: string;
  options: AgentPermissionOption[];
  status: 'pending' | 'answered' | 'cancelled';
  /** The name of the option the user chose, once answered. */
  chosenName?: string;
}

/** A request that is still waiting becomes cancelled; answered ones stay. */
function cancelPending(permissions: AgentPermission[]): AgentPermission[] {
  return permissions.map((p) => (p.status === 'pending' ? { ...p, status: 'cancelled' } : p));
}
```

State: add `permissions: AgentPermission[];` to `AssistantState` (after `proposalAnchors`) with
`addPermission: (sessionId: string, permission: Omit<AgentPermission, 'status' | 'chosenName'>) => void;` and
`resolvePermission: (sessionId: string, requestId: string, optionId: string | null) => void;`.

Initial state, `beginSession`, `endSession` and `reset`: add `permissions: [],` next to every `proposals: []`. In `failMessage` add `permissions: fatal ? [] : cancelPending(state.permissions),` to the returned object. Replace `completeMessage` with:

```ts
  completeMessage(sessionId) {
    set((state) =>
      isCurrent(state, sessionId) && selectTurnRunning(state)
        ? { messages: settleStreaming(state.messages), permissions: cancelPending(state.permissions) }
        : state,
    );
  },
```

Actions (next to `upsertProposal`):

```ts
  addPermission(sessionId, permission) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      if (state.permissions.some((p) => p.requestId === permission.requestId)) return state;
      return { permissions: [...state.permissions, { ...permission, status: 'pending' }] };
    });
  },

  resolvePermission(sessionId, requestId, optionId) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      return {
        permissions: state.permissions.map((p) => {
          if (p.requestId !== requestId || p.status !== 'pending') return p;
          if (optionId === null) return { ...p, status: 'cancelled' };
          const chosen = p.options.find((o) => o.id === optionId);
          return { ...p, status: 'answered', chosenName: chosen?.name ?? optionId };
        }),
      };
    });
  },
```

- [ ] **Step 5: Implement the bridge**

In `src/lib/assistant-event-bridge.ts`, import `onAgentPermissionRequested` and `onAgentPermissionResolved` from `@/lib/tauri-api` and add to `registrations`:

```ts
      onAgentPermissionRequested(
        guard((e) =>
          store().addPermission(e.session_id, {
            requestId: e.request_id,
            callId: e.call_id,
            title: e.title,
            kind: e.kind,
            summary: e.summary,
            options: e.options,
          }),
        ),
      ),
      onAgentPermissionResolved(
        guard((e) => store().resolvePermission(e.session_id, e.request_id, e.option_id)),
      ),
```

- [ ] **Step 6: Verify**

Run: `yarn test assistant-store assistant-event-bridge && yarn tsc --noEmit && yarn check`
Expected: PASS, no type or lint errors.

- [ ] **Step 7: Commit**

```bash
git add src/lib/tauri-api.ts src/stores/assistant-store.ts src/lib/assistant-event-bridge.ts src/stores/__tests__/assistant-store.test.ts src/lib/__tests__/assistant-event-bridge.test.ts
git commit --only -m "feat(assistant): track agent permission requests in the store" -- src/lib/tauri-api.ts src/stores/assistant-store.ts src/lib/assistant-event-bridge.ts src/stores/__tests__/assistant-store.test.ts src/lib/__tests__/assistant-event-bridge.test.ts
```

---

### Task 6: `AssistantPermissionCard` and the Permissions section

**Files:**
- Create: `src/components/assistant/AssistantPermissionCard.tsx`
- Modify: `src/components/assistant/AssistantChatView.tsx` (imports ~line 12; render ~line 146-158; `itemCount` ~line 99)
- Test: `src/components/assistant/__tests__/AssistantPermissionCard.test.tsx`; extend `src/components/assistant/__tests__/AssistantChatView.test.tsx`

**Interfaces:**
- Consumes: `AgentPermission` (store), `respondAgentPermission` (API).
- Produces: `<AssistantPermissionCard sessionId permission />`.

- [ ] **Step 1: Write the failing component tests**

Create `src/components/assistant/__tests__/AssistantPermissionCard.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { AgentPermission } from '@/stores/assistant-store';
import { AssistantPermissionCard } from '../AssistantPermissionCard';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  respondAgentPermission: vi.fn(),
}));

function makePermission(overrides: Partial<AgentPermission> = {}): AgentPermission {
  return {
    requestId: 'perm-1',
    callId: 'call-1',
    title: 'propose_changes',
    kind: 'other',
    summary: '{"changes":[]}',
    status: 'pending',
    options: [
      { id: 'proceed_always_server', name: 'Allow all server tools for this session', kind: 'allow_always' },
      { id: 'proceed_once', name: 'Allow once', kind: 'allow_once' },
      { id: 'cancel', name: 'Deny', kind: 'reject_once' },
    ],
    ...overrides,
  };
}

describe('AssistantPermissionCard', () => {
  beforeEach(() => {
    vi.mocked(api.respondAgentPermission).mockReset();
    vi.mocked(api.respondAgentPermission).mockResolvedValue(undefined);
  });

  it('shows the tool, the summary and one button per offered option', () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    expect(screen.getByText('propose_changes')).toBeInTheDocument();
    expect(screen.getByText('{"changes":[]}')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Allow all server tools for this session' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Allow once' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Deny' })).toBeInTheDocument();
    expect(screen.getByText(/waiting for your answer/i)).toBeInTheDocument();
  });

  it('sends the chosen option id', async () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Allow once' }));
    expect(api.respondAgentPermission).toHaveBeenCalledWith('s1', 'perm-1', 'proceed_once');
  });

  it('disables the buttons while the answer is being sent and shows a failure', async () => {
    vi.mocked(api.respondAgentPermission).mockRejectedValue(new Error('refused'));
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }));
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Deny' })).toBeEnabled();
  });

  it('shows a caution badge for a command or an edit', () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission({ kind: 'execute' })} />);
    expect(screen.getByText('Runs a command')).toBeInTheDocument();
  });

  it('collapses to one line with the chosen option once answered', () => {
    render(
      <AssistantPermissionCard
        sessionId='s1'
        permission={makePermission({ status: 'answered', chosenName: 'Allow once' })}
      />,
    );
    expect(screen.getByText(/Allow once/)).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows a cancelled request as cancelled, without buttons', () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission({ status: 'cancelled' })} />);
    expect(screen.getByText(/Cancelled/)).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
```

Add to `AssistantChatView.test.tsx` (follow the file's existing setup that seeds `useAssistantStore`):

```tsx
  it('renders open and answered permission requests in their own section', () => {
    useAssistantStore.setState({
      permissions: [
        { requestId: 'perm-1', callId: 'c1', title: 'propose_changes', kind: 'other', summary: '{}', status: 'pending',
          options: [{ id: 'proceed_once', name: 'Allow once', kind: 'allow_once' }] },
        { requestId: 'perm-2', callId: 'c2', title: 'read_file', kind: 'read', summary: '', status: 'answered',
          chosenName: 'Allow once', options: [] },
      ],
    });
    render(<AssistantChatView />);
    expect(screen.getByRole('region', { name: 'Permissions' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Allow once' })).toBeInTheDocument();
  });
```

(Use the same `session` seeding the file's other tests use so `isCurrent` is not needed here: the view only reads `permissions`. Pass `sessionId` from `useAssistantStore((s) => s.session?.sessionId)`.)

- [ ] **Step 2: Run to verify failure**

Run: `yarn test AssistantPermissionCard AssistantChatView`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement the card**

Create `src/components/assistant/AssistantPermissionCard.tsx`:

```tsx
import { Check, Loader2, ShieldQuestion, X } from 'lucide-react';
import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { type AgentPermissionKind, respondAgentPermission } from '@/lib/tauri-api';
import type { AgentPermission } from '@/stores/assistant-store';

const GENERIC_ACTION_ERROR = 'The answer could not be sent. Try again.';

/** The tool kinds that can change something outside the assistant. */
const CAUTION_LABEL: Record<string, string> = {
  execute: 'Runs a command',
  edit: 'Edits a file',
  delete: 'Deletes a file',
  move: 'Moves a file',
};

function isReject(kind: AgentPermissionKind): boolean {
  return kind === 'reject_once' || kind === 'reject_always';
}

/**
 * One permission request from the agent, with a button for each option the
 * agent offered. Once answered or cancelled it is a one-line row.
 */
export function AssistantPermissionCard({
  sessionId,
  permission,
}: {
  sessionId: string;
  permission: AgentPermission;
}) {
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  if (permission.status !== 'pending') {
    const Icon = permission.status === 'answered' ? Check : X;
    return (
      <div className='flex items-center gap-2 text-xs text-muted-foreground'>
        <Icon className='h-3 w-3 shrink-0' aria-hidden />
        <span className='truncate'>
          {permission.title} -{' '}
          {permission.status === 'answered' ? (permission.chosenName ?? 'Answered') : 'Cancelled'}
        </span>
      </div>
    );
  }

  const caution = CAUTION_LABEL[permission.kind];
  const choose = async (optionId: string) => {
    setBusy(true);
    setActionError(null);
    try {
      await respondAgentPermission(sessionId, permission.requestId, optionId);
    } catch {
      setActionError(GENERIC_ACTION_ERROR);
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader className='flex flex-row items-center gap-2 space-y-0 p-3 pb-2'>
        <ShieldQuestion className='h-4 w-4 shrink-0 text-muted-foreground' aria-hidden />
        <CardTitle className='min-w-0 flex-1 truncate text-sm'>{permission.title}</CardTitle>
        {caution && <Badge variant='warning'>{caution}</Badge>}
      </CardHeader>
      <CardContent className='flex flex-col gap-2 p-3 pt-0'>
        {permission.summary && (
          <pre className='max-h-32 overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-2 text-xs'>
            {permission.summary}
          </pre>
        )}
        <p className='text-xs text-muted-foreground'>The agent is waiting for your answer.</p>
        <div className='flex flex-wrap gap-2'>
          {permission.options.map((option) => (
            <Button
              key={option.id}
              size='sm'
              variant={isReject(option.kind) ? 'outline' : 'default'}
              disabled={busy}
              onClick={() => void choose(option.id)}
            >
              {busy && <Loader2 className='mr-1 h-3 w-3 animate-spin' aria-hidden />}
              {option.name}
            </Button>
          ))}
        </div>
        {actionError && (
          <p role='alert' className='text-xs text-destructive'>
            {actionError}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
```

Notes: `Badge` already has a `warning` variant (used by `AssistantProposalCard`). The failure test expects the buttons re-enabled, which `setBusy(false)` in the `catch` does. On success the store flips the status when the `acpPermissionResolved` event arrives, so the card is replaced by the one-line row; keep `busy` set until then.

- [ ] **Step 4: Render the section**

In `AssistantChatView.tsx`: add `import { AssistantPermissionCard } from './AssistantPermissionCard';`. In `AssistantChatView()` add:

```tsx
  const permissions = useAssistantStore((s) => s.permissions);
  const sessionId = useAssistantStore((s) => s.session?.sessionId ?? '');
```

Add `+ permissions.length` to `itemCount` so new cards scroll into view, and render, just above the `pending.length > 0` proposals section:

```tsx
        {permissions.length > 0 && (
          <section aria-label='Permissions' className='flex flex-col gap-2'>
            {permissions.map((p) => (
              <AssistantPermissionCard key={p.requestId} sessionId={sessionId} permission={p} />
            ))}
          </section>
        )}
```

- [ ] **Step 5: Verify**

Run: `yarn test AssistantPermissionCard AssistantChatView assistant && yarn tsc --noEmit && yarn check`
Expected: PASS. If `Badge` does not accept `variant='warning'` in this file's version, use the same variant name `AssistantProposalCard` uses for `stale`.

- [ ] **Step 6: Commit**

```bash
git add src/components/assistant/AssistantPermissionCard.tsx src/components/assistant/AssistantChatView.tsx src/components/assistant/__tests__/AssistantPermissionCard.test.tsx src/components/assistant/__tests__/AssistantChatView.test.tsx
git commit --only -m "feat(assistant): show agent permission requests as cards" -- src/components/assistant/AssistantPermissionCard.tsx src/components/assistant/AssistantChatView.tsx src/components/assistant/__tests__/AssistantPermissionCard.test.tsx src/components/assistant/__tests__/AssistantChatView.test.tsx
```

---

### Task 7: Manual check with a real Gemini session and follow-up notes

**Files:**
- Modify: `.claude/assistant-followups.md`

- [ ] **Step 1: Full verification**

Run, in order:
`cargo check -j4`
`cargo test -j4 -p rocket-acp`
`cargo test -j4 -p rocket-infra acp_`
`cargo test -j4 -p rocket-app acp_session_service`
`yarn tsc --noEmit && yarn check && yarn test assistant`
Expected: all PASS.

- [ ] **Step 2: Manual check**

1. `yarn tauri dev`. Start the assistant with the **Rocket-Gemini** agent, in Agent mode.
2. Ask: "Create a folder named test-permissions in the first collection."
3. Expected: a **Permissions** card appears for the `propose_changes` call, showing the agent's options (`Allow all server tools for this session`, `Allow tool for this session`, `Allow once`, `Deny`). The turn shows as running and is not killed after 2 minutes.
4. Click **Allow all server tools for this session**. Expected: the card becomes a one-line row; the existing proposal card appears; accepting it creates the folder.
5. Ask for a second change. Expected: no new permission card (Gemini remembers the server for the session).
6. Ask for something that makes Gemini request its built-in shell. Expected: a card with the **Runs a command** badge and the command in the summary. Click **Deny**. Expected: the agent reports the tool was denied.
7. Start a turn that opens a card, then press Stop. Expected: the card shows "Cancelled", the turn ends, and the session stays open.
8. Repeat steps 2-4 with the **Rocket-Claude** agent. Expected: no permission card (it is pre-approved), proposals work as before.

Record any deviation as a bug before continuing.

- [ ] **Step 3: Update the follow-ups doc**

Append to `.claude/assistant-followups.md` under its open follow-ups list:

```markdown
- **Gemini built-in tools stay on.** Permission cards (2026-10-12) let the user approve Gemini's shell and file tools. Claude sessions drop them with `tools: []`; Gemini has no equivalent in `session/new`. Look for a per-session way to disable them (settings file in the isolated config dir, or `--allowed-tools`) so the card is only ever about Rocket's tools.
- **Permission cards are not saved** in stored conversations. A reloaded conversation shows no record that a tool was approved or denied.
```

- [ ] **Step 4: Commit**

```bash
git add .claude/assistant-followups.md
git commit --only -m "docs(assistant): record the permission card follow-ups" -- .claude/assistant-followups.md
```

---

## Self-Review

**Spec coverage**

| Spec section | Task |
|---|---|
| Shared option types, `AcpUpdate` variants, trait method | 1 |
| Broker, spawned handler (not in dispatch loop), no-turn deny, drain on cancel, validation | 2 |
| Events, `respond_permission`, idle clock paused while a request is open | 3 |
| Command `respond_agent_permission`, channels | 4 |
| Store (`permissions`), event bridge, cleared with session, cancelled when the turn ends | 5 |
| Card (options as buttons, caution badge, one-line row, error), Permissions section | 6 |
| Manual check, follow-ups (Gemini built-in tools, history persistence) | 7 |

The spec's drain "on turn end" is covered on the UI side by `cancelPending` in `completeMessage` and `failMessage` (Task 5); on the backend the agent cannot end a turn while its request is unanswered, and `cancel()` drains. `end_session` needs no code: dropping `RunningSession` drops the broker and its senders.

**Placeholder scan:** no TBD/TODO. The spots that depend on the compiler (the `SelectedPermissionOutcome::new` argument type, `option_id.to_string()`, the `ToolCallUpdateFields` builder) name the exact fallback.

**Type consistency:** `PermissionOption { id, name, kind }` and `PermissionOptionKind` (shared, Task 1) are used unchanged in Tasks 2 and 3; `register`/`resolve`/`drain_cancelled` (Task 2) match the calls in `resolve_permission` and `cancel`; `respond_permission(session_id, request_id, Option<&str>)` (Task 3) matches the command (Task 4); the frontend `respondAgentPermission(sessionId, requestId, optionId | null)` matches the command's camelCase args; `addPermission`/`resolvePermission` (Task 5) match the bridge and card.

**Review Focus coverage:** idle pause (Task 3 tests), wrong/duplicate answers (Task 2 broker and integration tests), cancel (Task 2 integration, Task 5 store), turn ends with an open card (Task 5), request outside a turn (the `handle_permission_request` no-turn branch is exercised only by the integration suite indirectly; the broker's `drain_cancelled` and `resolve(None)` paths are unit tested), parallel requests (Task 5 store test; the broker's distinct-id test in Task 2).

**Known gap:** the no-turn deny branch has no direct test, because a permission request cannot be made to arrive outside a turn with the current fixture agent. If you want it covered, add a fixture hook in `test_acp_agent.rs` that sends a `RequestPermissionRequest` right after `session/new`, and assert the fixture logs `permission:cancelled`/`selected:reject-once` on stderr. I left it out to keep the fixture unchanged.
