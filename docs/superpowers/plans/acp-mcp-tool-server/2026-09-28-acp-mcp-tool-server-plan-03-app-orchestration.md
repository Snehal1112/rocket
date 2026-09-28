# ACP MCP Tool Server — Plan 03: rocket-app Orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thread a `RunSource` tag through every `ExecuteRequestInput` construction path, add the `McpToolService` that lets an ACP agent list/run requests, edit scripts, and read/write non-secret env vars (gated by a per-collection opt-in flag and audited via a domain event), and make `AcpSessionService::start_session` collection-aware so it can (eventually) attach MCP servers to a session.

**Architecture:** All three deliverables live in `rocket-app`, the orchestration crate that already wires `rocket-collection`/`rocket-environment`/`rocket-http`/`rocket-history` into use-case services and injects `Box<dyn Trait>`/`Arc<dyn Trait>` dependencies — no filesystem or process I/O is added here. `McpToolService` is a new, independent service following the same trait-injection pattern as every other service in this crate (see `crates/rocket-app/CLAUDE.md`). `RunSource` threading touches five existing call sites that already build `ExecuteRequestInput`. `AcpSessionService` gains one new constructor dependency (`Arc<dyn CollectionRepository>`) and one new `start_session` parameter (`collection: Option<&str>`).

**Tech Stack:** Rust, `rocket-app`/`rocket-collection`/`rocket-environment`/`rocket-history`/`rocket-scripting`/`rocket-shared`/`rocket-acp` domain crates, `tokio` async, existing in-crate test-double patterns (`crate::test_doubles`), `cargo test -p rocket-app -j4` / `cargo check --workspace -j4` as verification gates (this repo requires `-j4` on every cargo invocation).

**Spec:** [docs/superpowers/specs/2026-09-28-acp-mcp-tool-server-design.md](../../specs/2026-09-28-acp-mcp-tool-server-design.md)

**Plan index (locked interface contracts):** [00-plan-index.md](00-plan-index.md)

**Depends on:** Plan 01 (domain contracts — `rocket_shared::RunSource`, `rocket_acp::McpServerSpec`, `AcpSessionClient::start_session`'s 5th `mcp_servers` parameter, `CollectionSettings.agent_autonomy_enabled`, `CollectionRepository::save_request_script` + `RequestScriptPhase`, `HistoryEntry::with_run_source`) and Plan 02 (infra implementations of `save_request_script` on `FsCollectionRepo`/`SharedPathCollectionRepo`). Both are assumed landed and stable per the plan index, even though their files may not exist yet at plan-authoring time.

## Global Constraints

- This repo requires `-j4` on every `cargo` invocation (`cargo check --workspace -j4`, `cargo test -p rocket-app -j4`).
- No `unwrap()` in production paths (test code may use `.expect("<reason>")` with a message, matching this crate's existing test style).
- No git CLI shell-outs (not applicable to this plan — no git code touched).
- `camelCase` `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on persistence structs — `ExecuteRequestInput` already carries this rename (it is an IPC DTO consumed directly by the Tauri `execute_request`/`run_load_test_command` commands), so its new field follows the same attribute; `McpToolService`'s own `McpRequestEntry`/`McpRunResult` are plain internal DTOs, not IPC types, so they get no serde rename attribute at all per this plan (Plan 04/05 decide their own wire representation when they expose these over MCP).
- Commits use Conventional Commits format (`feat:`, `fix:`, `chore:`) — per this repository's own instructions, every commit in this plan must be created via the `dev-workflow-skills:1-git-commit` plugin skill, not a freehand `git commit -m`.
- Rust DDD boundaries: this plan stays entirely inside `rocket-app`; no filesystem/process I/O is added (that is `rocket-infra`'s job in Plan 02/04/05).

## Deviations from the Plan Index (found during file verification)

The task brief for this plan required reading every referenced file in full before writing code against it, rather than trusting the index's paraphrased signatures. Four real discrepancies surfaced. Each is called out again inline at the task that resolves it, and summarized here up front:

1. **`runner_sequence::build_step_input` is not Runner-exclusive.** The index states it "sets `run_source: RunSource::Runner` directly in the struct literal... no new function parameter — this function's only purpose is building runner-driven requests." This is factually wrong: `flow_execution_service::build_execute_request_input` (the Flow node executor's real production code path, `flow_execution_service.rs:80-116`) also calls `build_step_input` to build its `ExecuteRequestInput`, specifically so it does not duplicate the mapping. Hard-coding `RunSource::Runner` inside `build_step_input` would mislabel every Flow-executed request's history entry as a Collection Runner run. **Fix:** `build_step_input` gains a `run_source: rocket_shared::RunSource` parameter; the Collection Runner's call site passes `RunSource::Runner`, the Flow executor's call site passes `RunSource::Flow`. See Task 2.
2. **`run_load_test`/`run_load_test_v2_command` build no `ExecuteRequestInput` in Rust at all.** The index says to "check `RequestExecutionService::run_load_test`... for where its `ExecuteRequestInput`s are built" and set `RunSource::LoadTest` there. In the live code, both `run_load_test_command` and `run_load_test_v2_command` (`src-tauri/src/commands/load_test.rs:17-38`) take `input: ExecuteRequestInput` as a raw Tauri IPC parameter — exactly like the manual `execute_request` command — and `RequestExecutionService::run_load_test`/`LoadTestService::run` pass it straight to `resolve_request`/`run_load_test_v2` without ever calling `finish_phases` or saving a `HistoryEntry`. There is no Rust call site to set `RunSource::LoadTest`, and doing so would have no observable effect today since load-test runs never persist a `HistoryEntry`. **Fix:** this plan does not add a `RunSource::LoadTest` call site (none exists); it only makes the two existing test-only `ExecuteRequestInput` struct literals in `load_test_service.rs`/`execution_service.rs` compile with the new field. See Task 2, Step 7.
3. **`get_env_var`/`set_env_var`/`get_test_results` cannot be implemented with the index's exact parameter lists.** `EnvironmentRepositoryFactory::for_collection(&self, collection: &str)` (`crates/rocket-environment/src/repository.rs:18-20`) is the only way this codebase resolves a named environment's `EnvironmentRepository` — confirmed against `RequestExecutionService::regular_env_repo` (`execution_service.rs:269-277`) and the Tauri `environments.rs` commands (`env_service_for(collection, ws_path)`, `src-tauri/src/commands/environments.rs:13-31`), which always resolve an environment by `(collection, name)`, never by `name` alone (per-collection `environments/` directories, no global lookup by bare name). The index's locked signatures `get_env_var(session_id, environment_name, key)` / `set_env_var(session_id, environment_name, key, value)` / `get_test_results(session_id, request_path)` omit `collection` entirely, so none of them can call `for_collection` or `check_autonomy_enabled` (which itself needs a collection to call `collection_repo.get_settings(collection)`). **Fix:** all three gain an explicit `collection: &str` parameter, consistent with `list_collection_requests`/`run_request`/`edit_script`, which already take one. See Task 3.
4. **`AcpSessionService::start_session`'s "enabled" branch has nothing to build yet.** The design spec says every tool call re-checks the flag and Plan 04/05 attach real MCP servers when it is on; but Plan 04 (HTTP backend) and Plan 05 (Stdio shim) do not exist yet, so there is no way to construct a real, non-empty `Vec<McpServerSpec>` in this plan even when a collection has opted in. **Fix:** `start_session` still checks the flag (and still propagates a `get_settings` error, so a broken collection fails loudly rather than silently), but always resolves to an empty server list in this plan — this is complete, correct behavior for what Plan 03 can deliver, not a placeholder: chat-only mode is exactly subproject C's existing behavior, and it is what every session gets today regardless of the flag. Plan 04/05 change only the "enabled" branch's return value later. See Task 4.

## Review Focus

- A tool call against a collection with `agent_autonomy_enabled` false must be refused by **every** one of `McpToolService`'s 6 methods, including the read-only ones (`list_collection_requests`, `get_test_results`) — covered by a table-driven test in Task 3.
- `get_env_var`/`set_env_var` must return the exact same error string for "key not found" and "key is secret" (an oracle risk otherwise) — covered by a dedicated test in Task 3, including a case-sensitive-key variant.
- Toggling `agent_autonomy_enabled` off mid-session must block the very next tool call (the flag is read fresh on every call, never cached) — covered by a dedicated test in Task 3.
- Every `ExecuteRequestInput` construction site must tag the correct `RunSource` (`Manual` for the plain Tauri command and load test, `Runner` for the Collection Runner, `Flow` for the Flow executor, `Agent` for `McpToolService::run_request`) and that tag must survive into the saved `HistoryEntry` — covered across Task 2's and Task 3's tests, including one full `execute()`-to-saved-`HistoryEntry` round trip.
- Two items from the plan index's own Review Focus are **intentionally not covered by this plan** and are called out here rather than silently skipped: (1) "the Stdio bridge process must never receive the token via argv" — the Stdio bridge does not exist until Plan 05; (2) "concurrent `run_request` and a manual UI send against the same request file must not corrupt either write" — the locking guarantee lives in `rocket-infra`'s `save_request_script`/`save_request` implementations (Plan 02), and a real concurrency test needs the filesystem-backed repo, not this plan's in-memory test doubles; Plan 06's integration tests are the right place for it.

---

## Task 1: Fix pre-existing `rocket-app` test doubles for the new `save_request_script` trait method

Plan 01 adds `fn save_request_script(&self, collection: &str, request_path: &str, phase: RequestScriptPhase, body: String) -> DomainResult<()>;` to the `CollectionRepository` trait and implements it on the two real `rocket-infra` repos (`FsCollectionRepo`, `SharedPathCollectionRepo`). Plan 01's own stated scope is `rocket-shared`/`rocket-acp`/`rocket-collection`/`rocket-history` — it does not touch `rocket-app`. Every `impl CollectionRepository for <Mock>` block that already exists inside `rocket-app`'s own `#[cfg(test)]` modules (and one `#[cfg(test)]`-gated production-adjacent module) is missing this new required trait method, so `cargo test -p rocket-app -j4` will not compile once Plan 01 lands, until this task runs. This task is a pure, mechanical compilation prerequisite for every later task in this plan.

**Files:**
- Modify: `crates/rocket-app/src/test_doubles.rs:41-123` (`InMemoryCollectionRepo`), `:126-206` (`SharedCollectionRepo`)
- Modify: `crates/rocket-app/src/oauth2_service.rs:497-577` (`StubCollectionRepo`, test module)
- Modify: `crates/rocket-app/src/collection_service.rs:296-428` (`MockCollectionRepo`, test module)
- Modify: `crates/rocket-app/src/load_test_service.rs:205-283` (`StubCollectionRepo`, test module)
- Modify: `crates/rocket-app/src/execution_service.rs:1859-1891` (`StubCollectionRepo`), `:3301-3321` (`RecordingCollectionRepo`), `:3386-3477` (`SharedCollectionRepo`, all test module)
- Modify: `crates/rocket-app/src/contract_service.rs:1137-1224` (`MockCollectionRepo`, test module)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:693-784` (`FakeCollectionRepo`, test module)

**Interfaces:**
- Consumes: `rocket_collection::RequestScriptPhase` (Plan 01, assumed to derive at least `Debug, Clone, Copy, PartialEq, Eq` matching every other small enum in that crate, e.g. `SandboxMode` in `crates/rocket-collection/src/settings.rs:24-30`) and the trait method signature `fn save_request_script(&self, collection: &str, request_path: &str, phase: rocket_collection::RequestScriptPhase, body: String) -> DomainResult<()>` (Plan 01).
- Produces: nothing new consumed by later tasks — this task only keeps the crate's test target compiling. Every implementor below returns `Ok(())` (a no-op stub matching every other unused mutation method already in these mocks), except `test_doubles.rs`'s `InMemoryCollectionRepo`/`SharedCollectionRepo` pair, which follow their existing "one owns state, one delegates" split.

- [ ] **Step 1: Add the stub to `test_doubles.rs`'s two `CollectionRepository` impls**

In `crates/rocket-app/src/test_doubles.rs`, find `InMemoryCollectionRepo`'s impl block (ends at line 123):

```rust
    fn save_request_variables(
        &self,
        _: &str,
        _: &str,
        _: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        Ok(())
    }
}
```

Replace with:

```rust
    fn save_request_variables(
        &self,
        _: &str,
        _: &str,
        _: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        Ok(())
    }
    fn save_request_script(
        &self,
        _: &str,
        _: &str,
        _: rocket_collection::RequestScriptPhase,
        _: String,
    ) -> DomainResult<()> {
        Ok(())
    }
}
```

Then find `SharedCollectionRepo`'s delegating impl block (ends at line 206):

```rust
    fn save_request_variables(
        &self,
        a: &str,
        b: &str,
        c: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.0.save_request_variables(a, b, c)
    }
}
```

Replace with:

```rust
    fn save_request_variables(
        &self,
        a: &str,
        b: &str,
        c: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.0.save_request_variables(a, b, c)
    }
    fn save_request_script(
        &self,
        a: &str,
        b: &str,
        c: rocket_collection::RequestScriptPhase,
        d: String,
    ) -> DomainResult<()> {
        self.0.save_request_script(a, b, c, d)
    }
}
```

- [ ] **Step 2: Add the stub to `oauth2_service.rs`'s `StubCollectionRepo`**

In `crates/rocket-app/src/oauth2_service.rs`, find (inside the `#[cfg(test)] mod tests` block, `impl CollectionRepository for StubCollectionRepo`):

```rust
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

Replace with:

```rust
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn save_request_script(
            &self,
            _: &str,
            _: &str,
            _: rocket_collection::RequestScriptPhase,
            _: String,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

- [ ] **Step 3: Add the stub to `collection_service.rs`'s `MockCollectionRepo`**

In `crates/rocket-app/src/collection_service.rs`, find (inside `impl CollectionRepository for MockCollectionRepo`):

```rust
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

Replace with the same pattern as Step 2 (identical replacement body), applied to this file's occurrence.

- [ ] **Step 4: Add the stub to `load_test_service.rs`'s `StubCollectionRepo`**

In `crates/rocket-app/src/load_test_service.rs`, find (inside `impl CollectionRepository for StubCollectionRepo`):

```rust
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

Replace with the same pattern as Step 2, applied to this file's occurrence.

- [ ] **Step 5: Add the stub to `execution_service.rs`'s three `CollectionRepository` impls**

In `crates/rocket-app/src/execution_service.rs`, `StubCollectionRepo`'s impl block ends with:

```rust
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

Apply the Step 2 pattern here too (own-state stub).

`RecordingCollectionRepo`'s impl block ends with the identical own-state `Ok(())` form — apply the same pattern.

`SharedCollectionRepo(Arc<RecordingCollectionRepo>)`'s delegating impl block ends with:

```rust
        fn save_request_variables(
            &self,
            a: &str,
            b: &str,
            c: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0.save_request_variables(a, b, c)
        }
    }
```

Replace with:

```rust
        fn save_request_variables(
            &self,
            a: &str,
            b: &str,
            c: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0.save_request_variables(a, b, c)
        }
        fn save_request_script(
            &self,
            a: &str,
            b: &str,
            c: rocket_collection::RequestScriptPhase,
            d: String,
        ) -> DomainResult<()> {
            self.0.save_request_script(a, b, c, d)
        }
    }
```

This file has three separate `impl CollectionRepository for ...` blocks — make all three edits (`StubCollectionRepo`, `RecordingCollectionRepo`, `SharedCollectionRepo`) before moving on.

- [ ] **Step 6: Add the stub to `contract_service.rs`'s `MockCollectionRepo`**

Same Step 2 pattern, applied to `crates/rocket-app/src/contract_service.rs`'s occurrence.

- [ ] **Step 7: Add the stub to `flow_execution_service.rs`'s `FakeCollectionRepo`**

In `crates/rocket-app/src/flow_execution_service.rs`, find (inside `impl CollectionRepository for FakeCollectionRepo`):

```rust
        fn save_request_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
    }
```

Replace with:

```rust
        fn save_request_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
        fn save_request_script(
            &self,
            _c: &str,
            _p: &str,
            _phase: rocket_collection::RequestScriptPhase,
            _body: String,
        ) -> DomainResult<()> {
            unimplemented!()
        }
    }
```

This matches this file's existing convention: unused mutation methods on this particular mock already `unimplemented!()` rather than `Ok(())`, and no test in this file exercises `save_request_script`.

- [ ] **Step 8: Verify the crate compiles and its full test suite still passes**

Run: `cargo test -p rocket-app -j4`
Expected: builds successfully and every pre-existing test still passes (this task adds no new tests of its own — it is a pure compilation fix).

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` plugin skill (per this repository's global instructions) to commit the 7 modified files with a `chore:`-prefixed message describing the `save_request_script` stub additions.

---

## Task 2: Thread `RunSource` through `ExecuteRequestInput` and every execution path

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:26-77` (`ExecuteRequestInput` struct), `:1337-1421` (`finish_phases`), `:1939-1962` (`sample_input` test helper)
- Modify: `crates/rocket-app/src/runner_sequence.rs:112-142` (`build_step_input`), `:270-347` (its 3 existing tests)
- Modify: `crates/rocket-app/src/collection_runner_service.rs:367-382` (`run_step`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:80-116` (`build_execute_request_input`)
- Modify: `crates/rocket-app/src/load_test_service.rs:316-337` (test-only `ExecuteRequestInput` literal)

**Interfaces:**
- Consumes: `rocket_shared::RunSource` (Plan 01: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)] #[serde(rename_all = "snake_case")] pub enum RunSource { #[default] Manual, Runner, LoadTest, Flow, Agent }`), `rocket_history::HistoryEntry::with_run_source(self, source: rocket_shared::RunSource) -> Self` (Plan 01).
- Produces: `ExecuteRequestInput.run_source: rocket_shared::RunSource` (new public field, `#[serde(default)]`), `runner_sequence::build_step_input(item: &RunItem, collection: &str, environment_name: Option<&str>, global_env_name: Option<&str>, request_guard_policy: RequestGuardPolicy, run_source: rocket_shared::RunSource) -> ExecuteRequestInput` (new trailing parameter — **deviates from the plan index**, see "Deviations from the Plan Index" item 1 above). Task 3 (`McpToolService::run_request`) calls this updated `build_step_input` signature directly, passing `rocket_shared::RunSource::Agent`.

- [ ] **Step 1: Add the `run_source` field to `ExecuteRequestInput`**

In `crates/rocket-app/src/execution_service.rs`, inside `pub struct ExecuteRequestInput { ... }` (starts at line 28), add a new field at the end, right before the closing brace of the struct (after `request_guard_policy`):

```rust
    /// Opt-in per-workspace policy: when a BeforeRequest script redirects the
    /// request via req.setUrl(), validate the new host against a blocklist of
    /// internal/loopback ranges before dispatch. Defaults to fully permissive.
    #[serde(default)]
    pub request_guard_policy: rocket_workspace::RequestGuardPolicy,
    /// Who initiated this execution. Defaults to `Manual` for any caller
    /// that does not set it explicitly (the plain Tauri `execute_request`
    /// command and load test commands), which is correct for those two
    /// paths. Every history entry this run produces carries this tag.
    #[serde(default)]
    pub run_source: rocket_shared::RunSource,
```

- [ ] **Step 2: Chain `with_run_source` onto the `HistoryEntry` built in `finish_phases`**

In `crates/rocket-app/src/execution_service.rs`, `finish_phases` (around line 1395), find:

```rust
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &redacted_url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
```

Replace with:

```rust
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &redacted_url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        )
        .with_run_source(input.run_source);
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
```

- [ ] **Step 3: Write a failing test proving `execute()` saves the input's `run_source` into history**

In `crates/rocket-app/src/execution_service.rs`'s `#[cfg(test)] mod tests` block, add (near `history_entry_redacts_external_secret_value_from_the_url`, reusing the same construction pattern):

```rust
    #[tokio::test]
    async fn history_entry_carries_the_input_run_source() {
        let history_repo = Box::new(MockHistoryRepo::new());
        let history_arc = history_repo.saved_entries_handle();

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("prod"))),
            Arc::new(MockExecutor::new(200)),
            history_repo,
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let mut input = sample_input("https://api.example.com/ping", None);
        input.run_source = rocket_shared::RunSource::Agent;

        svc.execute(input).await.expect("execute");

        let saved = history_arc.lock().expect("lock saved entries");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].run_source, rocket_shared::RunSource::Agent);
    }
```

Run: `cargo test -p rocket-app -j4 history_entry_carries_the_input_run_source`
Expected: FAIL to compile — `ExecuteRequestInput` has no `run_source` field yet if Step 1 were skipped, and `sample_input` (Step 8 below) does not build without its own field added. Since Steps 1 and 8 are both in this task, run this after Step 8 instead if working sequentially; if working test-first, expect a compile error mentioning the missing field until Steps 1 and 8 land.

- [ ] **Step 4: Add the `run_source` parameter to `build_step_input` and use it in the struct literal**

In `crates/rocket-app/src/runner_sequence.rs`, change the function signature (around line 112):

```rust
pub fn build_step_input(
    item: &RunItem,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    request_guard_policy: rocket_workspace::RequestGuardPolicy,
) -> ExecuteRequestInput {
```

to:

```rust
pub fn build_step_input(
    item: &RunItem,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    request_guard_policy: rocket_workspace::RequestGuardPolicy,
    run_source: rocket_shared::RunSource,
) -> ExecuteRequestInput {
```

and add `run_source,` as the last field in the `ExecuteRequestInput { ... }` literal this function returns (right after `request_guard_policy,`):

```rust
        actions: request.actions.clone(),
        request_guard_policy,
        run_source,
    }
```

Also update this function's doc comment (right above `pub fn build_step_input`) to note it is shared by two callers, not Runner-exclusive:

```rust
/// Builds the execution input for one run step.
///
/// Mirrors what the Request tab sends for a single send: request-level auth
/// (collection auth is merged later inside `resolve_request`), the saved
/// settings mapped onto `RequestOptions`, and all three script phases.
/// `request.runtime_auth` is deliberately ignored — the single-send path does
/// not consume it either, and the runner must not diverge from it.
///
/// `request_guard_policy` is the workspace's SSRF guard policy (see
/// `request_guard.rs` / Item 6's request-mutation host guard spec) — the
/// runner must apply the same policy to every step's BeforeRequest script as
/// a single send would, not silently default to permissive.
///
/// `run_source` is passed in, not hard-coded, because this function has two
/// callers: `collection_runner_service::run_step` (passes `Runner`) and
/// `flow_execution_service::build_execute_request_input` (passes `Flow`).
/// Hard-coding either value here would mislabel the other caller's history
/// entries.
pub fn build_step_input(
```

- [ ] **Step 5: Update `build_step_input`'s 3 existing test call sites and add one new test**

In `crates/rocket-app/src/runner_sequence.rs`'s `#[cfg(test)] mod tests`, update each existing call:

```rust
        let input = build_step_input(
            &item,
            "my-api",
            Some("dev"),
            Some("shared-global"),
            rocket_workspace::RequestGuardPolicy::default(),
        );
```

to:

```rust
        let input = build_step_input(
            &item,
            "my-api",
            Some("dev"),
            Some("shared-global"),
            rocket_workspace::RequestGuardPolicy::default(),
            rocket_shared::RunSource::Runner,
        );
```

(in `step_input_carries_scripts_path_and_scope_names`), and similarly append `rocket_shared::RunSource::Runner,` as the last argument to the `build_step_input(...)` calls in `step_input_maps_request_settings_onto_request_options` and `step_input_carries_the_request_guard_policy`.

Then add a new test asserting the parameter is not silently dropped:

```rust
    #[test]
    fn step_input_carries_the_run_source_it_was_given() {
        let item = RunItem {
            name: "Login".into(),
            request_path: "login.yml".into(),
            request: req("Login", "login.yml"),
        };

        let input = build_step_input(
            &item,
            "my-api",
            None,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
            rocket_shared::RunSource::Flow,
        );

        assert_eq!(input.run_source, rocket_shared::RunSource::Flow);
    }
```

Run: `cargo test -p rocket-app -j4 runner_sequence`
Expected: PASS (4 pre-existing tests plus the new one).

- [ ] **Step 6: Update the Collection Runner's call site to pass `RunSource::Runner`**

In `crates/rocket-app/src/collection_runner_service.rs`, `run_step` (around line 376), find:

```rust
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
            input.request_guard_policy.clone(),
        );
```

Replace with:

```rust
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
            input.request_guard_policy.clone(),
            rocket_shared::RunSource::Runner,
        );
```

- [ ] **Step 7: Update the Flow executor's call site to pass `RunSource::Flow`, and add a test**

In `crates/rocket-app/src/flow_execution_service.rs`, `build_execute_request_input` (around line 109), find:

```rust
    Ok(build_step_input(
        &item,
        collection,
        environment_name,
        None,
        rocket_workspace::RequestGuardPolicy::default(),
    ))
```

Replace with:

```rust
    Ok(build_step_input(
        &item,
        collection,
        environment_name,
        None,
        rocket_workspace::RequestGuardPolicy::default(),
        rocket_shared::RunSource::Flow,
    ))
```

Then, in this file's `#[cfg(test)] mod tests`, add a new test near `saved_source_resolves_via_collection_repo_and_reuses_build_step_input`:

```rust
    #[test]
    fn built_input_is_tagged_run_source_flow() {
        let input = sample_execute_input();
        assert_eq!(input.run_source, rocket_shared::RunSource::Flow);
    }
```

Run: `cargo test -p rocket-app -j4 flow_execution_service`
Expected: PASS.

- [ ] **Step 8: Make the two test-only `ExecuteRequestInput` struct literals compile**

`ExecuteRequestInput` derives no `Default`, so every direct struct literal needs the new field. There are exactly two remaining (both test-only; the production literal was already handled in Step 4 via `build_step_input`, and every other production path either goes through `build_step_input` or is deserialized straight off the wire, so `#[serde(default)]` already gives it `RunSource::Manual`).

In `crates/rocket-app/src/execution_service.rs`, `sample_input` (around line 1939), find the last field before the closing brace:

```rust
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }
```

Replace with:

```rust
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
            run_source: rocket_shared::RunSource::Manual,
        }
    }
```

In `crates/rocket-app/src/load_test_service.rs`'s test module (around line 336), find:

```rust
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        };
```

Replace with:

```rust
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
            run_source: rocket_shared::RunSource::Manual,
        };
```

This is the correct value for both: neither literal represents a real Rust-side load-test construction path (see "Deviations from the Plan Index" item 2 above — load test's real `ExecuteRequestInput` is built by the frontend and deserialized off the wire, where `#[serde(default)]` already yields `Manual`), and `sample_input` in `execution_service.rs` backs manual-send-shaped tests.

- [ ] **Step 9: Run the full crate test suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS, including the new test from Step 3 (`history_entry_carries_the_input_run_source`), Step 5's 4 `runner_sequence` tests, and Step 7's new Flow test.

- [ ] **Step 10: Run the workspace-wide compile check**

Run: `cargo check --workspace -j4`
Expected: PASS (no other crate references `build_step_input` or constructs `ExecuteRequestInput` directly — confirmed by grepping the whole workspace for `ExecuteRequestInput {` and `build_step_input(` during this plan's authoring; only the sites touched above exist).

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` plugin skill to commit the 5 modified files with a `feat:`-prefixed message describing the `RunSource` field and its threading through the Runner/Flow/manual paths.

---

## Task 3: Build `McpToolService`

**Files:**
- Create: `crates/rocket-app/src/mcp_tool_service.rs`
- Modify: `crates/rocket-app/src/lib.rs:18-19` (add `pub mod mcp_tool_service;`), `:48-49` (add the `pub use`)

**Interfaces:**
- Consumes: `rocket_collection::CollectionRepository` (`get_settings`, `get_request`, `get_summaries`, `save_request_script` — Plan 01/02), `rocket_collection::RequestScriptPhase` (Plan 01), `rocket_environment::EnvironmentRepositoryFactory::for_collection` (existing), `rocket_environment::{Environment, Variable}` (existing), `crate::runner_sequence::{build_step_input, RunItem}` (this plan's Task 2, updated signature), `RequestExecutionService::execute` (existing), `rocket_shared::events::{DomainEvent, EventPublisher}` (existing, plus Plan 01's new `DomainEvent::AcpToolInvoked { session_id, tool, summary }` variant), `rocket_scripting::{TestResult, TestStatus}` (existing), `rocket_shared::error::{DomainError, DomainResult}` (existing).
- Produces (consumed by Plan 04/05, and re-exported from `rocket-app`'s crate root):

```rust
pub struct McpRequestEntry { pub path: String, pub name: String, pub method: String, pub url: String }
pub struct McpRunResult { pub status: u16, pub duration_ms: u64, pub test_pass_count: usize, pub test_fail_count: usize }

pub struct McpToolService { /* private fields */ }
impl McpToolService {
    pub fn new(
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
        execution_svc: Arc<RequestExecutionService>,
        event_publisher: Arc<dyn rocket_shared::events::EventPublisher>,
    ) -> Self;

    pub fn list_collection_requests(&self, session_id: &str, collection: &str) -> DomainResult<Vec<McpRequestEntry>>;
    pub async fn run_request(&self, session_id: &str, collection: &str, request_path: &str, environment_name: Option<&str>) -> DomainResult<McpRunResult>;
    pub fn edit_script(&self, session_id: &str, collection: &str, request_path: &str, phase: rocket_collection::RequestScriptPhase, body: String) -> DomainResult<()>;
    // NOTE: collection is an ADDED parameter vs. the plan index — see
    // "Deviations from the Plan Index" item 3 above.
    pub fn get_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str) -> DomainResult<String>;
    pub fn set_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str, value: String) -> DomainResult<()>;
    pub fn get_test_results(&self, session_id: &str, collection: &str, request_path: &str) -> DomainResult<Vec<rocket_scripting::TestResult>>;
}
```

- [ ] **Step 1: Write the failing autonomy-gate test (table-driven across all 6 methods)**

Create `crates/rocket-app/src/mcp_tool_service.rs` with just enough scaffolding for the test module to reference the not-yet-implemented API (this will fail to compile, which is step 2's expected failure):

```rust
//! Lets an ACP agent act on a Rocket collection: list requests, run one,
//! edit a script, read/write a non-secret environment variable, and read
//! the last cached test results. Every method first re-checks the target
//! collection's `agent_autonomy_enabled` flag and refuses if it is off —
//! this is the safety valve described in the design spec, checked fresh on
//! every call so a mid-session toggle takes effect immediately. Every
//! successful call publishes `DomainEvent::AcpToolInvoked` for the audit
//! trail.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::Arc;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::execution_service::RequestExecutionService;
use crate::runner_sequence::{build_step_input, RunItem};

/// One request entry in a `list_collection_requests` result. `path` is
/// relative to the collection root, matching the shape `run_request` and
/// `edit_script` expect back.
#[derive(Debug, Clone, PartialEq)]
pub struct McpRequestEntry {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

/// Summary of one `run_request` call, enough for an agent to decide what to
/// do next without re-fetching the full response body.
#[derive(Debug, Clone, PartialEq)]
pub struct McpRunResult {
    pub status: u16,
    pub duration_ms: u64,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
}

/// The single generic error returned by `get_env_var`/`set_env_var` for both
/// "no such key" and "key is secret". Keeping these indistinguishable stops
/// the tool from being an oracle for enumerating which env var names are
/// secret-flagged.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

/// Orchestrates the 6 MCP tools an ACP agent can call against a collection.
/// Holds no process/filesystem state of its own — every method delegates to
/// an existing domain repository or service. `test_result_cache` is the one
/// piece of state this service owns: an in-memory, session-lifetime map from
/// `(session_id, request_path)` to the test results of that pair's most
/// recent `run_request` call, per the design spec's explicit choice not to
/// persist test results into `rocket-history`.
pub struct McpToolService {
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
    execution_svc: Arc<RequestExecutionService>,
    event_publisher: Arc<dyn EventPublisher>,
    test_result_cache: Mutex<HashMap<(String, String), Vec<rocket_scripting::TestResult>>>,
}

impl McpToolService {
    pub fn new(
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
        execution_svc: Arc<RequestExecutionService>,
        event_publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            collection_repo,
            environment_repo_factory,
            execution_svc,
            event_publisher,
            test_result_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Re-checks the opt-in flag for `collection`. Every public method calls
    /// this first, before doing anything else — including the read-only
    /// tools, per the design spec.
    fn check_autonomy_enabled(&self, collection: &str) -> DomainResult<()> {
        let settings = self.collection_repo.get_settings(collection)?;
        if !settings.agent_autonomy_enabled {
            return Err(DomainError::InvalidInput(format!(
                "the agent is not allowed to act on collection '{collection}' — \
                 enable \"Allow this agent to run requests and edit files\" in \
                 the chat panel first"
            )));
        }
        Ok(())
    }

    fn publish_tool_invoked(&self, session_id: &str, tool: &str, summary: String) {
        self.event_publisher.publish(DomainEvent::AcpToolInvoked {
            session_id: session_id.to_string(),
            tool: tool.to_string(),
            summary,
        });
    }

    pub fn list_collection_requests(
        &self,
        session_id: &str,
        collection: &str,
    ) -> DomainResult<Vec<McpRequestEntry>> {
        todo!("Step 3")
    }

    pub async fn run_request(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
    ) -> DomainResult<McpRunResult> {
        todo!("Step 5")
    }

    pub fn edit_script(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        phase: rocket_collection::RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        todo!("Step 7")
    }

    pub fn get_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
    ) -> DomainResult<String> {
        todo!("Step 9")
    }

    pub fn set_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
        value: String,
    ) -> DomainResult<()> {
        todo!("Step 9")
    }

    pub fn get_test_results(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<rocket_scripting::TestResult>> {
        todo!("Step 11")
    }
}
```

This `todo!()` scaffolding exists only transiently within this task's own steps (it is replaced before this task ends — see Steps 3, 5, 7, 9, 11 below) — it is not left in place at the end of the task, so it does not violate this plan's "no placeholders" rule for finished work.

Now add the test module at the bottom of the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex as StdMutex;

    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, Folder, Request as CollectionRequest, RequestScriptPhase,
        RequestSummary,
    };
    use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory, Variable};
    use rocket_shared::types::HttpMethod;

    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo,
        RecordingExecutor, RecordingPublisher, SharedHistoryRepo,
        SharedPublisher,
    };

    /// Collection repo double with mutable, per-collection settings (so a
    /// test can toggle `agent_autonomy_enabled` mid-test), configurable
    /// requests and summary trees, and a record of every
    /// `save_request_script` call. Purpose-built for this file rather than
    /// reusing `crate::test_doubles::InMemoryCollectionRepo`, which holds one
    /// immutable `Collection` and cannot support the mid-session-toggle test
    /// below.
    struct FakeCollectionRepo {
        settings: StdMutex<StdHashMap<String, CollectionSettings>>,
        requests: StdMutex<StdHashMap<(String, String), CollectionRequest>>,
        summaries: StdMutex<StdHashMap<String, Collection>>,
        saved_scripts: StdMutex<Vec<(String, String, RequestScriptPhase, String)>>,
    }

    impl FakeCollectionRepo {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                settings: StdMutex::new(StdHashMap::new()),
                requests: StdMutex::new(StdHashMap::new()),
                summaries: StdMutex::new(StdHashMap::new()),
                saved_scripts: StdMutex::new(Vec::new()),
            })
        }

        fn set_autonomy(&self, collection: &str, enabled: bool) {
            let mut settings = CollectionSettings::default();
            settings.agent_autonomy_enabled = enabled;
            self.settings
                .lock()
                .expect("lock FakeCollectionRepo settings")
                .insert(collection.to_string(), settings);
        }

        fn with_request(&self, collection: &str, path: &str, request: CollectionRequest) {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo requests")
                .insert((collection.to_string(), path.to_string()), request);
        }

        fn with_summaries(&self, collection: &str, tree: Collection) {
            self.summaries
                .lock()
                .expect("lock FakeCollectionRepo summaries")
                .insert(collection.to_string(), tree);
        }

        fn saved_scripts(&self) -> Vec<(String, String, RequestScriptPhase, String)> {
            self.saved_scripts
                .lock()
                .expect("lock FakeCollectionRepo saved_scripts")
                .clone()
        }
    }

    impl CollectionRepository for FakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, name: &str) -> DomainResult<Collection> {
            self.summaries
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
            self.get(name)
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<CollectionRequest> {
            self.requests
                .lock()
                .expect("lock")
                .get(&(collection.to_string(), path.to_string()))
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
            Ok(self
                .settings
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .unwrap_or_default())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn save_request_script(
            &self,
            collection: &str,
            request_path: &str,
            phase: RequestScriptPhase,
            body: String,
        ) -> DomainResult<()> {
            self.saved_scripts
                .lock()
                .expect("lock")
                .push((collection.to_string(), request_path.to_string(), phase, body));
            Ok(())
        }
    }

    /// A thin `Box<dyn CollectionRepository>`-shaped wrapper around one
    /// shared `Arc<FakeCollectionRepo>`, so the same repo instance can be
    /// handed to both `RequestExecutionService` (which owns a `Box`) and
    /// `McpToolService` (which owns an `Arc`) in the same test.
    struct SharedFakeCollectionRepo(Arc<FakeCollectionRepo>);
    impl CollectionRepository for SharedFakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            self.0.list()
        }
        fn get(&self, n: &str) -> DomainResult<Collection> {
            self.0.get(n)
        }
        fn get_summaries(&self, n: &str) -> DomainResult<Collection> {
            self.0.get_summaries(n)
        }
        fn create(&self, n: &str) -> DomainResult<Collection> {
            self.0.create(n)
        }
        fn delete(&self, n: &str) -> DomainResult<()> {
            self.0.delete(n)
        }
        fn rename(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.rename(a, b)
        }
        fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> {
            self.0.get_request(a, b)
        }
        fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> {
            self.0.save_request(a, b, c)
        }
        fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> {
            self.0.rename_request(a, b, c)
        }
        fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_request(a, b)
        }
        fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.create_folder(a, b)
        }
        fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_folder(a, b)
        }
        fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> {
            self.0.move_item(a, b, c, d)
        }
        fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> {
            self.0.reorder_items(a, b, c)
        }
        fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> {
            self.0.get_settings(n)
        }
        fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> {
            self.0.save_settings(n, s)
        }
        fn get_folder_chain_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_chain_variables(a, b)
        }
        fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_variables(a, b)
        }
        fn save_folder_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> {
            self.0.save_folder_variables(a, b, c)
        }
        fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_request_variables(a, b)
        }
        fn save_request_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> {
            self.0.save_request_variables(a, b, c)
        }
        fn save_request_script(
            &self,
            a: &str,
            b: &str,
            c: RequestScriptPhase,
            d: String,
        ) -> DomainResult<()> {
            self.0.save_request_script(a, b, c, d)
        }
    }

    /// Environment repo factory double whose `for_collection` handles all
    /// share one underlying map, so a `set_env_var` write is visible to a
    /// later `get_env_var` call even though each call asks for a fresh
    /// `Box<dyn EnvironmentRepository>`.
    struct FakeEnvRepoFactory {
        envs: Arc<StdMutex<StdHashMap<String, Environment>>>,
    }
    impl FakeEnvRepoFactory {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                envs: Arc::new(StdMutex::new(StdHashMap::new())),
            })
        }
        fn with_env(&self, env: Environment) {
            self.envs.lock().expect("lock").insert(env.name.clone(), env);
        }
    }
    impl EnvironmentRepositoryFactory for FakeEnvRepoFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(FakeEnvRepoHandle(Arc::clone(&self.envs)))
        }
    }
    struct FakeEnvRepoHandle(Arc<StdMutex<StdHashMap<String, Environment>>>);
    impl EnvironmentRepository for FakeEnvRepoHandle {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.lock().expect("lock").values().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            self.0.lock().expect("lock").insert(env.name.clone(), env.clone());
            Ok(())
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.lock().expect("lock").remove(name);
            Ok(())
        }
    }

    /// Builds an `McpToolService` plus its backing `RequestExecutionService`,
    /// sharing one `FakeCollectionRepo` and one `RecordingPublisher` between
    /// them so a test can both drive HTTP dispatch and inspect every
    /// `DomainEvent` (including `AcpToolInvoked`) either service published.
    fn service_with(
        collection_repo: Arc<FakeCollectionRepo>,
        env_factory: Arc<FakeEnvRepoFactory>,
        publisher: Arc<RecordingPublisher>,
    ) -> McpToolService {
        let executor = RecordingExecutor::new();
        let history = InMemoryHistoryRepo::new();
        // `Arc::clone(&executor)` is `Arc<RecordingExecutor>`; it coerces to
        // `Arc<dyn HttpExecutor>` automatically at this argument position —
        // an explicit `as Arc<dyn Trait>` cast is not valid Rust for `Arc`,
        // so this relies on function-argument coercion instead, matching
        // this crate's existing test style (e.g. `load_test_service.rs`'s
        // `let load_exec: Arc<dyn HttpExecutor> = Arc::new(...)`).
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedFakeCollectionRepo(Arc::clone(&collection_repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        McpToolService::new(
            collection_repo,
            env_factory,
            exec_svc,
            publisher,
        )
    }

    fn sample_request(name: &str) -> CollectionRequest {
        CollectionRequest::new(name, HttpMethod::Get, "https://api.test/ping")
    }

    /// Table-driven proof that all 6 tools refuse when autonomy is off — the
    /// Review Focus item this plan and the index both call out.
    #[test]
    fn every_tool_is_refused_when_autonomy_is_disabled() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(Environment::new("dev"));
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        assert!(svc.list_collection_requests("s1", "my-api").is_err());
        assert!(svc.edit_script("s1", "my-api", "login.yml", RequestScriptPhase::Tests, "// x".into()).is_err());
        assert!(svc.get_env_var("s1", "my-api", "dev", "HOST").is_err());
        assert!(svc.set_env_var("s1", "my-api", "dev", "HOST", "x".into()).is_err());
        assert!(svc.get_test_results("s1", "my-api", "login.yml").is_err());
        // run_request is async — checked in its own test below (Step 6) since
        // this test function is synchronous; the assertion set above already
        // covers every synchronous tool with the shared fixture.
    }

    #[tokio::test]
    async fn run_request_is_refused_when_autonomy_is_disabled() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("run_request must be refused when autonomy is disabled");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: FAIL to compile — every public method still `todo!()`s, so `.is_err()`/`.expect_err()` on a panic never runs; the test binary fails to link/panics immediately. This is the expected "step 2" failure for this task's first slice of behavior.

- [ ] **Step 2: Confirm the expected failure**

Run: `cargo test -p rocket-app -j4 every_tool_is_refused_when_autonomy_is_disabled`
Expected: test panics with "not yet implemented: Step 3" (or similar `todo!()` panic message) — proves the scaffolding compiles and the test actually executes the real method bodies once written.

- [ ] **Step 3: Implement `list_collection_requests`**

Replace the `list_collection_requests` `todo!()` body with:

```rust
    pub fn list_collection_requests(
        &self,
        session_id: &str,
        collection: &str,
    ) -> DomainResult<Vec<McpRequestEntry>> {
        self.check_autonomy_enabled(collection)?;
        let tree = self.collection_repo.get_summaries(collection)?;
        let mut entries = Vec::new();
        collect_request_entries(&tree.root, "", &mut entries);
        self.publish_tool_invoked(
            session_id,
            "list_collection_requests",
            format!("listed {} request(s) in '{collection}'", entries.len()),
        );
        Ok(entries)
    }
```

Add the private tree-walk helper below the `impl McpToolService` block (module-level, mirroring `runner_sequence::collect_items`'s depth-first walk but reading `RequestSummary` leaves instead of full `Request` bodies, since `get_summaries` returns `CollectionItem::Summary`, not `CollectionItem::Request`):

```rust
/// Depth-first walk of a `get_summaries()` tree, collecting one
/// `McpRequestEntry` per `CollectionItem::Summary` leaf. Mirrors
/// `runner_sequence::collect_items`'s traversal, but over summary leaves
/// instead of full `Request` bodies — the two item shapes are different
/// enum variants (`Summary` vs `Request`), so this is a separate, small
/// walk rather than a shared generic one.
fn collect_request_entries(
    folder: &rocket_collection::Folder,
    prefix: &str,
    out: &mut Vec<McpRequestEntry>,
) {
    for item in &folder.items {
        match item {
            rocket_collection::CollectionItem::Summary(summary) => {
                let Some(file_name) = summary.file_name.as_ref() else {
                    continue;
                };
                out.push(McpRequestEntry {
                    path: format!("{prefix}{file_name}"),
                    name: summary.name.clone(),
                    method: summary.method.clone(),
                    url: summary.url.clone(),
                });
            }
            rocket_collection::CollectionItem::Folder(sub) => {
                let dir_name = sub.dir_name.as_deref().unwrap_or(&sub.name);
                let sub_prefix = format!("{prefix}{dir_name}/");
                collect_request_entries(sub, &sub_prefix, out);
            }
            rocket_collection::CollectionItem::Request(_)
            | rocket_collection::CollectionItem::OpaqueItem(_) => {}
        }
    }
}
```

- [ ] **Step 4: Write the happy-path test for `list_collection_requests`**

```rust
    #[test]
    fn list_collection_requests_walks_folders_and_publishes_audit_event() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);

        let mut collection = Collection::new("my-api");
        collection.root.add_summary(RequestSummary {
            uid: "u1".into(),
            name: "Login".into(),
            method: "POST".into(),
            url: "https://api.test/login".into(),
            file_name: Some("login.yml".into()),
        });
        let mut auth = Folder::new("auth");
        auth.dir_name = Some("auth".into());
        auth.add_summary(RequestSummary {
            uid: "u2".into(),
            name: "Refresh".into(),
            method: "POST".into(),
            url: "https://api.test/refresh".into(),
            file_name: Some("refresh.yml".into()),
        });
        collection.root.add_subfolder(auth);
        repo.with_summaries("my-api", collection);

        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        let entries = svc
            .list_collection_requests("s1", "my-api")
            .expect("list_collection_requests");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "login.yml");
        assert_eq!(entries[1].path, "auth/refresh.yml");
        assert_eq!(entries[1].method, "POST");

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "list_collection_requests")),
            "expected an AcpToolInvoked event for list_collection_requests"
        );
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: `list_collection_requests_walks_folders_and_publishes_audit_event` and `every_tool_is_refused_when_autonomy_is_disabled`'s `list_collection_requests` assertion PASS; the remaining assertions in that test and every other new test still fail/panic on `todo!()`.

- [ ] **Step 5: Implement `run_request`**

Replace the `run_request` `todo!()` body with:

```rust
    pub async fn run_request(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
    ) -> DomainResult<McpRunResult> {
        self.check_autonomy_enabled(collection)?;
        let request = self.collection_repo.get_request(collection, request_path)?;
        let item = RunItem {
            name: request.name.clone(),
            request_path: request_path.to_string(),
            request,
        };
        let input = build_step_input(
            &item,
            collection,
            environment_name,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
            rocket_shared::RunSource::Agent,
        );
        let output = self.execution_svc.execute(input).await?;

        let test_pass_count = output
            .test_results
            .iter()
            .filter(|t| matches!(t.status, rocket_scripting::TestStatus::Passed))
            .count();
        let test_fail_count = output.test_results.len() - test_pass_count;

        self.test_result_cache
            .lock()
            .expect("lock McpToolService test_result_cache")
            .insert(
                (session_id.to_string(), request_path.to_string()),
                output.test_results.clone(),
            );

        self.publish_tool_invoked(
            session_id,
            "run_request",
            format!(
                "ran '{request_path}' in '{collection}' -> {} ({}ms)",
                output.response.status, output.response.duration_ms
            ),
        );

        Ok(McpRunResult {
            status: output.response.status,
            duration_ms: output.response.duration_ms,
            test_pass_count,
            test_fail_count,
        })
    }
```

- [ ] **Step 6: Write the happy-path test for `run_request`, proving `RunSource::Agent` reaches `HistoryEntry`**

```rust
    #[tokio::test]
    async fn run_request_dispatches_tags_history_agent_and_caches_test_results() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        let executor = RecordingExecutor::new();
        let history = InMemoryHistoryRepo::new();
        // As in `service_with` above, these rely on function-argument
        // coercion from `Arc<Concrete>` to `Arc<dyn Trait>` — `as` does not
        // perform this coercion for `Arc`.
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedFakeCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        let svc = McpToolService::new(
            Arc::clone(&repo),
            env_factory,
            Arc::clone(&exec_svc),
            Arc::clone(&publisher),
        );

        let result = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect("run_request");

        assert_eq!(result.status, 200);
        assert_eq!(result.test_pass_count, 0);
        assert_eq!(result.test_fail_count, 0);

        let saved = history.entries.lock().expect("lock history");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].run_source, rocket_shared::RunSource::Agent);
        drop(saved);

        let cached = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect("get_test_results after a run must be cached, not an error");
        assert!(cached.is_empty(), "this fixture's request has no test script, so no results");

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "run_request")),
            "expected an AcpToolInvoked event for run_request"
        );
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: PASS for both new tests plus the `run_request` line in `every_tool_is_refused_when_autonomy_is_disabled` / `run_request_is_refused_when_autonomy_is_disabled`.

- [ ] **Step 7: Implement `edit_script`**

Replace the `edit_script` `todo!()` body with:

```rust
    pub fn edit_script(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        phase: rocket_collection::RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.check_autonomy_enabled(collection)?;
        let phase_name = match phase {
            rocket_collection::RequestScriptPhase::PreRequest => "pre-request",
            rocket_collection::RequestScriptPhase::PostResponse => "post-response",
            rocket_collection::RequestScriptPhase::Tests => "tests",
        };
        self.collection_repo
            .save_request_script(collection, request_path, phase, body)?;
        self.publish_tool_invoked(
            session_id,
            "edit_script",
            format!("updated the {phase_name} script on '{request_path}' in '{collection}'"),
        );
        Ok(())
    }
```

- [ ] **Step 8: Write the happy-path test for `edit_script`**

```rust
    #[test]
    fn edit_script_saves_via_the_repository_and_publishes_audit_event() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        svc.edit_script(
            "s1",
            "my-api",
            "login.yml",
            RequestScriptPhase::PostResponse,
            "rok.setEnvVar('token', res.body.token);".into(),
        )
        .expect("edit_script");

        let saved = repo.saved_scripts();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].0, "my-api");
        assert_eq!(saved[0].1, "login.yml");
        assert_eq!(saved[0].2, RequestScriptPhase::PostResponse);
        assert!(saved[0].3.contains("setEnvVar"));

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "edit_script")),
            "expected an AcpToolInvoked event for edit_script"
        );
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: PASS for this test and `edit_script`'s line in the autonomy-disabled table test.

- [ ] **Step 9: Implement `get_env_var` and `set_env_var`**

Replace both `todo!()` bodies with:

```rust
    pub fn get_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
    ) -> DomainResult<String> {
        self.check_autonomy_enabled(collection)?;
        let repo = self.environment_repo_factory.for_collection(collection);
        let env = repo.get(environment_name)?;
        let value = env
            .variables
            .iter()
            .find(|v| v.key == key && !v.secret)
            .map(|v| v.value.clone())
            .ok_or_else(|| DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()))?;
        self.publish_tool_invoked(
            session_id,
            "get_env_var",
            format!("read variable '{key}' from environment '{environment_name}'"),
        );
        Ok(value)
    }

    pub fn set_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
        value: String,
    ) -> DomainResult<()> {
        self.check_autonomy_enabled(collection)?;
        let repo = self.environment_repo_factory.for_collection(collection);
        let mut env = repo.get(environment_name)?;
        let variable = env
            .variables
            .iter_mut()
            .find(|v| v.key == key)
            .ok_or_else(|| DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()))?;
        if variable.secret {
            return Err(DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()));
        }
        variable.value = value;
        repo.save(&env)?;
        self.publish_tool_invoked(
            session_id,
            "set_env_var",
            format!("wrote variable '{key}' in environment '{environment_name}'"),
        );
        Ok(())
    }
```

Note `set_env_var` deliberately does not create a missing key (see the doc comment on `VARIABLE_NOT_ACCESSIBLE` and the test below) — silently creating on "not found" while erroring on "is secret" would itself be the oracle the shared error message is meant to close.

- [ ] **Step 10: Write the tests for `get_env_var`/`set_env_var`, including the not-found/is-secret oracle test**

```rust
    fn env_with_vars() -> Environment {
        let mut env = Environment::new("dev");
        env.set_variable(Variable {
            key: "HOST".into(),
            value: "api.example.com".into(),
            enabled: true,
            secret: false,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env.set_variable(Variable {
            key: "API_KEY".into(),
            value: "sk-live-abc".into(),
            enabled: true,
            secret: true,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env
    }

    #[test]
    fn get_env_var_reads_a_non_secret_variable() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let value = svc
            .get_env_var("s1", "my-api", "dev", "HOST")
            .expect("HOST is non-secret and must be readable");
        assert_eq!(value, "api.example.com");
    }

    #[test]
    fn get_env_var_not_found_and_is_secret_produce_the_identical_error_message() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let not_found = svc
            .get_env_var("s1", "my-api", "dev", "NO_SUCH_KEY")
            .expect_err("unknown key must error");
        let is_secret = svc
            .get_env_var("s1", "my-api", "dev", "API_KEY")
            .expect_err("secret key must error");

        assert_eq!(not_found.to_string(), is_secret.to_string(), "the two error messages must be indistinguishable");

        // Case sensitivity: a differently-cased key is also just "not found",
        // not a secret-detection bypass or a distinct error shape.
        let wrong_case = svc
            .get_env_var("s1", "my-api", "dev", "host")
            .expect_err("key lookup is case-sensitive, so this must also be the same error");
        assert_eq!(wrong_case.to_string(), not_found.to_string());
    }

    #[test]
    fn set_env_var_writes_a_non_secret_variable_and_it_is_readable_back() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        svc.set_env_var("s1", "my-api", "dev", "HOST", "api2.example.com".into())
            .expect("set_env_var on a non-secret existing key");

        let value = svc
            .get_env_var("s1", "my-api", "dev", "HOST")
            .expect("read back");
        assert_eq!(value, "api2.example.com");

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "set_env_var")),
            "expected an AcpToolInvoked event for set_env_var"
        );
    }

    #[test]
    fn set_env_var_refuses_a_secret_variable_and_does_not_create_missing_keys() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let secret_err = svc
            .set_env_var("s1", "my-api", "dev", "API_KEY", "sk-new".into())
            .expect_err("writing a secret variable must be refused");
        let missing_err = svc
            .set_env_var("s1", "my-api", "dev", "NO_SUCH_KEY", "x".into())
            .expect_err("writing an unknown key must be refused, not create it");

        assert_eq!(
            secret_err.to_string(),
            missing_err.to_string(),
            "the two error messages must be indistinguishable"
        );
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: PASS for all 5 new tests, plus `get_env_var`/`set_env_var`'s lines in the autonomy-disabled table test.

- [ ] **Step 11: Implement `get_test_results`**

Replace the `get_test_results` `todo!()` body with:

```rust
    pub fn get_test_results(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<rocket_scripting::TestResult>> {
        self.check_autonomy_enabled(collection)?;
        let results = self
            .test_result_cache
            .lock()
            .expect("lock McpToolService test_result_cache")
            .get(&(session_id.to_string(), request_path.to_string()))
            .cloned()
            .ok_or_else(|| {
                DomainError::NotFound(format!(
                    "no cached test results for '{request_path}' in session '{session_id}' \
                     — run the request first"
                ))
            })?;
        self.publish_tool_invoked(
            session_id,
            "get_test_results",
            format!("read {} cached test result(s) for '{request_path}'", results.len()),
        );
        Ok(results)
    }
```

`get_test_results` returns a `DomainError::NotFound`, not an empty `Vec`, when nothing is cached — matching this codebase's existing convention for "no such recorded state" (e.g. `HistoryRepository::get` returns `DomainError::NotFound` for a missing entry, `crates/rocket-app/src/execution_service.rs`'s `MockHistoryRepo::get`), rather than the alternative of a silent empty vec that would look identical to "the request ran and passed zero tests."

- [ ] **Step 12: Write the not-cached test for `get_test_results`**

```rust
    #[test]
    fn get_test_results_errors_with_not_found_when_nothing_is_cached() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect_err("nothing has run yet in this session for this path");
        assert!(matches!(err, DomainError::NotFound(_)));
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: PASS.

- [ ] **Step 13: Write the mid-session-toggle test**

This is the plan index's own Review Focus item — proving the flag is re-read on every call, not cached at session start:

```rust
    #[test]
    fn disabling_autonomy_mid_session_blocks_the_very_next_call() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        svc.list_collection_requests("s1", "my-api")
            .expect("first call succeeds while autonomy is enabled");

        repo.set_autonomy("my-api", false);

        let err = svc
            .list_collection_requests("s1", "my-api")
            .expect_err("the very next call must be refused once autonomy is disabled");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
```

Run: `cargo test -p rocket-app -j4 mcp_tool_service`
Expected: PASS.

- [ ] **Step 14: Register the new module and its public types**

In `crates/rocket-app/src/lib.rs`, add the module declaration alphabetically between `load_test_service` and `oauth2_service`:

```rust
pub mod load_test_service;
pub mod mcp_tool_service;
pub mod oauth2_service;
```

and the re-export between their two `pub use` lines:

```rust
pub use load_test_service::LoadTestService;
pub use mcp_tool_service::{McpRequestEntry, McpRunResult, McpToolService};
pub use oauth2_service::OAuth2Service;
```

- [ ] **Step 15: Run the full crate test suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS — every test in `mcp_tool_service.rs` (14 new tests) plus every pre-existing test in the crate.

- [ ] **Step 16: Run the workspace-wide compile check**

Run: `cargo check --workspace -j4`
Expected: PASS. `McpToolService` is not yet consumed by `src-tauri` (that is Plan 05's job), so no other crate is affected.

- [ ] **Step 17: Commit**

Use the `dev-workflow-skills:1-git-commit` plugin skill to commit the new file and the two `lib.rs` lines with a `feat:`-prefixed message describing the new `McpToolService`.

---

## Task 4: `AcpSessionService` opt-in-aware MCP wiring

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs:1-81` (struct, constructors, `start_session`), `:340-665` (`#[cfg(test)] mod tests`)
- Modify: `src-tauri/src/commands/acp_sessions.rs:5-12` (`start_agent_session`)
- Modify: `src-tauri/src/lib.rs:380-384` (`acp_session_svc` construction)

**Interfaces:**
- Consumes: `rocket_acp::McpServerSpec` (Plan 01), the 5-arg `AcpSessionClient::start_session` (Plan 01 — this plan assumes Plan 01 already updated the trait, `rocket-infra`'s `AcpAgentClient`, and this exact file's production call site to pass `&[]` for the new parameter, per the plan index's explicit statement that Plan 01 reaches into this file for that one line), `rocket_collection::CollectionRepository::get_settings` (existing trait method; Plan 01 adds the `agent_autonomy_enabled` field it reads).
- Produces: `AcpSessionService::new`/`with_prompt_timeout` gain a new 4th parameter `collection_repo: Arc<dyn rocket_collection::CollectionRepository>` (inserted before the existing `prompt_timeout` parameter on `with_prompt_timeout`, and as the last parameter on `new`). `AcpSessionService::start_session` gains a new 3rd parameter `collection: Option<&str>`. The Tauri `start_agent_session` command gains a new `collection: Option<String>` parameter, matching.

- [ ] **Step 1: Confirm the current shape of the file, and reconcile with Plan 01's expected changes**

Read `crates/rocket-app/src/acp_session_service.rs` in full before editing. By the time this task runs, Plan 01 is expected to have already:
- changed `AcpSessionClient::start_session`'s trait signature (in `rocket-acp`) to take a 5th parameter `mcp_servers: &[rocket_acp::McpServerSpec]`;
- updated this file's production call (`self.session_client.start_session(&config.command, &config.args, cwd, &env)`) to `self.session_client.start_session(&config.command, &config.args, cwd, &env, &[])`.

If, on inspection, `crates/rocket-app/src/acp_session_service.rs`'s `#[cfg(test)] mod tests`' local `FakeSessionClient::start_session` implementation still only declares 4 parameters (i.e. Plan 01 updated the trait and the production call site but not this test double, since this file's test module is arguably outside Plan 01's own declared crate scope), add the 5th parameter to it now, ignored (no test in this task needs to inspect what was passed — every test below only checks `start_session`'s return value or error), before proceeding to Step 2:

```rust
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[rocket_acp::McpServerSpec],
        ) -> DomainResult<String> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok("session-1".to_string())
            }
        }
```

If it already has 5 parameters (Plan 01 already handled it), skip straight to Step 2.

- [ ] **Step 2: Add `collection_repo` to the struct and both constructors**

In `crates/rocket-app/src/acp_session_service.rs`, change:

```rust
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    prompt_timeout: Duration,
}
```

to:

```rust
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    prompt_timeout: Duration,
}
```

Change:

```rust
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            DEFAULT_PROMPT_TIMEOUT,
        )
    }

    /// Test seam only — production wiring (Plan 05) always uses `new`, which
    /// fixes this at the spec's 120-second constant. This constructor does
    /// not add end-user configurability.
    pub fn with_prompt_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            prompt_timeout,
        }
    }
```

to:

```rust
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            DEFAULT_PROMPT_TIMEOUT,
        )
    }

    /// Test seam only — production wiring (Plan 05) always uses `new`, which
    /// fixes this at the spec's 120-second constant. This constructor does
    /// not add end-user configurability.
    pub fn with_prompt_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            prompt_timeout,
        }
    }
```

- [ ] **Step 3: Add `collection` to `start_session` and build the (currently always-empty) MCP server list**

Change:

```rust
    pub async fn start_session(&self, agent_config_id: &str, cwd: &str) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: session_id.clone(),
            });
        Ok(session_id)
    }
```

to:

```rust
    pub async fn start_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        collection: Option<&str>,
    ) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];
        let mcp_servers = self.mcp_server_specs_for(collection)?;
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: session_id.clone(),
            });
        Ok(session_id)
    }

    /// Resolves the MCP servers to attach to a new session. A session with
    /// no target collection, or whose collection has not opted into agent
    /// autonomy, gets none — chat-only mode, identical to subproject C's
    /// existing behavior. No MCP server implementation exists yet (the HTTP
    /// backend lands in Plan 04, the Stdio shim in Plan 05), so an opted-in
    /// collection also gets an empty list today — there is nothing yet to
    /// attach. This still re-checks `get_settings` on every call (not just
    /// once), matching the design spec's "checked on every call" rule for
    /// the tools themselves, and still propagates a lookup failure instead
    /// of silently falling back to chat-only mode, so a broken collection
    /// name surfaces loudly rather than silently degrading.
    fn mcp_server_specs_for(
        &self,
        collection: Option<&str>,
    ) -> DomainResult<Vec<rocket_acp::McpServerSpec>> {
        let Some(collection) = collection else {
            return Ok(Vec::new());
        };
        let settings = self.collection_repo.get_settings(collection)?;
        if !settings.agent_autonomy_enabled {
            return Ok(Vec::new());
        }
        Ok(Vec::new())
    }
```

- [ ] **Step 4: Update the Tauri command**

In `src-tauri/src/commands/acp_sessions.rs`, change:

```rust
#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.start_session(&agent_config_id, &cwd).await
}
```

to:

```rust
#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: Option<String>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.start_session(&agent_config_id, &cwd, collection.as_deref())
        .await
}
```

- [ ] **Step 5: Update the `lib.rs` construction site**

In `src-tauri/src/lib.rs`, find (around line 380):

```rust
            let acp_session_svc = rocket_app::AcpSessionService::new(
                Box::new(rocket_infra::AcpAgentClient::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                acp_agent_config_svc,
            );
```

Replace with:

```rust
            // A standalone FsCollectionRepo at the same collections_dir the
            // exec_svc/collection_svc instances below also use — only needed
            // here for the agent_autonomy_enabled gate check, so a lightweight
            // fresh instance is simpler than restructuring construction order
            // to share one Arc (the spec's own reasoning for using AppHandle
            // lookups elsewhere in this file does not apply to this one
            // trait-only read).
            let acp_session_collection_repo: Arc<dyn rocket_collection::CollectionRepository> =
                Arc::new(FsCollectionRepo::new_standalone(collections_dir.clone()));

            let acp_session_svc = rocket_app::AcpSessionService::new(
                Box::new(rocket_infra::AcpAgentClient::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                acp_agent_config_svc,
                acp_session_collection_repo,
            );
```

`collections_dir` is already in scope at this point in `run()` (defined earlier, at `let collections_dir = workspace_base.join("collections");`, and reused again a few lines below by `exec_svc`'s own `FsCollectionRepo::new_standalone(collections_dir.clone())`), and `FsCollectionRepo`/`Arc` are already imported in this file (confirmed by `exec_svc`'s existing use of both a few lines below this insertion point).

- [ ] **Step 6: Update every existing test in this file's `#[cfg(test)] mod tests`**

Every existing call to `AcpSessionService::new(...)`/`AcpSessionService::with_prompt_timeout(...)` needs a new `collection_repo` argument, and every existing call to `service.start_session("agent-1", "/tmp")` needs a third `None` argument (preserving today's behavior — no collection, so no MCP servers, exactly as before this task). Add a shared test double and a helper near the top of the test module:

```rust
    /// Collection repo double for `AcpSessionService` tests. Settings default
    /// to `agent_autonomy_enabled: false` for any collection not explicitly
    /// configured via `set_autonomy`, matching `get_settings`'s documented
    /// "missing settings file" fallback in the real repositories.
    struct FakeAcpCollectionRepo {
        settings: Mutex<std::collections::HashMap<String, rocket_collection::CollectionSettings>>,
        settings_error_for: Mutex<Option<String>>,
    }
    impl FakeAcpCollectionRepo {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                settings: Mutex::new(std::collections::HashMap::new()),
                settings_error_for: Mutex::new(None),
            })
        }
        fn set_autonomy(&self, collection: &str, enabled: bool) {
            let mut settings = rocket_collection::CollectionSettings::default();
            settings.agent_autonomy_enabled = enabled;
            self.settings
                .lock()
                .expect("lock")
                .insert(collection.to_string(), settings);
        }
        fn fail_settings_for(&self, collection: &str) {
            *self.settings_error_for.lock().expect("lock") = Some(collection.to_string());
        }
    }
    impl rocket_collection::CollectionRepository for FakeAcpCollectionRepo {
        fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, name: &str) -> DomainResult<rocket_collection::Collection> {
            Err(DomainError::NotFound(name.into()))
        }
        fn get_summaries(&self, name: &str) -> DomainResult<rocket_collection::Collection> {
            self.get(name)
        }
        fn create(&self, _: &str) -> DomainResult<rocket_collection::Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<rocket_collection::Request> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(&self, _: &str, path: &str, _: &rocket_collection::Request) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            if self.settings_error_for.lock().expect("lock").as_deref() == Some(name) {
                return Err(DomainError::Internal("settings read failed".into()));
            }
            Ok(self
                .settings
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .unwrap_or_default())
        }
        fn save_settings(&self, _: &str, _: &rocket_collection::CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<rocket_collection::CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<rocket_collection::CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn save_request_script(
            &self,
            _: &str,
            _: &str,
            _: rocket_collection::RequestScriptPhase,
            _: String,
        ) -> DomainResult<()> {
            Ok(())
        }
    }
```

Then, for every existing call site in this file matching:

```rust
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
        );
```

(and its `with_prompt_timeout` variant), add `FakeAcpCollectionRepo::new(),` as the new argument right after `agent_config_service()`/before the timeout, e.g.:

```rust
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            FakeAcpCollectionRepo::new(),
        );
```

There are 9 such `AcpSessionService` construction call sites in this file's test module (`start_session_resolves_config_and_credential_and_publishes_started`, `send_prompt_publishes_every_chunk_before_finished_in_order`, `end_session_delegates_to_session_client`, `start_session_unknown_agent_config_id_errors`, `start_session_propagates_spawn_failure`, `start_session_propagates_credential_resolution_failure_unchanged`, `send_prompt_failure_publishes_failed_and_returns_the_error`, `send_prompt_timeout_kills_the_session_and_publishes_failed` (via `with_prompt_timeout`), `end_all_sessions_delegates_to_session_client`) — add `FakeAcpCollectionRepo::new(),` to every one of them.

Of those 9, only the 4 that actually call `.start_session(...)` (`start_session_resolves_config_and_credential_and_publishes_started`, `start_session_unknown_agent_config_id_errors`, `start_session_propagates_spawn_failure`, `start_session_propagates_credential_resolution_failure_unchanged`) also need their `service.start_session("agent-1", "/tmp")` call changed to `service.start_session("agent-1", "/tmp", None)` — the other 5 call `send_prompt`/`end_session`/`end_all_sessions` directly against a hardcoded `"session-1"` id and never call `start_session` at all, so they need only the constructor-argument fix above.

Run: `cargo test -p rocket-app -j4 acp_session_service`
Expected: PASS — every pre-existing test still passes with the new arguments (behavior for `collection: None` is identical to before this task).

- [ ] **Step 7: Write the new collection-gating tests**

Add, in the same test module:

```rust
    #[tokio::test]
    async fn start_session_with_no_collection_still_works_exactly_as_before() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            FakeAcpCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", None)
            .await
            .expect("start_session with no collection must keep working");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_disabled_still_succeeds_with_no_mcp_servers() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = FakeAcpCollectionRepo::new();
        collection_repo.set_autonomy("my-api", false);
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        let session_id = service
            .start_session("agent-1", "/tmp", Some("my-api"))
            .await
            .expect("a disabled collection must still be able to start a chat-only session");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_enabled_still_succeeds_since_no_mcp_backend_exists_yet() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = FakeAcpCollectionRepo::new();
        collection_repo.set_autonomy("my-api", true);
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        // Plan 03 has no HTTP backend or Stdio shim to attach yet, so an
        // opted-in collection behaves identically to a disabled one today —
        // this is this plan's complete, correct behavior (see this plan's
        // "Deviations from the Plan Index" item 4), not a bug to fix later.
        let session_id = service
            .start_session("agent-1", "/tmp", Some("my-api"))
            .await
            .expect("an opted-in collection must still start a session");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_propagates_a_collection_settings_lookup_failure() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = FakeAcpCollectionRepo::new();
        collection_repo.fail_settings_for("broken-collection");
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        let err = service
            .start_session("agent-1", "/tmp", Some("broken-collection"))
            .await
            .expect_err("a broken collection settings read must fail start_session, not silently degrade to chat-only");
        assert!(matches!(err, DomainError::Internal(_)));
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish when the settings lookup fails before any session starts"
        );
    }
```

Run: `cargo test -p rocket-app -j4 acp_session_service`
Expected: PASS for all 4 new tests.

- [ ] **Step 8: Run the full crate test suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS.

- [ ] **Step 9: Run the workspace-wide compile check**

Run: `cargo check --workspace -j4`
Expected: PASS — this changes two public signatures consumed outside `rocket-app` (`AcpSessionService::new`'s call site in `src-tauri/src/lib.rs`, already updated in Step 5; the `start_agent_session` Tauri command, already updated in Step 4). No other crate references either.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` plugin skill to commit all three modified files with a `feat:`-prefixed message describing the collection-aware MCP wiring in `AcpSessionService::start_session`.

---

## Next Plan

**Plan 04 — HTTP MCP backend** (`src-tauri`, `rmcp` + `axum`): implements `src-tauri/src/mcp/tool_server.rs`, an `rmcp` `ServerHandler` over Streamable HTTP wired to `McpToolService` via `AppHandle::state`, bound to `127.0.0.1` on a random port behind a per-session bearer token. It is the first consumer of `McpToolService::new`'s 4 dependencies and the first plan able to make `AcpSessionService::mcp_server_specs_for`'s "enabled" branch (Task 4, Step 3 of this plan) return a real, non-empty `Vec<McpServerSpec>` instead of the empty one this plan ships. See [00-plan-index.md](00-plan-index.md) for its locked interface (`McpHttpServerHandle`, `spawn_mcp_http_server`).

## Post-Implementation Review

- [ ] Dispatch an Opus-model subagent (`model: "opus"`) to review this plan's full diff (all files touched across Tasks 1–4) against:
  - **Interface conformance vs. the plan index** — confirm every signature this plan produces (`ExecuteRequestInput.run_source`, `build_step_input`'s new parameter, all 6 `McpToolService` methods, `AcpSessionService::new`/`start_session`) matches what this plan document specifies, flagging the four documented deviations from the index as intentional (not defects), and checking Plan 04/05 will be able to consume them as described in this plan's "Next Plan" section.
  - **Code quality and duplication** — in particular whether `collect_request_entries` (Task 3) duplicates too much of `runner_sequence::collect_items`'s traversal logic, and whether the repeated `FakeCollectionRepo`/`SharedFakeCollectionRepo`/environment-factory test doubles across `mcp_tool_service.rs` and `acp_session_service.rs` should be consolidated into `crate::test_doubles` instead.
  - **DDD boundaries** — confirm `mcp_tool_service.rs` contains no filesystem/process I/O (all I/O stays behind the injected trait objects) and that `acp_session_service.rs`'s new `collection_repo` field does not leak any `rocket-infra` concrete type into `rocket-app`.
  - **Test coverage of this plan's own Review Focus section** — confirm the autonomy-gate table test, the get/set_env_var oracle test, and the mid-session-toggle test all exist and pass, and confirm the two explicitly-deferred items (Stdio token-via-argv, concurrent write locking) are not silently missing from a later plan's scope.
  - The subagent has authority to fix anything it finds directly (not just report it), following this repo's existing conventions (`crates/rocket-app/CLAUDE.md`, `.claude/rules/rust-ddd-boundaries.md`), and must leave `cargo test -p rocket-app -j4` and `cargo check --workspace -j4` green before finishing.
