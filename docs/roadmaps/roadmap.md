# Rocket Roadmap

Master list of work that is planned but not yet implemented. Built from a multi-agent inventory on 2026-10-08.
Flow has its own detailed file: [flow-roadmap.md](flow-roadmap.md) (F-01 to F-56).

## How to maintain this file

- Items have stable IDs (`R-xx`). Never renumber. Add new items at the end of their section.
- Status: `open`, `planned`, `in-progress`, `unmerged`, `done`, `dropped`.
- When an item ships, set `done` and note the commit or PR. When dropped, keep the row with a reason.
- Most "implemented" calls in the inventory came from file and symbol presence, not behavior checks. Re-verify before relying on an evidence line.
- Plans derived from this file hold at most 3 tasks each, with an index file.
- Tasks that touch `.yml`, collection or environment models, auth or variable resolution start with:
  "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Related: `docs/roadmaps/plan-audit-2026-10-02.md`, `docs/features/rok-api-parity-notes.md`, `docs/features/contract-collaboration-enhancements.md`.

---

## 1. JavaScript API reference support (`rok.*` parity with Bruno)

Goal: every method on Bruno's JavaScript API Reference works as `rok.*`. No `bru` alias.
Notes: `docs/features/rok-api-parity-notes.md` (its status table is current: A, B and C are on main).

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-01 | Part A: sync, state-only gaps (variable reads and deletes, overlay, collection name, safe mode, cwd, test results) | done (on main) | spec `specs/2026-10-07-rok-js-api-parity-a-sync-design.md`, plans `plans/rok-parity-a/` | `getOauth2CredentialVar`, `resetOauth2Credential`, per-folder `__dirname`. |
| R-02 | Part B: async `sendRequest`, `runRequest`, `sleep` | done (on main; review fixes in plans/rok-parity-b-fixes/) | spec `specs/2026-10-07-rok-js-api-parity-b-async-design.md`, plans `plans/rok-parity-b/`, `plans/rok-parity-b-fixes/` | The follow-ups R-66 and R-67 at the end of this table. `req.onFail` stays a no-op. |
| R-03 | Part C: cookies and `jar()` | done (on main) | spec `specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`, plans `plans/rok-parity-c/` | Known limits: the snapshot URL is read when the script phase starts (a later `req.setUrl` is not seen); a `Set-Cookie` from `rok.sendRequest` or a nested `rok.runRequest` does not refresh the caller's snapshot (it is replaced only when a request-scoped write or a `jar()` call uses exactly the request URL string); a request with the jar off does not send script cookies; Safe-mode scripts see cookies of any domain; `maxAge` is derived. Manual check `docs/manual-tests/rok-cookies.md` is open. |
| R-04 | Part D: runner `iterationData` | open | notes only | Needs a runner data-file feature, then a spec. |
| R-05 | Part E: `rok.grpc.*` | open | notes only | Needs a spec. The gRPC dependency is now met on main. |
| R-66 | Re-resolve the outer request after the pre-request script | open (high priority) | spec "Known limits" in `specs/2026-10-07-rok-js-api-parity-b-async-design.md`; ruling 11 of `plans/rok-parity-b/00-plan-index.md` | Re-resolve the outer request after the pre-request script, so `await rok.runRequest('auth/login')` followed by a `{{token}}` header works. Today `begin_phases` resolves before scripts run, and nothing refreshes `PhaseState.var_ctx.env` after a script write. Until then the documented workaround is `req.setHeader`. Add a snippet showing it. Same area, another known limit from the spec: nested runs do not see the caller's unsaved env writes. |
| R-67 | Part B review minors | open | review of `plans/rok-parity-b/` | Collection load failure reported as "invalid request path" (`ExecutionScriptHost::run_request`, the collection load); doubled `rok.sendRequest` prefix in failure lines (the failure-line formatting in `ops/host.rs`); unsaved outer request gets 6 nested levels (`check_run_chain` in `run_request.rs`); `.YML` extension not stripped (`normalize_run_path` in `run_request.rs`); repeated response headers collapse and binary bodies arrive lossy (`bootstrap.js` `_hostResponse`); object runtime variables reach nested runs as JSON strings; `test()` typed `: void` in `rok-types.ts`; document that nested runs report `getExecutionMode()` as standalone; `runRequest` reloads the collection tree on every call. |
| R-69 | Cookie follow-ups (`rok.cookies`) | open | final review of `plans/rok-parity-c/`; `docs/features/rok-api-parity-notes.md` | Treat a blank `sameSite` as absent. Extra tests: IPv6 host, URL with userinfo, query and fragment, `__Host-` with an explicit path `/`, garbage `expires` in `to_entry`, `upsert` replacing a legacy empty-domain cookie, several jars matching one URL in one `remove`. Lock and concurrency tests are regression guards only (`cookie_store.rs`, `script_host_tests.rs`). Skip the per-script jar read when the script code does not mention `cookies`. A corrupt-jar YAML error text could echo a scalar. Jar-write results should refresh the snapshot for the request URL too. Narrow the slices in `rok-types.test.ts`. Run the manual checks in `docs/manual-tests/rok-cookies.md`. |

## 2. Scripting and sandbox

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-06 | JS script security: trust gate and static scanner | in-progress | trust gate done on local main (2026-10-11): `specs/2026-10-11-collection-trust-gate-design.md`, notes `docs/features/collection-trust.md`. Old draft `specs/2026-10-07-js-script-security-design.md` is superseded for the gate. | Static scanner is still open. Per-folder revoke of extra script folders is a follow-up. |
| R-07 | Configurable script timeout | open | script-limits spec follow-up | Setting plus UI. |
| R-08 | Env-var write confirmation dialog | open | env-var write audit spec section 4 | Optional follow-up. |
| R-09 | Secret-storage follow-ups: "re-save" nudge, `opencollection.yml` gap | open | secret-storage hardening spec section 7 | Decide and ticket. |
| R-10 | Prompt variables `{{prompt.VAR}}`, TypeScript scripts, npm libraries | open | runtime-assertions and SP3 specs | Deferred, no spec. |
| R-11 | Parameterised dynamic variables and `$` variable completion | open | dynamic-variables spec | Deferred. |
| R-68 | Check each redirect hop against the request guard when the policy is on (also covers `req.setUrl`); note the DNS-rebinding limit | open | `crates/rocket-infra/src/reqwest_executor.rs`, `crates/rocket-app/src/execution_service/script_host.rs` | `check_request_guard` checks only the first URL, so a `302 Location: http://169.254.169.254/` is followed unchecked, and DNS names that resolve to internal addresses (for example nip.io) also pass. Two options: a custom `redirect::Policy` that runs the guard per hop, or force `follow_redirects = false` for script-driven sends. Plan it together with sub-project C (cookies, R-03), which touches the same send options. |

## 3. Collection runner

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-12 | "Run folder" and "Run collection" sidebar entries | open | opens only from `TabBar.tsx:263` today | Add sidebar actions. |
| R-13 | Tag include and exclude filtering | open | runner design spec | Not used in `collection_runner_service.rs` yet. |
| R-14 | Assertions in the runner and in load tests | open | runtime-assertions spec (deferred) | Unverified in code. |
| R-15 | Scheduled and CI headless runs, persisted run history | open | runner spec non-goals | No spec. |

## 4. Unmerged and unfinished work

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-16 | ACP AI assist MCP tool server and the workspace assistant (six plans), plus the follow-up UI and agent config work | done (on local main, not pushed, 2026-10-11) | six plans fast-forwarded at `9666d1a1`; then History run-source badge and filter, agent activity dot and Console tool lines (`50578634`, `824f740d`, `e504bd10`), agent `extra_env`, masked stderr tail, 30 s handshake timeout, name validation (`7516a34d`, `32d3d01c`, `81238937`, `f6cc8d0e`). Plans `plans/acp-mcp-tool-server/`, `plans/workspace-ai-assistant/`. | Open hardening items in `docs/features/assistant-followups.md`. |
| R-17 | Assistant conversation history and resume (archive, then replay into a new session) | done (on local main, not pushed, 2026-10-11) | commits `a5fa5f5b` to `dc34d24a`; spec `specs/2026-10-11-assistant-conversation-history-design.md`, plans `plans/assistant-conversation-history/`, notes `docs/features/assistant-conversations.md` | Follow-ups: 0600 file mode, symlinked `assistant/` dirs and non-star `.gitignore`, expected workspace id on clear and delete, linked collections not scrubbed, bidi and zero-width stripping. |
| R-18 | ACP local credential sources: Rocket environment variable or OS keychain entry, next to RocketVault (one explicit source per config) | done (on local main, not pushed, 2026-10-11; final Opus review ready to merge) | commits `1cb5fea8` to `ae41bfce` (ids as on the branch), spec `specs/2026-10-11-agent-local-credential-design.md`, plans `plans/agent-local-credential/` | User to run the suites and try each source in the app. Follow-ups in `assistant-followups.md`: stale-entry hardening, vault-to-keychain blank field, toast fallback, test gaps. A token file and the agent's CLI login were refused on purpose (isolation). |
| R-19 | Claude PR review: merge-blocking gate and Claude-written release notes | open | `specs/2026-09-25-claude-pr-review-design.md` | Sub-projects 2 and 3. Also confirm the GitHub App and `CLAUDE_CODE_OAUTH_TOKEN` setup. |
| R-20 | RocketVault certificate source: confirm API contract | open | `docs/features/rocketvault-certificate-export-requirements.md` | Depends on an unpushed RocketVault v4.0.0 branch. |
| R-21 | Unpushed local main work (folder settings, JS script files, workspace assistant, trust gate, safety fixes, conversation history, folder tools) and the kept worktrees | open | local main at the merge of R-18 (see `git log`), worktree `acp-mcp-tool-server` kept | Run the full suites, then push when the user is ready. Folder-settings follow-ups are in `docs/features/folder-settings.md`. |

## 5. Contracts

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-22 | Model B warn and block enforcement | open | `rocket-collection/src/contract/types.rs:123-129` | Enum exists. No UI or enforcement. |
| R-23 | Stubbed `open_review` and `remind_reviewers`, and a `MarkBreaking` command | open | `ContractsTab.tsx`, `state_machine.rs:8,53` | Build commands and UI. |
| R-24 | Collaboration enhancements (git-only sharing, local first) | open | `docs/features/contract-collaboration-enhancements.md` | Ideas only. Needs a spec. |
| R-25 | Full legacy contract retirement (CD-006) | open | duplication remediation plan | Blocked on a product decision. |

## 6. Auth and import

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-26 | OAuth2: implicit grant, OIDC discovery, JWKS verification, multi-token management and persistence | open | `execute-request.ts:464`, `oauth2_service.rs:20` | Implicit is stubbed. |
| R-27 | GitHub issue #20: OAuth2 flows other than client_credentials silently skipped | open | issue #20 | Verify against R-26 first. |
| R-28 | GitHub issue #21: mTLS certificates saved but never loaded | done | issue #21 | Fixed on main by the environment client certificate series. Guarded by `mutual_tls_in_process_*` in rocket-infra. WebSocket and gRPC mTLS are listed as later plans in the issue #21 spec. |
| R-29 | GitHub issue #22: environment `extends` and `.env` have no effect | done (plans 1 and 2 of dotenv-default-and-picker, with R-61) | issue #22; `execution_service.rs` `process_env_for`, `EnvironmentDialog.tsx` (`useDotEnvStatus`, `pickDotEnvFile`), `commands/environments.rs` | None. Not run in the real app yet. |
| R-30 | Insomnia import | open | none | No spec. |
| R-69 | OpenAPI import: 3.0, 3.1 and Swagger 2.0 from a file or a link (GitHub links included), grouped by tag or path | in-progress | spec `specs/2026-10-25-openapi-import-and-sync-design.md`, plans `plans/2026-10-25-openapi-import-and-sync/` (P1 to P3 built on branch `openapi-import`, not merged) | Import works from a file or a link in the Import dialog. Open: manual check in the real app (private repos fail with a clear message until the token lands in P4). |
| R-70 | OpenAPI timed sync from a saved link (hour, day, 7 days, month), reviewed in a Spec sync tab, user edits kept | planned | same spec, plans P4 to P9 outlined in the plan index | Needs P3 first. Token in the keychain, scheduler, merge engine, apply with recovery journal, tab. |

## 7. Load test

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-31 | Real PDF export. The "PDF summary" option writes HTML under a `.pdf` name. | open | `export_service.rs:32`, `load-test-store.ts:328-346`, `commands/load_test.rs:43` | User-visible defect. Render a real PDF or rename the option. |
| R-32 | Test history and test-plan persistence | open | load-test spec follow-up | No spec. |

## 8. Quality, tickets and hygiene

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-33 | Dedup series | open | GitHub #46 to #91, tier trackers #89, #90, #91; report `docs/reports/component-duplication/` | 49 work packages. Runtime-neutral. |
| R-34 | Performance series | open | GitHub #92 to #104, tracker #104 | Start with the #92 measure-first baseline. #93 (HTTP client cache) and #94 (tree-load double parse) are the high-impact ones. |
| R-35 | Git integration audit backlog | open | `docs/reports/git-integration-review/` | Report says remediation is required before destructive or security workflows. |
| R-36 | GitHub #43: console shows secrets and tokens in plain text | open | issue #43 | Check against the Flow redaction items (F-02, F-16). |
| R-37 | GitHub #44: backend does not resolve `{{vars}}` in query params | open | issue #44 | |
| R-38 | GitHub #25: GraphQL, gRPC and WebSocket items invisible in the sidebar | open | issue #25 | Verify, protocol parity is on main and this may be fixed. |
| R-39 | AI assist chat bugs | dropped (obsolete, issues closed 2026-10-11 as not planned) | GitHub #33 to #36, all about the removed per-tab `AgentChatPanel` | None. |
| R-40 | `backend-log` event has no frontend listener | open | `tauri_tracing_layer.rs:118` | Add a Console listener. |
| R-41 | Raw `<button>` breaks the shadcn rule | open | `src/components/git/GitStashSection.tsx:244` | Replace with the shadcn Button. |
| R-42 | OpenCollection regression fixtures are orphaned | open | 2026-04-04 regression-prevention plan | Wire into a contract test or CI step. |
| R-43 | Variable popover returns null for folder and process sources | open | `VariablePopover.tsx:42` | |
| R-44 | Stale `.claude/worktrees/script-execution-limits` directory | open | not a registered worktree, no unique work | Delete after the user confirms. |

## 8b. Safety and correctness fixes (2026-10-10 to 2026-10-11)

Shipped on local main (not pushed). Details in `docs/features/collection-trust.md` and `docs/features/assistant-followups.md`.

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-45 | Path traversal check rejects parent-directory components | done (`d95afdfb`, `12b7fb50`) | `rocket-infra` `validate_path`, `Environment::validate_name` | None. |
| R-46 | Safe-mode scripts no longer see the host environment | done (`44fcaa97`) | `scripting/engine.rs` | None. |
| R-47 | Per-collection trust gate for Developer mode, extra script folders, agent runs and `{{process.env}}` | done (Task 1 to 3, `e3aa73e8` to `fc52bf12`) | `trust.yml` in app data, `CollectionTrustService` | Grandfather notice shipped. Per-folder revoke, History resend treated as scratch, notice has no host-env count. |
| R-48 | Secret flag persisted on collection, folder and request variables | done (`c7594099`, `dc9ff5b0`) | `rocket-infra` conversions | The value is still plain YAML. Variables saved earlier must be re-marked. |
| R-49 | Autosave and Save share one payload, dirty flag clears only for the saved snapshot | done (`ebc5addb`, `e85790c4`) | `request-save-mapper.ts`, `auto-save.ts` | Overlapping autosaves can land out of order. |
| R-50 | Global environment cache refreshed after save, delete, switch and workspace switch | done (`6d16bd60`, `113621b8`) | `environment-queries.ts` | Rename leaves a stale entry. |
| R-51 | Runtime variables fill the same request after the pre-request script (one level only) | done (`784d14ae` to `4e57cc05`) | `execution_service/runtime_fill.rs` | UI Runner tab carries no runtime between steps. A runtime var does not override a name the first pass filled. |
| R-52 | Cookie jar, templates, global envs and OAuth2 repo follow the active workspace | done (`d1c9d291` to `12919577`) | `SharedPath*` repos, `send_pins.rs` | Cookies of a late response go to the new workspace. |
| R-53 | CI runs checks, Rust cache path fixed | done (`ff81fc0e`) | `.github/workflows` | Confirm it runs green on GitHub. |
| R-54 | Full test run and manual check after this batch | done (user ran the suites and tried the assistant, 2026-10-11, working as expected) | `cargo test -p rocket-app -j4`, `-p rocket-infra -j4`, `yarn test` | `cargo fmt --check` still flags workspace-wide drift. Run `cargo fmt` once. Later runs: Rust suites green (2026-10-11); a full `yarn test` flakes on 5000 ms timeouts under load, see R-57. |
| R-55 | Events and IPC rework: missing-event audit, frontend event-to-cache layer, composite backend commands, internal subscribers, activity surface | planned (parked by the user on 2026-10-11, no spec yet) | `docs/engineering/events-and-ipc-rework.md`, GitHub issue #105 | Pick the first sub-project (A or C1) and brainstorm it. Large. |

## 8d. Environment inheritance and `.env` files (2026-10-18)

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-61 | `extends` and `dotEnvFilePath` on environments: flatten along the parent chain, `.env` as `{{process.env.NAME}}` behind the host environment gate, dialog picker, path field and read-only inherited rows (collection and global) | done (factory job 024, plans 1 to 4) | `rocket-environment` `flatten.rs`, `dot_env.rs`; `EnvironmentDialog.tsx`, `WorkspaceEnvironmentsTab.tsx`; spec `specs/2026-10-18-env-extends-dotenv-design.md` | None. |
| R-62 | Inherit the parent's external secret bindings and client certificates | open | follow-up of R-61 | Needs a decision on binding alias clashes. |
| R-63 | Show inherited variables (secrets masked) in the assistant chips and MCP environment views; let backend-executed HTTP (flows, `rok.runRequest`, `run_request`) resolve `{{process.env}}` | open | follow-up of R-61 | Two separate pieces, spec each. |
| R-65 | Collection export follow-ups: lossless Postman round trip (importer reads `event` scripts, folder auth and variables, oauth2, file and graphql bodies), single-file OpenCollection YAML, Insomnia and Postman Data Export targets | open | follow-up of the collection export (plans 1 to 3); `docs/features/collection-export.md`; spec `docs/superpowers/specs/2026-10-23-collection-export-design.md` | Three separate specs. Run the manual checks in `docs/manual-tests/collection-export.md` first. |
| R-64 | Duplicate request through the backend `duplicate_request` command (fresh uid, source by full path, unique name in the folder, no overwrite) | done | `crates/rocket-app/src/collection_service.rs`; spec `docs/superpowers/specs/2026-10-19-duplicate-request-backend-design.md` | Three follow-ups from the spec: non-HTTP kinds (GraphQL, WebSocket, gRPC), ordering the copy next to the source, duplicate a folder. |

## 8c. Assistant additions (2026-10-11)

| ID | Item | Status | Where | What remains |
|---|---|---|---|---|
| R-56 | Assistant folder tools: `get_folder_settings` read tool and `UpdateFolderSettings` proposal (headers, variables, scripts, docs; never auth) | done (on local main, not pushed; `4b3bb1cd` to `dd639d7e`, final Opus review ready to merge) | spec `specs/2026-10-11-assistant-folder-tools-design.md`, plans `plans/assistant-folder-tools/`, notes in `docs/features/folder-settings.md` | Follow-ups in `assistant-followups.md`: lost-update window in apply, parse-failure error names no folder, secret flag lost on re-propose, brief dirty-flag carry-over on a folder switch, two missing tool-server tests. |
| R-57 | `yarn test` flakes on 5000 ms timeouts when the whole suite runs in parallel | open | a full run fails 14 to 16 different files each time; every file passes alone or with `--maxWorkers=2` | Raise `testTimeout` in the Vitest config or cap the workers. Small. |
| R-58 | Agent edits of folder auth | open | deliberately out of scope of R-56 | Needs the user's security decision first. |
| R-59 | Tidy-ups from the tool reviews: `cargo fmt` once over the workspace, `/kb-refresh rocketapi`, mark follow-up rows in `assistant-followups.md` | open | | Small, separate commits. |
| R-60 | Software factory v3 series (plans 01 to 08): Rust driver with `git2`, headless agents, checks before review, risk gate, budget, series, dashboard tokens, harness and verification | built on branch `factory-v3/plan-08-verification`; dry run pending (user) | spec `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`, plans `docs/superpowers/plans/2026-10-13-software-factory-v3/`, design doc `~/data/rocket/Archive/rocket-tools-factory/software-factory.md` | Run the dry run recipe in `~/data/rocket/Archive/rocket-tools-factory/software-factory.md`, compare `factory tokens` with the job 001 baseline, then merge. Open follow-ups are in its open items list. |

## 9. Probably dropped

Confirm with the user before marking `dropped`.

- Shipsmart SaaS spec and plan. A separate product, only docs were committed.
- Theme playground plans (vscode-2026-theme plans 02 and 03).
- `rocket-identity` crate (SP6).
- JSON-schema lint in CodeMirror, 3-way merge, GraphQL body language. Likely obsolete since multi-line editors now use Monaco.

## 10. Flow

See [flow-roadmap.md](flow-roadmap.md). Seven batches B1 to B7, 56 items.

## Decisions log

- Ruling (2026-10-08): protocol parity plans 01 to 13 (REST, SOAP, GraphQL, WebSocket, gRPC) are on main. The `worktree-protocol-parity-rest` branch no longer exists.
- Ruling (2026-10-08): this file is the single index. Detail lives in the linked feature files.
- Ruling (2026-10-11): collection trust gate is per collection and stored in the user's app data. A pull, clone, import or settings save never grants access. Existing collections were grandfathered once.
- Ruling (2026-10-11): the workspace assistant branch was fast-forwarded into local main. Nothing was pushed.
- Ruling (2026-10-11): the assistant can read and propose edits to folder settings, but never folder auth. Auth edits need a separate security decision (R-58).
- Ruling (2026-10-11): an agent config has one explicit credential source (RocketVault, Rocket environment variable or keychain). No fallback chain. A token file and the agent's own CLI login are refused to keep session isolation (R-18).
- Ruling (2026-10-11): conversation history, folder tools and the earlier assistant UI work were rebased onto main and fast-forwarded into local main at `72e03596`. R-18 is still on its branch. Nothing was pushed.
- Ruling (2026-10-11): R-18 (local agent credential sources) was fast-forwarded into local main on the user's request. Nothing was pushed.
- Ruling (2026-10-13): the factory v3 series closes with plan 08. `/verify-rocket` checks changed crates with `-j4` and no workspace command, a harness test lints the cargo commands in its doc and in `CLAUDE.md` through the real guard, and an end-to-end scenario test with a scripted runner covers the merge, resume, fix round, risk, sync and budget paths.
