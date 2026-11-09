# Rust Style Roadmap (Google Rust style guide)

Goal: bring all Rust code in line with the applicable rules of https://google.github.io/styleguide/rustguide.html.
Source: a full audit of every crate on 2026-11-03 (17 crates, about 150k non-test lines). Each crate was read by a sonnet agent working from exact per-file counts, and the main findings were spot-checked against the source.

## How to maintain this file

- IDs are stable (`S-nn`). Never renumber. Add new items at the end of their phase.
- Status: `open`, `planned`, `in-progress`, `done`, `dropped`. When an item ships, set `done` and note the commit.
- Plans derived from this file hold at most 3 tasks each, with an index file.
- Counts are from the audit date. Re-count with the tool below before starting a phase.
- Verify every item with `cargo check -j4 -p <crate>`, `cargo clippy -j4 -p <crate>` and `cargo test -j4 -p <crate>`. Never use `--workspace`. The Tauri crate is `-p rocket`.
- One crate per plan. Commit with a pathspec, never `git add -A`. The user reviews each plan before it is committed.

## Measuring progress

`python3 -I .claude/tools/rust-style-metrics.py <crate>/src [top-N]` prints exact per-file counts for non-test code: unwrap, expect, panic, clone, `&Option`, `&Vec`/`&String`, glob imports, nested `use`, `push_str(&format!)`, `Arc`/`Rc`, `get_` functions and undocumented public items.

- It skips `tests.rs`, `*_tests.rs`, `test_*.rs` and everything after the first top-level `#[cfg(test)] mod name { .. }`.
- Known blind spots: files with several inline `#[cfg(test)]` blocks (for example `rocket-import/src/openapi/yaml_budget.rs`), and `#[path]` test modules (for example `rocket-infra/src/reqwest_executor_mtls_handshake.rs`, which is test-only).
- `undoc` is a heuristic. It counts `pub mod` and `pub use` lines in `lib.rs` and `mod.rs`, so treat those rows as false positives.

## Rules we apply and rules we skip

Apply: module layout and imports, ownership, parameter types, traits and derives, naming, docs, unwrap and expect calls, iterator chains, `unsafe`.
Skip on purpose:

- Chromium and Fuchsia items, "no async", "no std threads".
- "Avoid `Arc`" and "prefer generics over `dyn Trait`". Rocket's DDD wiring needs both.
- "Prefer small crates, flat layout". The crate split is settled.
- `get_` on lookup methods and on IPC or MCP command names. They are tied to the frontend `invoke` names, `app-commands.txt` and MCP tool names.
- `#[non_exhaustive]` on internal enums.
- Fixing every clone. Most clones in the audit are required by borrowed `&T` inputs that build owned DTOs.

## Audit result per crate

| Crate | Non-test lines | Verdict | Main debt |
|---|---|---|---|
| rocket-shared | 2.3k | PASS | docs |
| rocket-collection | 3.9k | PASS | 2 `&Option`, 1 nested `use`, docs |
| rocket-environment | 2.6k | PASS | docs, derives |
| rocket-history | 0.1k | PASS | docs |
| rocket-workspace | 0.4k | PASS | docs, `Workspace::new` |
| rocket-flow | 2.2k | PASS | nits only |
| rocket-http | 4.0k | MINOR | `load_test.rs` invariants and size, nested `use` |
| rocket-git | 4.3k | MINOR | `Debug` on secrets, 3 `unreachable!`, 8 globs |
| rocket-export | 2.5k | MINOR | clones in `postman_auth.rs`, docs |
| rocket-acp | 1.2k | PASS | 2 docs |
| rocket-audit | 0.5k | MINOR | docs (about 22), 2 avoidable clones |
| rocket-grpc | 0.5k | PASS | 4 docs |
| rocket-scripting | 0.7k | PASS | `Debug` on secret values |
| rocket-import | 11.9k | MINOR | `Debug` on secrets, 6 globs, large `importer.rs` |
| rocket-infra | 24.1k | MINOR | about 30 globs, `mod.rs` with logic, `Debug` on secrets |
| rocket-app | 32.9k | MINOR | docs (about 270), `Debug` on secrets, 2 production `expect` |
| src-tauri | 14.0k | MAJOR | 6 production unwrap calls, `run()` is 1,158 lines, docs (about 350) |

Clean everywhere: no `unsafe` outside `src-tauri/src/lib.rs`, no `catch_unwind`, no `for_each`, no `&Vec`/`&String`/`&Box` parameters in domain code, formatting clean (`cargo fmt --all --check` shows 0 diffs).

## Phase 0: decisions (no code)

Settle these first. They stop later phases from fixing things that are deliberate.

| ID | Decision | Status | Recommendation |
|---|---|---|---|
| S-01 | camelCase on persistence structs: `rocket-infra/src/oc/` (23 structs) and `contract_records/*.rs` use `rename_all = "camelCase"` | open | Keep. The OpenCollection on-disk format and the contract records are camelCase. Write an exception into `.claude/rules/rust-ddd-boundaries.md` and `tauri-ipc-boundaries.md`. |
| S-02 | `get_*` names on IPC commands (31 in `src-tauri/commands`), MCP tools (`mcp/tool_server.rs`) and repository traits | open | Accept. Do not rename. |
| S-03 | Glob re-exports in `lib.rs` and `mod.rs` (`rocket-git/src/lib.rs`, `rocket-infra/src/oc/mod.rs`, `conversions/mod.rs`) | open | Replace with named re-exports where the list is under about 30 names. Keep globs for `oc/` only if the list changes with the spec. Decide before Phase 2. |
| S-04 | Docs standard | open | Every public item gets a short full-sentence `///`. A module-level `//!` is enough for a `pub mod` list. Do not restate the name. IPC commands get one line. |
| S-05 | Files named after a topic (`types.rs`, `dynamic_vars.rs`, `repository.rs`, `repository_locator.rs`, `client_certificate_validation.rs`) | open | Accept. Rename only when a file is split anyway. |
| S-06 | Converters as free functions or `From<&T>` (`rocket-infra/conversions`) | open | Keep free functions unless a caller is already being changed. |

## Phase 1: safety (do first)

Secrets in `Debug`, production panics and the log forwarder. Small, high value, low risk. Use the existing hand-written redacting `Debug` as the pattern: `EnvPair` in `rocket-acp/src/agent_config.rs:14`, `FlowAuthTokenDto` in `src-tauri/src/commands/flow.rs:561` and `PasswordChangeDto` in `commands/proxy.rs:58`. Each fix gets a test that formats the value with `{:?}` and asserts the secret is absent.

### 1a. Redacting `Debug` (secrets under a derived `Debug`)

| ID | Type | Where | Secrets | Status |
|---|---|---|---|---|
| S-10 | `GitCredentials` | `rocket-git/src/credentials.rs:3` | password, token, passphrase | open |
| S-11 | `GitCredentialsPayload` | `src-tauri/src/commands/git.rs:823` | passphrase, password, token | open |
| S-12 | `BruAuth` and `BruYmlAwsV4Auth`, `BruYmlBasicAuth`, `BruYmlBearerAuth`, `BruYmlApiKeyAuth` | `rocket-import/src/bru/ast.rs:132`, `bru/yml_adapter.rs:122-150` | password, token, secret key, api-key value | open |
| S-13 | `HostScopes` (and `HostRunOutcome` through it) | `rocket-scripting/src/host.rs:71-78` | `secret_values` | open |
| S-14 | `OcAuthTyped` and the struct at `oc/auth.rs:99-101` | `rocket-infra/src/oc/auth.rs:19-51,99-101` | password, token, secret key, consumer secret, access token | open |
| S-15 | `McpHttpServerCredentials` | `rocket-app/src/acp_session_service.rs:92` | bearer token | open |
| S-16 | `StreamMasker` (and check `ChunkStream`) | `rocket-app/src/acp_stream.rs:53,138` | secret forms, unmasked buffer | open |
| S-17 | `OAuth2GetTokenRequest`, `OAuth2RefreshRequest` | `rocket-app/src/oauth2_service.rs:25,72` | client secret, password, refresh token | open |
| S-18 | Postman auth structs and the `flow_auth.rs` / `flow_run_cache.rs` types: check for derived `Debug` over secrets (not examined) | `rocket-export/src/postman_auth.rs`, `rocket-import/src/postman/ast.rs`, `rocket-app/src/flow_auth.rs` | unknown | open |
| S-19 | `SourceFile` dumps file bytes in `Debug` | `rocket-export/src/ports.rs:6` | file bytes (optional) | open |

### 1b. Production panics

| ID | Item | Where | Status |
|---|---|---|---|
| S-20 | Replace the unwrap after `parse()` on `"about:blank"` with a non-panicking parse (three places) | `src-tauri/src/commands/oauth2.rs:92,389,518` | open |
| S-21 | Replace the unwrap on `tx.lock()` before `.take()` with poison-tolerant locking | `commands/oauth2.rs:98,524` | open |
| S-22 | Map the poisoned lock to a `DomainError`, as `workspaces.rs:139-143` already does | `commands/collections.rs:253` | open |
| S-23 | Replace the unwrap after `strip_suffix(".json")` (which follows an `ends_with` check) with `if let Some(stem)` | `rocket-infra/src/migration.rs:296` | open |
| S-24 | Replace the poison `expect` with `unwrap_or_else(PoisonError::into_inner)` or a `DomainError` | `rocket-app/src/security_audit_service.rs:68,77` | open |
| S-25 | Replace three `unreachable!` with a real error or a restructured match | `rocket-git/src/git2_service/staging.rs:43,145`, `status_diff.rs:116` | open |
| S-26 | Replace the `(Option<Semaphore>, Option<RateDriver>)` pair with a `Limiter { Rps, Concurrency }` enum. This removes three `expect` calls and one `unreachable!` | `rocket-http/src/load_test.rs:523,533,566,708` | open |
| S-27 | `main.rs:14` expect becomes `eprintln!` plus `exit(1)`; `lib.rs:214` parse of a static directive | `src-tauri/src/main.rs:14`, `lib.rs:214` | open |
| S-28 | `create_dir_all(..).ok()` ignores a failure that later paths depend on; return the error or log a warning | `src-tauri/src/lib.rs:274,321,335` | open |
| S-29 | Add a `SAFETY:` label to the unsafe block (the explanation is above it but unlabeled) | `src-tauri/src/lib.rs:184` | open |
| S-30 | A comment or `# Panics` note for the audit-hash `expect` (it is a real invariant) | `rocket-audit/src/chain.rs:9` | open |

### 1c. Log forwarder

| ID | Item | Where | Status |
|---|---|---|---|
| S-31 | `FieldVisitor` forwards every logged field at INFO and above to the frontend with no redaction. Add a redactor for field names such as `token`, `authorization`, `password`, `secret`, `key`, `credential`. Follow `docs/features/console-masking.md`. Write the test first | `src-tauri/src/tauri_tracing_layer.rs:140-195` | open |
| S-32 | `on_new_span` and `on_record` are not implemented, so `span_fields` is always empty. Implement or delete the dead branch | `tauri_tracing_layer.rs:~190` | open |
| S-33 | Check that no `#[instrument]` or log-by-args call site captures secret command arguments | `commands/oauth2.rs:43`, `secret_managers.rs:149`, `openapi_sync.rs:837,890,964,990,1675`, `import.rs:407` | open |
| S-34 | `McpHttpServerHandle.token` is a `pub` field. Make it private with an accessor | `src-tauri/src/mcp/tool_server.rs:1162` | open |

Plans: S-10 to S-19 as three plans, S-20 to S-30 as four plans (by crate), S-31 to S-34 as one plan.

## Phase 2: imports

Mechanical, compile-checked, low risk.

| ID | Item | Where | Status |
|---|---|---|---|
| S-40 | Replace the glob re-exports in `lib.rs` with named re-exports (needs S-03) | `rocket-git/src/lib.rs:14-24` (8) | open |
| S-41 | Replace `pub use X::*` with named exports (needs S-03) | `rocket-infra/src/oc/mod.rs:17-32` (11), `conversions/mod.rs:23,25,27,42,47,53` (6) | open |
| S-42 | Replace `use crate::oc::*;` with explicit imports | 13 files in `rocket-infra/src/conversions/` (line 1; `graphql.rs:3`, `grpc.rs:3`, `websocket.rs:8`) | open |
| S-43 | Replace `use ..::ast::*;` with explicit imports | `rocket-import`: `converter/postman.rs:1`, `converter/grpc.rs:4`, `converter/request.rs:7`, `bru/parser.rs:2`, `bru/yml_adapter.rs:1`, `wsdl/parser.rs:7` | open |
| S-44 | Replace `use openapi::*;` inside a function | `rocket-app/src/contract_service.rs:717` | open |
| S-45 | Split nested `use` (one module per `use`), rocket-collection and rocket-http | `rocket-collection/src/contract/repository.rs:1`; `rocket-http`: `pkce.rs:1`, `wsse_sig.rs:5`, `ntlm_sig.rs:7`, `oauth1_sig.rs:6`, `util.rs:3` | open |
| S-46 | Split nested `use`, rocket-git and rocket-infra | `rocket-git/src/git2_service/branch.rs:1`; `rocket-infra`: `rocketvault/certificate_api.rs:7`, `rocketvault/certificates.rs:4`, `fs_audit_log_repo.rs:2,4-9`, `fs_compliance_profile_repo.rs:3`, `fs_contract_repo.rs`, check `grpc/reflection.rs:38` | open |
| S-47 | Split nested `use`, rocket-app and src-tauri | `rocket-app`: `execution_service.rs:3-11,24-26`, `websocket_service.rs:10`, `execution_service/websocket_resolution.rs:8`, `env_audit.rs:6`, `client_certificates.rs:9`; `src-tauri`: `commands/export.rs:4`, `commands/load_test.rs:64`, `commands/audit.rs:3`, `lib.rs:192` | open |
| S-48 | Qualify free-function calls with their module where the name is ambiguous | `rocket-import/src/openapi/sync/apply.rs:5-13` (`check`), `rocket-audit/src/event.rs:1`, `rocket-workspace/src/workspace.rs:2,39`, `repository_locator.rs:6,139`, and the `rocket-export`, `rocket-infra` and `execution_service.rs` bare imports (about 40, low value; do only when touching the file) | open |

Plans: one per crate (git, collection+http, infra, import, app+src-tauri).

## Phase 3: signatures

Parameter and type cleanups. Each changes a call site, so each needs its crate tests.

### 3a. `&Option<T>` to `Option<&T>`

| ID | Where | Status |
|---|---|---|
| S-50 | `rocket-collection/src/contract/snapshot.rs:157,171` | open |
| S-51 | `rocket-export/src/postman.rs:261`, closure at `postman_auth.rs:112` | open |
| S-52 | `rocket-import/src/openapi/sync/merge.rs:551,727-729`, closure at `:150` | open |
| S-53 | `rocket-http/src/oauth1_sig.rs:174` | open |
| S-54 | `rocket-git/src/git2_service/helpers.rs:492` | open |
| S-55 | `rocket-infra`: `reqwest_executor.rs:217`, `conversions/request.rs:210,219,246-248`, `conversions/folder_settings.rs:121`, `fs_collection/settings.rs:21,68,128,196`, `azurekeyvault/mod.rs:380` | open |
| S-56 | `rocket-app`: `flow_auth.rs:331,351,436,437`, `flow_run_cache.rs:322`, `flow_execution_service.rs:2308`, `oauth2_service.rs:699` | open |
| S-57 | `src-tauri`: `commands/oauth2.rs:184,460`, `mcp/tool_server.rs:592` | open |
| S-58 | `Box<Option<T>>` becomes `Option<Box<T>>` | `rocket-infra/src/oc/auth.rs:83,85` | open |

### 3b. Boolean parameters to enums

Do these where the call site is unclear. Skip deno-op ABI parameters (`rocket-infra/scripting/ops/fs.rs`, `rok.rs:232`, `state.rs:71`) and IPC argument shapes.

| ID | Where | Status |
|---|---|---|
| S-60 | `rocket-export`: `bruno.rs:153` `include_environments`; `scripts.rs:208` `markers` (low) | open |
| S-61 | `rocket-import/src/openapi`: `bundle.rs:463` (two bools), `entry.rs:36`, `source_url.rs:271` (pub), `merge.rs:354,363` `case_insensitive`, `reader.rs:172,603` `is_v2` (use a `SpecVersion` enum) | open |
| S-62 | `rocket-infra`: `fs_collection/tree.rs:15,235`, `reqwest_executor.rs:218`, `acp_agent_client.rs:1172` | open |
| S-63 | `rocket-app`: `git_service.rs:150,196` `force`, `openapi_sync_apply.rs:491` `include_token`, `oauth2_service.rs:226`, `execution_service.rs:272,599,608` (the `sent` pair is the least clear), `script_chain.rs:167`, `mcp_read_views.rs:206,491`, `proposal_service.rs:920,940,993`, `flow_poll.rs:64`, `flow_routing.rs:22,46,63`, `flow_run_cache.rs:1180`, `flow_execution_service.rs:2279,2396` | open |
| S-64 | `src-tauri/src/tauri_tracing_layer.rs:~47` `always_send` | open |

### 3c. Tuples, error types, std traits

| ID | Item | Where | Status |
|---|---|---|---|
| S-65 | Named structs for tuple returns and tuple parameters | `rocket-export`: `postman_env.rs:105-107`, `bruno.rs:158`, `postman.rs:141`; `rocket-http`: `load_test.rs:62,443`, `ntlm_sig.rs:255`; `rocket-git`: `helpers.rs:428,554`, `sanitize.rs:155,174,227,337`; `rocket-import`: `converter/request.rs:30,58,102,169`, `converter/grpc.rs:18`, `converter/postman.rs:186`, `importer.rs:1442`, `openapi/model.rs` (`responses`, `response_media`); `rocket-flow`: `lint.rs:548`, `validate.rs:53`; `rocket-infra`: `callback_server.rs:43-44`, `fs_dot_env_loader.rs:31,36,91`, `acp_permissions.rs:27`; `src-tauri`: `agent_session/scratch.rs:67,75` | open |
| S-66 | `parse_ssh_endpoint` returns `Result<_, ()>`; use a real error type | `rocket-git/src/git2_service/ssh_host_verification.rs:40` | open |
| S-67 | `Result<_, String>` becomes `DomainResult` (or a typed error) | `rocket-infra/src/fs_dot_env_loader.rs:31,36,91,157`; `src-tauri/src/mcp/tool_server.rs` (`check_len`, `check_opt`, `check_pairs`, `to_domain_patch`) | open |
| S-68 | `GrpcMethodType::parse` becomes `FromStr`; `GroupBy::parse` likewise (nit) | `rocket-collection/src/grpc_request.rs:33`; `rocket-import/src/openapi/layout.rs` | open |
| S-69 | `PostmanKeyValue::as_str_value` returns an owned `String`; rename to `to_string_value` | `rocket-import/src/postman/ast.rs:157` | open |
| S-70 | `request_type: String` and ApiKey `placement: String` become enums | `rocket-import/src/bru/ast.rs:84` | open |
| S-71 | `Workspace::new(&str)` takes `impl Into<String>` | `rocket-workspace/src/workspace.rs:28` | open |
| S-72 | Remove the unused `_workspace_id` parameter or document it; remove the dead `BruAuth::Unknown` | `rocket-import/src/importer.rs:742`, `bru/ast.rs:157` | open |
| S-73 | `get_response` becomes `response_from_state`; `get_summaries` becomes `load_summaries` | `rocket-infra/src/scripting/ops/res.rs:5`; `rocket-app/src/collection_service.rs:177` | open |
| S-74 | `oc/http.rs:170` `OcScripts` alias; decide newtype or remove | `rocket-infra/src/oc/http.rs:170` | open |
| S-75 | `Vec<&String>` / `impl IntoIterator<Item=&String>` takes `&str` | `rocket-app/src/redaction.rs:17,117` (low) | open |

Plans: 3a by crate (one plan each for collection+export, import, infra, app+src-tauri), 3b by crate, 3c by crate.

## Phase 4: clones and small allocations

Only the avoidable ones. Most clones in the audit are required.

| ID | Item | Where | Status |
|---|---|---|---|
| S-80 | `block(kind, Vec<(&str, String)>)` takes `&str` values. This removes about 30 of the 34 clones in the file | `rocket-export/src/postman_auth.rs:18,42-191` | open |
| S-81 | Hold `&FolderSettings`, not a clone | `rocket-export/src/postman.rs:179` | open |
| S-82 | Avoid full-vector clones when sorting and building the event | `rocket-http/src/load_test.rs:447,837,867-873` | open |
| S-83 | About 6 avoidable clones | `rocket-git`: `remote.rs:269,336`, `helpers.rs:368`, `status_diff.rs:60,75`, `branch.rs:33`, `conflict.rs:27` | open |
| S-84 | Double `new.clone()`, whole `skips` Vec and settings clone | `rocket-import/src/openapi/sync/apply.rs:132-141,188,457` | open |
| S-85 | Whole-request clones; `return c.clone()` forced by `&Option` (goes away with S-52) | `rocket-import/src/openapi/sync/plan.rs:454,498`, `merge.rs:226,734,749,813` | open |
| S-86 | Move instead of clone | `rocket-import/src/importer.rs:50,530,1444` | open |
| S-87 | Scope maps and each secret value cloned twice | `rocket-infra/src/scripting/ops/host.rs:243-249,172,223,178-181` | open |
| S-88 | `acp_agent_client.rs:347,352,364`; small ones in `conversions/request.rs:46,63,116-117`, `grpc.rs:44` | `rocket-infra` | open |
| S-89 | `chain.rs:7` clones the whole event to blank the hash; `chain.rs:44` `prev` can be `&str` | `rocket-audit/src/chain.rs:7,44` | open |
| S-90 | Avoidable clones in proposal and ACP services | `rocket-app`: `proposal_service.rs:847,1053-1078,1310-1326`, `acp_session_service.rs:272,339`, `workspace_service.rs:111-112,300,305`, `openapi_connect.rs:327-328`, `execution_service.rs:2941,3629,3992`, `flow_execution_service.rs:1381-1385,2581,1806,1892` | open |
| S-91 | `openapi_sync.rs:204-212,272-329`, `environments.rs:298-336`, `import.rs:50,70,87,107,124,334,417`, `workspaces.rs:142` (`.cloned()`) | `src-tauri/src/commands` | open |
| S-92 | `lib.rs:348` and `:295` | `src-tauri/src/lib.rs` | open |
| S-93 | `push_str(&format!(..))` becomes `write!` | `rocket-flow/src/lint.rs:368`; `rocket-export/src/scripts.rs:355,360`; `rocket-import`: `openapi/source_url.rs:364`, `openapi/sync/merge.rs:260,625`, `converter/wsdl.rs:21,67`; `rocket-app`: `mcp_read_views.rs:965,979,985,995,999`, `flow_service.rs:292,312`, `export_service.rs:78`; `src-tauri/commands/oauth2.rs:249,475,481` | open |
| S-94 | Share one percent-encode helper (duplicated) and drop the hand-rolled `urlencoding_encode` | `rocket-import/openapi/source_url.rs:364` + `sync/merge.rs:625`; `src-tauri/commands/oauth2.rs:249` | open |
| S-95 | `contract/diff.rs` has 56 clones. Judge them; take owned values only if the callers allow | `rocket-collection/src/contract/diff.rs` | open |
| S-96 | `name_exists` calls `to_lowercase()` per loop pass | `rocket-workspace/src/workspace.rs:73` | open |

## Phase 5: structure

Larger moves. Do each as its own plan with the tests green before and after.

| ID | Item | Where | Status |
|---|---|---|---|
| S-100 | Split the 1,158-line `run()`: the `.setup` closure (about 735 lines) into helpers by area under `app_setup/`, the `generate_handler!` list into its own file, and one shared `shutdown_all_sessions` for the signal and exit paths (they have drifted: the signal path skips `SyncSchedulerSlot`) | `src-tauri/src/lib.rs:146-1304,117-139,1268-1303` | open |
| S-101 | Factory for the repeated `SecretManagerService` + `AgentConfigService` pair and the 17 `TauriEventBus::new` calls | `src-tauri/src/lib.rs:547-586` | open |
| S-102 | Split `mcp/tool_server.rs` (2,297 lines) into `params.rs`, `tools.rs`, `server.rs` | `src-tauri/src/mcp/tool_server.rs` | open |
| S-103 | Move the TCP callback server and HTTP request parser out of the command layer into `rocket-http` behind a trait. Thin-command rule | `src-tauri/src/commands/oauth2.rs:590-680` | open |
| S-104 | One managed `OpenApiSyncCtx` instead of two `Arc`s passed to about 25 commands; consider splitting the 1,771-line file | `src-tauri/src/commands/openapi_sync.rs` | open |
| S-105 | Split `git.rs` (1,236 lines) and move the credentials conversion out of the command layer | `src-tauri/src/commands/git.rs` | open |
| S-106 | Move `FsCollectionRepo` and its trait impl out of `mod.rs` into `repo.rs` | `rocket-infra/src/fs_collection/mod.rs:40-505` | open |
| S-107 | Move `docs_text` out of `oc/mod.rs` | `rocket-infra/src/oc/mod.rs:49` | open |
| S-108 | Move the fetchers out of `mod.rs` into `fetcher.rs` | `rocket-infra/src/rocketvault/mod.rs` (1,012 lines), `azurekeyvault/mod.rs` (1,286 lines) | open |
| S-109 | Move error types and `redact()` out of `scripting/ops/mod.rs` into `error.rs` and `redact.rs` | `rocket-infra/src/scripting/ops/mod.rs:17-56` | open |
| S-110 | Move `Git2Service` and its impls out of `git2_service/mod.rs` | `rocket-git/src/git2_service/mod.rs:37-232` | open |
| S-111 | Split `load_test.rs` (1,735 lines) into scheduler, rate driver, runner and DTOs (after S-26) | `rocket-http/src/load_test.rs` | open |
| S-112 | Move `parse_file` and `parse_env_file` out of `bru/mod.rs` into `bru/loader.rs`; split `importer.rs` (1,459 lines) | `rocket-import/src/bru/mod.rs:10-41`, `importer.rs` | open |
| S-113 | Gate the recording test double behind a test-support feature; handle the poisoned mutex explicitly | `rocket-audit/src/publisher.rs:~15-50,37` | open |

## Phase 6: docs

The largest item by volume. Do it crate by crate, smallest first, and turn on `#![warn(missing_docs)]` (S-121) when a crate is done. Counts are the heuristic `undoc` figure.

| ID | Crate | Items | Notes | Status |
|---|---|---|---|---|
| S-120a | rocket-history | about 25 | `HistoryEntry`, `HistoryRepository` | open |
| S-120b | rocket-workspace | about 35 | `Workspace`, `WorkspaceRegistry`, `RepositoryId` methods | open |
| S-120c | rocket-shared | 17 | 16 `pub mod` lines (one `//!`), `DomainResult` | open |
| S-120d | rocket-environment, rocket-acp, rocket-grpc | 1, 2, 4 | `EnvironmentRepository`, `MAX_IDLE_TIMEOUT_SECS`, `summary()`, `call.rs:36,44,48`, `registry.rs:119` | open |
| S-120e | rocket-collection | 9 or more | `BACKUP_FILE_VERSION`, `OpenApiBackupRepository`, `OpenApiLocalRepository`, `CollectionFileTxFactory`, `StatusEvent`, `transition`, `ContractRepository`, `Template`, `TemplateRepository` | open |
| S-120f | rocket-audit | about 22 | `actor.rs:17`, `chain.rs:16`, `control.rs:5,14,20,102`, `event.rs:9,13,57,74,88`, `profile.rs:8,19,27,51`, `publisher.rs:9,32,36`, `repository.rs:5` | open |
| S-120g | rocket-export, rocket-flow | about 20, 2 | `ports.rs`, `report.rs`, `tree.rs`, `scripts.rs` fields and variants; `node.rs:101-102` | open |
| S-120h | rocket-http | about 13 | `Cookie`, `CookieRepository`, `HttpResponse`, `RequestOptions`, `SpecFetcher`, `PhaseKind`, `LoadTestPhase`, `SuccessRule` | open |
| S-120i | rocket-git | about 19 types | DTO files: `commit`, `conflict`, `credentials`, `status`, `diff` | open |
| S-120j | rocket-import | about 40 | `error.rs`, `report.rs`, `openapi/model.rs`, `sync/apply_types.rs`, `bru/yml_adapter.rs` | open |
| S-120k | rocket-infra | about 180 | `scripting/ops/req.rs` (17), `ops/mod.rs` (8), `callback_server.rs`, `clone_destination_capabilities.rs`, `fs_*_repo.rs`, `cookie_store.rs`, `reqwest_executor.rs`, `contract_records/*`; skip `lib.rs` re-exports | open |
| S-120l | rocket-app | about 270 | `git_service.rs` (35), `collection_service.rs` (27), `workspace_service.rs` (19), `conversation_service.rs`, `agent_config_service.rs`, `execution_service.rs`, `oauth2_service.rs`, `graphql_subscription/protocol.rs`, plus the 6 small services | open |
| S-120m | src-tauri | about 350 | 187 of 269 `#[tauri::command]` functions (one line each), `commands/mod.rs` gets one module doc, then `tool_server.rs`, `scratch.rs`, `cleanup.rs`, `run()` (with a `# Panics` note) | open |

Delegation: haiku can draft docs for self-explanatory items (constructors, plain structs). Use sonnet for traits, services and anything with an invariant. A review pass checks the wording.

## Phase 7: keep it fixed

| ID | Item | Status | Notes |
|---|---|---|---|
| S-121 | `#![warn(missing_docs)]` per crate after its Phase 6 item is done | open | Per crate only. Never workspace-wide. |
| S-122 | Add clippy lints in a `[lints]` table: `clippy::unwrap_used` outside tests, `clippy::ptr_arg`, `clippy::wildcard_imports`, `clippy::format_push_string`, `clippy::new_without_default` | open | Start with the cleanest crates, then widen. Check what the harness hooks expect first. |
| S-123 | Add a short "Rust style" section to `.claude/rules/rust-ddd-boundaries.md` that links here and records the Phase 0 rulings | open | Do not copy the guide. |
| S-124 | Re-run the metrics tool and update the crate table | open | After each phase. |

## Phase 8: nits (optional, last)

| ID | Item | Where | Status |
|---|---|---|---|
| S-130 | `#[must_use]` on pure public functions | `rocket-flow` (about 10), `rocket-export`, `rocket-history` builders, `rocket-git` (`ahead_behind`, `map_git2_status`, `mask_examples`), `rocket-import` (`convert*`, `fingerprint`, `item_digest`) | open |
| S-131 | Missing derives: `NoLintContext`, `HistoryEntry`/`HistoryFilter` `PartialEq`, `SchedulePlan`, `StatusEvent`, `ChainVerification` `Clone`, the null/test doubles in `rocket-environment`, about 30 `Fs*Repo` structs (`Debug`), `ServerEndpoint` (manual `Debug`), `TauriEventBus` (`Clone`) | open |
| S-132 | `Default` where clippy asks: `fs_file_tx.rs:132`, `websocket_client.rs:43`, `reqwest_executor.rs:751` | open |
| S-133 | Merge split `pub use` lines | `rocket-grpc/src/lib.rs:14-15`, `rocket-flow/src/lib.rs` | open |
| S-134 | `RwLock` instead of `Mutex` for `active_workspace_path` (reads dominate) | `src-tauri/src/lib.rs:277,917` | open |
| S-135 | Check whether the `Arc` is needed | `rocket-infra/src/grpc/proto_reader.rs:79` | open |

## Order of work

1. Phase 0 decisions (one sitting).
2. Phase 1 safety. Start with S-31 (log redaction) and S-20 to S-22 (production unwrap calls), then the `Debug` fixes.
3. Phase 2 imports, then Phase 3 signatures, one crate per plan, smallest crate first.
4. Phase 4 clones, only after Phase 3 in the same crate (S-85 depends on S-52).
5. Phase 5 structure, one plan each. `S-100` is the largest and needs its own design note.
6. Phase 6 docs, smallest crate first, enabling `missing_docs` per crate.
7. Phase 7 guardrails once the first crates are clean. Phase 8 last.

## Decisions log

- Ruling (2026-11-03): the Google guide applies except the items in "Rules we skip". Rocket's own hard rules in `CLAUDE.md` win on any conflict.
- Ruling (2026-11-03): the audit counts are from exact per-file measurement plus agent review. The first run of the counting tool over-counted test code and under-counted `execution_service.rs`. Both were fixed before the numbers above were used.
