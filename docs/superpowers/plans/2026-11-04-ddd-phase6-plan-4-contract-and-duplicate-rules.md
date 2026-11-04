# DDD Phase 6, Plan 4: contract scope and version rules, duplicate-request naming (D-65, D-66)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The contract scope match, the patch-version bump and the duplicate-request naming scheme live in `rocket-collection`, so the two services only call them.

**Architecture:** Add `ContractScope::covers` and `bump_patch_version` to `rocket-collection/src/contract/`, and a new `rocket-collection/src/duplicate.rs` with the pure naming and copy rules. `ContractService` and `CollectionService` call them and delete their private copies. The retry loop (exists check, exclusive create, `AlreadyExists` retry, give up after 1000) stays in `CollectionService`. Tests go into the domain crate first.

**Tech Stack:** Rust, `rocket-collection`, `rocket-app`.

**Spec:** `docs/roadmaps/ddd-roadmap.md` items D-65 and D-66; `docs/engineering/ddd-decisions.md` D-04 (narrow model, no new newtypes).

## Global Constraints

- Behavior is unchanged except one ruled fix: a patch number of `u32::MAX` no longer overflows (it takes the `-copy` fallback). Before, `patch + 1` panicked in a debug build and wrapped to 0 in a release build.
- `Request` and `ContractScope` keep their fields and serde shape. No wire or file format change.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (Task 2 touches the request model and file names).

## Review Focus

- A folder scope covers whole path components only: scope `auth` covers `auth/login.yml` and does not cover `authz/login.yml`.
- A request scope covers exactly its own path; a collection scope covers every path.
- `1.2.3` becomes `1.2.4`; `1.2.4294967295`, `1.0`, `v2`, `1.2.3-beta` and the empty string take the `-copy` fallback without a panic.
- The first duplicate is `Login copy`, then `Login copy 2`; a source with a `.yaml` or `.json` extension gets a `.yml` copy; a request in the collection root has no leading `/`.
- The duplicate gets a new uid, no `seq`, no `file_name`, and keeps every other field.
- All 1000 candidate names taken still ends in `Conflict`.

---

### Task 1: Contract rules (D-65)

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs` (`impl ContractScope`), `crates/rocket-collection/src/contract/mod.rs`
- Create: `crates/rocket-collection/src/contract/version.rs`
- Modify: `crates/rocket-app/src/contract_service.rs` (delete `covers` and `bump_patch_version`, move their tests)

**Interfaces:**
- Produces: `ContractScope::covers(&self, request_path: &Path) -> bool`
- Produces: `pub fn bump_patch_version(version: &str) -> String` in `contract/version.rs`, re-exported from `rocket_collection::contract`.
- Consumes: nothing from other tasks.

- [ ] **Step 1: Write the failing tests** in `types.rs` tests: `folder_scope_covers_whole_components_only` (`auth` covers `auth/login.yml`, not `authz/login.yml`, covers `auth` itself), `request_scope_covers_only_its_own_path`, `collection_scope_covers_every_path`. In `version.rs`: `bump_patch_version_increments_the_patch` (`1.2.3` to `1.2.4`, `0.0.0` to `0.0.1`), `bump_patch_version_falls_back_for_other_shapes` (`v2`, `1.0`, `` and `1.2.3-beta` give `<v>-copy`), `bump_patch_version_does_not_overflow` (`1.2.4294967295` gives `1.2.4294967295-copy`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection contract` — Expected: compile error (red).
- [ ] **Step 3: Implement** both. `covers` is the existing `match` on the three variants. `bump_patch_version` is the existing function with `checked_add(1)` and the same fallback.
- [ ] **Step 4: Switch** the three `covers(...)` calls and the one `bump_patch_version(...)` call in `contract_service.rs` to the new items; delete the private functions and the old `bump_patch_version_handles_edge_cases` test.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-collection` and `cargo test -j4 -p rocket-app contract` — Expected: pass.
- [ ] **Step 6: Commit** (`feat(collection): add contract scope and version rules`, then `refactor(app): use the contract rules from rocket-collection`).

### Task 2: Duplicate-request rules (D-66)

**Files:**
- Create: `crates/rocket-collection/src/duplicate.rs`
- Modify: `crates/rocket-collection/src/lib.rs`, `crates/rocket-app/src/collection_service.rs` (`duplicate_request_unlocked`)

**Interfaces:**
- Produces:
  ```rust
  pub const MAX_DUPLICATE_ATTEMPTS: u32 = 1000;
  pub struct DuplicateCandidate { pub path: String, pub name: String }
  /// n = 1 gives " copy", n >= 2 gives " copy {n}". Keeps the source folder and file stem.
  pub fn duplicate_candidate(source_path: &str, source_name: &str, n: u32) -> DuplicateCandidate;
  /// A clone of `source` with a new uid, `name`, `seq = None` and `file_name = None`.
  pub fn duplicate_of(source: &Request, name: &str) -> Request;
  ```
- Consumes: `generate_uid` and `Request` from `rocket-collection`.

- [ ] **Step 1: Write the failing tests** in `duplicate.rs`: `first_candidate_is_copy` (`auth/Login.yml`, `Login`, 1 gives `auth/Login copy.yml` and `Login copy`), `later_candidates_are_numbered` (n = 2 gives `Login copy 2.yml`), `root_request_has_no_leading_slash`, `yaml_and_json_sources_get_a_yml_copy` (`auth/Foo.json` gives `auth/Foo copy.yml`), `the_file_stem_is_used_for_the_path_and_the_name_for_the_name` (`auth/ab.yml` with name `A/B: C` gives path `auth/ab copy.yml`, name `A/B: C copy`), `duplicate_of_resets_identity_and_keeps_the_rest` (new uid, `seq` and `file_name` cleared, every other field equal).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection duplicate` — Expected: compile error (red).
- [ ] **Step 3: Implement** both functions from the code now in `duplicate_request_unlocked`.
- [ ] **Step 4: Replace** the suffix, path and copy construction in `duplicate_request_unlocked` with the two functions and `MAX_DUPLICATE_ATTEMPTS`. The loop, `path_exists` check, exclusive create and `Conflict` error stay.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-collection` and `cargo test -j4 -p rocket-app duplicate_request` — Expected: pass (the 14 existing `duplicate_request_*` service tests are the regression guard).
- [ ] **Step 6: Commit** (`feat(collection): add duplicate-request rules`, then `refactor(app): name duplicates through rocket-collection`).

### Task 3: Docs and checks

**Files:**
- Modify: `crates/rocket-collection/CLAUDE.md`, `docs/roadmaps/ddd-roadmap.md` (D-65 and D-66 done, D-67 stays open with a note that D-04 does not ask for it)

- [ ] **Step 1: Edit the docs.** Run `cargo check -j4 --all-targets` and `cargo clippy -j4 -p rocket-collection -p rocket-app --all-targets` — Expected: no new warnings.
- [ ] **Step 2: Commit** (`docs: mark DDD Phase 6 items D-65 and D-66 done`).
