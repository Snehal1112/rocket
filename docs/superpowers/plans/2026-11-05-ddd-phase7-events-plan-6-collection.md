# DomainEvent split, Plan 6: CollectionEvent, RecentWrites and the file watcher (D-74)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The last 17 variants leave `DomainEvent` and live in `CollectionEvent` in `rocket-collection`. `DomainEvent` is then empty, so this plan deletes it with `domain_event_port.rs`. The frontend sees the same channels and JSON, and `RecentWrites` (the file watcher echo guard) records the same collections.

**Architecture:** Same pattern as plans 1 to 5: one enum implementing `PublishedEvent`, here in `crates/rocket-collection/src/events.rs` (the spec's ownership table). Unlike earlier plans, `CollectionEvent` overrides `written_collections()`, and its publishers include `rocket-infra` (the file watcher), which already depends on `rocket-collection`. Task 1 adds the enum next to the old variants, so a temporary test can compare old and new JSON directly. Task 2 switches every publisher and recorder, then deletes `DomainEvent`. Task 3 updates the docs and re-scopes plan 7.

**Tech Stack:** Rust: `rocket-shared`, `rocket-collection`, `rocket-app`, `rocket-infra`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3, 4, 6 and steps 6 and 7 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`. Models: `crates/rocket-app/src/execution_events.rs` (plan 5), `crates/rocket-acp/src/events.rs` (private module plus `pub use`).

**Workspace:** git worktree `../rocket-events-p6` on branch `refactor/ddd-phase7-events-plan-6`, cut from local `main`. Delete the worktree after the merge. Do not touch other worktrees or the main checkout.

## Global Constraints

- The wire format is byte-identical. Tags and channels (today's `domain_event_port.rs`):

| Tag | Channel |
|---|---|
| `collectionCreated`, `collectionDeleted`, `collectionRenamed`, `requestSaved`, `requestDeleted`, `itemMoved`, `folderCreated`, `folderDeleted`, `itemsReordered`, `collectionSettingsSaved`, `folderVariablesSaved`, `folderSettingsSaved`, `requestVariablesSaved`, `collectionVariableWritten`, `fileChanged` | `collection-changed` |
| `collectionTrustChanged` | `collection-trust-changed` |
| `fileWatcherFailed` | `file-watcher-failed` |

- `written_collections()` keeps today's mapping exactly: `CollectionCreated` and `CollectionDeleted` give `[name]`; `CollectionRenamed` gives `[old_name, new_name]`; `ItemMoved` gives `[src_collection, dst_collection]`; `RequestSaved`, `RequestDeleted`, `FolderCreated`, `FolderDeleted`, `ItemsReordered`, `CollectionSettingsSaved`, `FolderVariablesSaved`, `FolderSettingsSaved`, `RequestVariablesSaved` and `CollectionVariableWritten` give `[collection]`. `CollectionTrustChanged`, `FileChanged` and `FileWatcherFailed` give nothing (the trust change is not a file write, and a watcher event must never mark its own collection as an echo). `also_emit_on` stays at its default.
- `CollectionEvent` keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, snake_case field names, types, doc comments **and field declaration order** as `DomainEvent` today. `FileChanged.collection` is `Option<String>` with no `skip_serializing_if`, so `None` serializes as `null`. Keep that.
- `FileChangeKind` stays in `rocket-shared/src/events.rs`, because `rocket-import` uses it too (the spec's several-contexts rule). This plan adds `PartialEq, Eq` to its derives so `CollectionEvent` can derive `Debug, Clone, Serialize, PartialEq, Eq`. That is additive and does not change its JSON.
- Ruling carried from plan 1: frozen JSON literals captured from the old `DomainEvent`, one per variant, plus a `None` form of `FileChanged.collection`. While `DomainEvent` still exists (Task 1), a temporary test also compares old and new serialization variant by variant. Task 2 deletes that test with `DomainEvent`.
- Ruling for this plan: `DomainEvent` is deleted in Task 2, not in plan 7, because it has no variants left and an empty enum only keeps dead recorder arms alive. Plan 7 keeps the move of the helper types that stay in `rocket-shared/src/events.rs` (the Flow DTOs, the four WebSocket and GraphQL payload enums) and the final docs.
- `rocket-collection` exposes the enum as `mod events;` plus `pub use events::CollectionEvent;`, both by hand in `lib.rs` in alphabetical and rustfmt order. Never run rustfmt on `lib.rs` or `mod.rs`; use `rustfmt --edition 2021` on leaf files only.
- No new crate dependency edge: `rocket-collection` already depends on `rocket-shared` and `serde_json`; `rocket-app`, `rocket-infra` and `src-tauri` already depend on `rocket-collection`. Confirm with `cargo tree -p rocket-collection -e normal --depth 1` that nothing new appears.
- `RecordingPublisher` (`crates/rocket-app/src/test_doubles.rs`) loses its `DomainEvent` store. `events()` is renamed `collection_events()` and returns `Vec<CollectionEvent>`. The rename is on purpose: every caller must change, so no `events().is_empty()` check can silently pass on events that now go to `others()`.
- Every test recorder that downcasts `DomainEvent` today (`collection_service.rs` around 1176 and 2375, `execution_service.rs` around 8039 and 14427, `openapi_sync_lock_tests.rs:48`, `rocket-infra/src/file_watcher.rs:259`, `test_doubles.rs`) downcasts `CollectionEvent` instead and still panics on types it does not expect. A recorder that ignored `DomainEvent` before ignores `CollectionEvent` now. The file watcher publishes from its own thread, so its tests assert on stored content.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`; long runs go to a log file. Run `cargo fmt --check -p rocket-shared -p rocket-collection -p rocket-app -p rocket-infra -p rocket` before every commit. Commits use a pathspec and only when the whole tree compiles. Bulk edits use `rg`, `sed` or `perl` (for example `perl -pi -e 's/DomainEvent::/CollectionEvent::/g'` per file), then fix imports by hand.

## Review Focus

- A dropped or wrong `written_collections()` entry: the watcher would echo an in-app save as an outside change, or hide a real outside change. Pinned per variant in Task 1 and by the `RecentWrites` and bus echo tests in Task 2.
- A channel typo silences the sidebar refresh (`collection-changed`) or the trust banner (`collection-trust-changed`): channel table test in Task 1, bus test in Task 2.
- `FileChanged` with `collection: None` must still serialize `"collection": null`: frozen literal in Task 1.
- A test that read `publisher.events()` and now sees nothing: the rename to `collection_events()` makes every caller a compile error, Task 2.
- Leftover references to `DomainEvent` in live code, tests or docs after the deletion: `rg` checks in Tasks 2 and 3.

---

### Task 1: CollectionEvent

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/events.rs`
- Modify: `crates/rocket-collection/src/lib.rs` (`mod events;`, `pub use events::CollectionEvent;`), `crates/rocket-shared/src/events.rs` (`FileChangeKind` derives `PartialEq, Eq`; nothing else)

**Interfaces:**
- Produces: `pub enum CollectionEvent` with exactly the 17 variants and field lists of `DomainEvent` today, `impl PublishedEvent for CollectionEvent` (`channel()` per the table, `to_json` via `serde_json::to_value(self)`, `written_collections()` per Global Constraints, `as_any`), at `rocket_collection::CollectionEvent`.
- Consumes: `rocket_shared::events::{FileChangeKind, PublishedEvent}`.

- [ ] **Step 1: Write the failing tests** in `events.rs`:
  - `collection_events_json_are_unchanged`: one `json!` literal per variant, plus `FileChanged` with `collection: None` (`"collection": null`) and with `Some`. Capture each literal by serializing the old `DomainEvent` value.
  - `collection_events_match_domain_event_json` (temporary, Task 2 deletes it): for each case, `serde_json::to_string(&CollectionEvent::X {..})` equals `serde_json::to_string(&DomainEvent::X {..})` as an exact string, which also pins field order.
  - `collection_events_keep_their_channels`: the set of (tag, channel) pairs equals the table above.
  - `written_collections_match_the_old_mapping`: every variant against the Global Constraints mapping, including the three empty ones.
  - `collection_events_emit_on_one_channel_only`: `also_emit_on()` is empty for every variant.
  - Move the contract tests that exist for these variants in `rocket-shared/src/events.rs` (the `*_wire_shape` tests, `collection_variable_written_serializes`, `wire_contract_collection_and_git_events`, `wire_contract_envelope_keys_are_free`, `wire_contract_watcher_events`) as `CollectionEvent` copies with the same assertions. Task 2 deletes the originals.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection events` — Expected: compile error (red).
- [ ] **Step 3: Implement** the enum, its `PublishedEvent` impl and the `lib.rs` lines; add the `FileChangeKind` derives.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-shared`, `cargo check -j4 --all-targets` — Expected: pass; `DomainEvent` is untouched and still has 17 variants. `cargo tree -p rocket-collection -e normal --depth 1` shows no new crate. fmt check clean.
- [ ] **Step 5: Commit** `feat(collection): add CollectionEvent` with a pathspec.

### Task 2: Publish CollectionEvent and delete DomainEvent

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files** (find every user with `rg -n "DomainEvent" crates src-tauri -g '*.rs'`):
- Publishers: `crates/rocket-app/src/collection_service.rs` (about 49 uses, two recorders), `collection_trust.rs`, `workspace_watcher.rs`, `execution_service.rs` (`CollectionVariableWritten` at about 2179 and 2207, two recorders), `crates/rocket-infra/src/file_watcher.rs`, `file_watcher/batch.rs` (`take` and `refresh_all_event` return `CollectionEvent`), `file_watcher/echo.rs` (tests).
- Tests and doubles: `crates/rocket-app/src/test_doubles.rs`, `proposal_service.rs`, `collection_service_examples_tests.rs`, `duplicate_request_e2e_tests.rs`, `history_service.rs`, `mcp_tool_service.rs`, `flow_execution_service.rs`, `openapi_sync_lock_tests.rs`, `src-tauri/src/tauri_event_bus.rs`.
- Delete: `crates/rocket-shared/src/domain_event_port.rs`, its `pub mod` line in `crates/rocket-shared/src/lib.rs` (by hand), the `DomainEvent` enum and its tests in `crates/rocket-shared/src/events.rs`, and the temporary comparison test from Task 1.
- Modify: `crates/rocket-shared/src/published_event.rs` (`downcast_ref_is_none_for_another_type` uses a second local probe type instead of `DomainEvent`), the `NullEventPublisher` tests in `events.rs` (publish a local probe event).

**Interfaces:**
- Consumes: `rocket_collection::CollectionEvent` (Task 1).
- Produces: `RecordingPublisher::collection_events(&self) -> Vec<CollectionEvent>` (replaces `events()`); `others()` doc comment lists `CollectionEvent` instead of `DomainEvent`.

- [ ] **Step 1: Update the tests first**: every `publisher.events()` call becomes `collection_events()` with the same expected values and counts; recorders downcast `CollectionEvent`; the `RecentWrites` tests in `echo.rs` record `CollectionEvent` values with the same expectations; the bus tests in `tauri_event_bus.rs` use `CollectionEvent::RequestSaved` and `FileWatcherFailed` and expect the same channels, and the echo test still sees the collection recorded.
- [ ] **Step 2: Run** `cargo check -j4 --all-targets` — Expected: compile errors (red) at the publishers.
- [ ] **Step 3: Implement**: switch every publisher to `&CollectionEvent::...`, then delete `DomainEvent`, `domain_event_port.rs` and the temporary test.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-infra file_watcher`, `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass. `rg -n "\bDomainEvent\b" crates src-tauri -g '*.rs'` lists only comments that describe history (for example "captured from the old `DomainEvent`"), no code. fmt check clean.
- [ ] **Step 5: Commit** `refactor: publish CollectionEvent and remove DomainEvent` with a pathspec.

### Task 3: Docs, plan 7 scope and checks

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/CLAUDE.md` (no `DomainEvent`; the wire-format section points at the per-context enums and their tests; `FileChangeKind`, the Flow DTOs and the WebSocket/GraphQL payload enums are what is left in `events.rs`), `crates/rocket-collection/CLAUDE.md` (the `events` module and `written_collections`), `crates/rocket-app/CLAUDE.md` (the fire-and-forget line and `RecordingPublisher::collection_events()`), `crates/rocket-infra/CLAUDE.md` if it names `DomainEvent`, root `CLAUDE.md` (the "`DomainEvent` published fire-and-forget" line), `.claude/rules/tauri-ipc-boundaries.md` (the "DomainEvent Wire Format" section: per-context enums, contract tests in each enum's `events.rs`), `docs/roadmaps/ddd-roadmap.md` (D-74 progress: plan 5 merged, plan 6 done and unmerged, `DomainEvent` deleted, plan 7 left; D-74 stays open), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 5 "done, merged"; plan 6 file and "done, unmerged"; plan 7 scope becomes "move the helper types left in `rocket-shared/src/events.rs` to their contexts, final docs")
- Modify comments that describe current code and still name `DomainEvent` (for example `crates/rocket-acp/src/session.rs:8`, and the payload comments in `src/lib/tauri-api.ts` around lines 3351, 3586 and 3669, which become "like every backend event"; comment-only, then `bun run tsc --noEmit` and `bun run check`), found with `rg -n "DomainEvent" crates src-tauri src .claude CLAUDE.md -g '!docs/superpowers/**'`. Comments about history stay.

- [ ] **Step 1: Edit the docs.** Verify every claim with `rg` first.
- [ ] **Step 2: Run** `cargo clippy -j4 -p rocket-shared -p rocket-collection -p rocket-app -p rocket-infra --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). fmt check — Expected: no diff.
- [ ] **Step 3: Commit** `docs: record DomainEvent split plan 6 (D-74)`.
