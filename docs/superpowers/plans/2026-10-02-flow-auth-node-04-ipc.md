# Flow Auth Node — Plan 4: IPC and startup wiring

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4 of 7.** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-03-executor.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-05-frontend-node.md`**
**Recommended model: Sonnet** (thin glue; the security-sensitive parts are small and spelled out).

**Goal:** Let the UI hand tokens to a run (`run_flow` accepts `authTokens`) and make the backend able to fetch non-interactive tokens at startup wiring, so the feature works end to end from the frontend API.

**Architecture:** `RunFlowInputDto` gains an `auth_tokens` map whose values never serialize or print. `run_flow` calls `run_with_auth`. `src-tauri/src/lib.rs` builds a second `OAuth2Service` for an `OAuth2ServiceFetcher` and hands it to `FlowExecutionService`. The TypeScript `runFlow` gets an optional `authTokens` argument that is sent only when non-empty.

**Tech Stack:** Rust (Tauri v2 commands, serde), TypeScript (Vitest).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- No new IPC command may read or return a token. `run_flow` only receives them.
- Token values never serialize and never print: the DTO field is `skip_serializing` and its value type has a redacting `Debug`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs.
- No `unwrap()` in production Rust paths; never shell out to `git`.
- Existing `runFlow` callers and tests that call it with 2–4 arguments keep working unchanged.
- Conventional commits.
- Before each commit run `cargo fmt` (the plan's code is not rustfmt-checked) and re-run the task's tests.
- If `yarn check` reports formatting or import-order findings, run `yarn lint` (auto-fix), then re-run `yarn check`.
- Verification: `cargo check -j4`, `cargo test -p rocket <name>`, `yarn tsc --noEmit`, `yarn test src/lib/queries/__tests__/flow-api.test.ts`.

## File Structure

| File | Change |
|---|---|
| `src-tauri/src/commands/flow.rs` | `FlowAuthTokenDto`, `auth_tokens`, `into_parts`, `run_flow` uses `run_with_auth`, tests |
| `src-tauri/src/lib.rs` | Second `OAuth2Service` -> `OAuth2ServiceFetcher` -> `with_token_fetcher` |
| `src/lib/tauri-api.ts` | `runFlow(..., authTokens?)` |
| `src/lib/queries/__tests__/flow-api.test.ts` | New `runFlow` tests |

---

### Task 1: `run_flow` accepts tokens

**Files:**
- Modify: `src-tauri/src/commands/flow.rs`

**Interfaces:**
- Consumes: `rocket_app::{FlowAuthTokens, SuppliedToken}`, `FlowExecutionService::run_with_auth` (Plans 2–3).
- Produces: IPC input `{ input: { collection, flowName, environmentName, globalEnvName, authTokens?: { [nodeId]: { accessToken } } } }`.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

In `src-tauri/src/commands/flow.rs` `mod tests`, add:

```rust
    #[test]
    fn run_flow_input_carries_auth_tokens_into_the_run() {
        let json = r#"{
            "collection": "my-api",
            "flowName": "login",
            "environmentName": null,
            "globalEnvName": null,
            "authTokens": { "a": { "accessToken": "tok-123456" } }
        }"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        let (input, tokens) = dto.into_parts();

        assert_eq!(input.collection, "my-api");
        assert_eq!(input.flow_name, "login");
        assert_eq!(
            tokens.get("a").map(|t| t.access_token.as_str()),
            Some("tok-123456")
        );
    }

    #[test]
    fn run_flow_input_without_auth_tokens_has_none() {
        let json = r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        let (_input, tokens) = dto.into_parts();
        assert!(tokens.is_empty());
    }

    #[test]
    fn run_flow_input_never_prints_or_serializes_a_token() {
        let json = r#"{
            "collection": "c", "flowName": "f", "environmentName": null, "globalEnvName": null,
            "authTokens": { "a": { "accessToken": "super-secret-token" } }
        }"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        assert!(
            !format!("{dto:?}").contains("super-secret-token"),
            "Debug must redact tokens"
        );
        assert!(
            !serde_json::to_string(&dto)
                .expect("serialize")
                .contains("super-secret-token"),
            "Serialize must omit tokens"
        );
    }
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p rocket run_flow_input`
Expected: compile errors (`no method into_parts`, no field `auth_tokens`).

- [ ] **Step 4: Implement**

Replace the `RunFlowInputDto` struct and its `From` impl in `src-tauri/src/commands/flow.rs` with:

```rust
/// A token the UI obtained before the run. `Debug` never shows the value.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAuthTokenDto {
    pub access_token: String,
}
impl std::fmt::Debug for FlowAuthTokenDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FlowAuthTokenDto(<redacted>)")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowInputDto {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
    /// Tokens the UI obtained for Auth nodes, keyed by node id. Never serialized.
    #[serde(default, skip_serializing)]
    pub auth_tokens: std::collections::HashMap<String, FlowAuthTokenDto>,
}
impl RunFlowInputDto {
    /// Splits the DTO into the run input and the tokens, so the tokens cannot
    /// be dropped by accident.
    pub fn into_parts(self) -> (RunFlowInput, rocket_app::FlowAuthTokens) {
        let tokens = self
            .auth_tokens
            .into_iter()
            .map(|(node_id, t)| {
                (
                    node_id,
                    rocket_app::SuppliedToken {
                        access_token: t.access_token,
                    },
                )
            })
            .collect();
        let input = RunFlowInput {
            collection: self.collection,
            flow_name: self.flow_name,
            environment_name: self.environment_name,
            global_env_name: self.global_env_name,
        };
        (input, tokens)
    }
}
```

Change the body of `run_flow` from `flow_exec.run(&exec, input.into()).await` to:

```rust
    let (run_input, tokens) = input.into_parts();
    flow_exec.run_with_auth(&exec, run_input, tokens).await
```

Add `FlowAuthTokens` and `SuppliedToken` re-exports check: `rocket_app::FlowAuthTokens` and `rocket_app::SuppliedToken` are re-exported from `crates/rocket-app/src/lib.rs` (Plan 2, Task 1 Step 3). If `cargo check` cannot find them, add them to that `pub use flow_auth::{...}` line.

If any other code in `src-tauri` used `RunFlowInput::from(dto)` or `dto.into()` for this DTO, the compiler will point at it; switch it to `into_parts()` and discard the tokens only where the caller is a test.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket run_flow_input && cargo check -j4`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/flow.rs crates/rocket-app/src/lib.rs
git commit -m "feat(flow): accept UI-supplied auth tokens in run_flow"
```

---

### Task 2: Startup wiring of the token fetcher

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `OAuth2ServiceFetcher::new(OAuth2Service)`, `FlowExecutionService::with_token_fetcher` (Plan 2).
- Produces: a flow run can fetch client-credentials and password tokens in the backend with the same repos, collection-environment client certificates and TLS provider as the OAuth2 commands.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Build the OAuth2 service through a factory**

In `src-tauri/src/lib.rs`, replace the whole `let oauth2_svc = ... ;` statement (the one that starts `let oauth2_svc = rocket_app::oauth2_service::OAuth2Service::new(` and ends with `.with_token_client_provider(Arc::new(rocket_infra::ReqwestTokenClientProvider));`) with:

```rust
            // OAuth2Service — stand-alone service for token acquisition flows.
            // Uses its own repo instances pointed at the same paths as the exec service.
            // A factory, because the Flow runner needs a second instance for its
            // token fetcher and `OAuth2Service` is not `Clone`.
            let make_oauth2_service = || {
                rocket_app::oauth2_service::OAuth2Service::new(
                    Box::new(FsEnvironmentRepo::with_secret_store(
                        environments_dir.clone(),
                        env_secret_store(),
                    )),
                    Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
                )
                // Client certificates live on a collection's own environment, and a token
                // endpoint that needs mutual TLS gets the matching one.
                .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
                    Arc::clone(&active_workspace_path),
                )))
                .with_token_client_provider(Arc::new(rocket_infra::ReqwestTokenClientProvider))
            };
            let oauth2_svc = make_oauth2_service();
```

If the compiler reports that `environments_dir` was moved earlier or is not `Clone`, derive a fresh path the same way the original code obtained it instead of cloning, and keep the closure otherwise identical.

- [ ] **Step 3: Give the flow runner the fetcher**

In the `let flow_exec_svc = rocket_app::FlowExecutionService::new(...)` chain, after `.with_callback_listener(...)`, add:

```rust
            // Auth nodes fetch client-credentials and password tokens through the
            // same OAuth2 stack as the Authentication tab.
            .with_token_fetcher(Box::new(rocket_app::flow_auth::OAuth2ServiceFetcher::new(
                make_oauth2_service(),
            )));
```

and remove the `;` that previously ended the `.with_callback_listener(...)` statement so the chain continues (the final `;` is the one after `.with_token_fetcher(...)`).

- [ ] **Step 4: Check it builds**

Run: `cargo check -j4`
Expected: PASS.

- [ ] **Step 5: Run the Rust suites that could be affected**

Run: `cargo test -p rocket -j4 && cargo test -p rocket-app -j4 oauth2 flow`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(flow): wire an OAuth2 token fetcher into the flow runner"
```

---

### Task 3: `runFlow` accepts tokens in the frontend API

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Test: `src/lib/queries/__tests__/flow-api.test.ts`

**Interfaces:**
- Consumes: IPC shape from Task 1.
- Produces (used by Plan 6):

```ts
export interface FlowAuthToken { accessToken: string }
export const runFlow: (
  collection: string, flowName: string,
  environmentName?: string | null, globalEnvName?: string | null,
  authTokens?: Record<string, FlowAuthToken>,
) => Promise<FlowRunSummary>
```

- [ ] **Step 1: Write the failing tests**

Append inside the `describe` block of `src/lib/queries/__tests__/flow-api.test.ts`, after the last `runFlow` test:

```ts
  it('runFlow sends authTokens when there are some', async () => {
    vi.mocked(invoke).mockResolvedValue({ runId: 'r', steps: [], stoppedReason: 'completed' });
    const { runFlow } = await import('@/lib/tauri-api');
    await runFlow('my-collection', 'My Flow', null, null, { a: { accessToken: 'tok-123456' } });
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'my-collection',
        flowName: 'My Flow',
        environmentName: null,
        globalEnvName: null,
        authTokens: { a: { accessToken: 'tok-123456' } },
      },
    });
  });

  it('runFlow leaves authTokens out when the map is empty', async () => {
    vi.mocked(invoke).mockResolvedValue({ runId: 'r', steps: [], stoppedReason: 'completed' });
    const { runFlow } = await import('@/lib/tauri-api');
    await runFlow('my-collection', 'My Flow', null, null, {});
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'my-collection',
        flowName: 'My Flow',
        environmentName: null,
        globalEnvName: null,
      },
    });
  });
```

- [ ] **Step 2: Run to verify the first one fails**

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: FAIL (`authTokens` not sent).

- [ ] **Step 3: Implement**

In `src/lib/tauri-api.ts`, replace the `runFlow` definition with:

```ts
/** A token the UI obtained for an Auth node. Held in memory only, never persisted. */
export interface FlowAuthToken {
  accessToken: string;
}

export const runFlow = (
  collection: string,
  flowName: string,
  environmentName?: string | null,
  globalEnvName?: string | null,
  authTokens?: Record<string, FlowAuthToken>,
) =>
  invoke<FlowRunSummary>('run_flow', {
    input: {
      collection,
      flowName,
      environmentName: environmentName ?? null,
      globalEnvName: globalEnvName ?? null,
      // Sent only when there is something to send, so a flow without Auth nodes
      // calls the command exactly as before.
      ...(authTokens && Object.keys(authTokens).length > 0 ? { authTokens } : {}),
    },
  });
```

- [ ] **Step 4: Run the checks**

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts src/components/flow/__tests__/FlowToolbar.test.tsx && yarn tsc --noEmit && yarn check`
Expected: all PASS (the toolbar's existing `toHaveBeenCalledWith('my-collection', 'my-flow', null, null)` assertions are unaffected because the toolbar still passes four arguments until Plan 6).

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/queries/__tests__/flow-api.test.ts
git commit -m "feat(flow): let runFlow pass auth tokens to the backend"
```

---

**End of Plan 4.** Verify: `cargo check -j4`, `cargo test -p rocket -j4`, `yarn tsc --noEmit`, `yarn test src/lib/queries/__tests__/flow-api.test.ts`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-05-frontend-node.md`** (store, Auth node, palette, editor, `auth` handle).
