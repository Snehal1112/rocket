# DomainEvent split, Plan 2: EnvironmentEvent and WorkspaceEvent (D-74)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `EnvironmentSaved`, `EnvironmentDeleted` and the eight workspace events leave `DomainEvent` and live in `EnvironmentEvent` (`rocket-environment`) and `WorkspaceEvent` (`rocket-workspace`). The frontend sees the same channels and JSON.

**Architecture:** Same pattern as plan 1 (`HistoryEvent`, `GrpcEvent`): one enum per context implementing `PublishedEvent`, an `events.rs` file in the context crate, the variants, their channel arms, golden `CHANNELS` entries and `one_of_each()` entries removed from `DomainEvent` (74 becomes 72, then 64), publishers switched, and each test recorder that now sees a new type stores it (or keeps it in a second list) instead of panicking. Frozen JSON literals replace any old-versus-new comparison.

**Tech Stack:** Rust: `rocket-shared`, `rocket-environment`, `rocket-workspace`, `rocket-infra`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3 and step 2 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`. Model: `crates/rocket-history/src/events.rs` and `crates/rocket-grpc/src/events.rs`.

**Workspace:** git worktree `../rocket-events-p2` on branch `refactor/ddd-phase7-events-plan-2`, cut from local `main`. Delete the worktree after the merge. Do not touch other worktrees or the main checkout.

## Global Constraints

- The wire format is byte-identical. Tags and channels: `environmentSaved` and `environmentDeleted` both on `environment-changed`; `workspaceCreated` on `workspace-created`, `workspaceSwitched` on `workspace-switched`, `workspaceRenamed` on `workspace-renamed`, `workspaceClosed` on `workspace-closed`, `workspaceDeleted` on `workspace-deleted`, `workspacePinned` on `workspace-pinned`, `workspaceUnpinned` on `workspace-unpinned`, `workspaceDescriptionUpdated` on `workspace-description-updated`.
- Each new enum keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, field names (snake_case: `old_name`, `new_name`) and types as today; `WorkspaceDescriptionUpdated.description` stays `Option<String>` (serializes as `null` when `None`).
- `also_emit_on` and `written_collections` stay at their defaults. In particular an environment event must still never feed `RecentWrites`.
- Ruling carried from plan 1: frozen JSON literals captured from the old `DomainEvent` before the variants are removed, one per variant.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`. `rustfmt --edition 2021` only on leaf files, never on `lib.rs` or `mod.rs`; reorder `pub use` lines by hand. Commits use a pathspec and only when the whole tree compiles; a task that cannot compile in groups is one commit.
- Test recorders (every `downcast_ref::<DomainEvent>` site) panic on events they do not know. A recorder that must now also see an `EnvironmentEvent` or `WorkspaceEvent` downcasts that type too (a second list, or a combined list of strings), and still panics on anything else.

## Review Focus

- Both environment events share the channel `environment-changed`, and all eight workspace events keep their own channel; a typo silences a frontend listener: channel and frozen-literal tests (Tasks 1 and 2).
- `WorkspaceRenamed` keeps `old_name` and `new_name` (snake_case) and `WorkspaceDescriptionUpdated` with `None` serializes `"description": null`: Task 2.
- `RecentWrites` still ignores `EnvironmentSaved`: the existing echo test keeps passing with the new type: Task 1.
- A recorder that sees an environment or workspace event does not panic or lose it; one that sees any other foreign type still panics: Tasks 1 and 2.
- `rocket-workspace` gains `serde_json` as a dependency only; no cycle, and `DomainEvent` ends with 64 variants and 64 golden entries: Task 2.

---

### Task 1: EnvironmentEvent

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-environment/src/events.rs`
- Modify: `crates/rocket-environment/src/lib.rs` (module and re-export `EnvironmentEvent`, by hand in rustfmt order), `crates/rocket-shared/src/events.rs` (remove the two variants and their contract-test uses), `crates/rocket-shared/src/domain_event_port.rs` (arms, `CHANNELS` entries, `one_of_each()` entries; 74 to 72)
- Modify (publishers and tests): `crates/rocket-app/src/environment_service.rs`, `crates/rocket-app/src/environment_services.rs`, `crates/rocket-app/src/env_audit.rs`, `crates/rocket-app/src/proposal_service.rs` if it publishes the event, `crates/rocket-app/src/execution_service.rs` (test recorder around the `EnvironmentSaved` secret-write assertion), `crates/rocket-infra/src/file_watcher/echo.rs` (tests), `src-tauri/src/tauri_event_bus.rs` (tests). Find every user with `grep -rn "EnvironmentSaved\|EnvironmentDeleted" crates src-tauri --include='*.rs'`.

**Interfaces:**
- Produces: `pub enum EnvironmentEvent { EnvironmentSaved { name: String }, EnvironmentDeleted { name: String } }` implementing `PublishedEvent` (`channel()` is `"environment-changed"` for both), re-exported as `rocket_environment::EnvironmentEvent`.
- Consumes: `PublishedEvent`, `RecordingPublisher::others()` (rocket-app test doubles).

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removing the variants, serialize both with the old `DomainEvent` once and paste `json!` literals (`{"type":"environmentSaved","name":"dev"}`, `{"type":"environmentDeleted","name":"dev"}`). In `events.rs`: `environment_events_json_are_unchanged`, `both_environment_events_use_one_channel` (`environment-changed`), `environment_events_have_no_extra_channels_or_written_collections`. In `echo.rs`: the existing record-ignores test now builds `EnvironmentEvent::EnvironmentSaved`. In the bus tests: `emit_channels(&EnvironmentEvent::...)` is the single channel `environment-changed`. Update the `environment_service`, `environment_services` and `env_audit` tests to assert through a recorder that downcasts `EnvironmentEvent` (or `others()` on the shared `RecordingPublisher`) with the same expected names.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-environment events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum; remove the variants, arms and golden entries from `rocket-shared`; switch the publishers to `&EnvironmentEvent::...`; fix every recorder the compiler or a test run flags, following the recorder rule in Global Constraints.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-environment`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-infra file_watcher` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; `one_of_each_covers_every_variant` counts 72. `grep -rn "DomainEvent::Environment" crates src-tauri --include='*.rs'` prints nothing.
- [ ] **Step 5: Commit** (one commit if the tree cannot compile in groups): `feat(environment): add the EnvironmentEvent enum` for the enum and shared removal, `refactor: publish environment changes as EnvironmentEvent` for the rest.

### Task 2: WorkspaceEvent

**Files:**
- Create: `crates/rocket-workspace/src/events.rs`
- Modify: `crates/rocket-workspace/Cargo.toml` (add `serde_json.workspace = true`), `crates/rocket-workspace/src/lib.rs` (module and re-export `WorkspaceEvent`), `crates/rocket-shared/src/events.rs` (remove the eight variants and their contract tests, which move), `crates/rocket-shared/src/domain_event_port.rs` (eight arms, `CHANNELS` entries, `one_of_each()` entries; 72 to 64), `crates/rocket-app/src/workspace_service.rs` (publishes `WorkspaceEvent`; the `EventNames` recorder downcasts `WorkspaceEvent` and still panics on anything else), `src-tauri/src/tauri_event_bus.rs` (tests), and every other user from `grep -rn "DomainEvent::Workspace" crates src-tauri --include='*.rs'`

**Interfaces:**
- Produces: `pub enum WorkspaceEvent { WorkspaceCreated{id,name,path}, WorkspaceSwitched{id,name,path}, WorkspaceRenamed{id,old_name,new_name}, WorkspaceClosed{id}, WorkspaceDeleted{id}, WorkspacePinned{id}, WorkspaceUnpinned{id}, WorkspaceDescriptionUpdated{id,description: Option<String>} }` (all `String` fields except `description`) implementing `PublishedEvent` with the channels in Global Constraints.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removal, serialize one value per variant with the old `DomainEvent` (including `WorkspaceDescriptionUpdated` with `Some("d")` and with `None`) and paste `json!` literals. In `events.rs`: `workspace_events_json_are_unchanged` (all variants plus the `None` case giving `"description": null`), `workspace_events_keep_their_channels` (the eight names), `renamed_keeps_snake_case_field_names` (`old_name`, `new_name` present, `oldName` absent), `workspace_events_have_no_extra_channels_or_written_collections`. Move the existing workspace contract tests from `rocket-shared/src/events.rs` into this file, with `WorkspaceEvent`. Bus test: `emit_channels(&WorkspaceEvent::...)` is the single expected channel. Update the `workspace_service.rs` recorder tests (`switched:`, `closed:` and so on) to the new type with unchanged expected strings.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-workspace events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum and the dependency; remove variants, arms and golden entries from `rocket-shared`; switch the publish sites.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-workspace`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app workspace` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; the shared count is 64. `grep -rn "DomainEvent::Workspace" crates src-tauri --include='*.rs'` prints nothing; `cargo tree -p rocket-workspace -e normal --depth 1` shows only `rocket-shared`, `serde`, `serde_json` and `serde_yaml` among the new/known deps.
- [ ] **Step 5: Commit** `feat(workspace): add the WorkspaceEvent enum` (enum, dependency, shared removal) and `refactor(app): publish workspace changes as WorkspaceEvent`, or one commit if the groups do not compile separately.

### Task 3: Docs and checks

**Files:**
- Modify: `crates/rocket-shared/CLAUDE.md` (the `DomainEvent` description and the "moved out" list: 64 variants left), `crates/rocket-environment/CLAUDE.md`, `crates/rocket-workspace/CLAUDE.md` if it exists (the new `events` module), `docs/roadmaps/ddd-roadmap.md` (D-74 progress: plan 1 is merged, plan 2 done; the "81 variants" in the D-74 row becomes "81 at the start, 64 after plan 2"; D-74 and D-71 stay open), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 1 "done, merged", plan 2 file and "done, unmerged")

- [ ] **Step 1: Edit the docs.** Run `cargo clippy -j4 -p rocket-shared -p rocket-environment -p rocket-workspace -p rocket-infra -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). Run `cargo fmt --check -p rocket-environment -p rocket-workspace` — Expected: no diff in the files this plan touched.
- [ ] **Step 2: Commit** `docs: record DomainEvent split plan 2 (D-74)`.
