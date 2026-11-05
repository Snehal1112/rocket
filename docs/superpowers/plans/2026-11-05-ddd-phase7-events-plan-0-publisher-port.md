# DomainEvent split, Plan 0: the PublishedEvent port (D-74 foundation)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every event reaches the bus through a `PublishedEvent` trait object, with `DomainEvent` as its only implementer for now, and the frontend sees the same channels and the same JSON.

**Architecture:** `rocket-shared` gets `PublishedEvent` (channel, JSON, extra channels, written collections, `as_any`). `DomainEvent` implements it in a temporary file, which takes over the 81-arm channel table from `TauriEventBus::channel_for` and the `written_collections` match from `RecentWrites`. `EventPublisher::publish` takes `&dyn PublishedEvent`. The bus and `RecentWrites` use the trait methods, so `channel_for` and the `BranchSwitched`/`BranchMerged` special case disappear. Later plans add one enum per context.

**Tech Stack:** Rust: `rocket-shared`, `rocket-infra`, `rocket-app`, `rocket-acp`, `src-tauri`.

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 1, 4 and step 0 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`.

**Workspace:** run in a git worktree `../rocket-events-p0` on branch `refactor/ddd-phase7-events-plan-0`, cut from the commit that holds this plan. The worktree has its own cold `target` dir (no shared target across worktrees). Do not touch the `rocket-phase5-fixes` worktree.

## Global Constraints

- The wire format is byte-identical: same channels, same JSON, `seq` and `tsMs` still added only by the bus.
- `EventPublisher` stays object-safe; every `Arc<dyn EventPublisher>` and `Box<dyn EventPublisher>` keeps its type. `NullEventPublisher` stays.
- No new event, no renamed channel, no payload field change, no frontend change.
- Spec amendment (ruling made while planning): `PublishedEvent` also has `fn as_any(&self) -> &dyn Any` and `impl dyn PublishedEvent { fn downcast_ref::<T>() }`, because about 35 test recorders store `DomainEvent` and need it back from the trait object. Task 3 writes this into the spec.
- Production code has no `unwrap`; test code may use `expect`. `cargo` commands take `-j4`; never `--workspace`.
- Commits use a pathspec (`git commit --only -m ... -- <paths>`); the commit-gate hook runs `cargo check -j4`, so a commit is made only when the whole tree compiles.

## Review Focus

- A wrong or missing channel for one of the 81 variants (the frontend listener goes silent): pinned by the golden table test in Task 1.
- Branch switch and branch merge must still reach `collection-changed` as well as their own channel; every other event emits on exactly one channel: Task 1 and Task 2 tests.
- Echo suppression: events that name written collections still feed `RecentWrites`, and `FileChanged`, environment and git events still do not: Task 1 test (moved) and the existing echo tests.
- A recorder that receives a non-`DomainEvent` must not panic the publisher path: `downcast_ref` returns `None` (Task 1 test).
- `to_json` equals `serde_json::to_value(&event)` for every variant, so the envelope output does not change: Task 1 test.

---

### Task 1: The port and the DomainEvent implementation (additive)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-shared/src/published_event.rs` (trait, `downcast_ref`, small test enum), `crates/rocket-shared/src/domain_event_port.rs` (`impl PublishedEvent for DomainEvent`, the golden table and `one_of_each()` test support; deleted in the last plan of the series)
- Modify: `crates/rocket-shared/src/lib.rs` (two `pub mod` lines, re-export `PublishedEvent` from `rocket_shared::events` via `events.rs`: `pub use crate::published_event::PublishedEvent;`)

**Interfaces:**
- Produces:
  ```rust
  pub trait PublishedEvent: Send + Sync + 'static {
      fn channel(&self) -> &'static str;
      fn to_json(&self) -> Result<serde_json::Value, serde_json::Error>;
      fn also_emit_on(&self) -> &'static [&'static str] { &[] }
      fn written_collections(&self) -> Vec<&str> { Vec::new() }
      fn as_any(&self) -> &dyn std::any::Any;
  }
  impl dyn PublishedEvent { pub fn downcast_ref<T: 'static>(&self) -> Option<&T>; }
  ```
- `impl PublishedEvent for DomainEvent`: `channel` is the move of the `channel_for` arms in `src-tauri/src/tauri_event_bus.rs`; `to_json` is `serde_json::to_value(self)`; `also_emit_on` returns `&["collection-changed"]` for `BranchSwitched` and `BranchMerged`, else `&[]`; `written_collections` is the move of `written_collections` in `crates/rocket-infra/src/file_watcher/echo.rs`.
- Leaves `EventPublisher` and `channel_for` as they are (removed in Task 2).

- [ ] **Step 1: Write the failing tests.**
  - `published_event.rs` with a local test enum `Probe` implementing the trait: `downcast_ref_returns_the_concrete_event` (`Some` for `Probe`), `downcast_ref_is_none_for_another_type` (`None` for `DomainEvent`), `default_hooks_are_empty` (`also_emit_on()` empty, `written_collections()` empty).
  - `domain_event_port.rs`: a `#[cfg(test)] fn one_of_each() -> Vec<DomainEvent>` with one value of all 81 variants, and these tests: `one_of_each_covers_every_variant` (81 values, 81 distinct `type` tags in `to_json`), `every_variant_keeps_its_channel` (for each value, `channel()` equals the entry in a `const CHANNELS: [(&str, &str); 81]` keyed by the JSON `type` tag), `to_json_equals_serde_to_value` (for every value), `only_branch_events_emit_on_a_second_channel` (`BranchSwitched` and `BranchMerged` give `["collection-changed"]`, all others `[]`), `written_collections_name_the_written_collections` (moved from the echo tests: `RequestSaved` gives its collection, `ItemMoved` gives source and destination, `CollectionRenamed` gives old and new; `FileChanged`, `EnvironmentSaved` and `GitStatusChanged` give none).
  - Build `CHANNELS` before moving anything: write a script that reads the `DomainEvent::X { .. } => "channel"` arms of `channel_for` and prints `(lowerCamelCase(X), "channel")` pairs, then paste the output. The current `channel_for` is the definition of correct.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-shared domain_event_port published_event` — Expected: compile error (red).
- [ ] **Step 3: Implement** the trait and `downcast_ref` in `published_event.rs`, and the `DomainEvent` impl in `domain_event_port.rs` (copy the arms; do not edit `channel_for` yet). Register both modules in `lib.rs`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-shared` — Expected: all pass, including the existing contract tests.
- [ ] **Step 5: Commit** (`feat(shared): add the PublishedEvent port`), with `git commit --only` on the four paths.

### Task 2: Switch the publisher to the port across the workspace (one atomic change)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (`EventPublisher::publish(&self, event: &dyn PublishedEvent)`, `NullEventPublisher`, the object-safety test)
- Modify: `crates/rocket-infra/src/file_watcher/echo.rs` (`RecentWrites::record(&self, event: &dyn PublishedEvent)` calls `event.written_collections()`; delete the local `written_collections`; its tests pass `&DomainEvent`), `crates/rocket-infra/src/file_watcher.rs`, `crates/rocket-infra/src/file_watcher/batch.rs`
- Modify: `src-tauri/src/tauri_event_bus.rs`, `src-tauri/src/commands/openapi_sync.rs`, other `src-tauri` files the compiler names
- Modify: every `rocket-app` and `rocket-acp` file the compiler names (about 30; `grep -rln "EventPublisher\|\.publish(" crates src-tauri --include='*.rs'`)

**Interfaces:**
- Consumes: `PublishedEvent` and `DomainEvent: PublishedEvent` from Task 1.
- Produces: `fn publish(&self, event: &dyn PublishedEvent)` on `EventPublisher`; in the bus, a pure `fn emit_channels(event: &dyn PublishedEvent) -> Vec<&'static str>` (own channel first, then `also_emit_on()`), used by `publish`.

- [ ] **Step 1: Write the failing tests.** In `rocket-shared` `events.rs` tests: `null_publisher_accepts_a_published_event` (`NullEventPublisher.publish(&DomainEvent::CollectionCreated { name: "a".into() })`) and update `_assert_object_safe` to still hold. In `tauri_event_bus.rs` tests: `emit_channels_adds_collection_changed_for_branch_switch_and_merge` (two channels, own channel first) and `emit_channels_is_one_channel_for_other_events` (`RequestSaved` gives `["request-saved"]`-style single entry from its table value). Retarget the existing bus tests that call `channel_for(&e)` to `e.channel()`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-shared null_publisher` — Expected: compile error (red).
- [ ] **Step 3: Change the trait and fix the callers with scripts, then the compiler.**
  - Call sites: `publish(DomainEvent::` and `publish(\n DomainEvent::` become `publish(&DomainEvent::`; a bound variable becomes `publish(&event)`.
  - Implementers that only forward (`self.0.publish(event)`): change the signature only and add the `PublishedEvent` import.
  - Recorders and other implementers that store or match the event: the first line is `let Some(event) = event.downcast_ref::<DomainEvent>() else { return };`, then use `event` as `&DomainEvent` (clone where stored). Do not change what the tests assert.
  - Bus: `publish` calls `writes.record(event)`, takes `event.to_json()` (log and return on error, as today), adds the envelope with the same `add_envelope`, and emits on each channel from `emit_channels`. Delete `channel_for`, the `matches!` special case and the `DomainEvent` import that becomes unused.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets` — Expected: clean, no new warnings. Then `grep -rn "channel_for\|publish(DomainEvent" crates src-tauri --include='*.rs'` — Expected: no output.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-infra file_watcher`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-acp` and `cargo test -j4 -p rocket-app-tauri tauri_event_bus` (use the real `src-tauri` package name from its `Cargo.toml`) — Expected: pass.
- [ ] **Step 6: Commit** in groups once the tree compiles: `refactor(shared): publish through the PublishedEvent port` (shared), `refactor(infra): record writes through the PublishedEvent port` (infra), `refactor(app): publish events by reference` (app, acp), `refactor(tauri): emit through the PublishedEvent port` (src-tauri).

### Task 3: Spec amendment, docs and checks

**Files:**
- Modify: `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (section 1: add `as_any` and `downcast_ref`, and why; section 4: no extra serialization cost, because the bus already built a `serde_json::Value` before), `crates/rocket-shared/CLAUDE.md` (the `EventPublisher` line and a `PublishedEvent` line), `.claude/rules/tauri-ipc-boundaries.md` (wire-format section mentions the port), `docs/roadmaps/ddd-roadmap.md` (D-74 progress note: plan 0 done), the series index status

- [ ] **Step 1: Edit the docs.** Run `cargo clippy -j4 -p rocket-shared -p rocket-infra -p rocket-app -p rocket-acp --all-targets` and clippy for the `src-tauri` package — Expected: no new warnings (the existing "very complex type" warning in `execution_service.rs` is allowed).
- [ ] **Step 2: Compare the wire format by hand once.** Run `cargo test -j4 -p rocket-shared every_variant_keeps_its_channel to_json_equals` — Expected: pass; this plus Task 2's bus tests is the proof that no payload changed.
- [ ] **Step 3: Commit** (`docs: record the PublishedEvent port (D-74 plan 0)`).
