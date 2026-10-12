# Workspace AI assistant: decisions and follow-ups

This page lists the decisions and follow-ups for the six workspace-assistant plans on branch `worktree-acp-mcp-tool-server`. The design is in `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md` and the plans are in `docs/superpowers/plans/workspace-ai-assistant/`. Verified on 2026-10-11 against HEAD (after S1/S2). Fixed items are left out.

## Decisions (rulings)

- Per-tab AI Assist was removed (Plan 05 Task 3), together with the old `start_agent_session` command, its TS wrapper and the per-tab session code. Why: the workspace assistant replaces it. Cost if wrong: the per-tab Scripts AI Assist cannot come back without reverting that commit.
- The UI takes the session id from the start command's return value, not from the `AcpSessionStarted` event. Why: the event is published before `track()`. Cost if wrong: a dead session can stay tracked until the next sweep when the UI races it.
- A queued prompt (`InvalidInput`) no longer publishes `AcpSessionFailed`: the backend returns the error and keeps the session. The UI treats any `agent-session-failed` event as fatal, and a rejected send is handled as a non-fatal turn failure. Cost if wrong: a real failure that arrives as a rejected send would leave a dead session looking alive.
- Assistant copy must say that only environment-supplied credentials are used (an empty `CLAUDE_CONFIG_DIR`). Why: the assistant does not read a stored Claude login. Cost if wrong: users expect a login that is not used.
- Chip-resource text is built by a backend IPC that reuses the masked read views, so masking lives in one place. The frontend only selects, dedupes and caps chips. Why: the frontend-only masking was weaker than the backend. Cost if wrong: the leak surface stays in the frontend.
- The scratch-dir owner check uses a `current_uid()` probe, not libc (libc is not a dependency). Cost: a small race in a hostile `TMPDIR`.
- The scratch-dir removal uses sync `remove_dir_all` on a tokio worker. Why: the directories are tiny. Cost: a short block on one worker.
- Raw request bodies and scripts are not masked in the read tools (accepted by the brief). Cost: a secret typed into a body reaches the agent unmasked.

## Open follow-ups

### Sessions and cleanup

- `current_uid()` reads the uid by path from a probe file. Use `file.metadata()` on the open handle instead (`src-tauri/src/agent_session/scratch.rs` ~223-233).
- The idle sleep restarts on every update during the drain phase (`crates/rocket-app/src/acp_session_service.rs` ~323).
- `tool_status_from_wire` falls back to `InProgress` while `category_to_wire` returns `None` (`crates/rocket-infra/src/acp_agent_client.rs` ~719 and ~789). Make them consistent.
- The startup sweep treats a scratch root with no pid marker as stale. There is no 60 s grace for a root that a concurrent instance is creating (`scratch.rs` ~115).
- The outline goes stale between session start and the first prompt (`src-tauri/src/commands/acp_sessions.rs` ~34-43).
- `start_workspace_session` duplicates the credential and env setup (`crates/rocket-app/src/acp_session_service.rs` ~201-262).
- rustfmt drift across the workspace. Run `cargo fmt` once. CI exists.

### Read tools and masking

- The history URL keeps percent-encoded secrets unmasked. `mask_url` masks by field name only (`crates/rocket-app/src/mcp_read_views.rs` ~294).
- `check_in_workspace` lists collections on every call (`crates/rocket-app/src/mcp_tool_service.rs` ~249-254).
- A cache entry can be reinserted after `forget_session`. `run_request` also does not re-check the session pin or the workspace after `execute().await` (`mcp_tool_service.rs` ~766 and ~826). A workspace switch during a long run can cache results under the old workspace.
- `run_request` scripts can persist env vars. This is a deferred threat-model note.
- No focused test checks that a `get_settings` failure makes `run_request` fail (`mcp_tool_service.rs` ~215).
- The `pre{{x}}` test should say that an embedded placeholder in a credential-named field (for example an `Authorization` header with `pre{{x}}`) stays masked by design (`is_reference_only` in `mcp_read_views.rs` ~323).

### Proposals

- A duplicate `CreateFolder` in one batch is not rejected. Validate the batch in `propose` (`crates/rocket-app/src/proposal_service.rs` ~134-200).
- The `ended` set grows without limit (`proposal_service.rs` ~303). Bound it or expire entries. Fix this together with the batch check above.
- `accept` still holds the proposal mutex across the apply (needed for exactly-once). Other sessions' `list` and `propose` calls wait behind a slow accept. IPC commands are async, so the UI thread is not blocked.
- `create_request_exclusive` uses `hard_link`, which fails on FAT, exFAT and some SMB mounts. There is no `create_new` fallback (`crates/rocket-infra/src/fs_collection/requests.rs` ~172).
- Fixture names `GatedEnvFactory`, `GatedEnvFactory2` and `GatedEnvRepoOwned` are confusing (`proposal_service.rs` ~1341-1351). The JoinError mapping is repeated in `crates/rocket-app/src/tool_server.rs` ~947 and ~969.
- `forget_session` and the proposal `clear_session` are separate calls. No test checks that both run on every end path.
- Folder auth edits are not offered to the assistant. This needs a security decision first.
- A header replaced by an `UpdateFolderSettings` patch loses its description.
- A folder patch replaces whole sections. There is no per-item merge.
- The folder diff's before side can show the value of a secret-flagged variable. Mask it as the request diff does.
- gRPC requests do not inherit folder settings.
- `apply` reads folder settings without the collection lock, so a manual save landing in that sub-millisecond gap is reverted. It needs compare-and-swap in `edit_folder_yml`. The same window exists for request and env-var applies.
- An unparseable `folder.yml` on propose gives the agent a generic error. The spec says it should name the folder.
- Re-proposing a secret-flagged folder variable silently makes it non-secret, and the diff before side drops the flag.
- The dirty flag can briefly carry over on a folder switch, which gives a spurious Accept block.
- No tool-server test covers a successful get_folder_settings read in Ask mode, or a folder.yml parse failure.
- Closing a Folder Settings tab with unsaved edits still does not warn.

### Dead code and small items

- Any JSON-RPC prompt error ends the session (`acp_session_service.rs` ~303). Plan 01 behaviour; the user restarts.

### Agent actions and UI wiring

- The Rust `UiState` struct (`src-tauri/src/commands/ui_state.rs`) lacks `sidebarWidth`, `isConsoleOpen` and `consoleHeight`. The frontend values are dropped on each save. This is pre-existing since 2026-05-01.

### Agent config

- `extra_env` is not implemented.
- Agent stderr is discarded, so a failed start shows no reason (`acp_agent_client.rs` ~279).
- There is no handshake timeout.
- Hardening ideas for the agent credential sources: when a config becomes `keychain` from another source (or is new), delete any old entry for its id first, so a failed best-effort delete cannot leave a stale one behind.
- Editing a vault agent, choosing Keychain and leaving the field blank saves an agent with no entry. It fails closed with a clear message at Test or session start. A fix should check that the source was not `keychain` when the dialog opened, not `!hasSavedCredential`, because a locked keychain reads as "not saved".
- The failure toast after a credential save shows the raw backend text. Every backend error on that path is value-free today. A fixed message would be defense in depth.
- Stored conversations do not scrub agent credentials from the env or keychain sources (same as vault values).
- The credential commands run the keychain call on a runtime thread (no `spawn_blocking`).
- A token typed into a prompt is stored.
- A failed best-effort keychain delete can leave a stale entry that a later keychain config with the same id would inherit.
- `has_local_credential` uses `is_some`, while resolve ignores entries that are blank after trim.
- Tests in `acp_session_service` use the real process env reader.

### Chips and vault

- The keyring read in the vault path is synchronous, so the 8 s timeout cannot interrupt it (`chips.rs` ~206).
- The composer shows a generic "Could not load" toast, not the backend vault message (`Composer.tsx` ~147).

### Panel UI

- `aria-valuemin` is fixed at 320.
- A tool line or proposal card that grows without a new message is not followed by the scroll.
- The ScriptsTab `aria-controls` change has no test.
- Test hygiene in `AssistantChatView.test.tsx`: `scrollIntoView` is overwritten and never restored.

### Composer and chips

- Prompt history keeps typed secrets in localStorage (`src/lib/assistant/prompt-history.ts`).
- `fontSize` is a literal `'13px'` in `prompt-editor-theme.ts`.
- The `variableContext` toggle tooltip has not had a visual check.

## Known limitations

- Stop right after Send can do nothing. Cancel can arrive before the prompt reaches the agent. The UI disables Stop until the first update, so the cost is one ignored click. A turn-started flag would fix it.
- A start still in flight at a webview reload can track after the `end_stale_assistant_sessions` sweep and survive until the next sweep. The sweep runs on each load.
- The no-replace move maps `NotFound` and `AlreadyExists` to Stale. A raw rename race error is still `Io`, so Accept ends Failed.
- Exclusive creates only catch exact-name collisions on case-sensitive Linux. Case and Unicode variants rely on the `path_exists` checks at propose and accept time.
- The assistant uses only environment-supplied credentials. It does not use a stored Claude login. This is deliberate.
- Cookies and others are unaffected.

## Multi-agent support (2026-10-12)

Design and verified Gemini facts: `.claude/acp-multi-agent-support.md`. Plans: `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md`.

Open follow-ups:

- Gemini Google login (OAuth): needs `oauth_creds.json` copied into the private home with mode 0600. Not built; needs a security review first.
- The Plan 2 real-key run showed Google accepts the `propose_changes` schema, so no Gemini-only flattened schema is needed.
- A failed or empty agent turn shows nothing in the UI (seen with Gemini). Rocket should show the stop reason or the error.
- Gemini advertises its models in the `models` field of `session/new` and changes model with `session/set_model`, not through `configOptions`. The composer model picker therefore shows nothing for Gemini until Rocket reads `models`.
- rmcp has two `sse_retry` settings (server config and `LocalSessionManager.session_config`). Both are off, because the priming event hides the tools from Gemini CLI. Keep both off when upgrading rmcp.
- `real_mcp_client_lists_and_calls_tools_over_http` fails at `run_request` because the collection run trust is not confirmed in its fixture.
- `GEMINI.md` and the first-prompt reminder are ordinary history in Gemini 0.63 and may be summarised in a very long session. Only `GEMINI_SYSTEM_MD` is compaction-proof, but it replaces Gemini's whole system prompt.
- The permission card plan (`docs/superpowers/plans/2026-10-12-acp-permission-card.md`) is still needed for `Generic` agents, whose permission requests Rocket denies today.
- Other agents, one plan each: Qwen, Codex (no single no-built-ins switch, no SSE), OpenCode (merges global config), Goose (developer built-in on by default), Copilot, Cursor (cannot remove built-ins).

## Reasoning and guidance (2026-10-12)

Plans 4 to 6 in `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md` cover thinking visibility, reasoning level and guidance. What is deliberately left as it is:

- Stored conversation messages stay capped at 16 KiB each. A docs proposal is applied from the proposal card, so only the saved chat copy is shortened.
- The agent still has no response history. `get_history` returns briefs only. A live response needs Agent mode, the collection's run switch and the local trust confirmation, or the user attaching the last response with `#`. The instructions now say so.
- `run_request` returns an 8 KB masked response body at most. Raising it needs a security review of the masking.
- Claude often sends empty thinking text on recent models, so the Reasoning block can stay empty for Claude. A summarized-thinking option in the adapter `_meta` is untested.
- Gemini returns its models in the older `models` field and no config options, so the composer shows no model picker for Gemini. The agent dialog's Gemini model field covers it.
- The model setting for Claude (the composer's Model picker) is still remembered per agent only.
