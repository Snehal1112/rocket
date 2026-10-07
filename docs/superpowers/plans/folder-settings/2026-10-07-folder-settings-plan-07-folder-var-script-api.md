# Folder Settings, Plan 07: Folder variable access from scripts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** scripts read folder variables with `rok.getFolderVar(key)`, the Rocket counterpart of Bruno's `bru.getFolderVar(key)`. It works in request scripts and in folder scripts (plan 06), in every phase, and the Monaco Scripts editor offers it in IntelliSense and in the snippet sidebar.

**Architecture:** no new data loading is needed. `RequestExecutionService::build_variable_scopes` (`crates/rocket-app/src/execution_service.rs` lines 599-605) already fills `VariableContext.folder` from `get_folder_chain_variables`, which merges the chain with `rocket_collection::settings::merge_folder_chain_variables` (outermost first, inner folder wins, disabled entries skipped). That context reaches the engine unchanged in `ScriptContext.variables` for every phase, and plan 06 runs folder scripts with the same `PhaseState.var_ctx`. So this plan adds one read op in `rocket-infra` (`op_rok_get_folder_var`, mirroring `op_rok_get_collection_var`), wires it into `bootstrap.js` and the `rocket_scripting_ext` op table, proves it end to end in `rocket-app` with the real filesystem repo and the real Deno engine, and adds the typing and snippets in `src/components/editor/rok-types.ts`.

**Tech Stack:** Rust (deno_core ops, tokio tests, tempfile), React + TypeScript, Monaco extra libs, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** [2026-10-07-folder-settings-design.md](../../specs/2026-10-07-folder-settings-design.md), Runtime rules row "Variable access in scripts". Locked names: [00-plan-index.md](00-plan-index.md).

**Depends on:** plan 02 (`get_folder_settings` and `save_folder_settings` on `FsCollectionRepo`) and plan 06 (folder scripts run through `chain_scripts`). Task 1 test `folder_script_reads_folder_vars_with_get_folder_var` needs both.

## Decisions recorded by this plan

- **Where the code goes.** `rocket-scripting` needs no change. It defines only the contract (`ScriptContext`, `ScriptResult`), and `ScriptContext.variables: VariableContext` already carries `folder`. The `rok.*` surface is implemented in `rocket-infra`: ops in `crates/rocket-infra/src/scripting/ops/rok.rs`, the JS wrapper in `crates/rocket-infra/src/scripting/bootstrap.js`, registration and engine tests in `crates/rocket-infra/src/scripting/engine.rs`.
- **Read only, no `rok.setFolderVar`.** Evidence that no script write path exists today: `ScriptResult` (`crates/rocket-scripting/src/result.rs`) has write lists for runtime, environment, collection and global environment only; `rok.rs` has no folder setter op; `bootstrap.js` exposes no setter; `apply_script_side_effects` (`execution_service.rs` lines 844-926) applies no folder writes. The only folder write path is the declarative `ActionSetVariable` scope `"folder"` in `apply_actions` (lines 1248-1270), which writes the request's direct parent folder and is not a script API. Bruno's script API also has only a folder getter. Adding a setter would need a new `ScriptResult` field, a rule for which folder in the chain is written, and vault hold-back handling, with no requirement asking for it. YAGNI: out of scope. Task 1 and Task 2 each pin this with a test so a setter is not added by accident.
- **Folder scope is per request.** `rok.getFolderVar` reads the request's merged chain. An outer folder's script therefore sees an inner folder's value when the same key is set on both, which is what `{{key}}` resolves to for that request. Task 1 test `folder_script_reads_folder_vars_with_get_folder_var` pins this.
- **Missing key returns `""`.** This matches `rok.getCollectionVar`, `rok.getEnvVar` and `rok.getGlobalEnvVar`, which all use `unwrap_or_default`.
- **Two tasks, not three.** The read accessor needs no domain or context change (see Architecture), so the backend is one task and IntelliSense plus snippets is the second. The write accessor is a decision, not a task.

## Global Constraints

- Never call `unwrap` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo and target one crate. Never run `cargo test --workspace`.
- No `#[serde(rename_all = "camelCase")]` changes. This plan adds no persistence or IPC struct.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`), then commit with the same pathspec. Peer sessions share this repo's index.
- Frontend: shadcn/ui primitives and `lucide-react` only. This plan adds data to existing components and no new UI.
- Code comments are short full sentences ending with a punctuation mark.

## Review Focus

1. `rok.getFolderVar` returns the innermost folder's value, falls back to an outer folder for keys the inner folder does not set, and a disabled inner entry never shadows an enabled outer one (Task 1 test `request_script_reads_merged_folder_chain_with_get_folder_var`).
2. `rok.getFolderVar` reads only the folder scope. A key present in collection, environment, request or runtime scope but not in a folder returns `""` (Task 1 test `rok_get_folder_var_ignores_other_scopes`).
3. A folder's own script (plan 06) can call `rok.getFolderVar` and sees the request's merged chain (Task 1 test `folder_script_reads_folder_vars_with_get_folder_var`).
4. No folder setter exists in the sandbox or in the typings (Task 1 test `rok_has_no_folder_var_setter`, Task 2 test `%s typings declare getFolderVar and no setter`).
5. The snippet is offered in all three phase lists, so the request Scripts tab and the folder Script and Test sub-tabs (plan 11, which reuses these lists) both show it (Task 2 tests `appears in the tests-phase (default), pre-request and post-response snippet lists` and `inserts rok.getFolderVar from the pre-request snippet list`).

---

## Task 1: `rok.getFolderVar` in the script engine, proven end to end

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs` (new op after `op_rok_get_collection_var`, lines 67-77)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (op table at line 166, tests after `rok_get_secret_var_reads_from_context`, lines 409-420)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (`globalThis.rok` at line 38)
- Create: `crates/rocket-app/src/execution_service/folder_var_script_tests.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (module declaration after `pub mod websocket_resolution;`, line 27)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`mod tests`), `crates/rocket-app/src/execution_service/folder_var_script_tests.rs`

**Interfaces:**
- Consumes:
  - `rocket_environment::VariableContext { pub folder: HashMap<String, String>, .. }` (`crates/rocket-environment/src/context.rs`).
  - `crate::scripting::state::ScriptInputState { variables: VariableContext, .. }` and `deno_core::{op2, OpState}`.
  - `CollectionRepository::{create, create_folder, save_folder_variables, save_request}` (existing) and, from plan 02, `get_folder_settings(&self, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>` and `save_folder_settings(&self, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>`.
  - `rocket_collection::FolderSettings { pre_request_script: Option<String>, .. }` (plan 01).
  - `RequestExecutionService::new(..)`, `with_script_engine(Box<dyn ScriptEngine>)`, `execute(ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput>`.
  - `crate::test_doubles::{EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, RecordingExecutor, SharedHistoryRepo}`.
  - `rocket_infra::FsCollectionRepo::new_standalone(PathBuf)`, `rocket_infra::scripting::DenoScriptEngine::new()` (dev-dependency of `rocket-app`).
- Produces:
  - `pub fn op_rok_get_folder_var(state: &OpState, #[string] key: String) -> String` in `crates/rocket-infra/src/scripting/ops/rok.rs`, registered in `rocket_scripting_ext`.
  - JS: `rok.getFolderVar(key: string) => string` (`""` when the key is not in the folder scope).

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (folder `request.variables` and variable precedence).

- [ ] **Step 2: Write the failing engine tests**

In `crates/rocket-infra/src/scripting/engine.rs`, inside `mod tests`, insert directly after the test `rok_get_secret_var_reads_from_context` (ends at line 420):

```rust
    #[tokio::test]
    async fn rok_get_folder_var_reads_folder_scope() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.folder.insert("tenant".into(), "acme".into());
        let mut ctx = minimal_ctx("rok.setVar('t', rok.getFolderVar('tenant'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("t").expect("t present"), "acme");
    }

    #[tokio::test]
    async fn rok_get_folder_var_ignores_other_scopes() {
        // The key exists everywhere except the folder scope, so the getter must return "".
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.collection.insert("k".into(), "from-collection".into());
        vars.env.insert("k".into(), "from-env".into());
        vars.request.insert("k".into(), "from-request".into());
        vars.runtime.insert("k".into(), "from-runtime".into());
        let mut ctx = minimal_ctx("rok.setVar('k', rok.getFolderVar('k'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.runtime_vars.get("k").expect("k present"), "");
    }

    #[tokio::test]
    async fn rok_has_no_folder_var_setter() {
        // Folder variables are read-only from scripts. This guards against an accidental setter.
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('s', typeof rok.setFolderVar)");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(
            result.runtime_vars.get("s").expect("s present"),
            "undefined"
        );
    }
```

- [ ] **Step 3: Run the engine tests and confirm they fail**

Run: `cargo test -j4 -p rocket-infra rok_get_folder_var`

Expected: FAIL. Both `rok_get_folder_var_*` tests panic on `unexpected error: Some("... TypeError: rok.getFolderVar is not a function ...")`.

Run: `cargo test -j4 -p rocket-infra rok_has_no_folder_var_setter`

Expected: PASS already. It is a guard test and must keep passing after Step 7.

- [ ] **Step 4: Write the failing end-to-end tests in `rocket-app`**

In `crates/rocket-app/src/execution_service.rs`, replace line 27:

```rust
pub mod websocket_resolution;
```

with:

```rust
pub mod websocket_resolution;

#[cfg(test)]
mod folder_var_script_tests;
```

Create `crates/rocket-app/src/execution_service/folder_var_script_tests.rs`:

```rust
//! End-to-end checks that scripts read folder variables through `rok.getFolderVar`.
//! They use the real filesystem repo and the real script engine, so they cover the
//! whole path from `folder.yml` to the JS sandbox.

use std::sync::Arc;

use rocket_collection::{CollectionRepository, CollectionVariable, Request as CollectionRequest};
use rocket_http::RequestOptions;
use rocket_infra::scripting::DenoScriptEngine;
use rocket_infra::FsCollectionRepo;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{Auth, HttpMethod};

use super::{ExecuteRequestInput, RequestExecutionService};
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, RecordingExecutor,
    SharedHistoryRepo,
};

const COLLECTION: &str = "my-api";
const REQUEST_PATH: &str = "outer/inner/get-user.yml";
const URL: &str = "https://example.com/users/1";

fn var(key: &str, value: &str, enabled: bool) -> CollectionVariable {
    CollectionVariable {
        key: key.into(),
        value: value.into(),
        initial_value: value.into(),
        enabled,
        secret: false,
    }
}

/// Builds `my-api/outer/inner/get-user.yml` with variables on both folders.
fn seed_collection(base: &std::path::Path) {
    let repo = FsCollectionRepo::new_standalone(base.to_path_buf());
    repo.create(COLLECTION).expect("create collection");
    repo.create_folder(COLLECTION, "outer")
        .expect("create outer folder");
    repo.create_folder(COLLECTION, "outer/inner")
        .expect("create inner folder");
    repo.save_folder_variables(
        COLLECTION,
        "outer",
        vec![
            var("shared", "outer-value", true),
            var("outerOnly", "o1", true),
            var("toggled", "outer-on", true),
        ],
    )
    .expect("save outer vars");
    repo.save_folder_variables(
        COLLECTION,
        "outer/inner",
        vec![
            var("shared", "inner-value", true),
            var("toggled", "inner-off", false),
        ],
    )
    .expect("save inner vars");
    let request = CollectionRequest::new("Get User", HttpMethod::Get, URL);
    repo.save_request(COLLECTION, REQUEST_PATH, &request)
        .expect("save request");
}

fn service(base: &std::path::Path) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        RecordingExecutor::new(),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(FsCollectionRepo::new_standalone(base.to_path_buf())),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(DenoScriptEngine::new()))
}

fn input(pre_request_script: Option<&str>) -> ExecuteRequestInput {
    ExecuteRequestInput {
        skip_folder_scripts: false,
        skip_history: false,
        flow_vars: std::collections::HashMap::new(),
        method: HttpMethod::Get,
        url: URL.into(),
        headers: vec![],
        query_params: vec![],
        body: None,
        auth: Auth::None,
        options: RequestOptions::default(),
        environment_name: None,
        collection: Some(COLLECTION.into()),
        request_name: Some("Get User".into()),
        pre_request_script: pre_request_script.map(str::to_string),
        post_response_script: None,
        tests_script: None,
        request_path: Some(REQUEST_PATH.into()),
        global_env_name: None,
        assertions: vec![],
        tags: vec![],
        path_params: vec![],
        actions: vec![],
        request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
    }
}

#[tokio::test]
async fn request_script_reads_merged_folder_chain_with_get_folder_var() {
    let dir = tempfile::tempdir().expect("tempdir");
    seed_collection(dir.path());
    let script = "console.log([rok.getFolderVar('shared'), rok.getFolderVar('outerOnly'), \
                  rok.getFolderVar('toggled'), rok.getFolderVar('missing')].join('|'))";

    let out = service(dir.path())
        .execute(input(Some(script)))
        .await
        .expect("execute");

    assert!(
        out.script_error.is_none(),
        "script error: {:?}",
        out.script_error
    );
    let lines: Vec<&str> = out
        .console_entries
        .iter()
        .map(|e| e.message.as_str())
        .collect();
    // Inner wins, outer fills gaps, a disabled inner entry does not shadow, a missing key is "".
    assert!(
        lines.contains(&"inner-value|o1|outer-on|"),
        "console: {lines:?}"
    );
}

#[tokio::test]
async fn folder_script_reads_folder_vars_with_get_folder_var() {
    let dir = tempfile::tempdir().expect("tempdir");
    seed_collection(dir.path());
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    // Read first so the save keeps the folder's variables.
    let mut settings = repo
        .get_folder_settings(COLLECTION, "outer")
        .expect("read outer settings");
    settings.pre_request_script =
        Some("console.log('outer-script:' + rok.getFolderVar('shared'))".into());
    repo.save_folder_settings(COLLECTION, "outer", &settings)
        .expect("save outer settings");

    let out = service(dir.path())
        .execute(input(None))
        .await
        .expect("execute");

    assert!(
        out.script_error.is_none(),
        "script error: {:?}",
        out.script_error
    );
    let lines: Vec<&str> = out
        .console_entries
        .iter()
        .map(|e| e.message.as_str())
        .collect();
    // The folder scope is the request's merged chain, so the outer script sees the inner value.
    assert!(
        lines.contains(&"outer-script:inner-value"),
        "console: {lines:?}"
    );
}
```

If plans 05 or 06 added fields to `ExecuteRequestInput`, add them to the `input` literal with their neutral value (the same value `sample_input` in `execution_service.rs` uses).

- [ ] **Step 5: Run the end-to-end tests and confirm they fail**

Run: `cargo test -j4 -p rocket-app folder_var_script_tests`

Expected: FAIL. Both tests panic on `script error: Some("... rok.getFolderVar is not a function ...")`. If `folder_script_reads_folder_vars_with_get_folder_var` instead fails because no console line appears and `script_error` is `None`, plan 06 is not merged: stop and finish plan 06 first.

- [ ] **Step 6: Add the op**

In `crates/rocket-infra/src/scripting/ops/rok.rs`, insert directly after `op_rok_get_collection_var` (ends at line 77):

```rust

/// rok.getFolderVar(key) — reads the request's merged folder-chain scope.
/// The innermost folder wins and disabled entries are already left out.
#[op2]
#[string]
pub fn op_rok_get_folder_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .folder
        .get(&key)
        .cloned()
        .unwrap_or_default()
}
```

- [ ] **Step 7: Register the op and expose it in the sandbox**

In `crates/rocket-infra/src/scripting/engine.rs`, in the `rocket_scripting_ext` op list, replace:

```rust
        rok::op_rok_get_collection_var,
        rok::op_rok_set_collection_var,
```

with:

```rust
        rok::op_rok_get_collection_var,
        rok::op_rok_set_collection_var,
        rok::op_rok_get_folder_var,
```

In `crates/rocket-infra/src/scripting/bootstrap.js`, replace:

```js
    setCollectionVar:  (key, value) => __ops.op_rok_set_collection_var(key, JSON.stringify(value)),
```

with:

```js
    setCollectionVar:  (key, value) => __ops.op_rok_set_collection_var(key, JSON.stringify(value)),
    getFolderVar:      (key)        => __ops.op_rok_get_folder_var(key),
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run:
- `cargo test -j4 -p rocket-infra rok_get_folder_var`
- `cargo test -j4 -p rocket-infra rok_has_no_folder_var_setter`
- `cargo test -j4 -p rocket-infra scripting::engine`
- `cargo test -j4 -p rocket-app folder_var_script_tests`
- `cargo check -j4 -p rocket-infra`
- `cargo check -j4 -p rocket-app`

Expected: PASS everywhere, no new warnings.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path, then commit with the same pathspec:

```bash
git add crates/rocket-infra/src/scripting/ops/rok.rs \
  crates/rocket-infra/src/scripting/engine.rs \
  crates/rocket-infra/src/scripting/bootstrap.js \
  crates/rocket-app/src/execution_service.rs \
  crates/rocket-app/src/execution_service/folder_var_script_tests.rs
```

Suggested subject: `feat(scripting): add rok.getFolderVar for folder variables`.

---

## Task 2: IntelliSense typing and snippets for `rok.getFolderVar`

**Files:**
- Modify: `src/components/editor/rok-types.ts` (`ROK_DEFS` lines 447-448, the three `rok.getCollectionVar("key")` snippet items at lines 133-137, 299-303 and 401-405)
- Test: `src/components/editor/__tests__/rok-types.test.ts`
- Test: `src/components/request/__tests__/ScriptSnippetSidebar.test.tsx`

**Interfaces:**
- Consumes: `ROK_SNIPPETS`, `PRE_REQUEST_SNIPPETS`, `POST_RESPONSE_SNIPPETS`, `ROK_TYPE_DEFS_FOR_PHASE(phase: ScriptPhase): string` and `ScriptSnippetSidebar({ onInsert, snippets?, maxWidth? })`. Task 1's `rok.getFolderVar(key) => string`.
- Produces: `getFolderVar(key: string): unknown;` in the `rok` declaration for every phase (the per-phase `addExtraLib` in `MonacoWrapper.tsx` picks it up unchanged), and a `rok.getFolderVar("key")` expression item in the `rok` sub-group of all three snippet lists. `ScriptsTab.tsx` passes these lists to `ScriptSnippetSidebar` per phase, and plan 11 reuses `ScriptsTab`, so both the request and folder script editors get it with no further change.

- [ ] **Step 1: Write the failing tests**

In `src/components/editor/__tests__/rok-types.test.ts`, append at the end of the file:

```ts
describe('rok.getFolderVar coverage', () => {
  it('appears in the tests-phase (default), pre-request and post-response snippet lists', () => {
    for (const groups of [ROK_SNIPPETS, PRE_REQUEST_SNIPPETS, POST_RESPONSE_SNIPPETS]) {
      expect(rokItemLabels(groups)).toContain('rok.getFolderVar("key")');
    }
  });

  it.each([
    'pre-request',
    'post-response',
    'tests',
  ] as const)('%s typings declare getFolderVar and no setter', (phase) => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE(phase);
    expect(defs).toContain('getFolderVar(key: string): unknown;');
    expect(defs).not.toContain('setFolderVar');
  });
});
```

In `src/components/request/__tests__/ScriptSnippetSidebar.test.tsx`, add this import below the existing `import { ScriptSnippetSidebar } from '../ScriptSnippetSidebar';` line:

```tsx
import { PRE_REQUEST_SNIPPETS } from '@/components/editor/rok-types';
```

and add this test as the last `it` inside `describe('ScriptSnippetSidebar', ...)`:

```tsx
  it('inserts rok.getFolderVar from the pre-request snippet list', () => {
    const onInsert = vi.fn();
    render(<ScriptSnippetSidebar onInsert={onInsert} snippets={PRE_REQUEST_SNIPPETS} />);
    fireEvent.click(screen.getByText('rok.getFolderVar("key")'));
    expect(onInsert).toHaveBeenCalledWith('rok.getFolderVar("key")');
  });
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test --run rok-types ScriptSnippetSidebar`

Expected: FAIL. The snippet-list test reports `expected [...] to include 'rok.getFolderVar("key")'`, the three typing tests report the defs do not contain `getFolderVar(key: string): unknown;`, and the sidebar test fails with `Unable to find an element with the text: rok.getFolderVar("key")`.

- [ ] **Step 3: Add the typing**

In `src/components/editor/rok-types.ts`, inside `ROK_DEFS`, replace:

```ts
  /** Write a collection variable (persisted to opencollection.yml). */
  setCollectionVar(key: string, value: unknown): void;
```

with:

```ts
  /** Write a collection variable (persisted to opencollection.yml). */
  setCollectionVar(key: string, value: unknown): void;
  /** Read a folder variable. The innermost folder wins and disabled entries are skipped. Returns "" when unknown. Read-only. */
  getFolderVar(key: string): unknown;
```

- [ ] **Step 4: Add the snippet to all three lists**

The `rok.getCollectionVar("key")` item has the same text in `ROK_SNIPPETS`, `POST_RESPONSE_SNIPPETS` and `PRE_REQUEST_SNIPPETS`. Replace all three occurrences (Edit with `replace_all: true`) of:

```ts
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
```

with:

```ts
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          {
            label: 'rok.getFolderVar("key")',
            kind: 'expression',
            code: 'rok.getFolderVar("key")',
          },
```

Confirm afterwards that `rok.getFolderVar("key")` appears exactly three times as a `label` in the file.

- [ ] **Step 5: Run the tests and the frontend checks**

Run:
- `yarn test --run rok-types ScriptSnippetSidebar`
- `yarn tsc --noEmit`
- `yarn check`

Expected: PASS. If `yarn check` reports only formatting in the two test files, run `yarn format` and re-run `yarn check`.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path, then commit with the same pathspec:

```bash
git add src/components/editor/rok-types.ts \
  src/components/editor/__tests__/rok-types.test.ts \
  src/components/request/__tests__/ScriptSnippetSidebar.test.tsx
```

Suggested subject: `feat(scripts): offer rok.getFolderVar in IntelliSense and snippets`.

---

## Next Plan

**Execution order:** this is plan 07 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 08: Folder tab shell, store action and sidebar click](2026-10-07-folder-settings-plan-08-folder-tab-shell.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 08 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

[Plan 08: Folder tab shell, store action, sidebar click](2026-10-07-folder-settings-plan-08-folder-tab-shell.md). It depends on plan 04 only. Chain to it automatically when this one finishes.
