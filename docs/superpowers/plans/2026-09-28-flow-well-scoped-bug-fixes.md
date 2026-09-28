# Flow Well-Scoped Bug Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five well-scoped, already-diagnosed GitHub issues on the Flow feature (#23/#24, #27, #28, #29, #30) — the backend global-environment-scope gap and its Flow-specific symptom, the Output node's missing captured value, the missing "running" node state, cycle-error edges not being highlighted, and missing e2e/git-visibility test coverage.

**Architecture:** Each task is a self-contained backend-to-frontend slice. Task 1 fixes a `rocket-app`-wide gap (`RequestExecutionService::build_variable_scopes` never populates `global_env`); Task 2 is the Flow-specific consumer of that fix (threading `global_env_name` through `FlowExecutionService`). Tasks 3–5 each add one field/event to the existing `FlowStepResult`/`DomainEvent::FlowStepCompleted` IPC contract and thread it through `tauri_event_bus.rs` → `tauri-api.ts` → `FlowToolbar.tsx`/`FlowCanvas.tsx`. Task 6 adds test coverage only, no behavior change.

**Tech Stack:** Rust (Tauri v2 backend, DDD crates), React 18 + TypeScript (Vite, Zustand, `@xyflow/react`), Vitest, `cargo test`.

**Spec:** GitHub issues [#23](https://github.com/Snehal1112/rocket/issues/23), [#24](https://github.com/Snehal1112/rocket/issues/24), [#27](https://github.com/Snehal1112/rocket/issues/27), [#28](https://github.com/Snehal1112/rocket/issues/28), [#29](https://github.com/Snehal1112/rocket/issues/29), [#30](https://github.com/Snehal1112/rocket/issues/30) — each already contains verified file/line evidence and a fix sketch (dated 2026-09-28); this plan re-verified every claim against current source before writing tasks below.

## Global Constraints

- Rust: never use the panicking unwrap shorthand in production paths (test code may use `.expect("...")`).
- Rust: never shell out to the `git` CLI — use the `git2` crate (`rocket-git::Git2Service`) directly, as this plan's Task 6 does.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only — never on persistence structs.
- Commits: conventional commits format (`feat:`, `fix:`, `chore:`, etc.), and every commit in this plan MUST be created via the `dev-workflow-skills:1-git-commit` skill (`Skill` tool) — never a freeform `git commit -m "..."`.
- `cargo check`/`cargo test` invocations in this repo must pass `-j4`.
- Tasks 1, 2, and 6 touch variable-resolution scope and/or read/write `.yml` files (`FsFlowRepo`), which trips this project's OpenCollection injection rule — each of those tasks' first step is reading `docs/superpowers/specs/opencollection-spec-reference.md`. Tasks 3, 4, and 5 touch only Flow's own event/graph plumbing (`rocket-flow` is explicitly a non-OpenCollection Rocket-only extension per its own `CLAUDE.md`) and do not trigger the rule.

## Review Focus

- An inline Request node's **body or header** placeholder (not just its URL) must also resolve against the newly-populated global env scope — Task 2's end-to-end test only exercises the URL; Task 1's own test additionally proves the underlying `resolve_request()` path is scope-agnostic (URL/headers/body all go through the same `vars` map), so this is covered structurally, not just by the URL assertion.
- A **Skipped** node (never dispatched because an upstream dependency failed) must never emit a `FlowStepStarted` event — Task 3's test asserts this explicitly, not just that `Success`/`Failed` nodes get one.
- An **Input** node's captured value must NOT leak into `FlowStepResult.value` — only `Output` nodes should ever report a value (Task 4's test asserts both sides of this).
- A **self-loop** cycle (a single node with an edge back to itself) must report that one edge as the cycle edge, not an empty list — Task 5's `self_loop_is_reported_as_cycle` test update covers this boundary case, distinct from the multi-node cycle case.
- The Flow save→reload round trip test (Task 6) must go through a **fresh** repo/service instance, not the same one that just saved — reusing the same instance would pass even if the on-disk serialization silently dropped a field, since the original in-memory value never actually left the process.

---

## Task 1: Backend resolves the global environment scope in `build_variable_scopes` (#23)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

**Interfaces:**
- Consumes: existing `RequestExecutionService::env_repo: Box<dyn EnvironmentRepository>` field (already the workspace-level global-environment repo; see `begin_phases`' existing manual patch at the call site being replaced).
- Produces: `build_variable_scopes(&self, global_env_name: Option<&str>, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>, external_secrets: &HashMap<String, String>) -> VariableContext` and `build_variable_context(&self, global_env_name: Option<&str>, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>, external_secrets: &HashMap<String, String>) -> HashMap<String, String>` — both gain a new **first** parameter. Every later task that calls these (none do outside this file) must pass `input.global_env_name.as_deref()`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/rocket-app/src/execution_service.rs` (near the other `global_env`-related tests, e.g. after the existing `global env scope must be populated from global_env_name` script test):

```rust
#[tokio::test]
async fn resolve_request_populates_global_env_scope_for_url_resolution() {
    let mut global_env = Environment::new("shared-global");
    global_env.set_variable(Variable::new("ORG_ID", "acme"));
    let env_repo = MultiEnvRepo::new(vec![global_env]);

    let executor = Arc::new(MockExecutor::new(200));
    let executor_arc = Arc::clone(&executor);

    let svc = RequestExecutionService::new(
        Box::new(env_repo),
        executor,
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    );

    let mut input = sample_input("https://example.com/{{ORG_ID}}", None);
    input.global_env_name = Some("shared-global".into());

    svc.execute(input).await.expect("execute should succeed");

    let last_url = executor_arc.last_url.lock().expect("lock").clone();
    assert_eq!(
        last_url,
        Some("https://example.com/acme".to_string()),
        "a global env var must resolve in the sent request URL, not just script scope"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-app -j4 resolve_request_populates_global_env_scope_for_url_resolution`
Expected: FAIL — assertion `last_url == Some("https://example.com/acme")` fails because `{{ORG_ID}}` stays unresolved (`last_url == Some("https://example.com/{{ORG_ID}}")`), since `resolve_request()` never sees the global scope today.

- [ ] **Step 3: Add `global_env_name` to `build_variable_scopes` and populate `ctx.global_env`**

In `crates/rocket-app/src/execution_service.rs`, change the signature and doc comment of `build_variable_scopes` (currently around line 357-369):

```rust
    /// Builds a scope-separated `VariableContext` from all backend-accessible
    /// scopes (global env, collection, environment, folder-chain,
    /// request-level).
    ///
    /// Reused by `build_variable_context()` and `execute()`.
    fn build_variable_scopes(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> VariableContext {
        // Precedence (lowest → highest): global_env < collection < env < folder < request.
        let mut ctx = VariableContext::default();

        if let Some(name) = global_env_name {
            if let Ok(global_env) = self.env_repo.get(name) {
                for var in global_env.variables.iter().filter(|v| v.enabled) {
                    ctx.global_env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }

        let effective_val = |cv: &rocket_collection::CollectionVariable| -> String {
```

(Keep the rest of the function body — the `collection`/`environment_name`/folder/request blocks and the trailing `ctx.external_secrets = ...` — exactly as-is; only the new `global_env_name` block above and the signature/doc comment change.)

- [ ] **Step 4: Thread the new parameter through `build_variable_context` and `resolve_request`**

```rust
    /// Builds a flattened variable map from all backend-accessible scopes
    /// (global env, collection, environment, folder-chain, request-level).
    ///
    /// Reused by `resolve_request()`, `run_load_test()`, and OAuth2 commands.
    pub fn build_variable_context(
        &self,
        global_env_name: Option<&str>,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        self.build_variable_scopes(
            global_env_name,
            collection,
            environment_name,
            request_path,
            external_secrets,
        )
        .flatten()
    }
```

In `resolve_request` (currently building `vars` via `self.build_variable_context(input.collection.as_deref(), ...)`), pass `input.global_env_name.as_deref()` as the new first argument:

```rust
        let vars = self.build_variable_context(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
```

- [ ] **Step 5: Update `begin_phases`' call site and remove its now-redundant manual patch**

`begin_phases` currently calls `build_variable_scopes` without a global-env name and then manually re-inserts global env vars into `var_ctx.global_env` afterward (the block starting `if let Some(name) = input.global_env_name.as_deref() { if let Ok(global_env) = self.env_repo.get(name) { ... } }`, right after the `build_variable_scopes` call). Update the call to pass the name, and delete the now-duplicate manual patch block entirely:

```rust
        let var_ctx = self.build_variable_scopes(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
```

(Change `let mut var_ctx = ...` to `let var_ctx = ...` since nothing mutates it afterward once the manual patch block is deleted — check for a later `mut` requirement before removing `mut`; if any other code in `begin_phases` still mutates `var_ctx`, keep `mut`.)

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p rocket-app -j4 resolve_request_populates_global_env_scope_for_url_resolution`
Expected: PASS

- [ ] **Step 7: Run the full crate's test suite to catch regressions**

Run: `cargo test -p rocket-app -j4`
Expected: PASS — in particular the existing `global env scope must be populated from global_env_name` script test (which exercised the old manual-patch path) must still pass, now via the unified `build_variable_scopes` path.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill to commit `crates/rocket-app/src/execution_service.rs`.

---

## Task 2: Flow execution threads `global_env_name` through to executed requests (#24)

**Depends on Task 1** (this task only has an effect once `build_variable_scopes`/`resolve_request` honor `ExecuteRequestInput.global_env_name`).

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`
- Modify: `src-tauri/src/commands/flow.rs`
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/flow/FlowToolbar.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: Task 1's `RequestExecutionService::build_variable_scopes`/`resolve_request` (global env now honored end-to-end); `getActiveGlobalEnvName(): string | undefined` (already exported by `src/lib/tauri-api.ts`, already used by `src/lib/execute-request.ts`).
- Produces: `RunFlowInput.global_env_name: Option<String>`; `build_execute_request_input(collection_repo, collection, environment_name, global_env_name: Option<&str>, node)` (gains a new 4th positional parameter, before `node`); `RunFlowInputDto.global_env_name: Option<String>`; `runFlow(collection, flowName, environmentName, globalEnvName?)` (gains a new 4th parameter); `FlowToolbarProps.globalEnvName?: string | null`.

- [ ] **Step 1: Write the failing Rust test — parameter is threaded**

Add to `crates/rocket-app/src/flow_execution_service.rs`'s `#[cfg(test)] mod tests`, near `saved_source_resolves_via_collection_repo_and_reuses_build_step_input`:

```rust
    #[test]
    fn build_execute_request_input_threads_global_env_name() {
        let mut saved = Request::new(
            "Get Auth Token",
            HttpMethod::Get,
            "https://api.example.com/login",
        );
        saved.tags = vec!["auth".to_string()];
        let repo = FakeCollectionRepo::new().with_request("my-api", "auth/login.yml", saved);

        let node = saved_flow_node("n1", "auth/login.yml");
        let input =
            build_execute_request_input(&repo, "my-api", Some("dev"), Some("shared-global"), &node)
                .expect("saved source must resolve");

        assert_eq!(input.global_env_name.as_deref(), Some("shared-global"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-app -j4 build_execute_request_input_threads_global_env_name`
Expected: FAIL to compile — `build_execute_request_input` does not yet accept a 4th `global_env_name` argument.

- [ ] **Step 3: Add `global_env_name` to `RunFlowInput` and thread it through `build_execute_request_input`**

In `crates/rocket-app/src/flow_execution_service.rs`:

```rust
/// Input DTO for `FlowExecutionService::run`.
#[derive(Debug, Clone)]
pub struct RunFlowInput {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
}
```

Update `build_execute_request_input`'s signature and its `build_step_input` call (currently passing a hardcoded `None`):

```rust
pub fn build_execute_request_input(
    collection_repo: &dyn rocket_collection::CollectionRepository,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    node: &FlowNode,
) -> DomainResult<ExecuteRequestInput> {
    // ... unchanged body up to the build_step_input call ...
    Ok(build_step_input(
        &item,
        collection,
        environment_name,
        global_env_name,
        rocket_workspace::RequestGuardPolicy::default(),
    ))
}
```

Update its one call site inside `execute_node`'s `FlowNodeKind::Request` branch:

```rust
            rocket_flow::FlowNodeKind::Request { .. } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    node,
                )?;
```

Fix the two other existing call sites in the same file's tests that will now fail to compile (`saved_source_propagates_not_found_instead_of_defaulting`, `inline_source_builds_request_from_embedded_fields`, `inline_source_with_unparseable_method_is_invalid_input_not_a_panic`, `non_request_node_is_rejected`, and `sample_execute_input`'s call) by inserting a `None,` argument in each (positionally after `environment_name`, before `node`) — e.g. `build_execute_request_input(&repo, "my-api", None, None, &node)`.

- [ ] **Step 4: Add `global_env_name: None,` to every existing `RunFlowInput { ... }` literal**

Add the new field to all 6 existing struct literals in this file's test module (the compiler's "missing field" errors will point at each): the 5 inline literals inside `empty_flow_completes_immediately_with_no_steps`, `single_request_node_executes_and_reports_success`, `unknown_flow_name_errors_before_publishing_started`, `cancelling_before_the_run_starts_stops_it_immediately`, `failed_node_skips_only_its_downstream_dependents`, plus the `run_input(flow_name: &str)` helper function used by the "whole-plan review" test section. Each gets `global_env_name: None,` added after `environment_name: None,`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rocket-app -j4 build_execute_request_input_threads_global_env_name`
Expected: PASS

- [ ] **Step 6: Write the failing end-to-end Rust test — a run actually resolves the global var**

Add to the same test module (place after `run_input`/`wire` are defined, in the "whole-plan review" section, so `RecordingExecutor`/`StaticEnvRepo` are already imported via `crate::test_doubles::*`):

```rust
    #[tokio::test]
    async fn a_run_resolves_a_global_env_placeholder_in_an_inline_requests_url() {
        let mut global_env = rocket_environment::Environment::new("shared-global");
        global_env.set_variable(rocket_environment::Variable::new("ORG_ID", "acme"));

        let executor = RecordingExecutor::new();
        let exec = RequestExecutionService::new(
            Box::new(StaticEnvRepo(global_env)),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );

        let flow = Flow {
            name: "global-env-flow".to_string(),
            nodes: vec![request_flow_node("a", "https://api.example.com/{{ORG_ID}}")],
            edges: Vec::new(),
        };
        let service = service_with_flow(flow);

        let mut input = run_input("global-env-flow");
        input.global_env_name = Some("shared-global".to_string());
        let summary = service.run(&exec, input).await.expect("run must succeed");

        assert_eq!(summary.steps[0].status_code, Some(200));
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/acme".to_string()],
            "the flow-executed request must resolve {{ORG_ID}} against the global environment"
        );
    }
```

- [ ] **Step 7: Run test to verify it fails, then implement**

Run: `cargo test -p rocket-app -j4 a_run_resolves_a_global_env_placeholder_in_an_inline_requests_url`
Expected: FAIL before Step 3's fix is applied (or if Step 3 was skipped) — `sent_urls()` would be `["https://api.example.com/{{ORG_ID}}"]`. Since Step 3 already threads `global_env_name` and Task 1 already fixed `resolve_request`, this should now PASS once Steps 1-5 above are done; if it still fails, re-check Step 3's `execute_node` call site was updated.

Run: `cargo test -p rocket-app -j4 a_run_resolves_a_global_env_placeholder_in_an_inline_requests_url`
Expected: PASS

- [ ] **Step 8: Update the IPC DTO — `RunFlowInputDto` and `RunFlowInputDto -> RunFlowInput`**

In `src-tauri/src/commands/flow.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowInputDto {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
}
impl From<RunFlowInputDto> for RunFlowInput {
    fn from(i: RunFlowInputDto) -> Self {
        Self {
            collection: i.collection,
            flow_name: i.flow_name,
            environment_name: i.environment_name,
            global_env_name: i.global_env_name,
        }
    }
}
```

- [ ] **Step 9: Run the crate's full test suite**

Run: `cargo check -p rocket-app -j4 && cargo check --workspace -j4 && cargo test -p rocket-app -j4`
Expected: PASS — `cargo check --workspace` catches any remaining `RunFlowInput { ... }` literal or `build_execute_request_input(...)` call site outside `rocket-app` (there should be none; `src-tauri`'s `run_flow` command builds `RunFlowInput` only via `input.into()` from the DTO, already updated in Step 8).

- [ ] **Step 10: Commit backend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `crates/rocket-app/src/flow_execution_service.rs` and `src-tauri/src/commands/flow.rs`.

- [ ] **Step 11: Thread `globalEnvName` through the frontend — `runFlow`**

In `src/lib/tauri-api.ts`, update `runFlow`:

```ts
export const runFlow = (
  collection: string,
  flowName: string,
  environmentName?: string | null,
  globalEnvName?: string | null,
) =>
  invoke<FlowRunSummary>('run_flow', {
    input: {
      collection,
      flowName,
      environmentName: environmentName ?? null,
      globalEnvName: globalEnvName ?? null,
    },
  });
```

- [ ] **Step 12: Update the failing frontend test for the new `runFlow` signature**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, update line ~88's assertion to include the new 4th argument (still `null`, since `renderToolbar()` does not pass a `globalEnvName` prop):

```ts
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null, null),
    );
```

- [ ] **Step 13: Run test to verify it fails, then update `FlowToolbar`**

Run: `yarn test FlowToolbar`
Expected: FAIL — `runFlow` is called with only 3 arguments today.

In `src/components/flow/FlowToolbar.tsx`, add the prop and pass it through:

```ts
interface FlowToolbarProps {
  collection: string;
  flowName: string;
  environmentName: string | null;
  globalEnvName?: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: NodeDetail) => void;
  onRunStateChange: (state: 'running' | 'done', runId?: string) => void;
  tabRunState?: 'idle' | 'running' | 'done';
  tabRunId?: string;
  onBeforeRun?: () => Promise<boolean>;
}

export function FlowToolbar({
  collection,
  flowName,
  environmentName,
  globalEnvName,
  onPatchStatus,
  onRunStateChange,
  tabRunState,
  tabRunId,
  onBeforeRun,
}: FlowToolbarProps) {
```

And in `handleRun`, update the `runFlow` call:

```ts
      const summary = await runFlow(collection, flowName, environmentName, globalEnvName);
```

- [ ] **Step 14: Run test to verify it passes**

Run: `yarn test FlowToolbar`
Expected: PASS

- [ ] **Step 15: Write a new test asserting `globalEnvName` is forwarded when set**

Add to `src/components/flow/__tests__/FlowToolbar.test.tsx`:

```ts
  it('forwards globalEnvName to runFlow when provided', async () => {
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        globalEnvName='shared-global'
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        'shared-global',
      ),
    );
  });
```

Run: `yarn test FlowToolbar`
Expected: PASS

- [ ] **Step 16: Wire `FlowPane` to read and pass the active global environment name**

In `src/components/flow/FlowPane.tsx`, import `getActiveGlobalEnvName` alongside the other `tauri-api` imports, and compute/pass it:

```ts
import {
  type CollectionSummary,
  type FlowEdge,
  type FlowNode,
  type FlowNodeStatus,
  getActiveGlobalEnvName,
  listCollections,
  listFlows,
  saveFlow,
} from '@/lib/tauri-api';
```

Inside the component body (near where `activeEnvironmentName` is read), and in the `<FlowToolbar>` JSX:

```ts
  const globalEnvName = getActiveGlobalEnvName();
```

```tsx
        <FlowToolbar
          collection={collectionName}
          flowName={flowName}
          environmentName={activeEnvironmentName}
          globalEnvName={globalEnvName}
          onPatchStatus={(nodeId, status, detail) =>
```

- [ ] **Step 17: Run the full frontend test suite for the flow feature**

Run: `yarn test src/components/flow`
Expected: PASS

- [ ] **Step 18: Commit frontend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src/lib/tauri-api.ts`, `src/components/flow/FlowToolbar.tsx`, `src/components/flow/FlowPane.tsx`, and `src/components/flow/__tests__/FlowToolbar.test.tsx`.

---

## Task 3: Emit `FlowStepStarted` so nodes show a running state mid-execution (#28)

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `crates/rocket-app/src/flow_execution_service.rs`
- Modify: `src-tauri/src/tauri_event_bus.rs`
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/flow/FlowToolbar.tsx`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: `crate::test_doubles::{RecordingPublisher, SharedPublisher}` (already in `crates/rocket-app/src/test_doubles.rs`).
- Produces: `DomainEvent::FlowStepStarted { run_id: String, node_id: String }`; Tauri event channel `"flow-step-started"`; `onFlowStepStarted(handler): Promise<UnlistenFn>` in `src/lib/tauri-api.ts`.

- [ ] **Step 1: Write the failing Rust test**

Add to `crates/rocket-app/src/flow_execution_service.rs`'s test module, in the "whole-plan review" section (after `run_input`/`wire` are defined):

```rust
    #[tokio::test]
    async fn step_started_is_published_before_step_completed_and_never_for_a_skipped_node() {
        let flow = Flow {
            name: "two-nodes".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
        };
        let publisher = RecordingPublisher::new();
        let service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
        );
        let exec = exec_failing_for_url("https://api.example.com/a");

        service
            .run(&exec, run_input("two-nodes"))
            .await
            .expect("run must complete even with a failed node");

        let events = publisher.events();
        let started_ids: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } => Some(node_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started_ids,
            vec!["a".to_string()],
            "node b is Skipped (its dependency a failed) and must never get a started event"
        );

        let started_idx = events
            .iter()
            .position(|e| matches!(e, DomainEvent::FlowStepStarted { node_id, .. } if node_id == "a"))
            .expect("started event for a must exist");
        let completed_idx = events
            .iter()
            .position(|e| matches!(e, DomainEvent::FlowStepCompleted { node_id, .. } if node_id == "a"))
            .expect("completed event for a must exist");
        assert!(
            started_idx < completed_idx,
            "started must publish strictly before completed"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-app -j4 step_started_is_published_before_step_completed_and_never_for_a_skipped_node`
Expected: FAIL to compile — `DomainEvent::FlowStepStarted` does not exist yet.

- [ ] **Step 3: Add the `FlowStepStarted` variant**

In `crates/rocket-shared/src/events.rs`, add a new variant in the Flow events section, immediately before `FlowStepCompleted`:

```rust
    /// Emitted immediately before a node is dispatched — once per node that
    /// is actually attempted, never for a node marked `Skipped` (those never
    /// reach dispatch).
    FlowStepStarted {
        run_id: String,
        node_id: String,
    },
```

- [ ] **Step 4: Publish it from the run loop**

In `crates/rocket-app/src/flow_execution_service.rs`'s `run()` method, immediately before the existing `let result = match nodes_by_id.get(node_id.as_str()) { ... };` line (i.e. after the `skipped.contains(node_id)` check/`continue` and the cancellation check, so a skipped or cancelled-before-dispatch node never gets one):

```rust
            self.events.publish(DomainEvent::FlowStepStarted {
                run_id: run_id.clone(),
                node_id: node_id.clone(),
            });

            let result = match nodes_by_id.get(node_id.as_str()) {
```

- [ ] **Step 5: Map the new event to a Tauri channel**

In `src-tauri/src/tauri_event_bus.rs`, add a mapping in the Flow events group, before the existing `FlowStepCompleted` line:

```rust
            DomainEvent::FlowStepStarted { .. } => "flow-step-started",
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p rocket-app -j4 step_started_is_published_before_step_completed_and_never_for_a_skipped_node`
Expected: PASS

- [ ] **Step 7: Run the full backend test suites**

Run: `cargo check --workspace -j4 && cargo test -p rocket-shared -j4 && cargo test -p rocket-app -j4`
Expected: PASS — `cargo check --workspace` catches any other exhaustive match on `DomainEvent` that the new variant would break (there should be none outside `tauri_event_bus.rs`, since other matchers use wildcard arms).

- [ ] **Step 8: Commit backend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/flow_execution_service.rs`, and `src-tauri/src/tauri_event_bus.rs`.

- [ ] **Step 9: Add the frontend binding**

In `src/lib/tauri-api.ts`, add, immediately before the `FlowStepCompletedEvent` interface:

```ts
export interface FlowStepStartedEvent {
  type: 'flowStepStarted';
  run_id: string;
  node_id: string;
}

export const onFlowStepStarted = (
  handler: (event: FlowStepStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepStartedEvent>('flow-step-started', (e) => handler(e.payload));
```

- [ ] **Step 10: Write the failing frontend test**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, add `onFlowStepStarted: vi.fn()` to the `vi.mock('@/lib/tauri-api', ...)` return object, and in `beforeEach`, mirror the `onFlowStepCompleted` mock setup:

```ts
let startedStepHandler: Parameters<typeof tauriApi.onFlowStepStarted>[0] | undefined;
```

```ts
    startedStepHandler = undefined;
    vi.mocked(tauriApi.onFlowStepStarted).mockImplementation(async (h) => {
      startedStepHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
```
(add alongside the existing `startedHandler`/`stepHandler` mock setup, and clear it via `vi.mocked(tauriApi.onFlowStepStarted).mockClear();` next to the other `mockClear()` calls)

Then add a new test:

```ts
  it('shows a node as running when flow-step-started fires', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedStepHandler).toBeDefined());
    started('run-123');
    startedStepHandler?.({ type: 'flowStepStarted', run_id: 'run-123', node_id: 'node-a' });
    expect(onPatchStatus).toHaveBeenCalledWith('node-a', 'running');
  });
```

- [ ] **Step 11: Run test to verify it fails**

Run: `yarn test FlowToolbar`
Expected: FAIL — `FlowToolbar` never subscribes to `onFlowStepStarted`.

- [ ] **Step 12: Subscribe to `onFlowStepStarted` in `FlowToolbar`**

In `src/components/flow/FlowToolbar.tsx`, import `onFlowStepStarted` alongside `onFlowStepCompleted`:

```ts
import {
  cancelFlowRun,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
```

In the resumed-run `useEffect` (the one guarded by `if (!resumedRunId) return;`), add a second subscription alongside the existing `onFlowStepCompleted` one, tracking both unlisten functions:

```ts
  useEffect(() => {
    if (!resumedRunId) return;
    let unlistenStep: UnlistenFn | undefined;
    let unlistenStarted: UnlistenFn | undefined;
    let disposed = false;
    void onFlowStepStarted((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, 'running');
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStarted = fn;
    });
    void onFlowStepCompleted((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, event.status, {
        statusCode: event.status_code ?? undefined,
        durationMs: event.duration_ms ?? undefined,
        error: event.error ?? undefined,
      });
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStep = fn;
    });
    return () => {
      disposed = true;
      unlistenStarted?.();
      unlistenStep?.();
    };
  }, [resumedRunId]);
```

In `handleRun`'s primary subscription block, add the third listener and include it in `unlistenRefs.current`:

```ts
    const unlistenStepStarted = await onFlowStepStarted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, 'running');
    });
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, event.status, {
        statusCode: event.status_code ?? undefined,
        durationMs: event.duration_ms ?? undefined,
        error: event.error ?? undefined,
      });
    });
    unlistenRefs.current = [unlistenStarted, unlistenStepStarted, unlistenStep];
```

- [ ] **Step 13: Run test to verify it passes**

Run: `yarn test FlowToolbar`
Expected: PASS

- [ ] **Step 14: Run the full frontend flow test suite**

Run: `yarn test src/components/flow`
Expected: PASS

- [ ] **Step 15: Commit frontend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src/lib/tauri-api.ts`, `src/components/flow/FlowToolbar.tsx`, and `src/components/flow/__tests__/FlowToolbar.test.tsx`.

---

## Task 4: Output node displays its captured value after a run (#27)

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/flow/FlowToolbar.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx`
- Modify: `src/components/flow/nodes/OutputNode.tsx`

**Interfaces:**
- Consumes: `CapturedOutput::Value(VariableValue)` (existing, from `execute_node`'s `Input`/`Output` branches).
- Produces: `FlowStepResult.value: Option<String>`; `DomainEvent::FlowStepCompleted.value: Option<String>`; `OutputNodeData.value?: string` (renamed from `result`).

- [ ] **Step 1: Write the failing Rust test**

Add to `crates/rocket-app/src/flow_execution_service.rs`'s test module (near `load_ordered_nodes_returns_dependency_order_for_a_valid_flow`, which already uses `linear_flow()`: Input "a" (value `"bob"`) → Output "b" wired via edge `"e1"` with expression `"response.body"`):

```rust
    #[tokio::test]
    async fn a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes() {
        let service = service_with_flow(linear_flow());
        let exec = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("bob"),
            }),
        );

        let summary = service
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("run must succeed");

        let step_for = |id: &str| {
            summary
                .steps
                .iter()
                .find(|s| s.node_id == id)
                .expect("step must be recorded")
        };
        assert_eq!(
            step_for("b").value.as_deref(),
            Some("bob"),
            "the Output node must report its captured value"
        );
        assert_eq!(
            step_for("a").value, None,
            "an Input node must never report a value, only Output nodes do"
        );
    }
```

(`run_input` here needs `collection: "my-api"` to match `linear_flow()`'s expectations — confirm it does; `service_with_engine`/`FixedJsonqEngine` are already defined earlier in this same test module.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-app -j4 a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes`
Expected: FAIL to compile — `FlowStepResult` has no `value` field yet.

- [ ] **Step 3: Add `value` to `FlowStepResult` and `DomainEvent::FlowStepCompleted`**

In `crates/rocket-app/src/flow_execution_service.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    /// The node's captured output value, populated only for `Output`-kind
    /// nodes (see `result_to_step`). `None` for a Request node (its result is
    /// the HTTP response, not a single value), an Input node, or a node that
    /// never produced output (Skipped/Failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}
```

In `crates/rocket-shared/src/events.rs`, add the matching field to `FlowStepCompleted`:

```rust
    FlowStepCompleted {
        run_id: String,
        node_id: String,
        status: FlowNodeStatus,
        status_code: Option<u16>,
        duration_ms: Option<u64>,
        error: Option<String>,
        /// The node's captured value, populated only for `Output`-kind
        /// nodes. See `rocket_app::flow_execution_service::FlowStepResult`.
        value: Option<String>,
    },
```

- [ ] **Step 4: Populate `value` only for Output-kind nodes in `result_to_step`**

In `crates/rocket-app/src/flow_execution_service.rs`, change `result_to_step` to accept the node so it can check its kind, and set `value` accordingly:

```rust
fn result_to_step(
    node_id: &str,
    node: Option<&FlowNode>,
    result: &DomainResult<CapturedOutput>,
) -> FlowStepResult {
    match result {
        Ok(CapturedOutput::Request(out)) => {
            let status = out.response.status;
            let success = out.response.is_success();
            FlowStepResult {
                node_id: node_id.to_string(),
                status: if success {
                    FlowNodeStatus::Success
                } else {
                    FlowNodeStatus::Failed
                },
                status_code: Some(status),
                duration_ms: Some(out.response.duration_ms),
                error: (!success).then(|| format!("non-2xx response: {status}")),
                value: None,
            }
        }
        Ok(CapturedOutput::Value(v)) => {
            let is_output = matches!(
                node.map(|n| &n.kind),
                Some(FlowNodeKind::Output { .. })
            );
            FlowStepResult {
                node_id: node_id.to_string(),
                status: FlowNodeStatus::Success,
                status_code: None,
                duration_ms: None,
                error: None,
                value: is_output.then(|| v.data().to_string()),
            }
        }
        Err(e) => FlowStepResult {
            node_id: node_id.to_string(),
            status: FlowNodeStatus::Failed,
            status_code: None,
            duration_ms: None,
            error: Some(e.to_string()),
            value: None,
        },
    }
}
```

Update its call site in `run()`. Currently the dispatch match discards the `&FlowNode` reference once it returns `result`; capture it first:

```rust
            let node_opt = nodes_by_id.get(node_id.as_str()).copied();
            let result = match node_opt {
                Some(node) => {
                    self.execute_node(exec, &input, &flow, node, &captured, &external_secrets)
                        .await
                }
                None => Err(DomainError::Internal(format!(
                    "node '{node_id}' is missing from the flow"
                ))),
            };

            let step = result_to_step(node_id, node_opt, &result);
```

Add `value: None,` to the `FlowStepResult` struct literal built for a Skipped node (the block right after the `if skipped.contains(node_id) { ... }` check).

Add `value: step.value.clone(),` to both `self.events.publish(DomainEvent::FlowStepCompleted { ... })` call sites (the Skipped-node one and the main dispatch one).

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rocket-app -j4 a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes`
Expected: PASS

- [ ] **Step 6: Run the full backend test suites**

Run: `cargo check --workspace -j4 && cargo test -p rocket-shared -j4 && cargo test -p rocket-app -j4`
Expected: PASS

- [ ] **Step 7: Commit backend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `crates/rocket-app/src/flow_execution_service.rs` and `crates/rocket-shared/src/events.rs`.

- [ ] **Step 8: Thread `value` through the frontend IPC types**

In `src/lib/tauri-api.ts`, add `value: string | null;` to both `FlowStepResult` and `FlowStepCompletedEvent`:

```ts
export interface FlowStepResult {
  nodeId: string;
  status: FlowRunNodeStatus;
  statusCode: number | null;
  durationMs: number | null;
  error: string | null;
  value: string | null;
}
```

```ts
export interface FlowStepCompletedEvent {
  type: 'flowStepCompleted';
  run_id: string;
  node_id: string;
  status: FlowRunNodeStatus;
  status_code: number | null;
  duration_ms: number | null;
  error: string | null;
  value: string | null;
}
```

- [ ] **Step 9: Thread `value` through `FlowToolbar` and `FlowCanvas`'s detail types**

In `src/components/flow/FlowToolbar.tsx`, update `NodeDetail` and both `onPatchStatus` call sites plus the final-summary loop:

```ts
type NodeDetail = { statusCode?: number; durationMs?: number; error?: string; value?: string };
```

In the resumed-run effect's `onFlowStepCompleted` handler, the main handler's `onFlowStepCompleted` handler, and the `for (const step of summary.steps)` loop in `handleRun`, add `value: <event|step>.value ?? undefined` to the `detail`/third-argument object passed to `onPatchStatus`/`onPatchStatusRef.current`.

In `src/components/flow/FlowCanvas.tsx`, add `value?: string` to the `nodeDetail` prop's inline type in `FlowCanvasProps` and in `toRfNodes`'s parameter type:

```ts
  nodeDetail?: Record<
    string,
    { statusCode?: number; durationMs?: number; error?: string; value?: string }
  >;
```

- [ ] **Step 10: Rename `OutputNodeData.result` to `value` and render it**

In `src/components/flow/nodes/OutputNode.tsx`:

```ts
export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  hasCycleError?: boolean;
  value?: string;
}
```

```tsx
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{data.value ?? '—'}</div>
```

(Since `toRfNodes` already spreads `...nodeDetail?.[n.id]` directly into each node's `data`, and Step 9 made the wire-level field name `value`, no remapping is needed — `nodeDetail[id].value` lands on `data.value` automatically.)

- [ ] **Step 11: Run the full frontend flow test suite**

Run: `yarn test src/components/flow`
Expected: PASS — the existing `FlowPane.test.tsx` assertion `expect(flagged.map((c) => c.textContent)).toEqual(['Out a—', 'Out b—'])` must still pass unchanged, since an Output node with no run yet still renders `'—'` regardless of the field rename.

- [ ] **Step 12: Commit frontend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src/lib/tauri-api.ts`, `src/components/flow/FlowToolbar.tsx`, `src/components/flow/FlowCanvas.tsx`, and `src/components/flow/nodes/OutputNode.tsx`.

---

## Task 5: Cycle-detection save error highlights the offending edges, not just nodes (#29)

**Files:**
- Modify: `crates/rocket-flow/src/graph.rs`
- Modify: `crates/rocket-app/src/flow_service.rs`
- Modify: `src/lib/flow-wiring.ts`
- Modify: `src/components/flow/FlowCanvas.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Test: `src/components/flow/__tests__/FlowCanvas.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx`

**Interfaces:**
- Consumes: none new (pure extension of existing `FlowGraphError`/error-string parsing).
- Produces: `FlowGraphError::Cycle { node_ids: Vec<String>, edge_ids: Vec<String> }`; error message format `"flow contains a cycle through node(s): a, b; edge(s): e1, e2"`; `parseCycleErrorMessage(message: string): { nodeIds: string[]; edgeIds: string[] } | null` (new, in `src/lib/flow-wiring.ts`); `FlowCanvasProps.cycleEdgeIds?: string[]`.

- [ ] **Step 1: Write the failing Rust tests for `cycle_nodes_and_edges`**

In `crates/rocket-flow/src/graph.rs`'s test module, update `self_loop_is_reported_as_cycle` and `longer_cycle_is_reported_with_all_member_nodes` to also assert edge ids, and `cycle_error_leaves_out_nodes_only_downstream_of_the_cycle` to include the new field:

```rust
    #[test]
    fn self_loop_is_reported_as_cycle() {
        let flow = Flow {
            name: "self-loop".to_string(),
            nodes: vec![node("a")],
            edges: vec![edge("e1", "a", "a")],
        };
        let err = topological_sort(&flow).expect_err("must detect cycle");
        match err {
            FlowGraphError::Cycle { node_ids, edge_ids } => {
                assert_eq!(node_ids, vec!["a".to_string()]);
                assert_eq!(edge_ids, vec!["e1".to_string()]);
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }
```

```rust
    #[test]
    fn longer_cycle_is_reported_with_all_member_nodes() {
        let flow = Flow {
            name: "cycle".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                edge("e1", "a", "b"),
                edge("e2", "b", "c"),
                edge("e3", "c", "a"),
            ],
        };
        let err = topological_sort(&flow).expect_err("must detect cycle");
        match err {
            FlowGraphError::Cycle {
                mut node_ids,
                mut edge_ids,
            } => {
                node_ids.sort();
                edge_ids.sort();
                assert_eq!(
                    node_ids,
                    vec!["a".to_string(), "b".to_string(), "c".to_string()]
                );
                assert_eq!(
                    edge_ids,
                    vec!["e1".to_string(), "e2".to_string(), "e3".to_string()]
                );
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }
```

```rust
    #[test]
    fn cycle_error_leaves_out_nodes_only_downstream_of_the_cycle() {
        // z -> a <-> b -> c -> d: z is upstream, c and d are downstream.
        let flow = Flow {
            name: "cycle-with-tail".to_string(),
            nodes: vec![node("z"), node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e0", "z", "a"),
                edge("e1", "a", "b"),
                edge("e2", "b", "a"),
                edge("e3", "b", "c"),
                edge("e4", "c", "d"),
            ],
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::Cycle {
                node_ids: vec!["a".to_string(), "b".to_string()],
                edge_ids: vec!["e1".to_string(), "e2".to_string()],
            })
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-flow -j4`
Expected: FAIL to compile — `FlowGraphError::Cycle` has no `edge_ids` field yet.

- [ ] **Step 3: Add `edge_ids` to `FlowGraphError::Cycle` and compute it**

In `crates/rocket-flow/src/graph.rs`, update the enum:

```rust
#[derive(Debug, Clone, PartialEq, Error)]
pub enum FlowGraphError {
    /// Holds the nodes and edges that lie on a cycle (or on a path between
    /// two cycles). Nodes/edges only downstream of a cycle are left out.
    #[error("cycle detected through node(s): {node_ids:?}, edge(s): {edge_ids:?}")]
    Cycle {
        node_ids: Vec<String>,
        edge_ids: Vec<String>,
    },
    #[error("edge references unknown node: {node_id}")]
    UnknownNode { node_id: String },
    /// Two nodes in `flow.nodes` share the same id.
    #[error("duplicate node id: {node_id}")]
    DuplicateNode { node_id: String },
}
```

Update `topological_sort`'s error construction:

```rust
    if order.len() != flow.nodes.len() {
        let (node_ids, edge_ids) = cycle_nodes_and_edges(flow, &in_degree);
        return Err(FlowGraphError::Cycle { node_ids, edge_ids });
    }
```

Rename `cycle_nodes` to `cycle_nodes_and_edges`, keep its existing node-peeling logic exactly as-is, and add an edge-filtering step at the end:

```rust
/// Picks the nodes and edges to report after Kahn's algorithm stalls. Every
/// node it did not emit still has a non-zero in-degree. That set also holds
/// nodes that only sit downstream of a cycle, so this peels those off by
/// running Kahn's algorithm backwards over the leftover subgraph. An edge is
/// reported when both its endpoints are cycle nodes (never just downstream).
fn cycle_nodes_and_edges(
    flow: &Flow,
    in_degree: &HashMap<&str, usize>,
) -> (Vec<String>, Vec<String>) {
    let leftover: HashSet<&str> = in_degree
        .iter()
        .filter(|(_, &degree)| degree > 0)
        .map(|(&id, _)| id)
        .collect();

    let mut out_degree: HashMap<&str, usize> = leftover.iter().map(|&id| (id, 0)).collect();
    let mut predecessors: HashMap<&str, Vec<&str>> =
        leftover.iter().map(|&id| (id, Vec::new())).collect();
    for edge in &flow.edges {
        let (source, target) = (edge.source_node_id.as_str(), edge.target_node_id.as_str());
        if leftover.contains(source) && leftover.contains(target) {
            *out_degree
                .get_mut(source)
                .expect("source is in leftover, which seeded out_degree") += 1;
            predecessors
                .get_mut(target)
                .expect("target is in leftover, which seeded predecessors")
                .push(source);
        }
    }

    let mut queue: VecDeque<&str> = out_degree
        .iter()
        .filter(|(_, &degree)| degree == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut peeled: HashSet<&str> = HashSet::new();
    while let Some(id) = queue.pop_front() {
        peeled.insert(id);
        for &prev in predecessors
            .get(id)
            .expect("queued ids come from out_degree, which shares keys with predecessors")
        {
            let degree = out_degree
                .get_mut(prev)
                .expect("prev came from predecessors, built only from leftover ids");
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(prev);
            }
        }
    }

    let cycle_node_ids: HashSet<&str> = leftover
        .iter()
        .copied()
        .filter(|id| !peeled.contains(id))
        .collect();

    // Keep the caller's node order so the error message is stable.
    let node_ids = flow
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| cycle_node_ids.contains(id))
        .map(str::to_string)
        .collect();

    // Keep the caller's edge order so the error message is stable.
    let edge_ids = flow
        .edges
        .iter()
        .filter(|e| {
            cycle_node_ids.contains(e.source_node_id.as_str())
                && cycle_node_ids.contains(e.target_node_id.as_str())
        })
        .map(|e| e.id.clone())
        .collect();

    (node_ids, edge_ids)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS

- [ ] **Step 5: Fix the now-broken build in `flow_service.rs`**

`cargo check --workspace -j4` will now fail at `crates/rocket-app/src/flow_service.rs`'s `FlowGraphError::Cycle { node_ids }` match arm (missing `edge_ids`). Update it:

```rust
            FlowGraphError::Cycle { node_ids, edge_ids } => {
                rocket_shared::error::DomainError::InvalidInput(format!(
                    "flow contains a cycle through node(s): {}; edge(s): {}",
                    node_ids.join(", "),
                    edge_ids.join(", ")
                ))
            }
```

- [ ] **Step 6: Update the existing `flow_service.rs` test to also assert edge ids**

In `crates/rocket-app/src/flow_service.rs`'s test module, extend `save_rejects_cyclic_graph_and_names_the_nodes`:

```rust
    #[test]
    fn save_rejects_cyclic_graph_and_names_the_nodes() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let err = svc
            .save("demo", cyclic_flow())
            .expect_err("cyclic flow must be rejected");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        let message = err.to_string();

        // Parse the node id list between "node(s): " and the "; edge(s):"
        // separator, so single letters inside the message prose cannot
        // satisfy the check by accident.
        let node_segment = message
            .split("node(s): ")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap_or_else(|| panic!("error should list the cyclic nodes, got: {message}"));
        let mut ids: Vec<&str> = node_segment.split(", ").collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["a", "b"], "got: {message}");

        let edge_segment = message
            .split("edge(s): ")
            .nth(1)
            .unwrap_or_else(|| panic!("error should list the cyclic edges, got: {message}"));
        let mut edge_ids: Vec<&str> = edge_segment.split(", ").collect();
        edge_ids.sort_unstable();
        assert_eq!(edge_ids, vec!["e1", "e2"], "got: {message}");
    }
```

- [ ] **Step 7: Run the full backend test suites**

Run: `cargo check --workspace -j4 && cargo test -p rocket-flow -j4 && cargo test -p rocket-app -j4`
Expected: PASS

- [ ] **Step 8: Commit backend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `crates/rocket-flow/src/graph.rs` and `crates/rocket-app/src/flow_service.rs`.

- [ ] **Step 9: Write the failing frontend test for `parseCycleErrorMessage`**

Read `src/lib/flow-wiring.ts` first to see its existing exports and import style. Then create (if it does not already exist) `src/lib/__tests__/flow-wiring.test.ts` and add:

```ts
import { describe, expect, it } from 'vitest';
import { parseCycleErrorMessage } from '@/lib/flow-wiring';

describe('parseCycleErrorMessage', () => {
  it('extracts node ids and edge ids from the backend cycle-rejection message', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2';
    expect(parseCycleErrorMessage(message)).toEqual({
      nodeIds: ['a', 'b'],
      edgeIds: ['e1', 'e2'],
    });
  });

  it('returns an empty edge list when no edge segment is present', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b';
    expect(parseCycleErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: [] });
  });

  it('returns null for an unrelated error message', () => {
    expect(parseCycleErrorMessage('Invalid input: flow name is empty')).toBeNull();
  });
});
```

- [ ] **Step 10: Run test to verify it fails**

Run: `yarn test flow-wiring`
Expected: FAIL — `parseCycleErrorMessage` does not exist yet.

- [ ] **Step 11: Implement `parseCycleErrorMessage`**

Add to the end of `src/lib/flow-wiring.ts`:

```ts
export interface CycleError {
  nodeIds: string[];
  edgeIds: string[];
}

// Backend's FlowService::save rejects a cyclic flow with the plain string
// "Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2"
// (ids joined by ", ", node/edge segments joined by "; " — see
// flow_service.rs's save()). The edge segment is optional so an older-format
// message without it still parses.
export function parseCycleErrorMessage(message: string): CycleError | null {
  const match = message.match(
    /flow contains a cycle through node\(s\): ([^;]*)(?:; edge\(s\): (.*))?$/,
  );
  if (!match) return null;
  return {
    nodeIds: match[1].split(', ').map((s) => s.trim()),
    edgeIds: match[2] ? match[2].split(', ').map((s) => s.trim()) : [],
  };
}
```

- [ ] **Step 12: Run test to verify it passes**

Run: `yarn test flow-wiring`
Expected: PASS

- [ ] **Step 13: Thread `cycleEdgeIds` through `FlowCanvas`**

In `src/components/flow/FlowCanvas.tsx`, add the prop and use it in `toRfEdges`:

```ts
export interface FlowCanvasProps {
  // ... existing fields ...
  cycleNodeIds?: string[];
  cycleEdgeIds?: string[];
}
```

```ts
function toRfEdges(
  edges: FlowEdge[],
  selectedIds: ReadonlySet<string>,
  cycleEdgeIds?: string[],
): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    sourceHandle: 'result',
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
    selected: selectedIds.has(e.id),
    style: cycleEdgeIds?.includes(e.id) ? { stroke: '#ef4444', strokeWidth: 2 } : undefined,
  }));
}
```

Destructure `cycleEdgeIds` in `FlowCanvasInner`'s props and pass it through `toRfEdges`:

```ts
function FlowCanvasInner({
  nodes,
  edges,
  nodeStatus,
  onNodesChange,
  onEdgesChange,
  onConnect,
  onAddNode,
  flowCollectionName,
  nodeDetail,
  cycleNodeIds,
  cycleEdgeIds,
}: FlowCanvasProps) {
```

```ts
  const rfEdges = useMemo(
    () => toRfEdges(edges, selectedEdgeIds, cycleEdgeIds),
    [edges, selectedEdgeIds, cycleEdgeIds],
  );
```

- [ ] **Step 14: Write the failing frontend test for edge styling**

In `src/components/flow/__tests__/FlowCanvas.test.tsx`, add:

```tsx
  it('gives a cycle edge a distinct stroke style', () => {
    const graph: FlowNode[] = [
      { id: 'a', kind: { kind: 'Output', label: 'A' }, position: { x: 0, y: 0 } },
      { id: 'b', kind: { kind: 'Output', label: 'B' }, position: { x: 200, y: 0 } },
    ];
    const graphEdges: FlowEdge[] = [
      {
        id: 'e1',
        sourceNodeId: 'a',
        targetNodeId: 'b',
        targetField: 'value',
        expression: 'response.body',
      },
    ];
    render(
      <FlowCanvas
        nodes={graph}
        edges={graphEdges}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
        cycleEdgeIds={['e1']}
      />,
    );
    const path = screen.getByTestId('rf__edge-e1').querySelector('path');
    expect(path).toHaveStyle({ stroke: '#ef4444' });
  });
```

- [ ] **Step 15: Run test to verify it fails, then passes**

Run: `yarn test FlowCanvas`
Expected: FAIL before Step 13, then PASS after.

- [ ] **Step 16: Wire `FlowPane` to parse and pass `cycleEdgeIds`**

In `src/components/flow/FlowPane.tsx`, import `parseCycleErrorMessage`:

```ts
import { buildEdgeFromConnection, parseCycleErrorMessage } from '@/lib/flow-wiring';
```

Add `cycleEdgeIds` state next to `cycleNodeIds`:

```ts
  const [cycleEdgeIds, setCycleEdgeIds] = useState<string[]>([]);
```

Replace `handleSave`'s regex-based catch block:

```ts
    } catch (err) {
      // Plan 07's save_flow rejects with the plain string
      // "Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2"
      // (ids joined by ", ", node/edge segments joined by "; "). Parse and
      // flag both, rather than showing only a generic toast.
      const message = String(err);
      const parsed = parseCycleErrorMessage(message);
      if (parsed) {
        setCycleNodeIds(parsed.nodeIds);
        setCycleEdgeIds(parsed.edgeIds);
      }
      toast.error(`Could not save flow: ${message}`);
      return false;
    }
```

And on the success path, reset both:

```ts
      setCycleNodeIds([]);
      setCycleEdgeIds([]);
      markClean(tab.id);
```

Pass the new prop to `<FlowCanvas>`:

```tsx
      <FlowCanvas
        nodes={tab.nodes}
        edges={tab.edges}
        nodeStatus={tab.nodeStatus}
        nodeDetail={tab.nodeDetail}
        cycleNodeIds={cycleNodeIds}
        cycleEdgeIds={cycleEdgeIds}
        onNodesChange={(nodes) => updateFlowNodes(tab.id, nodes)}
        onEdgesChange={(edges) => updateFlowEdges(tab.id, edges)}
        onConnect={handleConnect}
        onAddNode={handleAddNode}
        flowCollectionName={tab.collectionName}
      />
```

- [ ] **Step 17: Update the existing `FlowPane.test.tsx` cycle test's mocked message to the new format**

In `src/components/flow/__tests__/FlowPane.test.tsx`, update the `saveFlow` rejection value in `'flags the node ids named in a cycle rejection'`:

```ts
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2',
    );
```

(No other change needed — the test only asserts on node-card ring styling, which `parseCycleErrorMessage` still extracts correctly.)

- [ ] **Step 18: Run the full frontend flow test suite**

Run: `yarn test src/components/flow && yarn test flow-wiring`
Expected: PASS

- [ ] **Step 19: Commit frontend changes**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src/lib/flow-wiring.ts`, `src/lib/__tests__/flow-wiring.test.ts`, `src/components/flow/FlowCanvas.tsx`, `src/components/flow/FlowPane.tsx`, `src/components/flow/__tests__/FlowCanvas.test.tsx`, and `src/components/flow/__tests__/FlowPane.test.tsx`.

---

## Task 6: End-to-end test coverage for Flow save→reload and git visibility (#30)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/flow.rs` (test module only)
- Modify: `src/components/flow/__tests__/FlowPane.test.tsx`

No production code changes — this task only adds test coverage over the existing, already-correct save/load path.

**Interfaces:**
- Consumes: `rocket_app::FlowService::{save, get}` (existing), `rocket_infra::FsFlowRepo::new(base_dir: PathBuf)` (existing), `rocket_git::{Git2Service, GitService, GitStatus}` (existing), `tempfile::TempDir` (already a dev-dependency of `src-tauri`).

- [ ] **Step 1: Write the failing backend round-trip test**

In `src-tauri/src/commands/flow.rs`'s `#[cfg(test)] mod tests`, add (reusing the existing `sample_dto()` helper):

```rust
    #[test]
    fn save_flow_then_reopening_with_a_fresh_repo_instance_returns_the_same_flow() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        std::fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");
        let dto = sample_dto();

        {
            let svc = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
                dir.path().to_path_buf(),
            )));
            svc.save("acme", dto.clone().into()).expect("save flow");
        }

        // A fresh repo/service instance over the same directory simulates
        // reopening the app, rather than reusing the instance that saved.
        let svc2 = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
            dir.path().to_path_buf(),
        )));
        let loaded = svc2.get("acme", &dto.name).expect("get flow after reopen");
        let loaded_dto: FlowDto = loaded.into();

        assert_eq!(
            serde_json::to_value(&loaded_dto).expect("serialize loaded"),
            serde_json::to_value(&dto).expect("serialize original"),
            "the flow saved by one instance must round-trip identically through a fresh instance"
        );
    }
```

- [ ] **Step 2: Run test to verify it currently passes (it should — this proves the existing gap is test coverage, not behavior)**

Run: `cargo test -p rocket -j4 save_flow_then_reopening_with_a_fresh_repo_instance_returns_the_same_flow` (adjust package name to whatever `src-tauri`'s crate is named in `Cargo.toml` — check the `[package] name` field if `-p rocket` does not resolve)
Expected: PASS — this is new coverage over already-correct behavior, not a bug fix.

- [ ] **Step 3: Write the failing git-visibility test**

Add to the same test module:

```rust
    #[test]
    fn saving_a_flow_makes_the_file_visible_as_untracked_in_git_status() {
        use rocket_git::{GitService, GitStatus};

        let dir = tempfile::TempDir::new().expect("create temp dir");
        std::fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");

        let git = rocket_git::Git2Service::new();
        let repo_path = dir.path().to_str().expect("utf8 path");
        git.init(repo_path).expect("init git repo");

        let svc = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
            dir.path().to_path_buf(),
        )));
        svc.save("acme", sample_dto().into()).expect("save flow");

        let status = git.status(repo_path).expect("git status");
        let flow_file = status
            .files
            .iter()
            .find(|f| f.path.ends_with("login-then-fetch.yml"))
            .unwrap_or_else(|| {
                panic!(
                    "expected the saved flow file to appear in git status, got: {:?}",
                    status.files
                )
            });
        assert_eq!(flow_file.status, GitStatus::Untracked);
    }
```

- [ ] **Step 4: Run tests, fixing any import/API mismatch**

Run: `cargo check -p rocket -j4` (or the correct `src-tauri` package name), fix any incorrect import path (e.g. if `GitStatus`/`FileStatus` are not re-exported at `rocket_git`'s crate root, check `crates/rocket-git/src/lib.rs`'s `pub use` list and adjust the `use` statement accordingly), then:

Run: `cargo test -p rocket -j4 saving_a_flow_makes_the_file_visible_as_untracked_in_git_status`
Expected: PASS

- [ ] **Step 5: Run the full src-tauri test suite**

Run: `cargo check --workspace -j4 && cargo test -p rocket -j4` (adjust package name as needed)
Expected: PASS

- [ ] **Step 6: Commit backend test coverage**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src-tauri/src/commands/flow.rs`.

- [ ] **Step 7: Write the failing frontend round-trip test**

In `src/components/flow/__tests__/FlowPane.test.tsx`, add `getFlow: vi.fn()` to the existing `vi.mock('@/lib/tauri-api', ...)` return object:

```ts
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    getFlow: vi.fn(),
  };
});
```

Add the import and a new test inside the `'FlowPane save'` describe block:

```ts
import { getFlow, listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
```

```ts
  it('a saved flow reloads with the same nodes and edges when reopened', async () => {
    const store = new Map<string, { name: string; nodes: FlowNode[]; edges: FlowEdge[] }>();
    vi.mocked(saveFlow).mockImplementation(async (collection, flow) => {
      store.set(`${collection}/${flow.name}`, flow);
    });
    vi.mocked(getFlow).mockImplementation(async (collection, name) => {
      const saved = store.get(`${collection}/${name}`);
      if (!saved) throw new Error(`no such flow: ${collection}/${name}`);
      return saved;
    });

    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());

    await usePaneStore.getState().openFlowTab('demo', 'my-flow');

    const { root } = usePaneStore.getState();
    const reopened =
      root.type === 'leaf'
        ? root.tabs.find((t) => t.tabType === 'flow' && t.flowName === 'my-flow')
        : undefined;
    expect(reopened).toBeDefined();
    expect(reopened && 'nodes' in reopened ? reopened.nodes : undefined).toEqual(flowTab.nodes);
    expect(reopened && 'edges' in reopened ? reopened.edges : undefined).toEqual(flowTab.edges);
  });
```

(This requires `FlowNode`/`FlowEdge` types — import them from `@/lib/tauri-api` alongside the existing imports if not already imported in this file.)

- [ ] **Step 8: Run test to verify it fails, then check it passes**

Run: `yarn test FlowPane`
Expected: This exercises the already-correct `saveFlow`→`getFlow`→`openFlowTab` path with a real shared fake store (rather than independently-hardcoded mocks), so it should PASS immediately — this is new coverage, not a fix. If it fails, investigate whether `openFlowTab`'s tab-matching logic (multiple flow tabs with the same `flowName` across different `openTab` calls) needs a more specific lookup than `.find(...)` — adjust the test's lookup, not `pane-store.ts`, since this task makes no production changes.

- [ ] **Step 9: Run the full frontend flow test suite**

Run: `yarn test src/components/flow`
Expected: PASS

- [ ] **Step 10: Commit frontend test coverage**

Use the `dev-workflow-skills:1-git-commit` skill to commit `src/components/flow/__tests__/FlowPane.test.tsx`.

---

## Final verification (after all six tasks)

- [ ] Run `cargo check --workspace -j4` — must pass with no warnings-as-errors introduced.
- [ ] Run `cargo test --workspace -j4` — must pass.
- [ ] Run `yarn tsc --noEmit` — must pass.
- [ ] Run `yarn check` — must pass (Biome lint + format, read-only).
- [ ] Run `yarn test` (full suite) — must pass.
- [ ] Per this project's `CLAUDE.md`: confirm `yarn tauri build` or at minimum `cargo check --workspace -j4` + `yarn tsc --noEmit` both succeed before considering this plan mergeable — the project's own rule requires the code to actually build before a PR.
