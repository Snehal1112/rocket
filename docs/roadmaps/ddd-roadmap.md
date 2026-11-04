# DDD Architecture Roadmap

Goal: make the code match the architecture Rocket documents (`CLAUDE.md`, `.claude/rules/rust-ddd-boundaries.md`, `.claude/rules/tauri-ipc-boundaries.md`, the crate `CLAUDE.md` files).
Source: a DDD audit on 2026-11-03 of all 17 crates, run as five sub-audits (app-layer I/O, domain-crate purity, domain model richness, ports and command layer, bounded contexts). The main findings were spot-checked against the source.
Related: [rust-style-roadmap.md](rust-style-roadmap.md) (code style), R-55 in [roadmap.md](roadmap.md) (events and IPC rework, parked).

## How to maintain this file

- IDs are stable (`D-nn`). Never renumber. Add new items at the end of their phase.
- Status: `open`, `planned`, `in-progress`, `done`, `dropped`. When an item ships, set `done` and note the commit.
- Plans derived from this file hold at most 3 tasks each, with an index file. Structure moves (Phase 6 and 7) need a spec first.
- Verify with `cargo check -j4 -p <crate>`, `cargo clippy -j4 -p <crate>`, `cargo test -j4 -p <crate>` and `bun run tsc --noEmit` when an IPC DTO changes. Never use `--workspace`. The Tauri crate is `-p rocket`.
- Tasks that touch `.yml`, collection or environment models, auth or variable resolution start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Verdict

**Rocket follows DDD at the level of dependencies and ports. It does not follow it at the level of the domain model and context boundaries.**

| Area | Result | Evidence |
|---|---|---|
| Dependency direction | CLEAN | Domain crates depend only on `rocket-shared` (plus a few sibling domain crates). No cycles. `rocket-infra` is a dev-dependency of `rocket-app` and `rocket-import`, never a normal one. |
| Ports and adapters | MOSTLY | About 40 repository and service traits are defined in domain crates and implemented in `rocket-infra`. `rocket-infra` defines none. `rocket-app` takes `Box<dyn ..>` and `Arc<dyn ..>`. |
| Composition root | CLEAN | `src-tauri/src/lib.rs` wires infra into the app (but see D-60: it is one 1,158-line function). |
| Events through a port | CLEAN | `EventPublisher` in `rocket-shared`, only `TauriEventBus` emits, no crate under `crates/` depends on `tauri`. |
| Domain crate purity | MOSTLY CLEAN | 11 of 14 domain crates have no I/O. The leaks are in `rocket-import`, `rocket-http`, `rocket-git`, and two minor ones in `rocket-shared` and `rocket-collection`. |
| App layer purity | VIOLATED (mildly) | `rocket-app` does about 12 filesystem operations and one direct `reqwest` call in production code. |
| Command layer | MOSTLY THIN | About 75% of 269 commands only delegate. About 12 commands carry real logic or build repositories. |
| Domain model | MIXED, leans anemic | Entities are public-field structs. Rules are free functions that callers can skip. Several rules live in `rocket-app`. |
| Bounded contexts | WEAK | `rocket-shared` is 62% context-specific code, `DomainEvent` is one global enum with 81 variants, and many context rules live in `rocket-app`. |
| Documentation vs code | STALE | `rocket-http`, `rocket-git`, `rocket-import` and `rocket-app` `CLAUDE.md` files claim a purity the code does not have. |

## Phase 0: decisions (no code)

| ID | Decision | Status | Recommendation |
|---|---|---|---|
| D-01 | Which crates are "mixed" on purpose? `rocket-git` (port and libgit2 adapter), `rocket-http` (types plus load-test harness), `rocket-import` (importers that read and write files). | done (2026-11-04, see `docs/engineering/ddd-decisions.md`) | Accept `rocket-git` as mixed (already documented). For `rocket-import` and `rocket-http`, decide between "move the I/O behind ports" (D-30, D-31) and "document them as mixed". Move the I/O if the importers are to be unit-testable without a disk. |
| D-02 | Is `rocket-app` allowed to be big (about 33k non-test lines), or should contexts become their own crates? | done (2026-11-04, see `docs/engineering/ddd-decisions.md`) | Split by context over time (D-80 to D-82), starting with OpenAPI sync and flow runtime. Do not start before the Phase 3 and 4 cleanups. |
| D-03 | Events: one global `DomainEvent` or one enum per context? | open | This is R-55, parked. Per-context enums behind the same wire format, with the contract tests in `rocket-shared/src/events.rs` kept as the guard. Needs its own spec. |
| D-04 | Rich domain model: private fields and validating constructors, or keep public fields plus validators? | done (2026-11-04, see `docs/engineering/ddd-decisions.md`) | Start narrow: add aggregate methods where callers already bypass rules (`WorkspaceRegistry`, `Flow`). Do not convert every struct. |
| D-05 | Name of the assistant context: `rocket-acp` or `rocket-assistant`? | done (2026-11-04, see `docs/engineering/ddd-decisions.md`) | Pick one name for the crate, events (`Acp*` and `Assistant*`) and services. |

## Phase 1: correctness bug found by the audit

| ID | Item | Where | Status |
|---|---|---|---|
| D-10 | `WorkspaceService::create` creates the workspace directories and writes `workspace.yml` before it checks for a duplicate name. A duplicate name returns `AlreadyExists` but leaves orphan files. Reorder: load the registry and check `name_exists` first, then create the directories and config. Add a test that a duplicate create leaves the disk untouched. | `rocket-app/src/workspace_service.rs:101-109` | done (2026-11-04, duplicate name is checked before any disk write) |
| D-11 | Same pattern check for the other `registry.workspaces.push` site (`link external collection`): check before side effects. | `rocket-app/src/workspace_service.rs:277-289` (`open_workspace`) | done (2026-11-04, audited, no bug; pinned by tests) |
| D-12 | `apply_transition` maps `InvalidTransition` to `ContractError::Internal` and drops the event name, so the message is misleading. Keep the event in the error. | `rocket-app/src/contract_service.rs:942` | done (2026-11-04, event name kept in the error; `StatusEvent` derives `Debug`) |

## Phase 2: make the docs true

Cheap, and it stops agents and people from trusting claims the code breaks. Do this before moving code, then update again as items below ship.

| ID | Item | Where | Status |
|---|---|---|---|
| D-20 | `rocket-http` said "no I/O implementations" while it had a live HTTP POST and a `reqwest::Client` in trait signatures. Text corrected; D-50 removed both. | `crates/rocket-http/CLAUDE.md:7` | done (2026-11-04) |
| D-21 | `rocket-git` says "pure domain crate, no I/O". `Git2Service` does filesystem, process and network work. Say "port plus libgit2 adapter". | `crates/rocket-git/CLAUDE.md:23` | done (2026-11-04, text restored when its fix item ships) |
| D-22 | `rocket-import` says it delegates all disk writes to `rocket-infra`. It writes with `std::fs` itself and `rocket-infra` is a dev-dependency only. | `crates/rocket-import/CLAUDE.md:7` | done (2026-11-04, text restored when its fix item ships) |
| D-23 | `rocket-app` says it never touches the filesystem or I/O. Document the exceptions until D-40 to D-44 ship, and that the crate is large. | `crates/rocket-app/CLAUDE.md` | done (2026-11-04, exceptions list removed when D-45 shipped, the crate has no direct I/O) |
| D-24 | `rocket-collection` doc says "no filesystem access" but `openapi_consent.rs:49` calls `canonicalize`. Fix after D-34. | `crates/rocket-collection/CLAUDE.md` | done (2026-11-04, text restored when its fix item ships) |

## Phase 3: command layer back to thin

The rule: commands validate, call a service and map the result. Each item moves logic into a `rocket-app` service (or an infra adapter behind a port) and leaves the command as glue. Update `src-tauri/app-commands.txt` only if a command name changes (it should not).

| ID | Item | Where | Status |
|---|---|---|---|
| D-30 | Delete the hand-rolled `TcpListener` and HTTP request parser. Use the existing `CallbackListener` port and `HyperCallbackListener` adapter, as the other OAuth path already does. **Finding (2026-11-04): the premise is wrong.** Only Flow "Wait for callback" uses that port. It serves `/cb/<token>` on a LAN or chosen host and returns JSON, but OAuth needs the exact path `http://localhost:{port}/callback` and an HTML reply, and no other OAuth path uses it. Needs a decision: a new loopback auth-code port, or extend `HyperCallbackListener`. | `src-tauri/src/commands/oauth2.rs:590-680` | done (2026-11-04, new loopback port; `LoopbackCallbackServer` in `rocket-infra`; manual checks open) |
| D-31 | Move the OAuth auth-code flow (PKCE, state check, token exchange, URL building) out of the command into `OAuth2Service`. Remove the deprecated `oauth2_auth_code_flow` if nothing calls it. | `commands/oauth2.rs:37-175,343-490` | done (2026-11-04, `OAuth2Service::authorize_with_code`; `oauth2_auth_code_flow` removed; manual checks open) |
| D-32 | Stop building `FsEnvironmentRepo` and `EnvironmentService` on every call. Build them once in the composition root and manage them as state. Move the collection-name path checks into the service. | `commands/environments.rs:23-50,158-170` | done (2026-11-04, `EnvironmentServices` in `rocket-app` owns the name check; wiring built once, repos still follow the active workspace; manual checks open) |
| D-33 | Same for the importer and OpenAPI repos: manage one factory in the composition root. Move `FsEnvFactory` out of the command module. Remove the second `SpecFetcher` implementation in `commands/import.rs`. | `commands/import.rs:18-35,160-230` | done (2026-11-04, `ImportServiceFactory` and `FsEnvFactory` in `src-tauri/src/import_wiring.rs`; the second `SpecFetcher` was a test stub, nothing to remove; manual checks open) |
| D-34 | `CollectionExportService` is built in each command; build it once. | `commands/export.rs:50,84` | done (2026-11-04, one managed `Arc<CollectionExportService>`; manual checks open) |
| D-35 | Move the `Contract` assembly (date parsing, status choice, ids, counters) from `attach_contract` and `update_contract` into `ContractService` or a mapper. | `commands/contract.rs:42-100` | done (2026-11-04, `NewContract`, `ContractEdit`, `attach_new`, `edit_contract` in `ContractService`; manual checks open) |
| D-36 | Move the contract audit hook and snapshot-path rebuilding out of `save_request`. Also fix the lock on the workspace mutex (see style roadmap S-22). | `commands/collections.rs:239-285` | done (2026-11-04, `ContractService::audit_saved_request` and `snapshot_path`; the panicking lock is gone; manual checks open) |
| D-37 | Directory scanning (`scan_collections_in_path`, `detect_cloned_structure`) is filesystem work in the command. Move it to a service backed by a port. | `commands/collections.rs:419-440,530-550` | done (2026-11-04, `CollectionDiscovery` and `DirectoryProbe` in `rocket-collection`, `FsDirectoryProbe` in `rocket-infra`; manual checks open) |
| D-38 | `list_ssh_key_paths` reads `~/.ssh`; `git_get_identity` and `git_set_identity` call `git2::Repository::open` directly and bypass `GitService`. Add the operations to `GitService`. | `commands/git.rs:896-966,1049-1075` | done (2026-11-04, identity and SSH key operations on `GitService`; manual checks open) |
| D-39 | `ui_state.rs` reads and writes files and YAML itself; `workspaces.rs` `restart_watcher` creates directories and publishes an event. Move both behind services. | `commands/ui_state.rs:45-58`, `commands/workspaces.rs:105-130` | done (2026-11-04, `UiStateRepository` with `FsUiStateRepo`; `FileWatcher` port with `WorkspaceWatcher`; manual checks open) |
| D-3A | `HttpExecutor` is implemented inside the MCP tool server (`src-tauri/src/mcp/tool_server.rs`), so an HTTP client is built outside `rocket-infra`. Inject the existing executor instead. | `src-tauri/src/mcp/tool_server.rs` | dropped (2026-11-04, premise wrong: the only `HttpExecutor` in `tool_server.rs` is a test stub inside `#[cfg(test)]`) |

Plans: D-30 and D-31 as one plan (OAuth), D-32 to D-34 as one, D-35 to D-37 as one, D-38 to D-3A as one.

## Phase 4: `rocket-app` back to no I/O

Each item replaces a direct `std::fs` or `reqwest` call with an existing or new port. The fixes keep behavior and add a port test double so the service becomes testable without a disk.

| ID | Item | Where | Status |
|---|---|---|---|
| D-40 | Contract attachments: `create_dir_all`, `metadata`, `copy`, `exists`, `read_dir`, `remove_file` (six sites). Add an attachment store port and an `rocket-infra` adapter. | `rocket-app/src/contract_service.rs:54,74,88,262,263,271` | done (2026-11-04) |
| D-41 | Workspace directory checks and `remove_dir_all`. Move behind the existing `WorkspaceRepository` or a new method on it. | `rocket-app/src/workspace_service.rs:267,329,378-379` | done (2026-11-04) |
| D-42 | `fs::canonicalize` for the per-collection lock key. Resolve the key in an infra helper or accept a pre-resolved path. | `rocket-app/src/collection_lock.rs:84` | done (2026-11-04) |
| D-43 | `fs::metadata` of the proto file for the cache key. Move to the `ProtoLoader` port. | `rocket-app/src/grpc_service.rs:475` | done (2026-11-04) |
| D-44 | `reqwest` client build (fallback) and token POST inside `OAuth2Service`. Require a `TokenClientProvider` and remove the fallback; move the POST behind the port. | `rocket-app/src/oauth2_service.rs:226-242,420-431` | done (2026-11-04) |
| D-45 | Environment reads: process env snapshot, `USER`/`USERNAME`, `current_exe`. Inject an `EnvReader` (the agent config service already does). | `execution_service.rs:1413,2674`, `contract_service.rs:349-350`, `acp_session_service.rs:707` | done (2026-11-04) |

## Phase 5: domain crates back to pure

| ID | Item | Where | Status |
|---|---|---|---|
| D-50 | `acquire_token` made a live HTTP POST from a domain crate, and `reqwest::Client` appeared in the `TokenClientProvider` trait signature. Done on 2026-11-04: `TokenClientProvider` has one method, `post_form`, with plain types; the POST and the client build moved into `ReqwestTokenClientProvider` in `rocket-infra`; the unused `acquire_token` was deleted. The OAuth config and token types stay in the domain. | `rocket-http/src/token_client.rs`, `rocket-infra/src/reqwest_executor.rs` | done |
| D-51 | `rocket-import` reads and writes the disk itself. Introduce a small file port implemented in `rocket-infra`, as `rocket-export` does for `CollectionFileSource`. Done on 2026-11-04: the `ImportFiles` port (`rocket-import/src/files.rs`) and `FsImportFiles`, with the writers, scanners, Bruno, Postman, WSDL and OpenAPI readers and the zip extractor all behind it. Left: the `exists()` and `is_dir()` path predicates, which also touch the disk and follow symlinks. | `rocket-import/src/` | done, predicates left |
| D-52 | `fs::canonicalize` in a domain function. Done on 2026-11-04: `consent_root_key` takes the path as given, and `FsOpenApiConsentRepo` canonicalizes before it calls it. | `rocket-collection/src/openapi_consent.rs`, `rocket-infra/src/fs_openapi_consent_repo.rs` | done |
| D-53 | `dirs::home_dir` in the shared crate. Done on 2026-11-04: `rocket-shared` lost the `dirs` dependency and keeps only the pure `expand_tilde_with`. `HostEnv` gained `home_dir()`, `rocket_infra::paths` has `home_dir()` and `expand_tilde()`, `GrpcService` takes a host env, and `rocket-git` (mixed) has its own `home.rs`. | `rocket-shared/src/paths.rs`, `rocket-infra/src/paths.rs` | done |
| D-54 | Duplicate `EnvironmentRepositoryFactory` trait in `rocket-import`. Done on 2026-11-04: `rocket-import` uses the one in `rocket-environment` (`for_collection`). | `rocket-import/src/importer.rs` | done |
| D-55 | `rocket-git` is accepted as mixed (D-01). Done on 2026-11-04: the `git2_service` module is private, `GitService` and the `lib.rs` re-exports are the only way in, and `crates/rocket-git/CLAUDE.md` says so. | `rocket-git/src/lib.rs` | done |

## Phase 6: richer domain model

Move rules to the aggregate that owns them, so callers cannot skip them. Each item keeps the existing behavior and adds tests in the domain crate first. Do these after D-04 is decided.

| ID | Item | Where (from, to) | Status |
|---|---|---|---|
| D-60 | `WorkspaceRegistry::add`, `rename` and a path-already-open check returning `AlreadyExists`. Done on 2026-11-04 (`ensure_can_add`, `add`, `rename`); `create`, `open_workspace` and `rename` in the service use them. `create` now also refuses a path that is already registered. `close`, `delete` and `switch` still edit the registry directly. | `rocket-workspace/src/workspace.rs`, `rocket-app/src/workspace_service.rs` | done |
| D-61 | Flow node fate and edge liveness (`is_live`, `decide_fate`, `NodeOutcome`, `NodeFate`). Done on 2026-11-04: `flow_routing.rs` moved to `rocket-flow/src/routing.rs` with its tests; `rocket-app` calls it from `flow_execution_service`, `flow_partial` and `flow_run_cache`. | `rocket-flow/src/routing.rs` | done |
| D-62 | Flow rename rules and a validity guard. Done on 2026-11-04: `Flow::ensure_valid` and `Flow::checked_rename`, with the save-error text in `rocket-flow/src/save_error.rs`; `FlowService::save` and `rename` use them. `Flow` fields stay public and `FlowRepository::save` still takes any `&Flow`, so a repository caller can still skip the guard. | `rocket-flow/src/flow.rs`, `rocket-app/src/flow_service.rs` | done (guard is by method, not by type) |
| D-63 | One `effective_auth(request, folders, collection)` in `rocket-collection`. Done on 2026-11-04 in `folder_settings.rs`; `merge_auth` is gone from `execution_service.rs`, whose send path and tests call `effective_auth`. | `rocket-collection/src/folder_settings.rs` | done |
| D-64 | Variable scope-to-layer population as `VariableContext::add_scope`. Done on 2026-11-04 (`Scope`, `ScopeEntry`, `MIN_SECRET_VALUE_LEN`); `scopes_inner` in `execution_service.rs` calls it for the global environment, collection, environment, folder and request scopes, and `redaction::MIN_REDACTION_LEN` is the same constant. | `rocket-environment/src/context.rs` | done |
| D-65 | Contract scope matching (`covers`) and `bump_patch_version`. Done on 2026-11-04: `ContractScope::covers` in `contract/types.rs` and `bump_patch_version` in `contract/version.rs`; `ContractService` calls them. One fix: a patch of `u32::MAX` takes the `-copy` fallback instead of overflowing. | `rocket-app/src/contract_service.rs` to `rocket-collection/src/contract/` | done |
| D-66 | Duplicate-request naming scheme (copy suffixes, uid and seq reset). The retry loop stays in the service. Done on 2026-11-04: `duplicate.rs` (`duplicate_candidate`, `duplicate_of`, `MAX_DUPLICATE_ATTEMPTS`); `duplicate_request_unlocked` calls them. | `rocket-app/src/collection_service.rs` to `rocket-collection` | done |
| D-67 | Validating newtypes where a bad value has already caused bugs or checks. Done on 2026-11-04 for two values: `CollectionName` (`names.rs`, used by `FsCollectionFileSource`) and `RelPath` (`rel_path.rs`, used by `FsFileTx` and the OpenAPI sync path check). Request names have no rule today, so they get no type. `Collection::validate_name` and `validate_rel_path` keep their rules; the types are not stored in structs, so no file or wire format changed. | `rocket-collection` | done |

## Phase 7: shared kernel and events

Shrink `rocket-shared` to the real kernel (about 1,300 lines: errors, request vocabulary, paths, validation, description, action, assertion, variable value, run source).

| ID | Item | Where | Status |
|---|---|---|---|
| D-70 | Delete the dead module `proxy.rs` (nothing imports it; the live types are in `rocket-http/src/proxy.rs`). Done on 2026-11-04. | `rocket-shared/src/proxy.rs` | done |
| D-71 | Move `acp.rs` to `rocket-acp`, `grpc.rs` to `rocket-grpc`, `load_test.rs` to `rocket-http`. Blocked by D-74 (checked 2026-11-04): `DomainEvent` has fields of these types (`ConfigOption`, `PermissionOption`, `GrpcMetadataPair`, `LoadTestProgressEvent`, `LoadTestResult`), so moving them now would make `rocket-shared` depend on `rocket-acp`, `rocket-grpc` and `rocket-http`. | `rocket-shared/src/{acp,grpc,load_test}.rs` | blocked by D-74 |
| D-72 | Unify the two OAuth2 models (`rocket-shared/oauth2.rs` persisted config, `rocket-http/oauth2.rs` runtime config and token). Blocked (checked 2026-11-04): `rocket_shared::types::Auth::OAuth2(Box<OAuth2Flow>)` holds the persisted model, so it cannot leave shared unless `Auth` moves too. Needs its own decision. | `rocket-shared/src/oauth2.rs`, `rocket-http/src/oauth2.rs` | open, needs a decision |
| D-73 | Move `certificate.rs` out of shared. Done on 2026-11-04: `ClientCertificate` and `VaultCertificateFormat` now live in `rocket-environment/src/certificate.rs`; `rocket-http` depends on `rocket-environment` for `VaultCertificateFormat`. Serde shape unchanged. | `rocket-shared/src/certificate.rs` | done |
| D-74 | Split `DomainEvent` (2,522-line file, 81 variants) into per-context enums in the owning crates, with a thin envelope and publisher port in shared. Keep the wire format byte-identical. Needs a spec (R-55). D-71 waits for it. | `rocket-shared/src/events.rs` | open |
| D-75 | `rocket-http/src/load_test.rs` publishes into the global enum directly. Inject a smaller progress port instead. Not blocked by D-74 (checked 2026-11-04): `rocket-http` can define a progress trait and `rocket-app` can adapt it to the existing `DomainEvent` variants; only moving `load_test.rs` (D-71) waits for D-74. | `rocket-http/src/load_test.rs:9` | open |

## Phase 8: contexts and crate structure

Larger moves, each with its own spec. Do not start before Phases 3, 4 and 6 are done for the same area.

| ID | Item | Where | Status |
|---|---|---|---|
| D-80 | Extract the flow runtime (13 `flow_*.rs` files, about 25k lines with tests) from `rocket-app` into `rocket-flow` or a runtime crate. Keep the service as thin wiring. | `rocket-app/src/flow_*.rs` | open |
| D-81 | Give OpenAPI sync a home: about 20 `openapi_sync_*` files and the rules (apply, skip, reset, consent, scheduler) are application code today. Move the rules to `rocket-collection` or a new `rocket-openapi` crate. | `rocket-app/src/openapi_*` | open |
| D-82 | Same for contract, proposal and conversation services: the types are in `rocket-collection` and `rocket-acp`, the rules in `rocket-app`. | `contract_service.rs`, `proposal_service.rs`, `conversation_service.rs` | open |
| D-83 | Move `AssistantMode` out of `mcp_tool_service.rs:77`. (D-05 ruled no rename of `rocket-acp`.) | `rocket-acp`, `rocket-app/src/mcp_tool_service.rs` | open |
| D-84 | Move the script-facing cookie types (`CookieEntry`, `CookieInput`) from `rocket-http/src/script_cookie.rs` into `rocket-scripting`, so `rocket-scripting` no longer depends on `rocket-http` for them. | `rocket-http/src/script_cookie.rs`, `rocket-scripting/src/host.rs:9` | open |
| D-85 | `rocket-http` holds load testing, WebSocket and spec fetching beside HTTP. Consider `rocket-loadtest` and `rocket-realtime` crates once D-75 is done. | `rocket-http/src` | open |
| D-86 | Pure logic stuck in the app layer (`graphql_*`, `assertion_evaluator`, `collection_trust`) as domain code with no I/O. | `rocket-app/src` | open |
| D-87 | Unify naming for the same concept: run summaries (`RunSummary`, `FlowRunSummary`), variables (`Variable`, `CollectionVariable`, `VariableValue`, `EnvVarWrite`), key-value rows (`Header`, `KeyValueEntry`, `GrpcMetadataPair`). Pick one glossary and write it down, including acp (protocol) versus assistant (feature) from D-05. | across crates | open |
| D-88 | `Template` lives in `rocket-history` by name only; decide its home (history or collection). | `rocket-history/src/template.rs` | open |

## Phase 9: keep it true

| ID | Item | Status | Notes |
|---|---|---|---|
| D-90 | Add a dependency-direction check to the test or CI run: a script that parses each `Cargo.toml` `[dependencies]` (not dev-dependencies) and fails if a domain crate depends on `rocket-infra`, `rocket-app` or `tauri`, or if `rocket-app` depends on `rocket-infra`. | open | The audit found the rules hold today. This keeps them from drifting. |
| D-91 | Add a grep check to the harness hooks for `std::fs`, `reqwest` and `tokio::fs` in `rocket-app/src` outside tests, with an allowlist that shrinks as Phase 4 ships. | open | Check what the existing hooks in `.claude/harness/` cover first. |
| D-92 | Add a short "Layer rules and known exceptions" section to `.claude/rules/rust-ddd-boundaries.md` that links here and lists the open exceptions. | open | Do not copy the docs. |
| D-93 | Re-run the audit (the five questions in this file's header) after Phases 3, 4 and 5. | open | The audit method is in `docs/roadmaps/rust-style-roadmap.md` and the tool in `.claude/tools/`. |

## Order of work

1. Phase 1 (the workspace create bug), then Phase 2 (docs), then Phase 0 decisions.
2. Phase 3 (command layer) and Phase 4 (app I/O) in parallel by area. Both are low risk and make the code match its rules.
3. Phase 5 (domain purity), `rocket-import` first (D-51), then `rocket-http` (D-50).
4. Phase 6 (rich model), one aggregate at a time, tests first.
5. Phase 7 (shared kernel). D-70 and D-71 are cheap and can go early. D-74 waits for the R-55 spec.
6. Phase 8 (contexts). Each item needs a spec. Start with D-80 or D-81.
7. Phase 9 once the first phases are done.

Overlap with the style roadmap: S-100 (split `run()`), S-102 (split `tool_server.rs`), S-103 (move the OAuth callback server, same as D-30) and S-105 (split `git.rs`) are structural fixes that serve both. Do them once.

## Decisions log

- Ruling (2026-11-03): `rocket-app` has `rocket-infra` only as a dev-dependency. An early draft of this audit read the dev-dependency lines as normal dependencies and was corrected after checking the `Cargo.toml` sections.
- Ruling (2026-11-03): findings from this audit are from grep and agent review. Counts of leak sites are approximate (a hit is a call site, not a unique operation).
- Ruling (2026-11-04): D-01 `rocket-git` stays mixed; `rocket-import` and `rocket-http` get ports (D-50, D-51). D-02 split `rocket-app` by context later, after Phases 3 and 4, each split with a spec. D-04 narrow domain model (`WorkspaceRegistry`, `Flow` first). D-05 keep `rocket-acp`, no rename, glossary in D-87. D-03 is proposed (per-context enums, parked with R-55) and still open. Details in `docs/engineering/ddd-decisions.md`.
