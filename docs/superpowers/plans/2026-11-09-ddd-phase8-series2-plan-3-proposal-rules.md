# Phase 8 series 2, plan 3: the proposal rules (D-82, part 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The structural path rules and the apply rules of the assistant's proposals live in `rocket-acp/src/proposal_rules.rs`, behind one new dependency edge `rocket-acp -> rocket-collection`. `ProposalService` keeps the queue, the session state and the fingerprint and masking functions. No behavior change.

**Architecture:** Add the edge, add one `Folder` method, then move the pure functions in two groups. No re-export at the old location; `proposal_service.rs` imports the moved names from `rocket_acp::proposal_rules`.

**Tech Stack:** Rust: `rocket-acp`, `rocket-collection`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series2-domain-rules-design.md` (section "Plan 3"). Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`. Run after plan 2.

## Global Constraints

- New dependency edge: `rocket-acp` gains `rocket-collection.workspace = true`. `rocket-collection` must NOT depend on `rocket-acp` (no cycle); `cargo tree -p rocket-collection -e normal --depth 1` must not list `rocket-acp`. `rocket-acp` gains no other workspace edge (in particular not `rocket-environment` and not `rocket-app`).
- Behavior is identical: every moved function keeps its body, its doc comment and its error texts. Tests that call a moved function directly move with it; tests that go through `ProposalService` stay.
- What stays in `proposal_service.rs`: the service, `pending_mut`, `finish`, `prune_resolved`, `unchanged`, `request_fingerprint`, `folder_shape`, `folder_fingerprint`, `env_var_fingerprint`, `validate_environment_name`, `refuse_masked_values`.
- No re-export, alias or shim at an old location. Search every target: `rg -n '<old name>' crates src-tauri` over whole directories. After each task run `cargo check -j4 -p <pkg> --tests` for every package whose files changed.
- Format: `rustfmt --edition 2021 --check <file>` on every touched `.rs` file; hand-fix drift in place.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. `cargo tree` takes no `-j`. Long runs go to a log file under the session scratchpad.
- Commits: conventional commits, one per task, with the `dev-workflow-skills:1-git-commit` skill, explicit staged paths. Do not edit any `CLAUDE.md` or the roadmap before task 3.
- Code comments are short, full sentences that end with a period.

## Review Focus

- A proposal path that resolved before no longer resolves: case-folding (`locate_with` with `fold`), nested folders, a path with `..`, an absolute path, a reserved name (pinned by moved tests plus the service tests that stay).
- `Folder::dir_or_name` differs from the old `folder_dir_name`: a folder with `dir_name = None` falls back to `name`; one with `dir_name = Some("x")` returns `"x"` (two direct tests).
- A moved `apply_patch` or `build_request` changes a stored request (header description lost on replace, script phase mapping, body mode); the service-level tests for accept stay green and the moved unit tests pass.
- The new edge pulls `rocket-environment` or `rocket-app` into `rocket-acp` by accident (`cargo tree` check).
- A missed importer in `src-tauri/tests` or `crates/*/tests`.

---

### Task 1: Add the edge, `Folder::dir_or_name`, and move the path rules

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection` folder and request model).

**Files:**
- Modify: `crates/rocket-acp/Cargo.toml`, `crates/rocket-acp/src/lib.rs`, `crates/rocket-collection/src/` (the file that defines `Folder`), `crates/rocket-app/src/runner_sequence.rs` (remove `folder_dir_name` at about line 158 and switch its callers), `crates/rocket-app/src/proposal_service.rs`
- Create: `crates/rocket-acp/src/proposal_rules.rs`

**Interfaces:**
- Produces: `Folder::dir_or_name(&self) -> &str` (`self.dir_name.as_deref().unwrap_or(&self.name)`). In `rocket_acp::proposal_rules` (all `pub`): `Target` (the enum with `HttpRequest`, `Folder(String)`, `Other`, `Missing`), `locate`, `locate_with`, `same_name`, `child_folder`, `is_folder`, `is_free`, `last_segment`, `check_reserved_name`, `join_path`, `parent_of`, `move_destination`, `normalize_path`, `normalize_paths`, `paths_of`, `validate_relative_path`, `validate_item_name`. Signatures and bodies are those at `proposal_service.rs` lines 92 and 920 to 1300 today.
- Consumes: nothing from other tasks.

- [ ] **Step 1: Pin `dir_or_name`.** Add `folder_dir_or_name_falls_back_to_the_name` and `folder_dir_or_name_prefers_the_dir_name` in `rocket-collection` (RED: the method does not exist). Implement the method on `Folder`.
- [ ] **Step 2: Replace the app helper.** `rg -n 'folder_dir_name' crates src-tauri` lists every caller. Replace each `folder_dir_name(x)` with `x.dir_or_name()` and delete the helper from `runner_sequence.rs`. `cargo check -j4 --tests` passes for `rocket-collection` and `rocket-app`.
- [ ] **Step 3: Baseline the rules' tests.** `cargo test -j4 -p rocket-app proposal_service` and note the total; list which tests call a path-rule function directly (they will move).
- [ ] **Step 4: Add the edge and move.** Add `rocket-collection.workspace = true` to `rocket-acp/Cargo.toml`. Create `proposal_rules.rs` with the functions and `Target` listed above, `pub mod proposal_rules;` in `rocket-acp/src/lib.rs`. Delete them from `proposal_service.rs`, which imports from `rocket_acp::proposal_rules`. Move the direct unit tests with them.
- [ ] **Step 5: Verify.** `cargo test -j4 -p rocket-acp proposal_rules` and `cargo test -j4 -p rocket-app proposal_service` pass (moved plus stayed tests equal the baseline total). `cargo check -j4 --tests` passes for `rocket-acp`, `rocket-collection`, `rocket-app` and `rocket`. `cargo tree -p rocket-acp -e normal --depth 1` lists `rocket-shared` and `rocket-collection` only among workspace crates; `cargo tree -p rocket-collection -e normal --depth 1` does not list `rocket-acp`.
- [ ] **Step 6: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move proposal path rules to acp`.

---

### Task 2: Move the apply rules

**Files:**
- Modify: `crates/rocket-acp/src/proposal_rules.rs`, `crates/rocket-app/src/proposal_service.rs`

**Interfaces:**
- Produces, in `rocket_acp::proposal_rules` (all `pub`): `sha256_hex(text: &str) -> String`, `apply_folder_patch(settings: &mut FolderSettings, patch: &FolderSettingsPatch)`, `check_folder_variables(patch: &FolderSettingsPatch) -> DomainResult<()>`, `set_script(request: &mut Request, phase: ScriptPhase, body: String)`, `stored_body(body: &Body) -> Option<Body>`, `build_request(proposed: &ProposedRequest) -> Request`, `apply_patch(request: &mut Request, patch: &RequestPatch)`. Bodies as at `proposal_service.rs` lines 884 and 1052 to 1331 today.
- Consumes: the module and the edge from task 1.

- [ ] **Step 1: Baseline.** `cargo test -j4 -p rocket-app proposal_service` total, and the list of tests that call these functions directly.
- [ ] **Step 2: Move.** Cut the seven functions (with doc comments) into `proposal_rules.rs`; `proposal_service.rs` imports them. If `sha256_hex` is also used by the functions that stay (`request_fingerprint`, `folder_fingerprint`, `env_var_fingerprint`), they import it from `rocket_acp::proposal_rules`; add `sha2` to `rocket-acp/Cargo.toml` only if the compiler asks and report it. Move the direct unit tests with the functions.
- [ ] **Step 3: Verify.** `cargo test -j4 -p rocket-acp proposal_rules` and `cargo test -j4 -p rocket-app proposal_service` pass (totals equal the baseline). `cargo check -j4 --tests` passes for `rocket-acp`, `rocket-app` and `rocket`. `cargo tree -p rocket-acp -e normal --depth 1` still shows no `rocket-environment` or `rocket-app`. `wc -l crates/rocket-app/src/proposal_service.rs` is reported before and after.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move proposal apply rules to acp`.

---

### Task 3: Docs and closing D-82 and D-86

**Files:**
- Modify: `crates/rocket-acp/CLAUDE.md`, `crates/rocket-collection/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md` (rows D-82 and D-86, decisions log), `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md` (series 2 status), `docs/superpowers/specs/2026-11-09-ddd-phase8-series2-domain-rules-design.md` (status line)

- [ ] **Step 1: Find stale statements.** `rg -n 'folder_dir_name|proposal_service|locate_with|apply_patch|proposal_rules' .claude docs crates --glob '*.md' --glob '!docs/superpowers/**'`; judge each hit against the code. `rocket-acp/CLAUDE.md` says the crate depends only on `rocket-shared` somewhere; fix that to name the new edge and why.
- [ ] **Step 2: Update.** Module-map entry for `proposal_rules.rs` and the new dependency in `rocket-acp/CLAUDE.md`, the `Folder::dir_or_name` note in `rocket-collection/CLAUDE.md`, the `ProposalService` row in `rocket-app/CLAUDE.md` (the path and apply rules now live in `rocket-acp`; fingerprints and masking stay), roadmap D-82 `done (<date>, contract, conversation and proposal structural rules moved; fingerprints and masking stay in rocket-app)` and a decisions-log line for the edge. Series 2 row of the index: `done (<date>) on branch worktree-ddd-phase8-series1`. Spec status: `approved 2026-11-09, implemented`.
- [ ] **Step 3: Verify.** Every path named in the changed docs exists (`ls`), and the `rg` from step 1 shows no stale statement.
- [ ] **Step 4: Commit:** `docs(ddd): close D-82 and update guides`.
