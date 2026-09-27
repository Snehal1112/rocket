# ACP Transport Plan 02: DomainEvent Variants — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four new `DomainEvent` variants — `AcpSessionStarted`, `AcpSessionChunk`, `AcpSessionFinished`, `AcpSessionFailed` — to `rocket-shared`, for `AcpSessionService` (Plan 04) to publish and `TauriEventBus` (Plan 05) to forward to the frontend.

**Architecture:** Purely additive to the existing `DomainEvent` enum, in the exact same shape and place the just-landed `FlowRunStarted`/`FlowStepCompleted`/`FlowRunFinished` trio was added — no other variant changes.

**Tech Stack:** Rust, serde.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Tauri IPC surface section, which names these four variants). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md`.

## Global Constraints

- `DomainEvent` is tagged `#[serde(tag = "type", rename_all = "camelCase")]` at the enum level — this renames variant names to camelCase for the `type` field, but struct-variant field names stay snake_case (confirmed by the existing `runner_step_completed_wire_shape` test's comment: "Struct-variant fields stay snake_case — the enum's rename_all only renames variants"). Do not add a per-variant or per-field rename attribute.
- Test code uses `.expect("message")` for fallible calls, never the bare panicking shorthand.

## Review Focus

- `AcpSessionChunk` and `AcpSessionFinished`/`AcpSessionFailed` all key on `session_id` — a wire-shape test should pin the exact field name (`session_id`, not `sessionId`) the same way `runner_step_completed_wire_shape` pins `run_id`, since the frontend (subproject C) will match on this exact string.
- `stop_reason` on `AcpSessionFinished` is a plain `String` (per the spec's explicit decision not to model ACP's `#[non_exhaustive]` `StopReason` as a fixed Rust enum) — a test should confirm an arbitrary/unrecognized string round-trips unchanged, not just a known value like `"end_turn"`, to guard against a future refactor accidentally constraining it to a closed set.

---

## Task 1: Four new `DomainEvent` variants

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

**Interfaces:**
- Produces: `DomainEvent::AcpSessionStarted { session_id }`, `AcpSessionChunk { session_id, text }`, `AcpSessionFinished { session_id, stop_reason }`, `AcpSessionFailed { session_id, error }` — consumed by Plan 04 (`AcpSessionService` publishes these) and Plan 05 (`TauriEventBus` maps them to named frontend events).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-shared/src/events.rs (add to the existing tests module)

#[test]
fn acp_session_started_wire_shape() {
    let event = DomainEvent::AcpSessionStarted {
        session_id: "sess-1".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(json, r#"{"type":"acpSessionStarted","session_id":"sess-1"}"#);
}

#[test]
fn acp_session_chunk_wire_shape() {
    let event = DomainEvent::AcpSessionChunk {
        session_id: "sess-1".into(),
        text: "Hello, ".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"acpSessionChunk","session_id":"sess-1","text":"Hello, "}"#
    );
}

#[test]
fn acp_session_finished_wire_shape_carries_arbitrary_stop_reason_unchanged() {
    // stop_reason is a plain String (not a closed Rust enum) precisely
    // because ACP's own StopReason is #[non_exhaustive] — an
    // agent-specific custom reason (the spec's example: an underscore-
    // prefixed value) must round-trip unchanged, not just a known value
    // like "end_turn".
    let event = DomainEvent::AcpSessionFinished {
        session_id: "sess-1".into(),
        stop_reason: "_custom_agent_reason".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"acpSessionFinished","session_id":"sess-1","stop_reason":"_custom_agent_reason"}"#
    );
}

#[test]
fn acp_session_failed_wire_shape() {
    let event = DomainEvent::AcpSessionFailed {
        session_id: "sess-1".into(),
        error: "agent process exited unexpectedly".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"acpSessionFailed","session_id":"sess-1","error":"agent process exited unexpectedly"}"#
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-shared acp_session -j4`
Expected: FAIL with "no variant named `AcpSessionStarted`" (compile error — the variants don't exist yet).

- [ ] **Step 3: Add the variants**

In `crates/rocket-shared/src/events.rs`, add to the `DomainEvent` enum — alongside the existing `// Flow events` block, following it:

```rust
    // ACP AI-assist session events — mirrors the Collection Runner/Flow
    // events above: each variant gets its own frontend channel.
    /// Emitted once a spawned agent process completes its ACP handshake.
    AcpSessionStarted {
        session_id: String,
    },
    /// Emitted per streamed text chunk while a prompt is being answered.
    AcpSessionChunk {
        session_id: String,
        text: String,
    },
    /// Emitted once a prompt's turn ends. `stop_reason` is the raw ACP
    /// `stopReason` string (`end_turn`, `max_tokens`, `max_turn_requests`,
    /// `refusal`, `cancelled`, or an agent-specific custom reason) — kept as
    /// a plain string rather than a fixed enum because ACP's own
    /// `StopReason` type is `#[non_exhaustive]`.
    AcpSessionFinished {
        session_id: String,
        stop_reason: String,
    },
    /// Emitted when a session ends abnormally: the agent process crashed,
    /// a protocol-level error occurred, or `send_prompt` timed out.
    AcpSessionFailed {
        session_id: String,
        error: String,
    },
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-shared -j4`
Expected: PASS — all tests including the 4 new ones, and every pre-existing test in this crate unaffected.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(shared): add ACP session domain events"
```

---

## Next Plan

[Plan 03: AcpAgentClient + fixture test agent](2026-09-27-acp-transport-plan-03-infra-client.md) — the concrete `rocket-infra` implementation of `AcpSessionClient` (Plan 01); does not directly consume these events (that happens in Plan 04), but is next in the index's numeric order.

## Post-Implementation Review

Before starting the next plan, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review the file this plan modified: `crates/rocket-shared/src/events.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interface — do the four new `DomainEvent`
>    variants match exactly what the plan index's locked interface contract
>    promises Plan 04/05 will consume (field names, types)?
> 2. Code quality and test coverage versus this plan's Review Focus section
>    (exact `session_id` field-name pinning, `stop_reason` round-tripping an
>    arbitrary/unrecognized string, not just a known value).
> 3. Consistency with this crate's existing conventions per
>    `.claude/rules/rust-ddd-boundaries.md` and this crate's own CLAUDE.md —
>    specifically the `#[serde(tag = "type", rename_all = "camelCase")]`
>    enum-level tagging with snake_case struct-variant fields, matching every
>    other event in this enum.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-shared -j4` and confirm it still
> passes. Report what you found and fixed.

Only proceed to the next plan once this review comes back clean (or its fixes are applied and re-verified).
