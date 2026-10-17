# Dedup T2.21 Plan 2: Shared Name Validation Helpers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the empty-name check and the slug-character predicate in `rocket-shared` and use them at the four places that repeat them.

**Architecture:** A small `rocket_shared::validation` module. Callers keep their own error messages and their extra rules, so output stays byte-identical.

**Tech Stack:** Rust, `cargo test -j4 -p <crate>`.

**Spec:** GitHub issue #76 (T2.21). Index: `2026-10-17-dedup-t2-21-index.md`. Run after Plan 1 (Task 3 of Plan 1 already edits `external_secret.rs`).

## Global Constraints

- Behaviour and every error message must stay byte-identical.
- Sibling domain crates depend only on `rocket-shared`, so the helpers live there.
- Cargo commands always pass `-j4`; targeted `-p <crate>` tests only.
- No unwrap calls in production paths. Explicit-path git adds only.
- Commit with the `dev-workflow-skills:1-git-commit` skill.
- Comments are short full sentences ending in a period.
- Overlap with ticket #55: keep the collection-only rules (no separators, no leading dot) in `Collection::validate_name`.

## Review Focus

- Whitespace-only names: `Collection`/`Workspace` reject `"   "` (trim), but alias and repository-component checks use a plain `is_empty()` and only reject `""`. The helper must not change either.
- Non-ASCII input in `repository_locator::validate_component` must still be rejected (the byte check and a char check agree for non-ASCII).
- `Workspace::validate_name` must still accept names with spaces and slashes, since it only checks emptiness today.

---

### Task 4: rocket_shared::validation module

**Files:**
- Create: `crates/rocket-shared/src/validation.rs`
- Modify: `crates/rocket-shared/src/lib.rs` (add `pub mod validation;`)

**Interfaces:**
- Consumes: `crate::error::{DomainError, DomainResult}`.
- Produces:
  - `pub fn non_empty_name(name: &str, message: &str) -> DomainResult<()>`: `Err(DomainError::InvalidInput(message.to_string()))` when `name.trim().is_empty()`, else `Ok(())`.
  - `pub fn is_slug_char(c: char) -> bool`: true for ASCII letters, digits, `_` and `-`.

- [ ] **Step 1: Write the tests** in `validation.rs`: `non_empty_name_rejects_blank_and_whitespace` (asserts the exact message passes through in `InvalidInput`), `non_empty_name_accepts_text`, `is_slug_char_accepts_alnum_underscore_dash`, `is_slug_char_rejects_dot_space_slash_and_non_ascii` (include `'é'`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-shared validation` and confirm FAIL (functions missing).
- [ ] **Step 3: Implement both functions** with one-line doc comments, and register the module in `lib.rs`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-shared` and confirm PASS.
- [ ] **Step 5: Commit** with the git-commit skill. Paths: both files above. Suggested type: `refactor:`.

---

### Task 5: Adopt the helpers at four call sites

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/collection.rs:37` (`Collection::validate_name`, empty check only)
- Modify: `crates/rocket-workspace/src/workspace.rs:37` (`Workspace::validate_name`)
- Modify: `crates/rocket-workspace/src/repository_locator.rs:137` (`validate_component`, predicate only)
- Modify: `crates/rocket-environment/src/external_secret.rs:33-67` (alias predicate only)

**Interfaces:**
- Consumes: `rocket_shared::validation::{non_empty_name, is_slug_char}` from Task 4.
- Produces: no new public API.

- [ ] **Step 1: Confirm the pins.** Existing tests: `validate_name_rejects_invalid_chars` (collection), `validate_name_rejects_empty` and `validate_name_accepts_valid` (workspace), `validate_bindings_*` (environment). Add `validate_component` tests in `repository_locator.rs` if none exist: accepts `"ws-1_a"`, rejects `""`, `"a.b"`, `"é"`, asserting `InvalidInput`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection`, `-p rocket-workspace` and `-p rocket-environment` and confirm PASS before editing.
- [ ] **Step 3: Replace the duplicates.**
  - `Collection::validate_name`: first check becomes `non_empty_name(name, "Collection name cannot be empty")?;`. The separator and leading-dot checks stay.
  - `Workspace::validate_name`: body becomes `non_empty_name(name, "Workspace name cannot be empty")`.
  - `validate_component`: replace the byte predicate with `value.chars().all(is_slug_char)`; keep the `is_empty()` check and the message.
  - `validate_external_secret_bindings`: replace the inline char closure with `alias.chars().all(is_slug_char)`; keep the plain `alias.is_empty()` check and both messages.
- [ ] **Step 4: Run** `cargo check -j4`, then `cargo test -j4 -p rocket-shared`, `-p rocket-collection`, `-p rocket-workspace`, `-p rocket-environment`. Confirm all PASS.
- [ ] **Step 5: Commit** with the git-commit skill. Paths: the four files above.
