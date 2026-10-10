# Workspace AI assistant: decisions and follow-ups

This page lists the decisions and follow-ups left after implementing the six workspace-assistant plans on branch `worktree-acp-mcp-tool-server`. The design is in `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md` and the plans are in `docs/superpowers/plans/workspace-ai-assistant/`. Items marked as fixed in the review ledgers are left out. Everything listed as open was still open at the end of the final review; items that the reviews confirmed as fixed are left out.

## Decisions (rulings)

- Per-tab AI Assist was removed (Plan 05 Task 3), together with the old `start_agent_session` command, its TS wrapper and the per-tab session code. Why: the workspace assistant replaces it. Cost if wrong: the per-tab Scripts AI Assist cannot come back without reverting that commit.
- The UI takes the session id from the start command's return value, not from the `AcpSessionStarted` event. Why: the event is published before `track()`. Cost if wrong: a dead session can stay tracked until the next sweep when the UI races it.
- `AcpSessionFailed` alone is not treated as session death, because a queued prompt (`InvalidInput`) also publishes it for a healthy session. Cost if wrong: the UI would tear down working sessions.
- Assistant copy must say that only environment-supplied credentials are used (an empty `CLAUDE_CONFIG_DIR`). Why: the assistant does not read a stored Claude login. Cost if wrong: users expect a login that is not used.
- Chip-resource text is built by a backend IPC that reuses the masked read views, so masking lives in one place. The frontend only selects, dedupes and caps chips. Why: the frontend-only masking was weaker than the backend. Cost if wrong: the leak surface stays in the frontend.
- The scratch-dir owner check uses a `current_uid()` probe, not libc (libc is not a dependency). Cost: a small race in a hostile `TMPDIR`.
- The scratch-dir removal uses sync `remove_dir_all` on a tokio worker. Why: the directories are tiny. Cost: a short block on one worker.
- Raw request bodies and scripts are not masked in the read tools (accepted by the brief). Cost: a secret typed into a body reaches the agent unmasked.

## Open follow-ups

### Sessions and cleanup

- `start_session` racing `end_all_sessions` can track a session after the drain. The client refuses new sessions after shutdown, so the session is orphaned until the next sweep.
- `current_uid()` reads the uid by path from a probe file. Use `file.metadata()` on the open handle instead (`scratch.rs`).
- The idle sleep restarts when the client reports a result during the drain phase. The message text now says "no update for Ns".
- `tool_status_from_wire` falls back to `InProgress`, while `category_to_wire` returns `None`. Make them consistent (Plan 01 Task 1).
- The startup sweep treats a scratch root with no pid marker as stale. A concurrent instance could be creating that root. Fix: treat an unmarked root younger than 60 s as live.
- The outline goes stale between session start and the first prompt.
- `start_workspace_session` duplicates the credential and env setup. There is no MockRuntime failure-path test for `start_workspace_assistant_inner`.
- The start path does not warn when `try_state` for `McpToolService` is `None`.
- No test checks that session start publishes no `AcpToolInvoked`.
- rustfmt width drift in `src-tauri/tests/acp_mcp_start_agent_session.rs` and `tool_server.rs`.

### Read tools and masking

- `get_folder_chain_settings` failure fails open (`unwrap_or_default`). Make it fail closed.
- The history URL keeps percent-encoded secrets unmasked.
- `check_in_workspace` lists collections on every call, and `list()` may auto-migrate data. Cache the result or document the migration.
- A cache entry can be reinserted after `forget_session`.
- `basic_header_values_from_secrets` clones the request. This is minor.
- `CapturedOutput` Debug output has not been checked for secrets.
- Missing tests: the D error arms and the folder chain at service level, a secret folder var, `pre{{x}}`, bearer auth, and the `test_doubles.rs` doc comment that sits on `set_request_variables`.
- rustfmt width drift in `crates/rocket-app/src/mcp_tool_service.rs` (import) and the `mcp_exec_svc` block in `lib.rs`.

### Proposals backend

- The store mutex is held across disk I/O and event publishing. A subscriber that calls back would deadlock. Release the lock before I/O.
- `CreateFolder` resets `folder.yml` if someone creates that folder by hand in a short window.
- A duplicate `CreateFolder` in one batch is not rejected.
- `run_request` scripts can persist env vars. This is a deferred spec threat-model note.
- The list, accept and reject IPC commands are sync, so they run on the main thread and do disk I/O under the proposal mutex. Make them async (`agent_proposals.rs`).
- `ProposalService.ended` grows by one id per session with no limit.
- `create_request_exclusive` uses `hard_link`, which fails on filesystems without hard links (exFAT, FAT, some SMB mounts), so Accept ends Failed there. Fall back to `create_new`. A crash between write and unlink leaves a hidden `.new-request.tmp.*` file.

### Dead code and small items

- `AcpSessionService::start_session` has no non-test callers (about 17 tests use it). Remove it with its autonomy gating and its `collection_repo` field once the tests are ported.
- Any JSON-RPC prompt error from the agent (for example a transient model overload) ends the session (`acp_agent_client.rs`). Plan 01 behaviour; the user restarts.
- A session with no workspace pin is not checked by `check_session_workspace`, which fails open for reads only. Fail closed.
- The no-replace move maps a vanished destination folder to Failed in some paths. Prefer Stale.

### Chips and vault masking

- `chips.rs` resolves vault secrets one environment after another under a single 8 s budget, so a collection with several vault-bound environments can time out every time. The chip is then refused, which is safe but unusable. Resolve in parallel or dedupe the bindings.
- `chips.rs` replaces a failed environment `list()` with an empty list, so only the named environment's vault secrets are resolved. A list error should refuse the chip.
- The keyring read in the vault path is synchronous, so the 8 s timeout cannot interrupt a hung keyring call.
- The composer shows a generic "Could not load" message, not the backend's vault message.

### Panel UI

- Scroll does not follow streamed text growth inside the last item.
- The panel width is not persisted (`assistantPanelWidth` is not in ui-state).
- The panel width is unbounded on a narrow window.
- `aria-controls` points at an element that is not mounted.
- Minor code cleanups: `randomUUID` is called inside `set` updaters, `currentStartToken` is module-level, there is an `as ToolActivityMessage` cast, and the disposal test does not cover overlapping mounts.

### Composer and chips

- Prompt history stores typed secrets in localStorage per workspace. Cap the entry length and clear it on workspace delete.
- Chip keys are ambiguous when a name contains `:`.
- The reference-tree query is not invalidated after changes.
- A triple-backtick fence in the composer is not handled.
- `variableContext` presence toggle recreates the view. The tooltip parent body needs a visual check, and `fontSize` is a literal.

## Known limitations

- Stop right after Send can do nothing. Cancel can arrive before the prompt reaches the agent. The UI disables Stop until the first update, so the cost is one ignored click. A turn-started flag would fix it.
- A start that is still in flight across a webview reload can orphan a session until the app exits or reloads.
- Exclusive creates only catch exact-name collisions on case-sensitive Linux. Case and Unicode variants rely on the `path_exists` checks at propose and accept time.
- The assistant uses only environment-supplied credentials. It does not use a stored Claude login.
