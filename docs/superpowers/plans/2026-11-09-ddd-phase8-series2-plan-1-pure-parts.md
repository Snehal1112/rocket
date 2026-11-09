# Phase 8 series 2, plan 1: the pure parts of D-86

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `graphql_document` lives in `rocket-collection`, `assertion_evaluator` in `rocket-scripting`, and `effective_capabilities` in `rocket-collection`. No behavior, JSON or file-format change.

**Architecture:** Pure moves with `git mv` (or a cut of one function into an existing module). No re-export at the old location. A pin test is green at the old location before each move.

**Tech Stack:** Rust: `rocket-collection`, `rocket-scripting`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series2-domain-rules-design.md` (section "Plan 1"). Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`.

**Workspace:** the git worktree `ddd-phase8-series1` (branch `worktree-ddd-phase8-series1`), which already holds series 1. Do not touch other worktrees or the main checkout.

## Global Constraints

- Serde shapes are unchanged. No re-export, alias or shim at an old location: `rocket-app` must not keep `pub mod graphql_document`, `pub mod assertion_evaluator` or the `effective_capabilities` re-export.
- Search every target for importers, not just library code: `rg -n '<old path or name>' crates src-tauri` over whole directories, so `src-tauri/tests`, `crates/*/tests`, benches and examples are included. After each task run `cargo check -j4 -p <pkg> --tests` for every package whose files changed.
- Format: after editing, run `rustfmt --edition 2021 --check <file>` on every touched `.rs` file and hand-fix drift in place (a write-mode run would rewrite child modules). Moves lengthen paths, so wrapped lines are likely.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. `cargo tree` takes no `-j`. The Tauri package is `rocket`. Long runs go to a log file under the session scratchpad.
- Commits: conventional commits, one per task, with the `dev-workflow-skills:1-git-commit` skill, explicit staged paths, never `git add -A`, `--all` or `.`. Do not edit any `CLAUDE.md` or the roadmap before task 3.
- Code comments are short, full sentences that end with a period.

## Review Focus

- A missed importer in a test, bench or `src-tauri/tests` target (the series 1 defect): `cargo check --tests` on `rocket-app` and `rocket`.
- `effective_capabilities` changes meaning: it must return `EffectiveCapabilities::untrusted()` for each of the three error cases (unknown collection, unreadable settings, store error), pinned by tests.
- `list_operations` / `select_operation` change on odd documents (anonymous operation, fragments, nested braces): the existing tests move unchanged.
- `GraphQlOperation` JSON changes: it crosses IPC in `src-tauri/src/commands/execution.rs`.
- `rocket-collection` gains a dependency it should not have: `cargo tree -p rocket-collection -e normal --depth 1` still lists only `rocket-shared` among workspace crates.

---

### Task 1: Move `graphql_document` to `rocket-collection`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection`).

**Files:**
- Move: `crates/rocket-app/src/graphql_document.rs` to `crates/rocket-collection/src/graphql_document.rs`
- Modify: `crates/rocket-collection/src/lib.rs`, `crates/rocket-app/src/lib.rs` (line 53), `crates/rocket-app/src/graphql_request.rs` (line 13), `crates/rocket-app/src/graphql_subscription.rs` (line 29), `crates/rocket-app/src/graphql_schema.rs` (line 397, a test), `src-tauri/src/commands/execution.rs` (line 1)

**Interfaces:**
- Produces: `rocket_collection::graphql_document::{GraphQlOperationKind, GraphQlOperation, list_operations, select_operation}` with unchanged signatures (`list_operations(&str) -> Vec<GraphQlOperation>`, `select_operation(&str, Option<&str>, bool) -> DomainResult<Option<String>>`).
- Consumes: nothing from other tasks.

- [ ] **Step 1: Pin the IPC shape.** Read the derives on `GraphQlOperation` and `GraphQlOperationKind`. If they derive `Serialize`, add `graphql_operation_json_shape_is_unchanged` in the file's test module (an exact `serde_json::json!` comparison of one operation of each kind, values read from a run, not guessed). If they do not derive serde, say so in the report. Run `cargo test -j4 -p rocket-app graphql_document` green.
- [ ] **Step 2: Move.** `git mv` the file. Add `pub mod graphql_document;` to `rocket-collection/src/lib.rs`. Remove `pub mod graphql_document;` from `rocket-app/src/lib.rs`. Switch the four importers to `rocket_collection::graphql_document::...` (in `rocket-app` files, `use rocket_collection::graphql_document::select_operation;` and the like).
- [ ] **Step 3: Verify.** `rg -n 'graphql_document' crates src-tauri` shows only `rocket_collection` paths and the new module. `cargo test -j4 -p rocket-collection graphql_document` passes with the moved tests (count them against the old file). `cargo check -j4 --tests` passes for `rocket-collection`, `rocket-app` and `rocket`. `cargo tree -p rocket-collection -e normal --depth 1` lists only `rocket-shared` among workspace crates.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move graphql_document to collection`.

---

### Task 2: Move `assertion_evaluator` to `rocket-scripting`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (assertions are collection request data).

**Files:**
- Move: `crates/rocket-app/src/assertion_evaluator.rs` to `crates/rocket-scripting/src/assertion_evaluator.rs`
- Modify: `crates/rocket-scripting/src/lib.rs`, `crates/rocket-scripting/Cargo.toml` (only if the compiler asks), `crates/rocket-app/src/lib.rs` (line 9), `crates/rocket-app/src/execution_service.rs` (the two call sites at about lines 3397 and 3498)

**Interfaces:**
- Produces: `rocket_scripting::{evaluate_assertions, assertion_outcomes}` with unchanged signatures (`evaluate_assertions(&[Assertion], &HttpResponse) -> Vec<TestResult>`, `assertion_outcomes(&[Assertion], &HttpResponse) -> Vec<AssertionOutcome>`).
- Consumes: nothing from other tasks.

- [ ] **Step 1: Baseline.** Run `cargo test -j4 -p rocket-app assertion_evaluator` and note the test count; it must be the same after the move.
- [ ] **Step 2: Move.** `git mv` the file. In it, replace `use rocket_scripting::{AssertionOutcome, TestResult, TestStatus};` with `use crate::{AssertionOutcome, TestResult, TestStatus};` (or the module path that exports them). Add `pub mod assertion_evaluator;` and `pub use assertion_evaluator::{assertion_outcomes, evaluate_assertions};` to `rocket-scripting/src/lib.rs`. Remove `pub mod assertion_evaluator;` from `rocket-app/src/lib.rs`. Point `execution_service.rs` at `rocket_scripting::assertion_outcomes` and `rocket_scripting::evaluate_assertions`. Add a `Cargo.toml` dependency only if the compiler requires it, and report which.
- [ ] **Step 3: Verify.** `rg -n 'assertion_evaluator' crates src-tauri` shows only the new module path. `cargo test -j4 -p rocket-scripting assertion_evaluator` passes with the same test count as the baseline. `cargo check -j4 --tests` passes for `rocket-scripting`, `rocket-app` and `rocket`. `cargo tree -p rocket-scripting -e normal --depth 1` shows no new workspace edge.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move assertion evaluator to scripting`.

---

### Task 3: Move `effective_capabilities`, then docs

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection` and trust data).

**Files:**
- Modify: `crates/rocket-collection/src/trust.rs`, `crates/rocket-collection/src/lib.rs` (export), `crates/rocket-app/src/collection_trust.rs` (remove the function at about line 28; the service call at about line 92), `crates/rocket-app/src/execution_service.rs` (about line 860), `crates/rocket-app/src/mcp_tool_service.rs` (about line 187), `crates/rocket-app/src/collection_service.rs` (tests at about lines 2515 and 2519), `crates/rocket-app/src/lib.rs` (line 119, drop `effective_capabilities` from the re-export), `src-tauri/src/import_wiring.rs` (about lines 518, 1205, 1260, 1327)
- Docs: `crates/rocket-collection/CLAUDE.md`, `crates/rocket-scripting/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md` (row D-86 and the decisions log), `docs/engineering/glossary.md` only if it names these items

**Interfaces:**
- Produces: `rocket_collection::effective_capabilities(repo: &dyn CollectionRepository, store: &dyn CollectionTrustStore, collection: &str) -> EffectiveCapabilities`, same body, same doc comment.
- Consumes: `resolve_effective`, `RequestedElevation`, `EffectiveCapabilities`, `CollectionTrustStore`, `CollectionRepository` (all already in `rocket-collection`).

- [ ] **Step 1: Pin the error cases.** In `rocket-collection`, with small in-test fakes of `CollectionRepository` and `CollectionTrustStore` (reuse an existing fake in the crate's trust tests if there is one), add `effective_capabilities_is_untrusted_for_an_unknown_collection`, `..._for_unreadable_settings` and `..._for_a_store_error`, each asserting equality with `EffectiveCapabilities::untrusted()`, plus `effective_capabilities_uses_the_grant_for_a_trusted_collection`. They fail to compile until the function moves (RED).
- [ ] **Step 2: Move.** Cut `effective_capabilities` and its doc comment from `collection_trust.rs` into `trust.rs` and export it from the crate root. In `collection_trust.rs` call `rocket_collection::effective_capabilities`; drop imports that became unused. Update the five other callers listed above. Remove `effective_capabilities` from the `rocket-app/src/lib.rs` re-export; `import_wiring.rs` imports it from `rocket_collection`.
- [ ] **Step 3: Verify.** `rg -n 'effective_capabilities' crates src-tauri` shows no `rocket_app::effective_capabilities` and no `crate::effective_capabilities`. `cargo test -j4 -p rocket-collection trust` passes (the new tests GREEN). `cargo test -j4 -p rocket-app collection_trust` and `-p rocket-app collection_service` pass. `cargo check -j4 --tests` passes for `rocket-collection`, `rocket-app` and `rocket`.
- [ ] **Step 4: Docs.** Update the three crate guides (new rows or module-map entries for `graphql_document`, `assertion_evaluator`, `effective_capabilities`; remove the matching statements from `rocket-app/CLAUDE.md`), mark D-86 `done (<date>, pure parts only: graphql_document, assertion_evaluator, effective_capabilities; the graphql services and the trust service stay)` and add a decisions-log line. Run `rg -n 'graphql_document|assertion_evaluator|effective_capabilities' .claude docs crates --glob '*.md' --glob '!docs/superpowers/**'` and fix any stale statement.
- [ ] **Step 5: Format and commit.** `rustfmt --edition 2021 --check` on the touched `.rs` files, then two commits: `refactor(ddd): move effective_capabilities to collection` (code) and `docs(ddd): close D-86 pure parts` (docs).
