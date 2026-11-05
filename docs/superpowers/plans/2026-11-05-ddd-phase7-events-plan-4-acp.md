# DomainEvent split, Plan 4: AcpEvent and the acp.rs types (D-74, D-71)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The 13 ACP session events and the 3 assistant conversation events leave `DomainEvent` and live in `AcpEvent` (`rocket-acp`). The four option types in `rocket-shared/src/acp.rs` move into `rocket-acp`, which finishes D-71. The frontend sees the same channels and JSON.

**Architecture:** Same pattern as plans 1 to 3: one enum implementing `PublishedEvent` in an `events.rs` of the context crate, with variants, channel arms, golden `CHANNELS` entries and `one_of_each()` entries removed from `DomainEvent` (47 becomes 31), publishers switched, recorders that now see the new type store it or use `others()`. The type move is not separable: `ConfigOption` and `PermissionOption` are fields of two variants, and `rocket-shared` cannot depend on `rocket-acp`, so the variants and the types must leave `rocket-shared` in the same commit. The type move is cheap because `rocket-acp` already re-exports all four types and every other crate imports them through `rocket_acp`.

**Tech Stack:** Rust: `rocket-shared`, `rocket-acp`, `rocket-app`, `rocket-infra`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3, 4 and step 4 of section 5; the D-71 ruling in section 3). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`. Model: `crates/rocket-flow/src/events.rs`, `crates/rocket-git/src/events.rs`, `crates/rocket-workspace/src/events.rs`.

**Workspace:** git worktree `../rocket-events-p4` on branch `refactor/ddd-phase7-events-plan-4`, cut from local `main`. Delete the worktree after the merge. Do not touch other worktrees or the main checkout.

## Global Constraints

- The wire format is byte-identical. Tags and channels:

| Tag | Channel |
|---|---|
| `acpSessionStarted` | `agent-session-started` |
| `acpSessionChunk` | `agent-session-chunk` |
| `acpThoughtChunk` | `agent-session-thought` |
| `acpSessionFinished` | `agent-session-finished` |
| `acpSessionFailed` | `agent-session-failed` |
| `acpToolInvoked` | `agent-tool-invoked` |
| `acpProposalCreated` | `agent-proposal-created` |
| `acpProposalResolved` | `agent-proposal-resolved` |
| `acpToolActivity` | `agent-session-tool-activity` |
| `acpConfigOptionsChanged` | `agent-session-config-options` |
| `acpUsage` | `agent-session-usage` |
| `acpPermissionRequested` | `agent-permission-requested` |
| `acpPermissionResolved` | `agent-permission-resolved` |
| `assistantConversationSaved` | `assistant-conversation-saved` |
| `assistantConversationDeleted` | `assistant-conversation-deleted` |
| `assistantConversationsCleared` | `assistant-conversations-cleared` |

- `AcpEvent` keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, snake_case field names (`session_id`, `stop_reason`, `call_id`, `request_id`, `option_id`, `cost_usd`, `proposal_id`, `conversation_id`) and types. `AssistantConversationsCleared` stays a unit variant (JSON is just `{"type":"assistantConversationsCleared"}`). `AcpUsage.cost_usd` is `Option<f64>`, so `AcpEvent` cannot derive `Eq` (derive `PartialEq` only). `AcpPermissionResolved.option_id` is `Option<String>` and serializes `null` when `None`.
- The four moved types (`ConfigOption`, `ConfigChoice`, `PermissionOption`, `PermissionOptionKind`) keep their exact serde attributes, derives and JSON (the nested keys are camelCase today; read them in `crates/rocket-shared/src/acp.rs` and do not change them). They move by `git mv` so history follows, to `crates/rocket-acp/src/options.rs`; the module doc comment changes because it no longer needs to explain why the types sit in `rocket-shared`.
- `also_emit_on` and `written_collections` stay at their defaults for `AcpEvent`.
- Ruling carried from plan 1: frozen JSON literals captured from the old `DomainEvent` before the variants are removed, one per variant. `AcpConfigOptionsChanged` and `AcpPermissionRequested` get a populated literal that includes a nested option (and `AcpUsage` one with `cost_usd` set and one with `None`; `AcpPermissionResolved` one with `Some` and one with `None`).
- No new crate dependency edge: `rocket-acp` already depends on `rocket-shared` and `serde_json`; `rocket-app`, `rocket-infra` and `src-tauri` already depend on `rocket-acp`. `rocket-shared` must end with no mention of `acp` in `lib.rs`.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`; long runs go to a log file or the background. `rustfmt --edition 2021` only on leaf files, never on `lib.rs` or `mod.rs`; reorder `pub use` lines by hand. Commits use a pathspec and only when the whole tree compiles. No Python for scripting: use `rg`, `sed` or `perl` for bulk edits (for example `perl -pi -e 's/DomainEvent::(Acp|AssistantConversation)/AcpEvent::$1/g'` per file, then fix imports by hand).
- Test recorders (every `downcast_ref::<DomainEvent>` site, including fully qualified paths and ones in `src-tauri`) panic on events they do not know. A recorder that must now see an `AcpEvent` downcasts that type too (or uses `RecordingPublisher::others()`, or a new typed `acp_events()` accessor next to `flow_events()` and `git_events()` in `crates/rocket-app/src/test_doubles.rs` if the tests read them back) and still panics on anything else. Some ACP events are published from spawned tasks, where a panic is swallowed, so tests assert on stored content.

## Review Focus

- A typo in one of the 16 channels silences a frontend listener (the `agent-*` names are not derived from the tags): channel test plus frozen literals, Task 1.
- `AssistantConversationsCleared` is a unit variant with a tag: JSON must be exactly `{"type":"assistantConversationsCleared"}`: Task 1.
- Nested option JSON (`options` arrays) must be unchanged, including camelCase keys inside `ConfigOption` and `PermissionOption`: Task 1 populated literals.
- `rocket-shared` must not depend on `rocket-acp` (cycle) and must not keep a dead `acp` module: Task 1 checks `cargo tree -p rocket-shared -e normal --depth 1` shows no `rocket-acp`, and `rg "acp" crates/rocket-shared/src/lib.rs` is empty.
- The ACP session service publishes from a streaming task: its recorder must store `AcpEvent` values and tests must assert on the stored list, not only on absence of a panic: Task 1.
- Closing D-71 must not claim D-74 is done: Task 2 keeps D-74 open.

---

### Task 1: AcpEvent and the acp.rs move

The type move and the variant move cannot compile apart, so this is one code task and normally one commit (`feat(acp): add AcpEvent and move the option types into rocket-acp`, plus a second `refactor: publish ACP events as AcpEvent` if the group split compiles).

**Files:**
- Create: `crates/rocket-acp/src/events.rs` (the enum and its tests)
- Move: `crates/rocket-shared/src/acp.rs` to `crates/rocket-acp/src/options.rs` (use `git mv`; update the module doc)
- Modify: `crates/rocket-acp/src/lib.rs` (`mod events; mod options;`, `pub use events::AcpEvent;`, replace `pub use rocket_shared::acp::{PermissionOption, PermissionOptionKind};` with `pub use options::{...}`, by hand in rustfmt order), `crates/rocket-acp/src/session_info.rs` (its `pub use rocket_shared::acp::{ConfigChoice, ConfigOption};` becomes `pub use crate::options::{ConfigChoice, ConfigOption};`), `crates/rocket-shared/src/lib.rs` (drop `pub mod acp;`), `crates/rocket-shared/src/events.rs` (remove the 16 variants, the `use crate::acp::...` import, and their contract tests, which move), `crates/rocket-shared/src/domain_event_port.rs` (16 arms, `CHANNELS` entries, `one_of_each()` entries; 47 to 31)
- Modify (publishers and tests; find them with `rg -c "DomainEvent::(Acp|AssistantConversation)" crates src-tauri -g '*.rs'`): `crates/rocket-app/src/acp_session_service.rs` (about 55 uses, one `downcast_ref::<DomainEvent>` recorder), `crates/rocket-app/src/mcp_tool_service.rs`, `crates/rocket-app/src/proposal_service.rs`, `crates/rocket-app/src/conversation_service.rs`, `crates/rocket-app/src/test_doubles.rs` (typed accessor if needed), `crates/rocket-infra/src/acp_agent_client.rs`, `src-tauri/src/mcp/tool_server.rs`, `src-tauri/src/tauri_event_bus.rs` (tests)

**Interfaces:**
- Produces: `pub enum AcpEvent` with exactly the 16 variants and field lists of the `DomainEvent` variants today (see the table and field rules in Global Constraints), `impl PublishedEvent for AcpEvent` (`channel()` per the table; `to_json` via `serde_json::to_value(self)`; defaults for the rest), re-exported as `rocket_acp::AcpEvent`. `rocket_acp::{ConfigOption, ConfigChoice, PermissionOption, PermissionOptionKind}` keep their paths for every consumer.
- Consumes: `rocket_shared::published_event::PublishedEvent` (check the exact path in `crates/rocket-flow/src/events.rs`), `RecordingPublisher::others()`.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removing anything, serialize all 16 variants with the old `DomainEvent` (the populated and `None` forms listed in Global Constraints) and paste `json!` literals into `events.rs`. Tests: `acp_events_json_are_unchanged` (all variants), `acp_events_keep_their_channels` (the 16 names from the table), `assistant_conversations_cleared_is_a_bare_tagged_object`, `acp_events_have_no_extra_channels_or_written_collections`. Move the existing ACP contract tests from `rocket-shared/src/events.rs` (the ones around the `AcpSessionStarted`, `AcpToolActivity`, `AcpConfigOptionsChanged`, `AcpPermissionRequested` and `AssistantConversationSaved` cases) into `events.rs` with `AcpEvent`, keeping their assertions. Bus test in `tauri_event_bus.rs`: `emit_channels(&AcpEvent::AcpSessionStarted{..})` is the single channel `agent-session-started`, and `AssistantConversationsCleared` gives `assistant-conversations-cleared`. Update the recorder tests in `acp_session_service.rs`, `mcp_tool_service.rs`, `proposal_service.rs` and `conversation_service.rs` to read `AcpEvent` values with unchanged expected strings and counts.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-acp events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** `git mv` the types; add the enum; remove the variants, arms, golden entries, the `acp` module and the import from `rocket-shared`; switch the publishers to `&AcpEvent::...`; fix every recorder the compiler or a test run flags, following the recorder rule in Global Constraints.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-acp`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-infra acp` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; `one_of_each_covers_every_variant` counts 31. `rg "DomainEvent::(Acp|AssistantConversation)" crates src-tauri -g '*.rs'` prints nothing; `rg "rocket_shared::acp" crates src-tauri -g '*.rs'` prints nothing; `cargo tree -p rocket-shared -e normal --depth 1` shows no `rocket-acp`.
- [ ] **Step 5: Commit** with a pathspec (one commit if the tree cannot compile in groups).

### Task 2: Docs, D-71 closure and checks

**Files:**
- Modify: `crates/rocket-shared/CLAUDE.md` (remove the `acp` module entry, the `DomainEvent` description and "moved out" list: 31 variants left; no ACP, gRPC or load test types remain), `crates/rocket-acp/CLAUDE.md` (the new `events.rs` and `options.rs` rows; `session_info.rs` row no longer says `rocket_shared::acp`), `crates/rocket-git/CLAUDE.md` (the `events` row says `GitService` in `rocket-app` publishes it; the publisher is `GitAppService`), `crates/rocket-app/CLAUDE.md` (any claim that ACP, proposal, conversation or MCP tool services publish `DomainEvent` becomes `AcpEvent`; verify each with `rg` first), `docs/roadmaps/ddd-roadmap.md` (D-71 row: all three groups moved, status done; D-74 progress: plan 3 is merged, plan 4 done and unmerged, 31 variants left; D-74 stays open), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 3 "done, merged", plan 4 file and "done, unmerged")

- [ ] **Step 1: Edit the docs.** Also check `.claude/rules/tauri-ipc-boundaries.md` still reads correctly.
- [ ] **Step 2: Run** `cargo clippy -j4 -p rocket-shared -p rocket-acp -p rocket-infra -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). Run `cargo fmt --check -p rocket-acp` — Expected: no diff in the files this plan touched.
- [ ] **Step 3: Commit** `docs: record DomainEvent split plan 4 and close D-71`.
