# rok parity B fixes 01: request guard and vault fail-closed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two security gaps the Part B review found: `rok.sendRequest` skipping the internal-host guard, and nested `rok.runRequest` sends skipping the RocketVault fail-closed check.

**Architecture:** Both fixes live in `crates/rocket-app/src/execution_service/script_host.rs`, which is a child module of `execution_service.rs` and so can call its private `check_request_guard` and `references_alias`. The vault fix adds the unresolved aliases to `PhaseState` and `ExecutionScriptHost`.

**Tech Stack:** Rust, `wiremock` (tests), `tokio`.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`; rulings in `00-plan-index.md`.

## Global Constraints

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (the work touches environment, variable and secret scope logic).
- Read `.claude/rules/rust-ddd-boundaries.md` and `crates/rocket-app/CLAUDE.md`.
- Cargo commands always carry `-j4`; never `--workspace` or `--all`. Run `cargo test -j4 -p rocket-app <filter>`.
- Never unwrap in production paths. No camelCase serde on these internal types.
- Commit through the `dev-workflow-skills:1-git-commit` skill, with a pathspec commit (no `git add -A`).

## Review Focus

- A script sends to `http://169.254.169.254/` with the policy on: rejected with the guard message, nothing reaches the executor.
- A script sends to the host the calling request already uses, policy on: allowed (the same-host exemption).
- Policy off: `sendRequest` to an internal host still works (the guard is opt-in).
- A nested request that mentions a failed alias is refused; one that does not mention it still runs.
- A nested run two levels deep (A runs B runs C) inherits the unresolved aliases.

---

### Task 1: `rok.sendRequest` obeys the request guard

**Files:**
- Modify: `crates/rocket-app/src/execution_service/script_host.rs` (`send_request`, lines 96-109)
- Test: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Consumes: `RequestExecutionService::check_request_guard(&self, original_url: &str, mutated_url: &str, policy: &rocket_workspace::RequestGuardPolicy) -> DomainResult<()>`; `ExecuteRequestInput.request_guard_policy` and `.url`.
- Produces: nothing new. `send_request` returns `HostError::Failed("rok.sendRequest: <guard message>")` on a block.

- [ ] **Step 1: Write the failing tests** in `script_host_tests.rs`, in the style of the existing end-to-end tests there (look at how one builds a service with a `wiremock` executor and a pre-request script that calls `rok.sendRequest`):
  - `send_request_blocks_an_internal_host_when_the_policy_is_on`: policy `RequestGuardPolicy { block_script_redirects_to_internal_hosts: true, ..Default::default() }`, script `await rok.sendRequest({ url: 'http://169.254.169.254/latest' })`. Assert the script error contains `blocked: script redirected request to internal host '169.254.169.254'` and `rok.sendRequest`, and that the mock executor received no call for that URL.
  - `send_request_to_the_calling_requests_own_host_is_allowed_with_the_policy_on`: the request URL and the script URL share the mock server host (`127.0.0.1`), policy on. Assert the mock saw the script call and the script finished without error.
  - `send_request_to_an_internal_host_is_allowed_when_the_policy_is_off`: default policy, same script as the first test aimed at the mock server. Assert success.
  - `run_request_nested_set_url_is_guarded_by_the_policy`: a saved nested request whose pre-request script does `req.setUrl('http://169.254.169.254/x')`, policy on, called through `rok.runRequest`. Assert the call rejects with the guard message (this pins ruling 1: no second check is needed).

- [ ] **Step 2: Run to verify the first test fails**

Run: `cargo test -j4 -p rocket-app send_request_blocks_an_internal_host`
Expected: FAIL (the call is not blocked). The second and third tests already pass; the fourth passes if the policy reaches nested runs, which is the point of the test.

- [ ] **Step 3: Implement the check at the top of `ExecutionScriptHost::send_request`**, before `http_request_for`: call `self.svc.check_request_guard(&self.input.url, &request.url, &self.input.request_guard_policy)` and map `Err(e)` to `HostError::Failed(format!("rok.sendRequest: {e}"))`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-app execution_service::script_host`
Expected: PASS, including the existing script host tests.

- [ ] **Step 5: Commit** (`fix(scripting): apply the request guard to rok.sendRequest`).

---

### Task 2: nested runs fail closed on an unresolved vault alias

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`PhaseState` at line 355, `begin_phases`, `script_host`, `execute_unpinned`, `execute_capturing_unpinned`, `execute_nested`)
- Modify: `crates/rocket-app/src/execution_service/script_host.rs` (`ExecutionScriptHost`, `run_request`)
- Test: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct UnresolvedAlias { pub alias: String, pub message: String }` in `execution_service.rs` (derive `Debug, Clone`).
  - `PhaseState.unresolved_aliases: Arc<Vec<UnresolvedAlias>>`, empty after `begin_phases`.
  - `ExecutionScriptHost.unresolved_aliases: Arc<Vec<UnresolvedAlias>>`, filled by `script_host()` from the state.
  - `execute_nested` takes a new last parameter `unresolved: &Arc<Vec<UnresolvedAlias>>` and stores it on the nested state, so deeper levels inherit it.

- [ ] **Step 1: Write the failing tests** in `script_host_tests.rs`. Build the environment with one external-secret binding whose vault fetch fails (reuse the fake vault fetcher the existing vault fail-closed tests for `execute` use; grep `references_alias` and `vault` in `execution_service/` tests to find it):
  - `run_request_refuses_a_nested_request_that_uses_a_failed_alias`: the outer request never mentions the alias; the saved nested request has the header `X-Key: {{vault.key}}`; the outer script does `await rok.runRequest('nested')`. Assert the script error contains `rok.runRequest:` and the vault error text, and that the executor got no request for the nested URL.
  - `run_request_runs_a_nested_request_that_does_not_use_the_failed_alias`: same environment, nested request without the alias. Assert it runs.
  - `run_request_inherits_failed_aliases_two_levels_deep`: A runs B (no alias) which runs C (uses the alias). Assert C is refused.

- [ ] **Step 2: Run to verify the first and third fail**

Run: `cargo test -j4 -p rocket-app run_request_refuses_a_nested_request`
Expected: FAIL (the nested request is sent with the placeholder unresolved).

- [ ] **Step 3: Implement.**
  - Add `UnresolvedAlias` and the `PhaseState` field. `begin_phases` sets it to an empty `Arc`.
  - `execute_unpinned` converts each `UnresolvedBinding` it tolerated (the ones it only warns about) into an `UnresolvedAlias { alias, message: error.to_string() }`. It passes them to the pipeline through a new `pub(crate) async fn execute_with_unresolved(&self, input, external_secrets, unresolved: Vec<UnresolvedAlias>)`; `execute_with_external_secrets` becomes a call to it with an empty vec so its other callers (Flow, vault certificates tests) stay unchanged. `execute_capturing_unpinned` gains an `unresolved: Vec<UnresolvedAlias>` parameter and assigns `state.unresolved_aliases = Arc::new(unresolved)` right after `begin_phases`. `execute_capturing` passes an empty vec.
  - `script_host()` copies `Arc::clone(&state.unresolved_aliases)` into the host.
  - In `ExecutionScriptHost::run_request`, after `build_step_input` and before `execute_nested`, loop over `self.unresolved_aliases`; when `self.svc.references_alias(&nested, &u.alias, &self.external_secrets)` is true, return `HostError::Failed(format!("rok.runRequest: {}", u.message))`. Pass `&self.unresolved_aliases` to `execute_nested`.

- [ ] **Step 4: Run to verify they pass, then the wider suite for the crate area**

Run: `cargo test -j4 -p rocket-app execution_service`
Expected: PASS (the review run showed 474 tests green before the change).

- [ ] **Step 5: Run `cargo check -j4` and `cargo clippy -j4 -p rocket-app`, then commit** (`fix(scripting): refuse nested runs that use an unresolved vault alias`).

---

## Next plan to execute

When both tasks are complete and the ledger shows "Task 2: complete", move on to `02-limits-and-docs.md` without asking.
