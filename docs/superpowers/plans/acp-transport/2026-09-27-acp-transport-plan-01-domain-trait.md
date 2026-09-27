# ACP Transport Plan 01: Domain Trait — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `AcpSessionClient` — the protocol-focused trait for spawning an ACP agent session, sending a prompt, and ending a session — to the existing `rocket-acp` crate.

**Architecture:** One new module in `rocket-acp`, a plain trait with no knowledge of `DomainEvent`, Tauri, or the `agent-client-protocol` crate's own types — only primitives, `DomainResult`, and a `tokio::sync::mpsc::UnboundedSender<String>` for streaming. Concrete implementation (Plan 03) and orchestration (Plan 04) build on this without this crate depending on either.

**Tech Stack:** Rust, `async-trait`, `tokio` (channel type only).

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Architecture & crate placement section). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md`.

## Global Constraints

- This trait must **not** depend on the `agent-client-protocol` crate — only `rocket-shared`, `async-trait`, and `tokio` (for the channel type). The concrete implementation in Plan 03 is the only place that crate is used.
- `rocket-acp`'s `Cargo.toml` currently has only `rocket-shared` and `serde` as `[dependencies]` (`serde_json` is dev-only). This plan adds `async-trait.workspace = true` and `tokio.workspace = true` to `[dependencies]`.
- Test code uses `.expect("message")` for fallible calls, never the bare panicking shorthand.

## Review Focus

- The trait must be object-safe (`Box<dyn AcpSessionClient>` compiles) since `AcpSessionService` (Plan 04) holds it as a trait object.
- `send_prompt`'s `chunk_tx` parameter type must be exactly `tokio::sync::mpsc::UnboundedSender<String>` — a plain channel, not a boxed closure or a publisher type — per the Global Constraint above. A reviewer should flag any drift toward publisher-shaped signatures here, since that responsibility belongs in `AcpSessionService`, not this trait.

---

## Task 1: `AcpSessionClient` trait

**Files:**
- Create: `crates/rocket-acp/src/session.rs`
- Modify: `crates/rocket-acp/src/lib.rs`
- Modify: `crates/rocket-acp/Cargo.toml`

**Interfaces:**
- Produces: `AcpSessionClient { start_session, send_prompt, end_session }` — consumed by Plan 03 (`AcpAgentClient` impl) and Plan 04 (`AcpSessionService`, which holds `Box<dyn AcpSessionClient>`).

- [ ] **Step 1: Add dependencies**

In `crates/rocket-acp/Cargo.toml`, add to `[dependencies]`:

```toml
async-trait.workspace = true
tokio.workspace = true
```

Run: `cargo check -p rocket-acp -j4`
Expected: succeeds (no code uses the new dependencies yet, but they must resolve).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-acp/src/session.rs
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use tokio::sync::mpsc;

    struct FakeSessionClient;

    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
        ) -> DomainResult<String> {
            Ok("session-1".to_string())
        }

        async fn send_prompt(
            &self,
            _session_id: &str,
            _prompt: String,
            chunk_tx: mpsc::UnboundedSender<String>,
        ) -> DomainResult<String> {
            let _ = chunk_tx.send("hello".to_string());
            Ok("end_turn".to_string())
        }

        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn trait_is_object_safe_and_callable_through_a_trait_object() {
        let client: Box<dyn AcpSessionClient> = Box::new(FakeSessionClient);

        let session_id = client
            .start_session("echo", &[], "/tmp", &[])
            .await
            .expect("start_session");
        assert_eq!(session_id, "session-1");

        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(&session_id, "hi".to_string(), tx)
            .await
            .expect("send_prompt");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(rx.recv().await, Some("hello".to_string()));

        client.end_session(&session_id).await.expect("end_session");
    }

    #[tokio::test]
    async fn start_session_errors_propagate_as_domain_errors() {
        struct FailingClient;
        #[async_trait::async_trait]
        impl AcpSessionClient for FailingClient {
            async fn start_session(
                &self,
                _command: &str,
                _args: &[String],
                _cwd: &str,
                _env: &[(String, String)],
            ) -> DomainResult<String> {
                Err(DomainError::InvalidInput("command not found".to_string()))
            }
            async fn send_prompt(
                &self,
                _session_id: &str,
                _prompt: String,
                _chunk_tx: mpsc::UnboundedSender<String>,
            ) -> DomainResult<String> {
                unreachable!("not exercised by this test")
            }
            async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
        }

        let client: Box<dyn AcpSessionClient> = Box::new(FailingClient);
        let err = client
            .start_session("bad-command", &[], "/tmp", &[])
            .await
            .expect_err("must propagate the error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-acp session::tests -j4`
Expected: FAIL with "cannot find trait `AcpSessionClient`" (compile error — the trait doesn't exist yet).

- [ ] **Step 4: Implement the trait**

```rust
// crates/rocket-acp/src/session.rs (add above the tests module)
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc::UnboundedSender;

/// Protocol-focused contract for driving one ACP agent session: spawning the
/// process and handshaking, sending a prompt and streaming its response, and
/// ending the session. Has no knowledge of `DomainEvent`, Tauri, or the
/// `agent-client-protocol` crate — those live in the concrete implementation
/// (`rocket-infra`'s `AcpAgentClient`, Plan 03) and the orchestration layer
/// (`rocket-app`'s `AcpSessionService`, Plan 04) respectively.
#[async_trait::async_trait]
pub trait AcpSessionClient: Send + Sync {
    /// Spawns the agent process and performs the `initialize` → `session/new`
    /// handshake. Returns the ACP-provided `sessionId`, used as-is for every
    /// later call — no separate Rocket-side id translation layer.
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> DomainResult<String>;

    /// Sends `session/prompt`. As `agent_message_chunk` updates arrive from
    /// the agent, their text is forwarded through `chunk_tx` — the caller
    /// reads it concurrently while this call is still pending. Resolves with
    /// the raw `stopReason` string once the agent's turn ends.
    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: String,
        chunk_tx: UnboundedSender<String>,
    ) -> DomainResult<String>;

    /// Ends the session by explicitly killing the agent process — dropping a
    /// connection handle does not kill the underlying child process by
    /// default in either `std` or `tokio`, so this must be an active kill,
    /// not passive cleanup. No ACP-level shutdown handshake exists.
    async fn end_session(&self, session_id: &str) -> DomainResult<()>;
}
```

- [ ] **Step 5: Register the module**

In `crates/rocket-acp/src/lib.rs`:

```rust
pub mod agent_config;
pub mod session;
pub use agent_config::{AgentConfig, AgentConfigRepository};
pub use session::AcpSessionClient;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-acp -j4`
Expected: PASS — 2 new tests, plus all existing `rocket-acp` tests unaffected.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-acp
git commit -m "feat(acp): add AcpSessionClient trait"
```

---

## Next Plan

[Plan 02: AcpSession DomainEvent variants](2026-09-27-acp-transport-plan-02-domain-events.md) — adds the `AcpSessionStarted`/`AcpSessionChunk`/`AcpSessionFinished`/`AcpSessionFailed` events this plan's trait results will be translated into by `AcpSessionService` (Plan 04). Independent of this plan's trait — can be done in either order, but numbered next per the index.

## Post-Implementation Review

Before starting Plan 02 (or Plan 03, whichever comes next in your execution order), dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-acp/Cargo.toml`, `crates/rocket-acp/src/lib.rs`,
> `crates/rocket-acp/src/session.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interface — does `AcpSessionClient` match
>    exactly what the plan index's locked interface contract promises Plan 03
>    (impl) and Plan 04 (orchestration) will consume?
> 2. Code quality — naming, doc comments, test coverage versus this plan's
>    Review Focus section (object-safety through a trait object, `chunk_tx`
>    staying a plain channel type rather than drifting toward a
>    publisher-shaped signature).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    specifically that `rocket-acp` still has zero dependency on
>    `agent-client-protocol` and contains no I/O.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-acp -j4` and
> `cargo check -p rocket-acp -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to the next plan once this review comes back clean (or its fixes are applied and re-verified).
