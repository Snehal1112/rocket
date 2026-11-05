# Phase 5 fixes, Plan 5: docs and tests (C-8, C-9, C-10, C-6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The crate docs and the roadmap say what the code does, the leftover unused items are gone, and the tests that silently do nothing without a home directory really run.

**Architecture:** Run this plan last, after Plans 1 to 4, so the docs describe the final state.

**Tech Stack:** Markdown, Rust tests.

**Spec:** `.claude/reviews/2026-11-04-ddd-phase5-audit.md` (C-6, C-8, C-9, C-10).

## Global Constraints

- Docs go in the existing files; do not add a new doc. Keep project `CLAUDE.md` small.
- Roadmap rows state facts with a date, in the style of the existing D-5x rows.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.

## Review Focus

- No doc line claims "all disk I/O goes through the repos" for `rocket-import`.
- A symlink to the project root and the real path share one consent record (test).
- The home-dir tests run on a machine with no `HOME` (they use an injected home).
- `cargo fmt --check` passes for the touched crates.

---

### Task 1: Docs and roadmap (C-8)

**Files:**
- Modify: `crates/rocket-import/CLAUDE.md` (~lines 72, 272), `crates/rocket-collection/CLAUDE.md` (~line 7), `docs/roadmaps/ddd-roadmap.md` (D-51, D-55 rows; new rows)

**Interfaces:** none.

- [ ] **Step 1: Find the lines.** `grep -n "rocket-infra\|canonicalize\|disk I/O\|all disk" crates/rocket-import/CLAUDE.md crates/rocket-collection/CLAUDE.md` and open each hit. (Expected: the three stale lines from the audit.)
- [ ] **Step 2: Fix them.** Import I/O goes through the `ImportFiles` port, `rocket-infra` is a dev concern only, no `canonicalize` call remains in the crate (true after Plan 1). Check each claim against the code before writing it.
- [ ] **Step 3: Roadmap.** D-51 done without "predicates left" (after Plan 1); D-55 notes the `git2` use in `src-tauri` is gone (after Plan 4). Add rows for the audit follow-ups with the next free IDs (`grep -o "D-[0-9]*" docs/roadmaps/ddd-roadmap.md | sort -u | tail`): import symlink hardening, import limits, token client hardening, dependency cleanup, each with its status and plan file.
- [ ] **Step 4: Run** `grep -rn "all disk I/O" crates/*/CLAUDE.md` — Expected: no output.
- [ ] **Step 5: Commit** with the `dev-workflow-skills:1-git-commit` skill (pathspec the docs).

### Task 2: Unused items (C-9)

**Files:**
- Modify: `crates/rocket-app/Cargo.toml` (~41), `crates/rocket-http/src/oauth2.rs` (`OAuthConfig`)

**Interfaces:** none.

- [ ] **Step 1: Check.** `grep -rn "OAuthConfig" crates src-tauri/src --include=*.rs` and `grep -rn "reqwest" crates/rocket-app/src crates/rocket-app/tests --include=*.rs`. (Expected: `OAuthConfig` is only defined and re-exported; `reqwest` unused in `rocket-app`.) If either is used, leave it and ledger why.
- [ ] **Step 2: Remove** the dev-dependency line and the unused struct plus its re-export.
- [ ] **Step 3: Run** `cargo fmt -p rocket-http -p rocket-app -- --check`. If it reports the two blank lines, run `cargo fmt -p rocket-http -p rocket-app`.
- [ ] **Step 4: Run** `cargo check -j4`, `cargo test -j4 -p rocket-http`, `cargo test -j4 -p rocket-app --no-run` — Expected: pass.
- [ ] **Step 5: Commit** (pathspec the touched files, `Cargo.lock`).

### Task 3: Tests that really run (C-10, C-6)

**Files:**
- Modify: `crates/rocket-infra/src/paths.rs` (~21-37), the consent repo tests in `crates/rocket-infra/src/fs_openapi_consent_repo.rs`
- Test: same files

**Interfaces:**
- Consumes: the existing `*_with(home: Option<&Path>, ...)` style helpers if they exist (read `paths.rs` first). If the logic only reads the real home, split a pure inner function that takes `home: Option<PathBuf>`; the public function calls it with the real value.

- [ ] **Step 1: Write the failing tests.** `expand_tilde_uses_the_given_home` (inner function with a temp home, no `if let Some(home)` wrapper), `expand_tilde_without_a_home_keeps_the_path`, `a_symlink_to_the_root_shares_one_consent_record` (`#[cfg(unix)]`: create a real dir and a symlink to it; save consent through one, read through the other; assert the record is found).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-infra consent paths` — Expected: the first two fail to compile until the inner function exists (red); the symlink test shows whether the behaviour already holds. If it fails, that is a real finding: stop, fix `consent_root_key` use in the repo, and ledger it.
- [ ] **Step 3: Implement** the inner function and replace the `if let Some(home)` test wrappers with direct assertions.
- [ ] **Step 4: C-6 note.** In `importer.rs` test doubles, extend `FakeFiles::list_dir` to return a configured listing (a `listings: Vec<(PathBuf, Vec<DirEntryInfo>)>` field) and add one end-to-end test, `import_runs_without_a_disk`, for a one-request collection. Skip this step and ledger it if Plan 1 changed `FakeFiles` in a way that makes it larger than 40 lines.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-infra`, `cargo test -j4 -p rocket-import`, `cargo clippy -j4 -p rocket-infra -p rocket-import -- -D warnings` — Expected: pass. Commit (pathspec the touched files).
