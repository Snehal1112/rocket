# ACP multi-agent support (Gemini first)

Status: plan, nothing implemented. Written 2026-10-12 from an investigation of Gemini CLI 0.63.0 (local binary, bundled source and docs, one live run against a fake model API) and a read of the Rocket ACP code.

## Why Claude works and Gemini does not

Rocket's isolation is Claude-only. It travels in `_meta.claudeCode.options` and `systemPrompt.append` (`crates/rocket-app/src/agent_isolation.rs:29-42`), plus `CLAUDE_CONFIG_DIR`. Gemini reads only `cwd` and `mcpServers` from `session/new`, so it keeps its built-in tools, loads the user's `~/.gemini`, never gets the Rocket instructions and never sees the `mcp__rocket__*` pre-allow. Its Rocket tool calls then trigger `session/request_permission`, which the deny-all handler cancels (`crates/rocket-infra/src/acp_agent_client.rs:338-352`).

## Design

A closed `AgentKind` enum (`Claude`, `Gemini`, `Generic`) on `AgentConfig`, with the presets in Rust. No trait. A pure `session_plan(kind, ctx)` function returns the per-session data (args, env, files, `_meta`, prompt prefix, tool names). I/O stays where it is today: scratch files in `src-tauri/src/agent_session/scratch.rs`, spawn in `rocket-infra`.

Every ACP agent surveyed (Claude, Codex, OpenCode, Gemini, Qwen, Goose, Kimi, Cursor, Copilot) differs on the same axes: spawn and auth, disabling built-ins, config isolation, instruction injection, tool names and pre-allow, MCP transport. A new agent is a new enum arm, not new machinery. A purely data-driven config was rejected: it cannot express credential copying, version flags, schema transforms or isolation checks, and it would make isolation-breaking flags editable.

## Verified Gemini facts (live run, CLI 0.63.0, fake model API)

- `initialize` advertises `mcpCapabilities {http, sse}`, so Rocket sends the HTTP spec with the Bearer header. This worked.
- The system settings file (`GEMINI_CLI_SYSTEM_SETTINGS_PATH`) is ignored unless the file and every parent directory is root-owned. Do not use it. Write the lockdown into the user settings file of a private home: `$GEMINI_CLI_HOME/.gemini/settings.json`.
- The MCP server is not started in an untrusted folder. Set `GEMINI_CLI_TRUST_WORKSPACE=true` for the scratch cwd.
- `tools.core: ["mcp_rocket_*"]` hides the built-ins but every MCP call is denied by policy. Use exact names instead, for example `["mcp_rocket_propose_changes", ...]`. Exact names also skip the permission prompt, so no permission request is sent.
- Gemini adds a `wait_for_previous` property to each tool schema and rewrites `["string","null"]` to `nullable`. `$ref`, `$defs`, `oneOf`, `const` and `additionalProperties` pass through unchanged. Real Google API acceptance of this schema is NOT tested (no key available).
- Tool names are `mcp_rocket_<tool>`. Auth by `GEMINI_API_KEY` works in an isolated home. OAuth needs `oauth_creds.json` copied in.

## Decision: Gemini does not need the permission card

The permission-card plan (`docs/superpowers/plans/2026-10-12-acp-permission-card.md`, 7 tasks, docs only on main) rejected auto-approve because a request has no reliable server signal. The live run shows a safer route for Gemini: Rocket writes the exact tool names into a private home's settings, so the allow rule is Rocket's own and no user config can inject a same-named tool. Do the Gemini preset first. Land the permission card afterwards for `Generic` agents and any agent without a pre-allow mechanism. Both rewrite the handler at `acp_agent_client.rs:338-352`, so they must not be developed at the same time.

## Plans (each at most 3 tasks)

### Plan 1: AgentKind plumbing (Claude behavior unchanged)

1. Add `#[serde(default)] kind: AgentKind` to `AgentConfig` (`crates/rocket-acp/src/agent_config.rs`), the IPC DTO (`src-tauri/src/commands/agent_configs.rs`, camelCase only there), the TS types and a kind select in `AgentConfigsDialog.tsx`. Old YAML loads as `Claude`. About 7 struct-literal test sites change. Per-kind reserved env list replaces `RESERVED_ENV_VAR`.
2. Add `SessionPlan` and `session_plan(kind, ctx)` in `rocket-app/src/agent_isolation.rs`. The `Claude` arm must reproduce today's `_meta`, env and prompt exactly (existing locked-shape tests stay green). `Generic` sends no `_meta` and no `CLAUDE_CONFIG_DIR`. Wire `start_workspace_session` and `session_env` to the plan, keeping the existing signature additive.
3. Deliver the Rocket instructions in the first prompt for kinds that ignore `systemPrompt.append`, next to the outline preamble (`PendingOutline` in `mcp_tool_service.rs`). Add a tool-name helper per kind (`mcp__s__t` vs `mcp_s_t`) and use it in the prompt text.

### Plan 2: Gemini preset

4. Gemini arm of `session_plan`. Files written into the scratch dir before spawn, confined to it (relative paths, no `..`, create-new, no symlink follow): `home/.gemini/settings.json` (exact-name `tools.core`, `skills.enabled:false`, `hooksConfig.enabled:false`, `mcp.allowed:["rocket"]`), `cwd/GEMINI.md`. Env: `GEMINI_CLI_HOME`, `GEMINI_CLI_TRUST_WORKSPACE=true`. Args: `--acp -e none --allowed-mcp-server-names rocket`. The exact tool names come from the tool registry, with a test that fails if the list and the registry diverge.
5. Auth for Gemini: `GEMINI_API_KEY` through the existing credential env path. Opt-in OAuth: copy `oauth_creds.json` into the isolated home with 0600. Per-kind placeholders in the agent config UI. Make the model option (`MODEL_CONFIG_ID`) skip cleanly when Gemini does not expose it.
6. Real-key verification, a gate before merging. With a real `GEMINI_API_KEY`: run the assistant in Edit mode, update a request's docs through `propose_changes`, confirm the proposal card appears. If the API rejects the schema (400), add a Gemini-only flattened schema (inline `$ref`, `enum` for the tag, no `$schema`) in the tool server. Record the result here.

### Plan 3: Follow-ups and other agents

7. Land the permission card plan (its own 7 tasks, run with the SDD ledger convention) for `Generic` agents.
8. Stdio bridge: add `get_info` with the tools capability to `ForwardingHandler` (`src-tauri/src/mcp/stdio_bridge.rs`), so agents without HTTP support see the tools. Update `rocket-app/CLAUDE.md` and `crates/rocket-acp/CLAUDE.md` for the agent kinds.
9. Later, one small plan per agent as an enum arm: Qwen, Codex (no single no-built-ins switch, no SSE), OpenCode (merges global config), Goose, Copilot, Cursor. Cursor and Goose have known isolation gaps, so check those first.

## Open risks

- Real API acceptance of the `propose_changes` schema (Plan 2, task 6).
- Gemini flags and settings keys can change between versions. Pin and check the version in the Gemini arm, and show a clear error on a mismatch.
- `Generic` agents get no lockdown: the user must configure it. Show that in the UI.
- Ancestor `.env` files and `GEMINI_CLI_IDE_WORKSPACE_PATH` can still leak into Gemini. Scrub that env variable for the Gemini kind.
- Gemini's shell tool is unsandboxed. The exact-name `tools.core` list removes it, so a test must assert the declared tool set.

## Verification log

- 2026-10-13, Gemini CLI 0.63.0, Plan 2: tool set pass (after the MCP priming fix: the model lists exactly the 11 `mcp_rocket_*` names and no built-ins), docs edit through `propose_changes` pass (a proposal card appeared and the docs line was applied; real Google API accepted the schema, also shown by a direct ACP probe with the real tool list), real `~/.gemini` untouched yes (newest change 00:46, from an earlier run of a pre-Gemini-kind agent, none since), scratch removed after the session yes.
- Two bugs found during this run and fixed: (1) rmcp sent an empty SSE priming event on every reply, and its `sse_retry` exists in two places (the server config and the session manager); with either on, Gemini CLI's MCP client registers no tool. Both are now off, with tests. (2) A failed turn shows nothing in the UI (still open, see followups).
- 2026-10-13, Gemini CLI 0.63.0, Plan 5 step 10 (headless, no Rocket UI): the settings files that `session_plan` generates were put in private `GEMINI_CLI_HOME` dirs and the real `gemini -p hello` was run against a fake local API. The test copy added `security.auth.selectedType = gemini-api-key`, which Rocket's ACP flow sets by authenticating. Pass: `gemini-2.5-pro` High sent `{"includeThoughts":true,"thinkingBudget":24576}`, Low sent `1024`. `gemini-3-pro-preview` High sent `{"includeThoughts":true,"thinkingLevel":"HIGH"}`, Low sent `"LOW"`. The CLI rewrote the 3-pro name to `gemini-3.1-pro-preview` in the request, and the override still applied. `includeThoughts` stayed true in all four. Steps 11 and 12 (real Claude and Gemini keys, Reasoning block text, Effort picker memory) are still open.
- 2026-10-13, Gemini CLI 0.63.0, MCP stream leak: in session mode every tool-call reply carried event ids, so Gemini's client resumed each finished reply (rmcp WARN "Resume failed") and then parked one idle GET stream per call (WARN "Shadow stream limit reached" past 32). Measured with the real CLI against the real server and a fake model API, 45 tool calls: legacy mode made 238 GETs and left 34 open streams, stateless mode with plain JSON replies made 1 GET (a 405) and left 0, all 45 calls succeeded in both. The server now runs `legacy_session_mode(false)` with `json_response(true)`. Effects: no `mcp-session-id`, no standalone SSE stream (GET answers 405), `tools/list` works without a prior `initialize`. A tool that emits a notification still gets an SSE reply for that call. Not tested with a real Claude adapter run: it uses the same TypeScript MCP SDK client as Gemini's, so check the Claude Effort and tool-call path once in the app.
