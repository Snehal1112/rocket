# ACP MCP Tool Server — Plan 06: Frontend Checkbox + Cross-Cutting Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the per-collection "agent autonomy" checkbox to `AgentChatPanel`, and close the five cross-cutting gaps the plan index's Review Focus calls out — all-6-tools autonomy refusal, secret/not-found error indistinguishability, mid-session toggle takes effect immediately, the Stdio bridge token never appearing in argv, and concurrent agent/manual writes against the same request file not corrupting either — before subproject D (the ACP MCP tool server) is considered done.

**Architecture:** This is the last of 6 plans and adds no new production Rust types. It wires a new optional `agentAutonomyEnabled` field through the existing `CollectionSettings` TS interface and `AgentChatPanel` component (reading/writing it via the already-existing `getCollectionSettings`/`saveCollectionSettings` Tauri commands), then adds new tests against the `McpToolService` and `AcpSessionService` APIs that Plans 01–05 already built, following this repo's established "inline `#[cfg(test)] mod tests` per source file, reuse `crate::test_doubles`" convention (confirmed by reading `crates/rocket-app/src/test_doubles.rs` and `crates/rocket-app/CLAUDE.md`) rather than introducing a new integration-test target.

**Tech Stack:** React 18 + TypeScript (shadcn/ui `Checkbox`), Rust (Tokio async tests, `wiremock` + `tempfile` for the concurrency test).

**Spec:** `docs/superpowers/specs/2026-09-28-acp-mcp-tool-server-design.md` ("Safety valve", "Testing" sections). Plan index (locked interfaces, Review Focus): `docs/superpowers/plans/acp-mcp-tool-server/00-plan-index.md`.

## Global Constraints

- Plans 01–05 of this series have already landed on `main` before this plan executes. Treat every interface in the plan index's "Locked interface contracts" section as stable fact: `McpToolService` (`crates/rocket-app/src/mcp_tool_service.rs`) with methods `list_collection_requests`, `run_request`, `edit_script`, `get_env_var`, `set_env_var`, `get_test_results`; `rocket_collection::RequestScriptPhase`; `rocket_shared::RunSource`; `rocket_acp::McpServerSpec`; `AcpSessionClient::start_session`'s 5-parameter signature (command, args, cwd, env, mcp_servers).
- `cargo test`/`cargo check` invocations in this repo always pass `-j4` (project convention).
- File format is `.yml` only, never `.json`, for any on-disk collection/environment persistence (OpenCollection spec compliance) — not directly touched by this plan's code, but every fixture built for the concurrency test round-trips through real `.yml` files via `FsCollectionRepo`, so this matters for that test's realism.
- `camelCase` rename applies only at the IPC/TS boundary (`CollectionSettings` in `src/lib/tauri-api.ts`), never on Rust persistence structs — no persistence-layer Rust code changes in this plan.
- UI: shadcn/ui primitives and `lucide-react` icons only — the new checkbox uses the existing `src/components/ui/checkbox.tsx` component, not a raw `<input type="checkbox">`.
- Zustand: `AgentChatPanel` already uses narrow per-field selectors (`usePaneStore((s) => s.beginAgentSession)`, etc.) — the new autonomy-checkbox state is local `useState`, not Zustand, and must not be added by destructuring the whole pane store.
- The bridge token (`ROCKET_MCP_TOKEN`) must never appear in a spawned/declared process's argument list, only in its environment — this is the exact property Task 3's test pins down.
- New Rust test code uses `.expect("message")` for fallible setup calls, not bare `.unwrap()`, matching this series' established test-writing convention (see `acp-transport` plan 06's Global Constraints).
- Component instance reuse: `RequestPanel`/`AgentChatPanel` is rendered with no `key` in `EditorGroup.tsx`, so the same component instance is reused across different request tabs (see `project_acp_ai_assist_feature.md` memory, subproject C's lesson). The new autonomy-checkbox state is collection-scoped, not tab/session-scoped, but it still must not silently apply a stale collection's fetched settings to whatever collection is showing *now* — Task 1's effect guards against this explicitly.

## Review Focus

- A tool call against a collection with `agentAutonomyEnabled` false must be refused by every one of the 6 `McpToolService` tools, not just the mutating ones — Task 2, `all_six_tools_refuse_when_agent_autonomy_disabled`.
- `get_env_var`/`set_env_var` must return the *same* error for "key not found" and "key is secret" — Task 2, `get_env_var_and_set_env_var_give_the_same_error_for_missing_and_secret_keys`.
- Toggling `agentAutonomyEnabled` off mid-session (not just checked once at session start) must refuse the very next tool call — Task 2, `toggling_autonomy_off_mid_session_refuses_the_very_next_call`.
- The Stdio bridge's declared command must never carry the bearer token in its argument list, only in its environment — Task 3, `stdio_mcp_server_spec_never_carries_the_token_in_argv`.
- Concurrent `run_request` (agent-driven, read-only against the request file) and a manual `save_request_script` write against the same request file must not corrupt the file or make `run_request` fail from a torn read — Task 4, `concurrent_run_request_and_manual_edit_script_do_not_corrupt_the_request_file`.

---

## Task 1: Frontend — `agentAutonomyEnabled` field + `AgentChatPanel` checkbox

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/tauri-api.ts:62-68` (`CollectionSettings` interface)
- Modify: `src/components/request/AgentChatPanel.tsx`

**Interfaces:**
- Consumes: `getCollectionSettings(name: string) => Promise<CollectionSettings>` and `saveCollectionSettings(collection: string, settings: Partial<CollectionSettings>) => Promise<void>` (`src/lib/tauri-api.ts:677-681`, unchanged by this task). `CollectionSettings` (see below).
- Produces: `CollectionSettings.agentAutonomyEnabled?: boolean` — no other file in the frontend constructs a full `CollectionSettings` object today (verified: `saveCollectionSettings` always takes a `Partial<CollectionSettings>`), so this optional field is purely additive and requires no other call-site changes.

- [ ] **Step 1: Add the field to the TS interface**

In `src/lib/tauri-api.ts`, update the `CollectionSettings` interface (currently lines 62-68):

```typescript
export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
  sandboxMode: SandboxMode;
  agentAutonomyEnabled?: boolean;
}
```

- [ ] **Step 2: Run the TypeScript compiler to confirm nothing else broke**

Run: `yarn tsc --noEmit`
Expected: PASS (this field is optional, so no existing `CollectionSettings` literal anywhere in the frontend needs updating).

- [ ] **Step 3: Add imports and local state to `AgentChatPanel`**

In `src/components/request/AgentChatPanel.tsx`, update the imports:

```typescript
import { Loader2, Send } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { useCollectionPath } from '@/lib/collection-path';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import {
  endAgentSession,
  getCollectionSettings,
  saveCollectionSettings,
  sendAgentPrompt,
  startAgentSession,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { AgentChatSession } from '@/types/pane-types';
```

Then, inside the `AgentChatPanel` function body, immediately after the existing `const [startError, setStartError] = useState<string | null>(null);` line, add:

```typescript
  const [agentAutonomyEnabled, setAgentAutonomyEnabled] = useState(false);
  // AgentChatPanel is reused across request tabs (EditorGroup renders
  // RequestPanel with no `key` — see project memory on subproject C's
  // per-tab lifecycle bug), so `collectionName` can change under an
  // in-flight fetch. This ref records which collection the latest fetch
  // was issued for, so a response for a since-abandoned collection can be
  // dropped instead of clobbering the checkbox for whatever collection is
  // showing now.
  const autonomyCollectionRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    autonomyCollectionRef.current = collectionName;
    if (!collectionName) {
      setAgentAutonomyEnabled(false);
      return;
    }
    const requestedFor = collectionName;
    getCollectionSettings(requestedFor)
      .then((settings) => {
        if (autonomyCollectionRef.current !== requestedFor) return;
        setAgentAutonomyEnabled(settings.agentAutonomyEnabled ?? false);
      })
      .catch((err) => {
        console.error('[AgentChatPanel] failed to load collection settings', err);
      });
  }, [collectionName]);

  const handleToggleAutonomy = async (checked: boolean) => {
    if (!collectionName) return;
    const previous = agentAutonomyEnabled;
    setAgentAutonomyEnabled(checked);
    try {
      const current = await getCollectionSettings(collectionName);
      await saveCollectionSettings(collectionName, {
        ...current,
        agentAutonomyEnabled: checked,
      });
    } catch (err) {
      console.error('[AgentChatPanel] failed to save collection settings', err);
      setAgentAutonomyEnabled(previous);
    }
  };

  const autonomyCheckbox = (
    <label className='flex items-center gap-2 text-xs text-muted-foreground'>
      <Checkbox
        checked={agentAutonomyEnabled}
        onCheckedChange={(checked) => void handleToggleAutonomy(checked === true)}
        disabled={!collectionName}
      />
      Allow this agent to run requests and edit files
    </label>
  );
```

- [ ] **Step 4: Render the checkbox in the pre-start panel**

In the same file, in the early-return block for `!agentSession || agentSession.status === 'ended' || agentSession.status === 'error'`, add `{autonomyCheckbox}` right after the `<Select>...</Select>` block and before the `{startError && ...}` line:

```tsx
        </Select>
        {autonomyCheckbox}
        {startError && <p className='text-xs text-destructive'>{startError}</p>}
```

- [ ] **Step 5: Render the checkbox in the active-session panel**

In the same file, in the final returned JSX (the active chat panel), add a new row right after the header `<div className='flex items-center justify-between border-b px-3 py-2'>...</div>` block and before `<ScrollArea className='flex-1'>`:

```tsx
      <div className='border-b px-3 py-2'>{autonomyCheckbox}</div>
      <ScrollArea className='flex-1'>
```

- [ ] **Step 6: Verify frontend checks pass**

Run: `yarn tsc --noEmit`
Expected: PASS

Run: `yarn check`
Expected: PASS (Biome lint/format clean; if it reports only formatting diffs, run `yarn format` and re-check)

- [ ] **Step 7: Commit**

```bash
git add src/lib/tauri-api.ts src/components/request/AgentChatPanel.tsx
```

Invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) to craft and create the commit for these staged changes. Do not write a freeform `git commit -m` message.

---

## Task 2: `McpToolService` safety-valve cross-cutting tests

**Files:**
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (its existing `#[cfg(test)] mod tests` block, added by Plan 03)

**Interfaces:**
- Consumes: `McpToolService::new(collection_repo: Arc<dyn CollectionRepository>, environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>, execution_svc: Arc<RequestExecutionService>, event_publisher: Arc<dyn EventPublisher>) -> Self` and its 6 tool methods, exactly as locked in the plan index. `crate::test_doubles::{InMemoryCollectionRepo, SharedCollectionRepo, NullEnvRepo, InMemoryHistoryRepo, SharedHistoryRepo, EmptySecretManagerRepo, NullCookieRepo, RecordingExecutor, StaticCollectionEnvRepoFactory}` (all `pub(crate)`, already in `crates/rocket-app/src/test_doubles.rs`). `rocket_environment::{NullSecretStore, NullVaultSecretFetcher}` (already public). `rocket_shared::events::NullEventPublisher` (already public).
- Produces: `build_gated_service(collection: Collection, environment: Environment) -> (Arc<McpToolService>, Arc<MutableAutonomyCollectionRepo>)`, a private test helper added by Step 1 below and reused by Steps 2 and 3 of this task — a `MutableAutonomyCollectionRepo` type is also added by Step 1 (Task 3 and Task 4 do not depend on it).

- [ ] **Step 1: Add the shared test fixtures and the "all 6 refuse" test**

At the bottom of `crates/rocket-app/src/mcp_tool_service.rs`'s `#[cfg(test)] mod tests` block (inside `mod tests { use super::*; ... }`, alongside whatever per-tool tests Plan 03 already added there), add:

```rust
    // ---- Cross-cutting safety-valve tests (Plan 06) ----
    //
    // `crate::test_doubles::InMemoryCollectionRepo` bakes its `Collection` in
    // at construction with no interior mutability, so it can't model the
    // checkbox being toggled mid-session. This lightweight repo wraps one in
    // a `Mutex` instead. Every method beyond `get_settings`/`get_summaries`
    // is `unreachable!()`: none of this task's tests exercise a tool call
    // that reaches them (the autonomy gate in `check_autonomy_enabled` runs
    // before anything else, and the toggle test only ever calls
    // `list_collection_requests`, which needs only `get_summaries`).
    struct MutableAutonomyCollectionRepo {
        collection: std::sync::Mutex<rocket_collection::Collection>,
    }

    impl MutableAutonomyCollectionRepo {
        fn new(collection: rocket_collection::Collection) -> Arc<Self> {
            Arc::new(Self {
                collection: std::sync::Mutex::new(collection),
            })
        }

        fn set_autonomy_enabled(&self, enabled: bool) {
            self.collection.lock().expect("lock").settings.agent_autonomy_enabled = enabled;
        }
    }

    impl rocket_collection::CollectionRepository for MutableAutonomyCollectionRepo {
        fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> {
            unreachable!("not exercised by these tests")
        }
        fn get(&self, _: &str) -> DomainResult<rocket_collection::Collection> {
            unreachable!("not exercised by these tests")
        }
        fn get_summaries(&self, _: &str) -> DomainResult<rocket_collection::Collection> {
            Ok(self.collection.lock().expect("lock").clone())
        }
        fn create(&self, _: &str) -> DomainResult<rocket_collection::Collection> {
            unreachable!("not exercised by these tests")
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<rocket_collection::Request> {
            unreachable!("not exercised by these tests")
        }
        fn save_request(
            &self,
            _: &str,
            _: &str,
            _: &rocket_collection::Request,
        ) -> DomainResult<String> {
            unreachable!("not exercised by these tests")
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn get_settings(&self, _: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            Ok(self.collection.lock().expect("lock").settings.clone())
        }
        fn save_settings(
            &self,
            _: &str,
            _: &rocket_collection::CollectionSettings,
        ) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            unreachable!("not exercised by these tests")
        }
        fn get_folder_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            unreachable!("not exercised by these tests")
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn get_request_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            unreachable!("not exercised by these tests")
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
        fn save_request_script(
            &self,
            _: &str,
            _: &str,
            _: rocket_collection::RequestScriptPhase,
            _: String,
        ) -> DomainResult<()> {
            unreachable!("not exercised by these tests")
        }
    }

    fn fixture_collection(autonomy_enabled: bool) -> rocket_collection::Collection {
        let mut collection = rocket_collection::Collection::new("mcp-fixture");
        collection.settings.agent_autonomy_enabled = autonomy_enabled;
        collection
    }

    fn fixture_environment_with_secret() -> rocket_environment::Environment {
        let mut env = rocket_environment::Environment::new("mcp-env");
        env.variables.push(rocket_environment::Variable {
            key: "API_TOKEN".to_string(),
            value: "sk-super-secret".to_string(),
            enabled: true,
            secret: true,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env.variables.push(rocket_environment::Variable {
            key: "BASE_URL".to_string(),
            value: "https://api.example.com".to_string(),
            enabled: true,
            secret: false,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env
    }

    /// Builds an `McpToolService` whose `CollectionRepository` is a
    /// `MutableAutonomyCollectionRepo` (returned alongside so a test can
    /// flip `agent_autonomy_enabled` between calls), backed by fully
    /// no-op/in-memory doubles for everything `run_request` would otherwise
    /// need — none of this task's tests actually dispatch HTTP.
    fn build_gated_service(
        collection: rocket_collection::Collection,
        environment: rocket_environment::Environment,
    ) -> (Arc<McpToolService>, Arc<MutableAutonomyCollectionRepo>) {
        let gate_repo = MutableAutonomyCollectionRepo::new(collection.clone());
        let exec_collection_repo = Box::new(crate::test_doubles::SharedCollectionRepo(
            crate::test_doubles::InMemoryCollectionRepo::new(collection),
        ));
        let execution_svc = Arc::new(RequestExecutionService::new(
            Box::new(crate::test_doubles::NullEnvRepo),
            crate::test_doubles::RecordingExecutor::new(),
            Box::new(crate::test_doubles::SharedHistoryRepo(
                crate::test_doubles::InMemoryHistoryRepo::new(),
            )),
            exec_collection_repo,
            Box::new(crate::test_doubles::NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(crate::test_doubles::EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        let env_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory> = Arc::new(
            crate::test_doubles::StaticCollectionEnvRepoFactory(environment),
        );

        let service = Arc::new(McpToolService::new(
            Arc::clone(&gate_repo) as Arc<dyn rocket_collection::CollectionRepository>,
            env_factory,
            execution_svc,
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        (service, gate_repo)
    }

    #[tokio::test]
    async fn all_six_tools_refuse_when_agent_autonomy_disabled() {
        let (service, _gate_repo) =
            build_gated_service(fixture_collection(false), fixture_environment_with_secret());

        assert!(
            service.list_collection_requests("session-1", "mcp-fixture").is_err(),
            "list_collection_requests must refuse when autonomy is disabled"
        );
        assert!(
            service
                .run_request("session-1", "mcp-fixture", "req.yml", None)
                .await
                .is_err(),
            "run_request must refuse when autonomy is disabled"
        );
        assert!(
            service
                .edit_script(
                    "session-1",
                    "mcp-fixture",
                    "req.yml",
                    rocket_collection::RequestScriptPhase::Tests,
                    "rok.test('x', () => {});".to_string(),
                )
                .is_err(),
            "edit_script must refuse when autonomy is disabled"
        );
        assert!(
            service
                .get_env_var("session-1", "mcp-fixture", "mcp-env", "BASE_URL")
                .is_err(),
            "get_env_var must refuse when autonomy is disabled"
        );
        assert!(
            service
                .set_env_var(
                    "session-1",
                    "mcp-fixture",
                    "mcp-env",
                    "BASE_URL",
                    "https://other.example.com".to_string(),
                )
                .is_err(),
            "set_env_var must refuse when autonomy is disabled"
        );
        assert!(
            service
                .get_test_results("session-1", "mcp-fixture", "req.yml")
                .is_err(),
            "get_test_results must refuse when autonomy is disabled"
        );
    }
```

- [ ] **Step 2: Run the new test to verify it fails against unimplemented/incorrect gating (sanity check), then confirm it passes**

Run: `cargo test -p rocket-app all_six_tools_refuse_when_agent_autonomy_disabled -j4`
Expected: PASS (Plan 03 already implements the autonomy gate on every tool per the locked interface contract; this test proves it covers all 6, not just the mutating ones). If it fails, the bug is in Plan 03's `check_autonomy_enabled` wiring on whichever tool failed — fix `crates/rocket-app/src/mcp_tool_service.rs`, not this test.

- [ ] **Step 3: Add the indistinguishable-error test**

In the same `mod tests` block, add:

```rust
    #[test]
    fn get_env_var_and_set_env_var_give_the_same_error_for_missing_and_secret_keys() {
        let (service, _gate_repo) =
            build_gated_service(fixture_collection(true), fixture_environment_with_secret());

        let missing_get = service
            .get_env_var("session-1", "mcp-fixture", "mcp-env", "DOES_NOT_EXIST")
            .expect_err("a nonexistent key must be refused");
        let secret_get = service
            .get_env_var("session-1", "mcp-fixture", "mcp-env", "API_TOKEN")
            .expect_err("a secret key must be refused");
        assert_eq!(
            missing_get.to_string(),
            secret_get.to_string(),
            "a missing key and a secret key must be indistinguishable, or get_env_var \
             becomes an oracle for which keys are secret-flagged"
        );

        let missing_set = service
            .set_env_var("session-1", "mcp-fixture", "mcp-env", "DOES_NOT_EXIST", "x".to_string())
            .expect_err("a nonexistent key must be refused");
        let secret_set = service
            .set_env_var("session-1", "mcp-fixture", "mcp-env", "API_TOKEN", "x".to_string())
            .expect_err("a secret key must be refused");
        assert_eq!(
            missing_set.to_string(),
            secret_set.to_string(),
            "a missing key and a secret key must be indistinguishable, or set_env_var \
             becomes an oracle for which keys are secret-flagged"
        );
    }
```

Note: this assumes `set_env_var` (like `get_env_var`) operates only on a key that already exists in the environment — it does not silently create a brand-new variable — since the Review Focus explicitly requires both tools to have a comparable "not found" failure mode. Confirm this against Plan 03's actual `set_env_var` implementation before running this test; if Plan 03 instead auto-creates missing keys, this is a real gap in Plan 03's design (not something to work around in the test) and should be raised in this plan's Post-Implementation Review below.

- [ ] **Step 4: Run and confirm it passes**

Run: `cargo test -p rocket-app get_env_var_and_set_env_var_give_the_same_error_for_missing_and_secret_keys -j4`
Expected: PASS

- [ ] **Step 5: Add the mid-session toggle test**

In the same `mod tests` block, add:

```rust
    #[tokio::test]
    async fn toggling_autonomy_off_mid_session_refuses_the_very_next_call() {
        let (service, gate_repo) =
            build_gated_service(fixture_collection(true), fixture_environment_with_secret());

        service
            .list_collection_requests("session-1", "mcp-fixture")
            .expect("first call must succeed while autonomy is enabled");

        // Simulates the user unchecking the AgentChatPanel checkbox and
        // saving mid-session: McpToolService re-reads `agent_autonomy_enabled`
        // from the repository on every call rather than caching it at
        // session start, so mutating the fake's backing store alone must be
        // enough to flip the very next call's outcome.
        gate_repo.set_autonomy_enabled(false);

        service
            .list_collection_requests("session-1", "mcp-fixture")
            .expect_err("the very next call after disabling must be refused");
    }
```

- [ ] **Step 6: Run the full test file and confirm everything passes**

Run: `cargo test -p rocket-app mcp_tool_service -j4`
Expected: PASS (all of Plan 03's existing tests plus the 3 new ones above).

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/mcp_tool_service.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) to craft and create the commit for these staged changes. Do not write a freeform `git commit -m` message.

---

## Task 3: `AcpSessionService` — Stdio bridge token-never-in-argv test

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs` (its existing `#[cfg(test)] mod tests` block)

**Interfaces:**
- Consumes: `rocket_acp::McpServerSpec` (locked shape: `Stdio { name: String, command: String, args: Vec<String>, env: Vec<(String, String)> }`), the existing `FakeSessionClient`/`FakeEventPublisher`/`SharedEventPublisher`/`agent_config_service()` test helpers already in this file (shown in full above), and `AcpSessionService`'s real, final shape after Plans 03/05 have both landed:
  ```rust
  pub fn new(
      session_client: Box<dyn AcpSessionClient>,
      event_publisher: Box<dyn EventPublisher>,
      agent_config_service: Arc<AgentConfigService>,
      collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
      mcp_sweeper: Box<dyn McpServerSweeper>,
  ) -> Self;

  pub async fn start_session(
      &self,
      agent_config_id: &str,
      cwd: &str,
      collection: &str,
      mcp_http: Option<rocket_app::McpHttpServerCredentials>,
  ) -> DomainResult<String>;
  ```
  (Plan 03 added `collection_repo`; Plan 05 added `mcp_sweeper` and changed `start_session`'s `collection` from `Option<&str>` to a required `&str`, plus the trailing `mcp_http` parameter.)
- Produces: nothing consumed by later tasks — this is a leaf test.

- [ ] **Step 1: Confirm the real `AcpSessionService::start_session`/`new` signatures match the shape above**

Before writing Step 2's test, open `crates/rocket-app/src/acp_session_service.rs` and read the current `start_session`/`new` signatures to confirm they match the shape shown above (this is what Plans 03 and 05, taken together, should have produced). If a name or parameter differs, adjust only the `AcpSessionService::new(...)`/`service.start_session(...)` calls in Step 2 to match reality — do not change the assertions after that call, which depend only on the locked `McpServerSpec::Stdio` shape.

- [ ] **Step 2: Add a `captured_mcp_servers` field to `FakeSessionClient` and the new test**

In the same file's `mod tests` block, update the `FakeSessionClient` struct (already updated by Plan 01 to accept `mcp_servers: &[McpServerSpec]` as `start_session`'s 5th parameter, per the locked trait) to add one new field, and capture that parameter in its `start_session` body:

```rust
    struct FakeSessionClient {
        start_should_fail: bool,
        prompt_chunks: Vec<String>,
        prompt_stop_reason: String,
        prompt_should_fail: bool,
        prompt_delay: Duration,
        end_session_called: Arc<AtomicBool>,
        end_all_sessions_called: Arc<AtomicBool>,
        captured_mcp_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>>,
    }
```

(Add `captured_mcp_servers: Arc::new(Mutex::new(Vec::new()))` to its `Default` impl, alongside the other fields already defaulted there.)

In its `AcpSessionClient` impl, update `start_session` to record the slice it receives:

```rust
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            mcp_servers: &[rocket_acp::McpServerSpec],
        ) -> DomainResult<String> {
            self.captured_mcp_servers
                .lock()
                .expect("lock")
                .extend_from_slice(mcp_servers);
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok("session-1".to_string())
            }
        }
```

Then add the test:

```rust
    #[tokio::test]
    async fn stdio_mcp_server_spec_never_carries_the_token_in_argv() {
        let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> =
            Arc::new(Mutex::new(Vec::new()));
        let client = FakeSessionClient {
            captured_mcp_servers: Arc::clone(&captured_servers),
            ..Default::default()
        };
        let publisher = Arc::new(FakeEventPublisher::new());
        // Autonomy must be enabled for "my-collection", and mcp_http must be
        // Some(...), for start_session to build any McpServerSpec at all —
        // per Plan 05, it always builds *both* an Http and a Stdio spec
        // together in that case (capability-based selection of which one the
        // agent actually uses happens later, inside AcpAgentClient).
        let collection_repo = Arc::new(FakeCollectionRepo::with_autonomy_enabled(
            "my-collection",
            true,
        ));
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
            Box::new(NullMcpServerSweeper),
        );

        service
            .start_session(
                "agent-1",
                "/tmp/my-collection",
                "my-collection",
                Some(rocket_app::McpHttpServerCredentials {
                    port: 54321,
                    token: "s3cr3t-token".to_string(),
                }),
            )
            .await
            .expect("start_session should succeed");

        let servers = captured_servers.lock().expect("lock");
        let stdio_args_and_env = servers.iter().find_map(|s| match s {
            rocket_acp::McpServerSpec::Stdio { args, env, .. } => {
                Some((args.clone(), env.clone()))
            }
            _ => None,
        });
        let (args, env) = stdio_args_and_env
            .expect("expected a McpServerSpec::Stdio to be attached when autonomy is enabled");

        let token = env
            .iter()
            .find(|(k, _)| k == "ROCKET_MCP_TOKEN")
            .map(|(_, v)| v.clone())
            .expect("the bridge token must be passed via env");
        assert!(!token.is_empty(), "token env value must not be empty");
        assert!(
            args.iter().all(|a| !a.contains(&token)),
            "the MCP bridge token must never appear in argv, got args: {args:?}"
        );
    }
```

This test reuses two test doubles Plan 05 already added to this file's `mod tests` block: `FakeCollectionRepo::with_autonomy_enabled(name, enabled)` (a `CollectionRepository` double whose `get_settings` returns a fixed `agent_autonomy_enabled` value) and `NullMcpServerSweeper` (a no-op `McpServerSweeper`). If either was named differently by the time this task runs, adjust only the names, not the test's structure.

- [ ] **Step 3: Run and confirm it passes**

Run: `cargo test -p rocket-app stdio_mcp_server_spec_never_carries_the_token_in_argv -j4`
Expected: PASS. Per Plan 05's real `start_session` implementation, `AcpSessionService` itself always builds *both* an `Http` and a `Stdio` `McpServerSpec` together whenever autonomy is enabled and `mcp_http` credentials were supplied — it does not filter by the agent's advertised capability (that capability-based selection of which one the agent actually uses happens one layer down, inside `AcpAgentClient` in `rocket-infra`, which `AcpSessionService`'s own unit tests never exercise). So this test should find a `McpServerSpec::Stdio` unconditionally, with no dependency on `agent_config_service()`'s fixture being wired any particular way; if it doesn't, that's a real regression in `start_session`, not a fixture-selection problem to work around.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/acp_session_service.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) to craft and create the commit for these staged changes. Do not write a freeform `git commit -m` message.

---

## Task 4: Concurrent `run_request` + manual `save_request_script` do not corrupt the file

**Files:**
- Modify: `crates/rocket-app/Cargo.toml` (dev-dependencies)
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (its `#[cfg(test)] mod tests` block)

**Interfaces:**
- Consumes: `rocket_infra::{FsCollectionRepo, ReqwestExecutor}` (real infra implementations — `rocket-infra` has no dependency on `rocket-app`, confirmed via `crates/rocket-infra/Cargo.toml`, so adding it as a `rocket-app` *dev*-dependency introduces no cycle), `wiremock` (already a `rocket-infra` dev-dependency; same version here), `dashmap::DashMap` (already a workspace dependency), `rocket_collection::RequestScriptPhase`, `McpToolService::run_request`, `CollectionRepository::save_request_script`.
- Produces: nothing consumed by later tasks — this is a leaf test.

- [ ] **Step 1: Add dev-dependencies**

In `crates/rocket-app/Cargo.toml`, add to the existing `[dev-dependencies]` block (currently just `tempfile = "3"`):

```toml
[dev-dependencies]
tempfile = "3"
rocket-infra = { path = "../rocket-infra" }
wiremock = "0.6"
dashmap = { workspace = true }
```

- [ ] **Step 2: Run a workspace check to confirm the new dev-dependency graph builds**

Run: `cargo check -p rocket-app -j4`
Expected: PASS

- [ ] **Step 3: Write the concurrency test**

In `crates/rocket-app/src/mcp_tool_service.rs`'s `mod tests` block, add:

```rust
    #[tokio::test]
    async fn concurrent_run_request_and_manual_edit_script_do_not_corrupt_the_request_file() {
        use dashmap::DashMap;
        use rocket_collection::RequestScriptPhase;
        use rocket_infra::FsCollectionRepo;
        use rocket_shared::types::HttpMethod;
        use tempfile::TempDir;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/echo"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let dir = TempDir::new().expect("tempdir");
        let locks: Arc<DashMap<String, Arc<std::sync::Mutex<()>>>> = Arc::new(DashMap::new());
        // Two FsCollectionRepo instances over the same directory and lock
        // map, mirroring how McpToolService and RequestExecutionService each
        // hold their own reference to a collection repo in production
        // wiring (src-tauri/src/lib.rs), rather than sharing one Arc end to
        // end. They must share the same `locks` map to exercise the real
        // per-collection mutex `save_request_script` takes.
        let mcp_repo = Arc::new(FsCollectionRepo::new(
            dir.path().to_path_buf(),
            Arc::clone(&locks),
        ));
        let exec_repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::clone(&locks));

        mcp_repo.create("mcp-fixture").expect("create collection");
        let req = rocket_collection::Request::new(
            "Echo",
            HttpMethod::Get,
            format!("{}/echo", mock_server.uri()),
        );
        mcp_repo
            .save_request("mcp-fixture", "req.yml", &req)
            .expect("save fixture request");

        let execution_svc = Arc::new(RequestExecutionService::new(
            Box::new(crate::test_doubles::NullEnvRepo),
            Arc::new(rocket_infra::ReqwestExecutor::new()),
            Box::new(crate::test_doubles::SharedHistoryRepo(
                crate::test_doubles::InMemoryHistoryRepo::new(),
            )),
            Box::new(exec_repo),
            Box::new(crate::test_doubles::NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(crate::test_doubles::EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        let env_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory> = Arc::new(
            crate::test_doubles::StaticCollectionEnvRepoFactory(rocket_environment::Environment::new(
                "unused",
            )),
        );
        let service = McpToolService::new(
            Arc::clone(&mcp_repo) as Arc<dyn rocket_collection::CollectionRepository>,
            env_factory,
            execution_svc,
            Arc::new(rocket_shared::events::NullEventPublisher),
        );

        let run_fut = service.run_request("session-1", "mcp-fixture", "req.yml", None);
        let write_repo = Arc::clone(&mcp_repo);
        let write_fut = tokio::task::spawn_blocking(move || {
            write_repo.save_request_script(
                "mcp-fixture",
                "req.yml",
                RequestScriptPhase::Tests,
                "rok.test('t', () => {});".to_string(),
            )
        });

        let (run_result, write_result) = tokio::join!(run_fut, write_fut);

        run_result.expect(
            "run_request must not fail from a concurrent write — a torn read would surface \
             as a YAML parse error inside get_request",
        );
        write_result
            .expect("save_request_script task must not panic")
            .expect("save_request_script must succeed");

        // The only writer here is save_request_script, so the file's final
        // on-disk content must be exactly its post-write state and must
        // still be syntactically valid YAML — a torn write from an
        // unguarded save would leave truncated bytes or a mix of old/new
        // content that fails to parse.
        let final_path = dir.path().join("mcp-fixture").join("req.yml");
        let final_content = std::fs::read_to_string(&final_path).expect("read final request file");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&final_content)
            .expect("request file must remain valid YAML after concurrent access");
        assert!(
            !parsed.is_null(),
            "request file must not be empty/corrupted after concurrent access"
        );
    }
```

- [ ] **Step 4: Run and confirm it passes**

Run: `cargo test -p rocket-app concurrent_run_request_and_manual_edit_script_do_not_corrupt_the_request_file -j4`
Expected: PASS. If `run_request` fails because it can't find a matching wiremock route, double check `req.yml`'s saved URL matches `mock_server.uri()` exactly (wiremock binds a random port per test run).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/Cargo.toml crates/rocket-app/src/mcp_tool_service.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) to craft and create the commit for these staged changes. Do not write a freeform `git commit -m` message.

---

## Task 5: Full-plan verification

**Files:** none (verification only).

**Interfaces:** none.

- [ ] **Step 1: Full workspace compile check**

Run: `cargo check --workspace -j4`
Expected: PASS

- [ ] **Step 2: Scoped test run across every crate this series touched**

Run: `cargo test -p rocket-acp -p rocket-app -p rocket-infra -p rocket -j4`
Expected: PASS

- [ ] **Step 3: Frontend type-check and lint**

Run: `yarn tsc --noEmit`
Expected: PASS

Run: `yarn check`
Expected: PASS

- [ ] **Step 4: Confirm no stray changes**

Run: `git status`
Expected: only the files this plan's tasks modified/created are dirty or already committed; nothing unexpected.

- [ ] **Step 5: Commit (only if Step 4 found anything not yet committed)**

If `git status` shows uncommitted changes at this point, stage them and invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) rather than writing a freeform `git commit -m` message. If everything was already committed per-task, there is nothing to do here.

---

## Subproject D completion

Once every task above is green, subproject D (the ACP MCP tool server) is implementation-complete, but per this project's established practice (see `project_acp_ai_assist_feature.md` memory — subprojects A and B shipped with type-check/test-suite green but were explicitly flagged as "not yet manually GUI-verified"), do not treat automated tests alone as proof the feature works end-to-end. Before marking subproject D done, manually verify, running the real desktop app (`yarn tauri dev`):

1. Open a collection, open the Scripts tab's AI Assist panel, and confirm the new "Allow this agent to run requests and edit files" checkbox appears, off by default. Toggle it, close and reopen the panel (switch tabs and back), and confirm the state persists. Confirm it also persists across a full app restart (i.e. it was actually written to that collection's `opencollection.yml` under `extensions.rocketapi.agentAutonomyEnabled`, not just in-memory frontend state).
2. With autonomy enabled, start a real agent session (a real ACP-compliant agent binary, e.g. Claude Code or Gemini CLI configured per subproject A) and prompt it to list the collection's requests, run one, edit a script, and read/write a non-secret environment variable. Confirm the agent actually performs these actions (not just claims to in chat text) — this is the entire point of subproject D.
3. With autonomy disabled, prompt the same agent to run a request. Confirm it receives a clear, agent-visible refusal it can relay to the user, not a silent failure, a crash, or a generic 403/500.
4. Confirm the in-process HTTP MCP server actually binds a random `127.0.0.1` port per session (observable via `lsof`/`netstat` while a session is active) and that the port is released when the session ends normally, when the app is quit mid-session, and on SIGTERM — reusing subproject B's exit-sweep machinery. No orphaned listener or token should survive any of these three exits.
5. For an agent that only advertises the Stdio MCP transport (or is forced into that path for testing), confirm the bridge subprocess actually launches with `--acp-mcp-stdio-bridge` and successfully proxies at least one real tool call end-to-end — not just unit-tested against a fake client.
6. After an agent-driven `run_request`, confirm the resulting entry in Rocket's History view is visibly distinguishable from a manually-run request (via `RunSource::Agent`), and confirm an `AcpToolInvoked` audit event fires for every tool call (visible at minimum via the `agent-tool-invoked` event in devtools, even though subproject E's dedicated audit-log UI doesn't exist yet).
7. Confirm quitting the app entirely while an agent session with an active MCP HTTP listener is running does not leave a zombie Rocket/bridge process or a bound port behind (`ps`/`lsof` after quit).

Update `project_acp_ai_assist_feature.md` memory to mark subproject D's status as done (with the same "not yet manually GUI-verified" caveat if the above steps haven't actually been run in a real desktop session yet) before starting subproject E's brainstorming.

## Post-Implementation Review

Before considering subproject D complete, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief. Unlike every earlier plan in this series, this review must cover **the whole subproject D series (Plans 01–06), not just this plan's diff** — mirroring how subproject B's final plan review covered its whole branch, not just its last plan.

> Review every file created or modified across all 6 plans of the `acp-mcp-tool-server` series (the full diff from wherever subproject D's work started on `main` through the tip of this branch — use `git log`/`git diff` against the pre-subproject-D commit, not just this plan's own commits). At minimum this touches: `crates/rocket-shared/src/events.rs`, `crates/rocket-shared`'s `RunSource` module, `crates/rocket-acp/src/mcp_server_spec.rs`, `crates/rocket-acp/src/session.rs`, `crates/rocket-collection/src/settings.rs`, `crates/rocket-collection/src/repository.rs`, `crates/rocket-infra/src/fs_collection/*` (settings + new script-saving code), `crates/rocket-infra/src/shared_path_collection_repo.rs`, `crates/rocket-infra/src/acp_agent_client.rs`, `crates/rocket-history/src/entry.rs`, `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/mcp_tool_service.rs`, `crates/rocket-app/src/acp_session_service.rs`, `crates/rocket-app/src/runner_sequence.rs`, `crates/rocket-app/src/flow_execution_service.rs`, `src-tauri/src/mcp/tool_server.rs`, `src-tauri/src/mcp/stdio_bridge.rs`, `src-tauri/src/tauri_event_bus.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`, `src/components/request/AgentChatPanel.tsx`.
>
> Check for:
> 1. **Interface-gap conformance against the plan index.** Re-read `docs/superpowers/plans/acp-mcp-tool-server/00-plan-index.md`'s "Locked interface contracts" section end to end and confirm every locked signature/type actually landed as specified (names, field order, defaults) — flag any drift.
> 2. **Two specific gaps this plan (06) could not verify on its own, because they are owned by earlier plans in the series:**
>    - The design spec's "Testing" section requires "a focused test for the Stdio shim, asserting it correctly proxies a request/response pair against a real (test-bound) instance of the HTTP backend and does nothing else." Confirm Plan 05 actually added this test (it is not listed as a named test anywhere in the plan index) — if missing, add it.
>    - The design spec's "Testing" section requires the secret-boundary test to cover "edge cases like case-sensitivity of the key and an environment containing duplicate-looking keys," beyond this plan's narrower "same error for not-found vs secret" test. Confirm Plan 03's own tests cover this depth — if missing, add it.
>    - Confirm `set_env_var`'s actual behavior on a key that does not yet exist in the environment (this plan's Task 2 test assumed it errors, matching `get_env_var`, rather than silently creating a new variable) — if Plan 03 built it the other way, decide whether that's an intentional, documented design choice or a gap, and fix or document accordingly.
> 3. **Security properties, verified by tracing the code (not just re-running tests):** the bearer token is never logged, traced, or placed in argv anywhere in the HTTP server, Stdio bridge, or MCP server spawn/config path; the `secret` boundary in `get_env_var`/`set_env_var` cannot be bypassed via case differences or whitespace in the key; every tool call re-checks `agent_autonomy_enabled` fresh (no caching at session start anywhere in the call chain).
> 4. **DDD boundary conformance** per `.claude/rules/rust-ddd-boundaries.md` and `.claude/rules/tauri-ipc-boundaries.md` — no new dependency added to `rocket-acp`/`rocket-collection`/`rocket-environment` beyond what the index locks, no MCP-protocol or Tauri-specific code leaking into domain crates, no bare `unwrap()` in production paths, `camelCase` rename only on IPC DTOs.
> 5. **Code quality and duplication** — the tool dispatcher's autonomy-check-then-publish-audit-event pattern should be one shared private helper on `McpToolService`, not repeated ad hoc per tool; the HTTP and Stdio transports must share one tool-implementation code path (per the spec's explicit "never two independent tool-handling code paths" requirement) — confirm this by reading both `src-tauri/src/mcp/tool_server.rs` and `src-tauri/src/mcp/stdio_bridge.rs` side by side.
>
> You have explicit authority to apply fixes directly for anything you find. After fixing, re-run `cargo check --workspace -j4`, `cargo test -p rocket-acp -p rocket-app -p rocket-infra -p rocket -j4`, and `yarn tsc --noEmit` + `yarn check`, and confirm they still pass. Report what you found and fixed, and explicitly call out anything from item 2 above that you could not resolve without redesigning an earlier plan's already-merged interface (flag it for human follow-up instead of forcing a fix).

Once this review comes back clean (or its fixes are applied and re-verified), and the manual verification checklist above has actually been run at least once in a real desktop session, subproject D is complete. Update `project_acp_ai_assist_feature.md` memory accordingly before starting subproject E's brainstorming.
