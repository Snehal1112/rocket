# DomainEvent split, Plan 3: FlowEvent and GitEvent (D-74)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The six Flow events and the eleven Git events leave `DomainEvent` and live in `FlowEvent` (`rocket-flow`) and `GitEvent` (`rocket-git`). The frontend sees the same channels and JSON, and a branch switch or merge still also fires `collection-changed`.

**Architecture:** Same pattern as plans 1 and 2: one enum per context implementing `PublishedEvent`, an `events.rs` file in the context crate, the variants, channel arms, golden `CHANNELS` entries and `one_of_each()` entries removed from `DomainEvent` (64 becomes 58, then 47), publishers switched, recorders that now see a new type store it (or use `others()`) instead of panicking. Frozen JSON literals replace any old-versus-new comparison. The Flow payload helper types (`FlowStepTrace`, `FlowLogEntry`, `FlowLiveProgress` and the rest) stay in `rocket-shared`; `FlowEvent` imports them from there.

**Tech Stack:** Rust: `rocket-shared`, `rocket-flow`, `rocket-git`, `rocket-infra`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3, 4 and step 3 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`. Model: `crates/rocket-environment/src/events.rs`, `crates/rocket-workspace/src/events.rs` and `crates/rocket-history/src/events.rs`.

**Workspace:** git worktree `../rocket-events-p3` on branch `refactor/ddd-phase7-events-plan-3`, cut from local `main`. Delete the worktree after the merge. Do not touch other worktrees or the main checkout.

## Global Constraints

- The wire format is byte-identical. Flow tags and channels: `flowRunStarted` on `flow-run-started`, `flowStepStarted` on `flow-step-started`, `flowStepProgress` on `flow-step-progress`, `flowStepCompleted` on `flow-step-completed`, `flowRunFinished` on `flow-run-finished`, `flowRequestPathsRewritten` on `flow-request-paths-rewritten`. All eleven Git events (`gitStatusChanged`, `gitCommit`, `gitPush`, `gitPull`, `branchSwitched`, `branchMerged`, `gitStashChanged`, `gitConflictDetected`, `gitCloned`, `gitRemoteAdded`, `gitRemoteRemoved`) use `git-changed`.
- `GitEvent::also_emit_on()` is `["collection-changed"]` for `BranchSwitched` and `BranchMerged` and empty for the other nine. `FlowEvent` keeps the default.
- `written_collections()` stays at its default (empty) for both enums: Git events never feed `RecentWrites`, and neither do Flow events.
- Each new enum keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, snake_case field names, types and every field attribute (`skip_serializing_if`, `default`, `Box`) as today. Nested helper DTOs stay camelCase and stay where they are.
- Ruling carried from plan 1: frozen JSON literals captured from the old `DomainEvent` before the variants are removed, one per variant. For variants with optional or skipped fields (`FlowRunStarted`, `FlowStepProgress`, `FlowStepCompleted`, `FlowRunFinished`, `FlowRequestPathsRewritten`) capture a minimal form (optionals absent) and a populated form (optionals set).
- `rocket-flow` moves `serde_json` from `[dev-dependencies]` to `[dependencies]` (it needs it for `to_json`). `rocket-git` already has it. No new crate dependency edge: both crates already depend on `rocket-shared`.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`. `rustfmt --edition 2021` only on leaf files, never on `lib.rs` or `mod.rs`; reorder `pub use` lines by hand. Commits use a pathspec and only when the whole tree compiles; a task that cannot compile in groups is one commit. No Python for scripting: use `rg`, `sed` or `perl` for bulk edits.
- Test recorders (every `downcast_ref::<DomainEvent>` site, including fully qualified paths) panic on events they do not know. A recorder that must now also see a `FlowEvent` or `GitEvent` downcasts that type too (a second list, or a combined list of strings) and still panics on anything else.

## Review Focus

- All Git events share `git-changed`, and only `BranchSwitched` and `BranchMerged` add `collection-changed`; a missed or extra extra-channel breaks the sidebar refresh or double-fires it: Task 2 tests `git_events_use_one_channel`, `only_branch_events_also_emit_on_collection_changed`, and the bus test that a switch emits `[git-changed, collection-changed]`.
- Every Flow channel keeps its own name; a typo silences a flow-run listener: Task 1 channel and frozen-literal tests.
- Optional and skipped fields of the Flow events keep their serialization (absent versus `null`, boxed values): Task 1 minimal and populated literals.
- `RecentWrites` still ignores Git events: the existing echo test keeps passing with `GitEvent`: Task 2.
- A recorder that sees a Flow or Git event does not panic or lose it; one that sees any other foreign type still panics. Recorders whose publish happens in a spawned task can swallow a panic: Flow runs publish from a task, so the flow recorders must store `FlowEvent` and be asserted on content, not only on absence of a panic: Task 1.

---

### Task 1: FlowEvent

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-flow/src/events.rs`
- Modify: `crates/rocket-flow/Cargo.toml` (`serde_json.workspace = true` under `[dependencies]`, remove it from `[dev-dependencies]`), `crates/rocket-flow/src/lib.rs` (module and re-export `FlowEvent`, by hand in rustfmt order), `crates/rocket-shared/src/events.rs` (remove the six variants and their contract-test uses; the Flow helper DTOs stay), `crates/rocket-shared/src/domain_event_port.rs` (arms, `CHANNELS` entries, `one_of_each()` entries; 64 to 58)
- Modify (publishers and tests): `crates/rocket-app/src/flow_execution_service.rs`, `crates/rocket-app/src/collection_service.rs` (publishes `FlowRequestPathsRewritten`; two recorders), `crates/rocket-app/src/flow_partial_run_tests.rs`, `crates/rocket-app/src/flow_request_ref_e2e_tests.rs`, `src-tauri/src/tauri_event_bus.rs` (tests). Find every user with `grep -rn "DomainEvent::Flow" crates src-tauri --include='*.rs'`.

**Interfaces:**
- Produces: `pub enum FlowEvent { FlowRunStarted{..}, FlowStepStarted{..}, FlowStepProgress{..}, FlowStepCompleted{..}, FlowRunFinished{..}, FlowRequestPathsRewritten{..} }` with exactly the field lists, types and attributes of the six `DomainEvent` variants today, implementing `PublishedEvent` (channels in Global Constraints), re-exported as `rocket_flow::FlowEvent`.
- Consumes: `PublishedEvent`, the Flow helper DTOs from `rocket_shared::events`, `RecordingPublisher::others()` (rocket-app test doubles).

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removing the variants, serialize each with the old `DomainEvent` (minimal and populated forms, see Global Constraints) and paste `json!` literals. In `events.rs`: `flow_events_json_are_unchanged`, `flow_events_keep_their_channels` (the six names), `flow_events_have_no_extra_channels_or_written_collections`. Bus test: `emit_channels(&FlowEvent::...)` is the single expected channel. Move the existing Flow contract tests from `rocket-shared/src/events.rs` into this file with `FlowEvent`. Update the recorders in `flow_execution_service.rs` (three), `collection_service.rs` (two) and the two flow test files to store `FlowEvent` and keep expected strings and counts.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-flow events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum; remove the variants, arms and golden entries from `rocket-shared`; switch the publishers to `&FlowEvent::...`; fix every recorder the compiler or a test run flags, following the recorder rule in Global Constraints.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-flow`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; `one_of_each_covers_every_variant` counts 58. `grep -rn "DomainEvent::Flow" crates src-tauri --include='*.rs'` prints nothing.
- [ ] **Step 5: Commit** (one commit if the tree cannot compile in groups): `feat(flow): add the FlowEvent enum` for the enum and shared removal, `refactor: publish flow events as FlowEvent` for the rest.

### Task 2: GitEvent

**Files:**
- Create: `crates/rocket-git/src/events.rs`
- Modify: `crates/rocket-git/src/lib.rs` (module and re-export `GitEvent`), `crates/rocket-shared/src/events.rs` (remove the eleven variants and their contract tests, which move), `crates/rocket-shared/src/domain_event_port.rs` (eleven arms in `channel()`, the `also_emit_on` arm for the two branch events, `CHANNELS` entries, `one_of_each()` entries; 58 to 47), `crates/rocket-app/src/git_service.rs` (publishes `GitEvent`; its recorders), `crates/rocket-infra/src/file_watcher/echo.rs` (the `GitStatusChanged` ignore test uses `GitEvent`), `src-tauri/src/tauri_event_bus.rs` (the branch switch and merge tests build `GitEvent`). Find every user with `grep -rnE "DomainEvent::(Git|Branch)" crates src-tauri --include='*.rs'`.

**Interfaces:**
- Produces: `pub enum GitEvent { GitStatusChanged{..}, GitCommit{..}, GitPush{..}, GitPull{..}, BranchSwitched{..}, BranchMerged{..}, GitStashChanged{..}, GitConflictDetected{..}, GitCloned{..}, GitRemoteAdded{..}, GitRemoteRemoved{..} }` with exactly the fields of the eleven `DomainEvent` variants today, implementing `PublishedEvent`: `channel()` is `"git-changed"` for all, `also_emit_on()` is `&["collection-changed"]` for `BranchSwitched` and `BranchMerged` only; re-exported as `rocket_git::GitEvent`.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removal, serialize one value per variant with the old `DomainEvent` and paste `json!` literals. In `events.rs`: `git_events_json_are_unchanged` (all eleven), `git_events_use_one_channel` (`git-changed`), `only_branch_events_also_emit_on_collection_changed` (two events return `["collection-changed"]`, the other nine return empty), `git_events_have_no_written_collections`. Move the existing Git contract tests from `rocket-shared/src/events.rs`. Bus tests: a `BranchSwitched` and a `BranchMerged` give `emit_channels` `["git-changed", "collection-changed"]`; a `GitCommit` gives `["git-changed"]`. Update the `git_service.rs` recorder tests to the new type with unchanged expected strings.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-git events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum; remove variants, arms and golden entries from `rocket-shared`; switch the publish sites in `git_service.rs`; fix flagged recorders.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-git`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app git`, `cargo test -j4 -p rocket-infra file_watcher` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; the shared count is 47. `grep -rnE "DomainEvent::(Git|Branch)" crates src-tauri --include='*.rs'` prints nothing.
- [ ] **Step 5: Commit** `feat(git): add the GitEvent enum` and `refactor(app): publish git changes as GitEvent`, or one commit if the groups do not compile separately.

### Task 3: Plan 2 follow-ups, docs and checks

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs` (the `EventNames` recorder gets an explicit `WorkspaceCreated { id, .. } => format!("created:{id}")` arm and loses the `_ => return` arm; add a test that a created event is recorded), `crates/rocket-workspace/CLAUDE.md` (the event fields stay snake_case on purpose), `crates/rocket-shared/CLAUDE.md` (the `DomainEvent` description and the "moved out" list: 47 variants left; fix "Implemented by `TauriEventBus` in `rocket-infra`" to `src-tauri/src/tauri_event_bus.rs`), `crates/rocket-flow/CLAUDE.md` and `crates/rocket-git/CLAUDE.md` (the new `events` module), `docs/roadmaps/ddd-roadmap.md` (D-74 progress: plan 2 is merged, plan 3 done, 47 variants left; D-74 and D-71 stay open), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 2 "done, merged", plan 3 file and "done, unmerged")

- [ ] **Step 1: Write the failing test** `workspace_created_is_recorded` in `workspace_service.rs`: creating a workspace through the service makes the recorder list `created:<id>`. Run `cargo test -j4 -p rocket-app workspace_created_is_recorded` — Expected: FAIL (the event is dropped today).
- [ ] **Step 2: Fix the recorder and edit the docs** as listed. Run `cargo test -j4 -p rocket-app workspace` — Expected: pass.
- [ ] **Step 3: Run** `cargo clippy -j4 -p rocket-shared -p rocket-flow -p rocket-git -p rocket-infra -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). Run `cargo fmt --check -p rocket-flow -p rocket-git` — Expected: no diff in the files this plan touched.
- [ ] **Step 4: Commit** `fix(app): record WorkspaceCreated in the test recorder` and `docs: record DomainEvent split plan 3 (D-74)`.
