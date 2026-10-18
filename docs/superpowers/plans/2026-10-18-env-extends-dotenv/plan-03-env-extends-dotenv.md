# Environment IPC and frontend send path Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The HTTP send, which builds its variables in the frontend, uses the backend's flattened variables and `.env` values, and the Console masks every `.env` value.

**Architecture:** A new Tauri command returns a flattened environment with each variable's source. `get_process_env_vars` takes the active environment names and returns the map plus the keys that came from `.env`. The frontend reads both through new query keys and feeds them to the existing variable context and masking.

**Tech Stack:** Rust (`src-tauri`), React, TypeScript, TanStack Query, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`

## Global Constraints

- New IPC DTOs use `#[serde(rename_all = "camelCase")]`. Persistence structs are untouched.
- Commands stay thin: validate, call the service, map output and error.
- Every `.env` value is masked in the Console as a secret, fail closed, with the existing reveal toggle. No `.env` value is written to history or a saved file.
- The raw environment queries stay for the editor. There is no TypeScript copy of the merge.
- A cycle error from the backend must surface as an error the send reports, never as silently missing variables.
- Zustand: never fully destructure store state at component top level. UI uses shadcn/ui primitives and lucide-react icons only.
- Verify with `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`, and per-crate `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`.

## Post-Implementation Review

Run after Task 3, on the finished IPC and send path, before the next plan starts. Do the app checks with `yarn tauri dev` and a collection whose environment `child` extends `base`.

- [ ] `cargo test -j4 -p rocket environments`, `yarn test`, `yarn tsc --noEmit` and `yarn check` pass. Evidence: the command output.
- [ ] In the app, a request using `{{baseVar}}` (defined only in `base`) sends with the right value under `child`, and `child`'s own value wins when both define it. Evidence: the request preview or Console row.
- [ ] A `.env` value used as `{{process.env.NAME}}` sends correctly and shows `••••••` in the Console, and `Show secrets` reveals it. Evidence: the Console row before and after the toggle.
- [ ] After editing `base` and saving, the next send uses the new value without a restart. Evidence: the send result.
- [ ] Creating a cycle on disk and sending shows an error naming both environments and sends nothing. Evidence: the error toast or Console line.
- [ ] No remaining frontend code reads `env.variables` for sending without the resolved query. Evidence: a grep of `src` with a reason for each remaining hit.

## Review Focus

- The resolved query is stale after an environment save, a parent save or a delete: all resolved and process queries are invalidated on any environment write (Task 2).
- A cycle error from `get_resolved_environment`: the send surfaces the error and does not send with partial variables (Task 2).
- A `.env` value shorter than 6 characters: it stays visible, the existing minimum applies and is documented in the test (Task 2).
- The process env result with no loader and no gate: an empty `dotEnvKeys` array, not undefined (Task 1).
- The gRPC, flow and runner readers of `env.variables` keep working with an environment that has no `extends` (Task 3).

## File Structure

- Modify `src-tauri/src/commands/environments.rs`: `get_resolved_environment`, `get_process_env_vars` (new argument and result). Modify `src-tauri/src/lib.rs` (register).
- Modify `src/lib/tauri-api.ts` (types and invokes), `src/lib/queries/environment-queries.ts` (keys, hooks, invalidation).
- Modify `src/lib/execute-request.ts`, `src/lib/variable-context.ts`, `src/lib/console-redaction.ts`.
- Modify `src/hooks/useGrpcVariableContext.ts`, `src/hooks/useCollectionVariableContext.ts`, `src/lib/dispatch-send.ts`, `src/lib/runner-execute.ts`, `src/lib/graphql-subscription-session.ts` (the other readers of `env.variables`).

---

### Task 1: The two backend commands

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/environments.rs`, `src-tauri/src/lib.rs`
- Test: inline tests next to the commands, or a mapping function tested directly

**Interfaces:**
- Consumes: `flatten_with_repo`, `RequestExecutionService::process_env_for`, `regular_env_repo`, `global_env_repo` from plans 1 and 2.
- Produces:
  - `ResolvedVariableDto { key: String, value: String, enabled: bool, secret: bool, source: String }`
  - `ResolvedEnvironmentDto { name: String, variables: Vec<ResolvedVariableDto>, warnings: Vec<String> }`
  - Command `get_resolved_environment(collection: Option<String>, name: String) -> Result<ResolvedEnvironmentDto, String>` (`None` means the global store).
  - `ProcessEnvDto { vars: HashMap<String, String>, dot_env_keys: Vec<String> }`
  - Command `get_process_env_vars(collection: Option<String>, environment: Option<String>, global_environment: Option<String>) -> ProcessEnvDto`.

- [ ] **Step 1: Write the failing tests** for the mapping functions `to_resolved_dto(&Flattened) -> ResolvedEnvironmentDto` and `to_process_env_dto(ProcessEnv) -> ProcessEnvDto`:
  - `resolved_dto_keeps_source_and_secret_flag`, `resolved_dto_serialises_camel_case`, `process_env_dto_always_has_a_dot_env_keys_array`, `a_cycle_maps_to_an_error_string_naming_the_environments`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket environments` (the `src-tauri` package is named `rocket`).

- [ ] **Step 3: Implement** the DTOs, mapping functions and the two commands, thin over the services. Register `get_resolved_environment` next to `get_process_env_vars` in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run to verify they pass:** the command above, then `cargo check -j4 -p rocket`. Expected: PASS.

---

### Task 2: The HTTP send uses resolved variables and masks .env values

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/lib/queries/environment-queries.ts`, `src/lib/execute-request.ts`, `src/lib/variable-context.ts`, `src/lib/console-redaction.ts`
- Test: `src/lib/execute-request.test.ts` (or the existing sibling test file), `src/lib/console-redaction.test.ts`, `src/lib/queries/environment-queries.test.ts`

**Interfaces:**
- Consumes: the two commands from Task 1.
- Produces:
  - `ResolvedEnvironment`, `ResolvedVariable`, `ProcessEnv` TypeScript types in `tauri-api.ts`, and `getResolvedEnvironment(collection: string | null, name: string): Promise<ResolvedEnvironment>`, `getProcessEnvVars(collection?: string | null, environment?: string | null, globalEnvironment?: string | null): Promise<ProcessEnv>`.
  - `environmentKeys.resolved(collection: string | null, name: string)` and `environmentKeys.process(scope, environment, globalEnvironment)`.
  - `collectSecretValues(lists, extra)` receives the `.env` values through `extra`.

- [ ] **Step 1: Write the failing tests**
  - `getActiveVariables_reads_the_resolved_environment_so_an_inherited_variable_resolves`.
  - `getGlobalVariables_reads_the_resolved_global_environment`.
  - `a_resolved_query_error_is_reported_and_the_send_does_not_use_partial_variables`.
  - `process_env_query_key_includes_the_active_environment_names`.
  - `dot_env_values_are_in_the_secret_values_and_masked_in_the_console_row`.
  - `a_dot_env_value_shorter_than_six_characters_stays_visible`.
  - `saving_an_environment_invalidates_the_resolved_and_process_queries`.

- [ ] **Step 2: Run to verify they fail:** `yarn test execute-request console-redaction environment-queries`.

- [ ] **Step 3: Implement.** Replace the cache reads in `getActiveVariables` and `getGlobalVariables` with `fetchQuery` on `environmentKeys.resolved(...)`, keeping the enabled filter. Update the private `getProcessEnvVars` to pass the active names and return the `ProcessEnv`. Pass the `.env` values (looked up by `dotEnvKeys`) to `collectSecretValues` as `extra`, and make `getSecretScope` use the resolved variables. Invalidate the `resolved` and `process` key prefixes in the environment save and delete hooks.

- [ ] **Step 4: Run to verify they pass:** the command above, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

---

### Task 3: The other frontend readers

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/hooks/useGrpcVariableContext.ts`, `src/hooks/useCollectionVariableContext.ts`, `src/lib/dispatch-send.ts`, `src/lib/runner-execute.ts`, `src/lib/graphql-subscription-session.ts`
- Test: the existing sibling test files of each, plus `src/hooks/useCollectionVariableContext.test.ts`

**Interfaces:**
- Consumes: `useResolvedEnvironment(collection: string | null, name: string | null)` (a hook added here in `environment-queries.ts`, a thin `useQuery` over `getResolvedEnvironment`), `ProcessEnv` from Task 2.
- Produces: no new public names beyond that hook.

- [ ] **Step 1: Write the failing tests**
  - `useCollectionVariableContext_includes_inherited_variables_and_their_secret_keys`.
  - `useGrpcVariableContext_includes_inherited_variables`.
  - `runner_and_dispatch_send_use_the_resolved_variables`.
  - `an_environment_without_extends_resolves_exactly_as_before`.
  - `graphql_subscription_session_receives_the_process_env_map_from_the_new_result_shape`.

- [ ] **Step 2: Run to verify they fail:** `yarn test useCollectionVariableContext useGrpcVariableContext dispatch-send runner-execute graphql-subscription-session`.

- [ ] **Step 3: Implement.** Add `useResolvedEnvironment`. Replace each direct `env.variables` read for sending with the resolved variables. Keep reads that build the editor (raw) unchanged. Update every caller of the old `getProcessEnvVars` result shape.

- [ ] **Step 4: Run to verify they pass:** the command above, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

Next plan: docs/superpowers/plans/2026-10-18-env-extends-dotenv/plan-04-env-extends-dotenv.md
