# Workspace AI Assistant Design

Date: 2026-10-09. Status: draft for review.

Builds on the ACP subprojects A to E (agent configs, transport, chat panel, MCP tool server). Those specs are in `docs/superpowers/specs/2026-09-27-*` and `2026-09-28-acp-mcp-tool-server-design.md`.

## 1. Goal

Today AI Assist lives inside one request's Scripts tab. The agent starts in the whole collection folder, loads the user's entire Claude Code setup, and has no idea which request is open. Input token use is high, and the MCP tools are not tied to any collection or workspace.

This design turns AI Assist into an application-level assistant for the current workspace:

- It knows the workspace's collections, folders and requests, and nothing outside the workspace.
- It can write and fix scripts and tests, and create and reorganize requests and folders.
- Every change is a proposal that the user previews and accepts. Nothing is written without an Accept.
- The user can pick the Claude model per need, and change it mid-session.
- A fresh session costs far fewer input tokens than today.

### Decisions made with the user

1. Scope: write and fix scripts and tests, and build and change the workspace (create and organize requests and folders).
2. Edits are preview and approve. The agent never writes to disk.
3. The panel is docked on the right edge and toggled from the title bar.
4. The existing per-collection switch now gates only sending real HTTP requests. Reading any collection in the workspace and proposing changes is always allowed.
5. The user can choose the Claude model.

### Non-goals for v1

- Deleting requests, folders or collections.
- Proposals for flows, load tests, contracts or git.
- Several parallel conversations.
- An "accept all" button.
- Importing from OpenAPI or other formats. Creating from a pasted curl command or a description is in scope, because the agent can build the request itself.
- Reading or writing anything outside the current workspace.

## 2. Architecture

```
Frontend                         Tauri commands                rocket-app                       agent process
-----------------------------    --------------------------    ----------------------------     ----------------------
AssistantPanel (docked)  ------> start_workspace_assistant --> AcpSessionService  ------------->  claude-agent-acp
assistant-store (zustand)        send_agent_prompt             WorkspaceToolService  <-- MCP -->  (isolated, no file
focus chip (open request)        set_assistant_model           ProposalService                      or shell tools)
proposal cards   <-- events ---- accept/reject_agent_proposal  (applies via CollectionService,
                                 list_agent_proposals           EnvironmentService)
```

### Session model

- One assistant session per workspace, owned by `assistant-store`, not by a request tab.
- `start_workspace_assistant(agent_config_id, model?)` resolves the active workspace in the backend. The frontend never sends a workspace path.
- Switching workspace ends the session and discards pending proposals, with a notice in the panel.
- On first mount after a webview load, the frontend asks the backend to end any assistant session it does not own. This closes the reload leak found in the lifecycle investigation.

### Agent isolation (the token fix)

`session/new` carries `_meta` with these options, and the process gets an empty config directory:

```
_meta.claudeCode.options = {
  settingSources: [],        // no user, project or local settings, CLAUDE.md, plugins or skills
  strictMcpConfig: true,     // only the MCP servers Rocket passes
  tools: [],                 // no built-in Bash, Read, Write, Edit, WebFetch, Task and so on
  allowedTools: ["mcp__rocket__*"],
  allowDangerouslySkipPermissions: false
}
_meta.systemPrompt = { append: "<Rocket instructions>" }
env CLAUDE_CONFIG_DIR = <empty per-session directory>
cwd = <empty per-session scratch directory>
```

The scratch directories are created per session and removed when it ends. The agent cannot read the workspace files, the user's home directory, or other workspaces.

Facts taken from reading `@agentclientprotocol/claude-agent-acp@0.88.0`, to be confirmed at the start of plan 1:

- `settingSources` defaults to user, project and local, and `systemPrompt` defaults to the `claude_code` preset.
- `_meta.claudeCode.options` is spread over the adapter defaults.
- `permissionMode` is not honored from `_meta`.

Open points to verify empirically: that `settingSources: []` removes plugin and hook text from the context, that `skills: []` and `plugins: []` are accepted if needed, and that Rocket's client can set `_meta` on `NewSessionRequest`. If any fails, the fallback is the empty `CLAUDE_CONFIG_DIR` plus a system prompt string.

Rocket registers no handler for `session/request_permission`. Plan 1 checks how MCP tool calls behave under the default permission mode. `allowedTools` is meant to make that unnecessary.

### Model selection

The adapter reports the available models as a session config option in the `session/new` response, and accepts `session/set_config_option` to change it mid-session.

- `start_workspace_assistant` returns `{ session_id, models: [{ id, label }], current_model }`.
- `set_assistant_model(session_id, model_id)` calls `session/set_config_option`.
- The panel shows a model dropdown in its header. The choice is remembered per agent config and applied when the next session starts.
- The list comes from what the credential allows, so Rocket does not hard-code model names.
- If the adapter returns no model option, the dropdown is hidden and the agent's default is used.

## 3. Tools (MCP, bound to the workspace)

One MCP server per session, as today. Its scope is the workspace, not a collection. Every tool that takes a `collection` checks that it is in the workspace. Paths keep today's traversal checks.

### Read tools (always allowed)

| Tool | Returns |
|---|---|
| `get_workspace_outline` | Compact index: collection, folder and `METHOD path` per request, with run permission per collection. Capped, see section 6. |
| `list_collections` | Names and request counts. |
| `get_request` | Full request definition, with credentials masked. |
| `get_collection_settings` | Auth type, headers, variables (secret values masked), and the run switch. |
| `get_environment` | Variable names and non-secret values. Secret variables appear by name only. |
| `get_history` | Last N runs for a request, status and truncated body. |
| `get_test_results` | Last test results from a run in this session. |
| `list_proposals` | The session's proposals and their status. |

Masking rules: variables flagged secret and vault values are never returned. Literal credentials in auth fields and in sensitive headers (Authorization, Cookie, API keys) are replaced with a mask. `{{variable}}` references are kept, because they carry no secret.

### Propose tool

`propose_changes(changes: [ProposedChange])` queues the changes and returns proposal ids. It writes nothing. Supported operations:

- `create_folder`
- `create_request`
- `update_request` (partial fields)
- `edit_script` (pre-request, post-response or tests)
- `move_item`
- `rename_item`
- `set_env_var` (non-secret values only)

### Run tool

`run_request` works only in collections whose switch is on. It keeps today's behavior: it uses the workspace `RequestGuardPolicy`, sanitizes errors, and caches test results for the session.

### Removed

`list_collection_requests` is replaced by the outline. `edit_script`, `set_env_var` and `get_env_var` become the propose and read tools above, so no tool writes directly.

## 4. Proposals

### Types

In `rocket-acp` (domain, no I/O):

```
AgentProposal { id, session_id, change: ProposedChange, summary, status, created_at }
status: Pending | Accepted | Rejected | Stale | Failed
ProposedChange: tagged enum by `op`, one variant per operation above
```

Each update-style change carries `base_fingerprint`: a hash of the item's stored form when the proposal was made. Creates record that the target path was free.

### Service

`ProposalService` in `rocket-app` holds pending proposals in memory, per session, with a cap of 50 pending. It depends on the existing collection and environment service traits, so it has no I/O of its own.

- **Accept:** re-reads the current item. If its fingerprint no longer matches, the proposal becomes `Stale` and nothing is written. Otherwise it applies through `CollectionService` and `EnvironmentService`. These are the same paths as manual edits, so name validation and `CollectionSaved`-style events still fire. A failure marks the proposal `Failed` with the message.
- **Reject:** marks it `Rejected`.
- The agent sees outcomes through `list_proposals` and can propose again against fresh state.

### Events and commands

- Events: `AcpProposalCreated { session_id, proposal_id, summary }` and `AcpProposalResolved { session_id, proposal_id, status }`, mapped by `TauriEventBus` to `agent-proposal-created` and `agent-proposal-resolved`.
- Commands: `list_agent_proposals`, `accept_agent_proposal`, `reject_agent_proposal`.

### Security effect

The accepted gap S-1 (an agent using `edit_script` plus `run_request` to send a secret to any host) now needs the user to accept the script edit first. Running remains gated per collection and by the SSRF guard.

## 5. UI

- **Panel.** `AssistantPanel`, docked on the right of the main layout, toggled by a Sparkles button in the title bar. The existing Bot button keeps opening agent configs.
- **Views.** Start view (agent picker and model), chat, and a proposals list in the chat flow.
- **Header.** Model dropdown, End session, and a permissions popover listing the workspace's collections with the run switch. This reuses the toggle logic from `AgentAutonomyToggle`; its label becomes "Allow the agent to run requests in this collection".
- **Proposal cards.** Each card shows a summary and Accept and Reject. Script and request edits show a before and after diff in a Monaco diff editor (Monaco is the project's editor for multi-line content). Creates show the new item's definition. A `Stale` or `Failed` card says why.
- **Focus.** A chip shows the open request. When the user sends a message, the focused request's definition, including unsaved script edits, is added to that message as context. If nothing is open, no chip is shown.
- **Request tab.** The AI Assist button in the Scripts tab opens the docked panel with that request as focus. The per-tab chat panel and per-tab session state are removed.
- **State.** `assistant-store` is a Zustand store with narrow selectors. The event bridge becomes a single workspace-level subscription. This removes the tab-lifecycle leak paths (parked tabs, deleted collections) that the per-tab design had.

## 6. Tokens and context

- The first message of a session carries the workspace outline. It is capped at 400 entries. Beyond that, the outline gives per-collection counts and the agent pages through `get_workspace_outline(collection, folder)`.
- Tool results are truncated: response bodies to 8 KB, history to 10 entries. Script bodies are returned in full.
- Success check for plan 1: compare input tokens for a first "hello" turn before and after the isolation change, if the adapter reports usage. The current cost is not measured, so the target is a large reduction, not a fixed number.

## 7. Safety and error handling

- API responses are untrusted input to the agent (prompt injection). The limits are: edits need approval, runs are gated per collection, the SSRF guard applies, and secrets are masked.
- Tool errors name what failed without echoing request content or credentials.
- A proposal that cannot apply never partially writes. Multi-change proposals apply one change at a time, and a failure stops the rest and reports which applied.
- Lifecycle: the session, its MCP server and its scratch directories end on workspace switch, End session, the 120-second prompt timeout and app exit. The timeout and failed-prompt paths currently skip the MCP server and cache cleanup. Plan 1 moves that cleanup into `AcpSessionService` so every path ends the MCP server.
- An agent that dies while idle is not detected today. It is out of scope for v1. The panel shows the failure on the next message.

## 8. Testing

The user runs all tests manually. Unit and integration tests are still written, and run only when asked.

- Rust unit tests: workspace scope checks, masking, outline cap, proposal apply, stale detection, failure handling.
- Integration tests with `MockRuntime`: session start with the isolation options, proposal events, accept and reject.
- Vitest: `assistant-store`, panel views, proposal cards, model dropdown, focus context.
- Manual checklist, written with each plan:
  - Token comparison before and after.
  - Model switching mid-session.
  - Accept, reject and stale proposals.
  - Workspace switch ends the session.
  - A second workspace is unreachable.

## 9. Build order

Four plans, built in order. Each plan has at most three tasks and its own review.

1. **Isolation, model selection and lifecycle.** `_meta` options, empty config and scratch directories, model list and `set_assistant_model`, MCP cleanup on every end path, stale-session sweep on webview load.
2. **Workspace-scoped read tools.** The workspace scope on the MCP server, the read tools, masking, outline and truncation.
3. **Proposals backend.** Types, `ProposalService`, `propose_changes`, events and commands, removal of the direct write tools.
4. **Panel UI.** `assistant-store`, `AssistantPanel`, proposal cards with diffs, permissions popover, focus context, the request-tab shortcut, removal of the per-tab chat.

## 10. Risks

- The adapter may not honor every isolation option. Plan 1 verifies this first, with the fallbacks listed in section 2.
- With built-in tools off, the agent depends entirely on Rocket's tools. A missing read tool shows up as a weaker answer. The tool list is meant to be extended.
- Proposals live in memory. A crash loses pending proposals, which is acceptable because nothing was written.
