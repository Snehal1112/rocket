# DDD Phase 6, Plan 2: Flow routing rules and guards move into rocket-flow (D-61, D-62)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The pure Flow rules (node fate, edge liveness, rename, validity with its error text) live in `rocket-flow`, so `FlowService` and the run code call them instead of owning them.

**Architecture:** `routing.rs` moves from `rocket-app` to `rocket-flow` with its tests. `Flow::ensure_valid` and `Flow::checked_rename` are added with tests first, and the save-error text builder moves with `ensure_valid`. Behavior and error texts do not change.

**Spec:** `docs/roadmaps/ddd-roadmap.md` items D-61 and D-62, `docs/engineering/ddd-decisions.md` D-04 (`Flow` second after `WorkspaceRegistry`).

## Global Constraints

- Error texts stay byte for byte: the rename texts and the canvas-parsed save text ("node(s): ...; edge(s): ...").
- `FlowRepository` and the `Flow` fields keep their shape (persistence and wire format do not change).
- No `unwrap` in production code. `cargo` commands take `-j4`.

## Review Focus

- A flow with one problem keeps the older single-problem text; several problems keep the "flow has N problems" header.
- A hostile id or label cannot fake the `node(s):` tail (the `message_safe` helpers move unchanged).
- A rename to the same name with outer spaces is refused as unchanged.

---

### Task 1: Flow guards (D-62)

**Files:** `crates/rocket-flow/src/flow.rs`, `crates/rocket-flow/src/save_error.rs` (moved from `flow_service.rs`), `crates/rocket-flow/src/lib.rs`, `crates/rocket-app/src/flow_service.rs`.

**Interfaces:** Produces `Flow::ensure_valid(&self) -> DomainResult<()>` and `Flow::checked_rename(old: &str, new: &str) -> DomainResult<String>`.

- [ ] **Step 1:** Write the failing tests (`ensure_valid_accepts_a_valid_flow`, `ensure_valid_refuses_a_duplicate_node_with_the_canvas_text`, `checked_rename_trims_the_new_name`, `checked_rename_refuses_a_blank_name`, `checked_rename_refuses_the_same_name`).
- [ ] **Step 2:** Run `cargo test -j4 -p rocket-flow guard_tests` — Expected: compile error (red).
- [ ] **Step 3:** Implement the two methods, move `graph_errors_message` and its helpers.
- [ ] **Step 4:** Run `cargo test -j4 -p rocket-flow` and `cargo test -j4 -p rocket-app flow_service` — Expected: pass.

### Task 2: Routing rules (D-61)

**Files:** `crates/rocket-flow/src/routing.rs` (moved), `crates/rocket-app/src/{flow_execution_service,flow_partial,flow_run_cache,lib}.rs`.

- [ ] **Step 1:** `git mv` the file, make its items `pub`, point the three users at `rocket_flow::routing`.
- [ ] **Step 2:** Run `cargo test -j4 -p rocket-flow` and `cargo test -j4 -p rocket-app flow` — Expected: pass.

### Task 3: Docs

- [ ] **Step 1:** Update `crates/rocket-flow/CLAUDE.md` and the roadmap rows; run `cargo clippy -j4 -p rocket-flow -p rocket-app --all-targets` — Expected: no new warnings.
