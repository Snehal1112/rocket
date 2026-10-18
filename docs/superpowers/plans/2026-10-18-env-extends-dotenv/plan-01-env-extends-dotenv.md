# Environment extends: flatten the parent chain Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** An environment with `extends` inherits its parent chain wherever the backend turns an environment into variables, and a cycle is refused on save and on send.

**Architecture:** A pure `flatten` module in `rocket-environment` merges an environment with its parents. `rocket-app` calls it from the one place each consumer reads an environment (`EnvironmentService::save`, `scopes_inner`, the OAuth2 context, the two masking readers). Nothing flattened is persisted.

**Tech Stack:** Rust, `rocket-environment`, `rocket-app`.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`

## Global Constraints

- Persistence structs keep `#[serde(default, skip_serializing_if = ...)]` and gain no camelCase rename.
- No panicking unwrap calls in production paths. No `git` CLI.
- Flattening is never persisted: saving writes only the environment's own variables and the `extends` name.
- The parent is looked up by name in the same store as the child (collection or global).
- A cycle or self-reference is rejected on save and fails the send with an error naming the environments in the cycle. A missing parent only warns and the child's own variables are used.
- Inherit variables only, with their secret flag kept. Parent external secret bindings and client certificates are not inherited.
- Cargo commands are per crate: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`.

## Post-Implementation Review

Run after Task 3, on the finished backend, before the next plan starts. Each check names the evidence that settles it.

- [ ] `cargo test -j4 -p rocket-environment flatten` and `cargo test -j4 -p rocket-app inherited` pass. Evidence: the test output.
- [ ] No call site still reads an environment's variables for sending without flattening: search `crates/rocket-app/src` and `src-tauri/src` for `enabled_variables()` and `.variables` and justify each remaining hit (the editor and `apply_env_writes` stay raw). Evidence: the list of hits with a reason each.
- [ ] A cycle is refused on save and fails a send with both names in the error. Evidence: the two named tests.
- [ ] Saving a child writes only its own variables and `extends`. Evidence: `save_never_writes_inherited_variables`.
- [ ] A parent-only secret is masked in a stored conversation and in a chip. Evidence: the Task 3 tests.
- [ ] `cargo check -j4 -p rocket-environment` and `cargo check -j4 -p rocket-app` are clean, and no panicking unwrap calls were added in production paths. Evidence: the command output and a grep of the diff.

## Review Focus

- A parent variable shadowed by a child entry with `enabled: false`: the key must not resolve (test in Task 1).
- A secret variable defined only in the parent: it must be in the send's secret sets and in both masking readers (tests in Tasks 2 and 3).
- A save that edits an environment already in a cycle through a different environment: the error names the cycle (Task 2).
- An `extends` naming an environment in the other store (collection env naming a global env): treated as a missing parent, a warning (Task 1).
- A long chain (more than 20 levels, no cycle) flattens without recursion failure (Task 1).

## File Structure

- Create `crates/rocket-environment/src/flatten.rs`: `Flattened`, `FlatVariable`, `flatten_environment`, `flatten_with_repo`, `find_extends_cycle`.
- Modify `crates/rocket-environment/src/lib.rs`: export the new items.
- Modify `crates/rocket-app/src/environment_service.rs`: cycle check in `save`.
- Modify `crates/rocket-app/src/execution_service.rs` (`scopes_inner`, global read about 752-762, regular read about 789-799) and `crates/rocket-app/src/oauth2_service.rs` (`build_variable_context`, about 294).
- Modify `crates/rocket-app/src/conversation_secrets.rs` (`add_env_secrets`, about 81) and `crates/rocket-app/src/mcp_tool_service/chips.rs` (secret values, about 263).

---

### Task 1: The pure flatten module

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-environment/src/flatten.rs`
- Modify: `crates/rocket-environment/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `flatten.rs`

**Interfaces:**
- Consumes: `Environment`, `Variable` (`key`, `value`, `enabled`, `secret`), `EnvironmentRepository::get(&str) -> DomainResult<Environment>`, `DomainError::InvalidInput` through the crate's `invalid(message)` helper.
- Produces:
  - `pub struct FlatVariable { pub variable: Variable, pub source: String }`
  - `pub struct Flattened { pub variables: Vec<FlatVariable>, pub warnings: Vec<String> }` with `pub fn enabled_pairs(&self) -> Vec<(&str, &str)>`.
  - `pub fn flatten_environment(env: &Environment, lookup: &dyn Fn(&str) -> Option<Environment>) -> DomainResult<Flattened>`
  - `pub fn flatten_with_repo(env: &Environment, repo: &dyn EnvironmentRepository) -> DomainResult<Flattened>` (lookup is `repo.get(name).ok()`).
  - `pub fn find_extends_cycle(env: &Environment, lookup: &dyn Fn(&str) -> Option<Environment>) -> Option<Vec<String>>` (the names in the cycle, in chain order).

- [ ] **Step 1: Write the failing tests** in `flatten.rs`, using a `HashMap<String, Environment>` lookup closure:
  - `child_value_wins_over_parent`: parent `a=1`, child `a=2` gives `a=2`, source is the child.
  - `parent_variable_is_inherited`: parent `a=1`, child none gives `a=1`, source is the parent name.
  - `three_level_chain_flattens_furthest_ancestor_first`: root `a=1,b=1`, mid `b=2`, leaf `c=3` gives `a=1,b=2,c=3`.
  - `disabled_child_variable_shadows_parent`: parent `a=1` enabled, child `a` disabled; `enabled_pairs()` has no `a`.
  - `secret_flag_survives_inheritance`: a parent secret variable stays `secret: true`.
  - `no_extends_returns_own_variables_with_own_source`.
  - `missing_parent_warns_and_keeps_child_variables`: one warning naming the parent, child variables kept, `Ok`.
  - `parent_in_another_store_is_a_missing_parent`: lookup returns `None` for the name, same as the case above.
  - `self_extends_is_a_cycle_error`: `Err` whose message contains the environment name.
  - `two_environment_cycle_names_both`: `Err` message contains both names.
  - `a_chain_of_thirty_environments_flattens`.
  - `find_extends_cycle_returns_none_for_a_chain_and_names_for_a_cycle`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-environment flatten` fails to compile (module missing).

- [ ] **Step 3: Implement the module.** Walk the chain iteratively from the child to the root with a visited set, then fold from the root down so later layers replace by key. Keep a disabled child entry (it shadows). Order of `variables`: first appearance in the fold. Add `pub mod flatten;` and `pub use` lines in `lib.rs`.

- [ ] **Step 4: Run to verify they pass:** `cargo test -j4 -p rocket-environment flatten`, then `cargo check -j4 -p rocket-environment`. Expected: PASS.

---

### Task 2: Use the flatten on save, send and OAuth2

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/environment_service.rs` (`save`, about line 42)
- Modify: `crates/rocket-app/src/execution_service.rs` (`scopes_inner`)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (`build_variable_context`)
- Test: inline tests in those three files, using `crates/rocket-app/src/test_doubles.rs`

**Interfaces:**
- Consumes: `flatten_environment`, `flatten_with_repo`, `find_extends_cycle`, `Flattened` from Task 1.
- Produces: no new public names. `EnvironmentService::save` returns `DomainError::InvalidInput` for a cycle. `scopes_inner` fills `ctx.global_env`, `ctx.env`, `secret_keys` and `secret_values` from the flattened variables.

- [ ] **Step 1: Write the failing tests**
  - `environment_service`: `save_rejects_self_extends`, `save_rejects_cycle_through_a_saved_environment` (message names both), `save_allows_a_missing_parent` (saves, no error), `save_never_writes_inherited_variables` (the stored environment has only its own variables).
  - `execution_service`: `a_variable_inherited_from_the_parent_resolves_in_scopes`, `a_child_variable_overrides_the_parent_in_scopes`, `a_cycle_fails_scope_building_with_the_cycle_names`, `an_inherited_secret_is_in_secret_values`, `a_global_environment_inherits_from_a_global_parent`.
  - `oauth2_service`: `oauth_context_includes_inherited_variables`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-app inherited` and the other filters; FAIL on the missing behavior.

- [ ] **Step 3: Implement.**
  - `EnvironmentService::save`: after the existing validations, call `find_extends_cycle(env, &|n| repo.get(n).ok())` and return `invalid(...)` naming the cycle. The incoming `env` stands in for its own name.
  - `scopes_inner`: replace both `repo.get(name)` plus `variables.filter(enabled)` reads with `flatten_with_repo(&env, repo)`. Keep `read_scope(strict, ...)` semantics. Fill the secret sets from flattened variables with `secret: true`. A cycle is an error in strict and lenient mode. Log each warning with the crate's existing logging macro.
  - `OAuth2Service::build_variable_context`: use `flatten_with_repo(&env, self.env_repo.as_ref())` and `enabled_pairs()`.
  - `apply_env_writes` (about line 1415) must keep reading the raw environment, so it is not changed.

- [ ] **Step 4: Run to verify they pass:** `cargo test -j4 -p rocket-app inherited`, `cargo test -j4 -p rocket-app environment_service`, `cargo test -j4 -p rocket-app oauth2_service`, then `cargo check -j4 -p rocket-app`. Expected: PASS.

---

### Task 3: Inherited secrets in the masking readers

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/conversation_secrets.rs` (`add_env_secrets`)
- Modify: `crates/rocket-app/src/mcp_tool_service/chips.rs` (secret values, about line 263)
- Test: inline tests in both files

**Interfaces:**
- Consumes: `flatten_with_repo`, `Flattened`.
- Produces: no new public names. Both readers treat a parent's secret value as a secret of the child.

- [ ] **Step 1: Write the failing tests**
  - `conversation_secrets_include_an_inherited_secret`: a child with no variables, a parent with a secret; the stored text containing the parent's secret is masked.
  - `conversation_secrets_fail_closed_on_a_cycle`: a cycle makes the strict listing an error, the same as an unreadable environment file.
  - `chip_masks_an_inherited_secret`: the chip text hides the parent's secret value.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-app conversation_secrets` and `cargo test -j4 -p rocket-app chip_masks`.

- [ ] **Step 3: Implement.** In each reader, flatten the environment through its repo before collecting `secret` variables. A flatten error in the strict readers propagates, so a missed secret never leaks.

- [ ] **Step 4: Run to verify they pass:** the two commands above, then `cargo check -j4 -p rocket-app`. Expected: PASS.

Next plan: docs/superpowers/plans/2026-10-18-env-extends-dotenv/plan-02-env-extends-dotenv.md
