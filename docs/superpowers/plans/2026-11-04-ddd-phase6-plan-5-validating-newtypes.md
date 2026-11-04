# DDD Phase 6, Plan 5: validating newtypes for collection names and relative paths (D-67)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection name that is safe to join onto a workspace path, and a relative path that is safe inside a file transaction, are each one type with one constructor in `rocket-collection`, so the two places that re-check them use the domain rule instead of their own copy.

**Architecture:** Add `CollectionName` (`names.rs`) and `RelPath` (`rel_path.rs`) to `rocket-collection`. Each has `parse(&str) -> DomainResult<Self>`, `as_str()`, `Display` and `AsRef<str>`. They are not stored in `Collection`, `Request` or any serde type, so no file or wire format changes (D-04: narrow, public fields stay). `CollectionName::parse` is the domain rule of `Collection::validate_name` plus the extra checks `FsCollectionFileSource` keeps for itself. `RelPath::parse` owns the rules of `validate_rel_path`, which stays as a thin function.

**Tech Stack:** Rust, `rocket-collection`, `rocket-infra`, `rocket-import`.

**Spec:** `docs/roadmaps/ddd-roadmap.md` item D-67; `docs/engineering/ddd-decisions.md` D-04 ("newtypes later through D-67").

## Global Constraints

- Scope ruling: only the two values that already have a duplicated check get a type. Request names have no rule today, so a newtype would invent behavior; they are left out.
- `Collection::validate_name` keeps its exact rules and error texts, because about 80 call sites use it. `CollectionName::parse` is stricter only by the checks already in `fs_collection_file_source::validate_name` (NUL byte, `.`, `..`, exactly one normal path component).
- `validate_rel_path` keeps its rules and error texts (`path "<rel>" <why>`).
- No change to serde shape, wire format, repository trait signatures or IPC DTOs.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (both tasks touch collection names and paths).

## Review Focus

- `CollectionName::parse` rejects: empty and blank, `a/b`, `a\b`, `.hidden`, `.`, `..`, a NUL byte; it accepts `my-api` and `also_valid.name`. On Windows it also rejects `C:x` (the component check); a test that needs Windows is `#[cfg(windows)]`.
- `FsCollectionFileSource` still answers every bad name with `Invalid collection name` (the existing `file_source_rejects_unsafe_collection_name` test stays green).
- `RelPath::parse` keeps every existing rejection: empty, backslash, NUL, leading `/`, empty or `.` or `..` part, colon in a part, Windows device names (`CON`, `nul.txt`), and anything in `.rocket/` that is not an owned state file.
- `FsFileTx::check_new_path` and the OpenAPI sync `check` reject exactly the same paths as before.

---

### Task 1: CollectionName

**Files:**
- Create: `crates/rocket-collection/src/names.rs`
- Modify: `crates/rocket-collection/src/lib.rs` (module and re-export), `crates/rocket-infra/src/fs_collection_file_source.rs` (delete the private `validate_name`, use the type)

**Interfaces:**
- Produces:
  ```rust
  pub struct CollectionName(String);
  impl CollectionName {
      /// The `Collection::validate_name` rule, plus: no NUL byte, not `.` or `..`, and exactly one normal path component.
      pub fn parse(name: &str) -> DomainResult<Self>;
      pub fn as_str(&self) -> &str;
  }
  // Display, AsRef<str>, Debug, Clone, PartialEq, Eq
  ```
- Consumes: `Collection::validate_name` from `collection.rs`.

- [ ] **Step 1: Write the failing tests** in `names.rs`: `parse_accepts_plain_names` (`my-api`, `also_valid.name`, `with space`), `parse_rejects_empty_and_blank`, `parse_rejects_separators` (`a/b`, `a\b`), `parse_rejects_dot_names` (`.hidden`, `.`, `..`), `parse_rejects_a_nul_byte`, `parse_keeps_the_text` (`as_str` and `to_string` equal the input), `parse_rejects_a_drive_prefix` (`#[cfg(windows)]`, `C:x`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection names` — Expected: compile error (red).
- [ ] **Step 3: Implement** `CollectionName` by moving the body of `fs_collection_file_source::validate_name` into `parse`, calling `Collection::validate_name` first. The error is the specific domain error from `Collection::validate_name`, or `InvalidInput("Invalid collection name")` for the extra checks.
- [ ] **Step 4: Switch** `FsCollectionFileSource` to `CollectionName::parse(collection).map_err(|_| generic)` so every bad name still gives `Invalid collection name`; delete the private function and its now-unused imports.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-collection` and `cargo test -j4 -p rocket-infra fs_collection_file_source` — Expected: pass.
- [ ] **Step 6: Commit** (`feat(collection): add the CollectionName type`, then `refactor(infra): check collection names through CollectionName`).

### Task 2: RelPath

**Files:**
- Create: `crates/rocket-collection/src/rel_path.rs`
- Modify: `crates/rocket-collection/src/file_tx.rs` (`validate_rel_path` delegates; the constants it uses move with the rules or stay, whichever keeps one copy), `crates/rocket-collection/src/lib.rs`, `crates/rocket-infra/src/fs_file_tx.rs` (`check_new_path`), `crates/rocket-import/src/openapi/sync/apply_paths.rs` (`check`)

**Interfaces:**
- Produces:
  ```rust
  pub struct RelPath(String);
  impl RelPath {
      /// The rules of `validate_rel_path`.
      pub fn parse(rel: &str) -> DomainResult<Self>;
      pub fn as_str(&self) -> &str;
  }
  // Display, AsRef<str>, Debug, Clone, PartialEq, Eq
  pub fn validate_rel_path(rel: &str) -> DomainResult<()>; // unchanged signature, calls RelPath::parse
  ```
- Consumes: nothing from Task 1.

- [ ] **Step 1: Write the failing tests** in `rel_path.rs`: `parse_accepts_nested_paths` (`requests/login.yml`, `.rocket/<an owned state file>`), `parse_rejects_empty`, `parse_rejects_backslash_and_nul`, `parse_rejects_absolute`, `parse_rejects_dot_parts` (`a//b`, `./a`, `a/../b`), `parse_rejects_a_colon`, `parse_rejects_windows_device_names` (`CON`, `a/nul.txt`), `parse_rejects_foreign_rocket_files` (`.rocket/other.txt`, `.ROCKET/x`), `parse_keeps_the_error_text` (the error for `a/../b` is `path "a/../b" has an empty, "." or ".." part`). Keep the existing `validate_rel_path` tests in `file_tx.rs` as the regression guard.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection rel_path` — Expected: compile error (red).
- [ ] **Step 3: Implement** `RelPath::parse` from the current `validate_rel_path` body; make `validate_rel_path` call it.
- [ ] **Step 4: Switch** `FsFileTx::check_new_path` and `apply_paths::check` to `RelPath::parse` (they use the returned `as_str()` where they used `rel`).
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-infra fs_file_tx` and `cargo test -j4 -p rocket-import openapi::sync` — Expected: pass.
- [ ] **Step 6: Commit** (`feat(collection): add the RelPath type`, then `refactor: check relative paths through RelPath`).

### Task 3: Docs and checks

**Files:**
- Modify: `crates/rocket-collection/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md` (D-67 done, with the scope ruling), `docs/engineering/ddd-decisions.md` only if D-04's follow-up list needs the D-67 line closed

- [ ] **Step 1: Edit the docs.** Run `cargo check -j4 --all-targets` and `cargo clippy -j4 -p rocket-collection -p rocket-infra -p rocket-import --all-targets` — Expected: no new warnings.
- [ ] **Step 2: Commit** (`docs: mark DDD Phase 6 item D-67 done`).
