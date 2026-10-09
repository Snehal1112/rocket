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
6. The prompt area follows GitHub Copilot Chat in VS Code: a growing multi-line box, a toolbar with model and mode pickers, context chips, `#` references and `/` commands, and a Send button that becomes Stop.
7. The prompt editor is a CodeMirror 6 component, `PromptEditor`. This is an approved exception to the "multi-line editors use Monaco" rule, limited to the AI Assistant. It is recorded in `CLAUDE.md` and `.claude/rules/frontend-component-guardrails.md`.
8. The assistant's modes are Rocket's own (Ask, Edit, Agent), enforced in the tool server. The adapter's own modes are not shown.

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

Facts taken from reading `@agentclientprotocol/claude-agent-acp@0.88.0`, to be confirmed at the start of plan 2:

- `settingSources` defaults to user, project and local, and `systemPrompt` defaults to the `claude_code` preset.
- `_meta.claudeCode.options` is spread over the adapter defaults.
- `permissionMode` is not honored from `_meta`.

Open points to verify empirically: that `settingSources: []` removes plugin and hook text from the context, that `skills: []` and `plugins: []` are accepted if needed, and that Rocket's client can set `_meta` on `NewSessionRequest`. If any fails, the fallback is the empty `CLAUDE_CONFIG_DIR` plus a system prompt string.

Rocket registers no handler for `session/request_permission`. Plan 1 checks how MCP tool calls behave under the default permission mode. `allowedTools` is meant to make that unnecessary.

### Model selection

The adapter reports the available models as a session config option in the `session/new` response, and accepts `session/set_config_option` to change it mid-session.

- `start_workspace_assistant` returns `{ session_id, models: [{ id, label }], current_model }`.
- `set_assistant_model(session_id, model_id)` calls `session/set_config_option`.
- The panel shows a model dropdown in the composer toolbar. The choice is remembered per agent config and applied when the next session starts.
- The list comes from what the credential allows, so Rocket does not hard-code model names.
- If the adapter returns no model option, the dropdown is hidden and the agent's default is used.
- The adapter also reports an `effort` option for models that support it. It appears and disappears with the model, so the UI re-reads the option list from `config_option_update` after every model change. Fast mode is not shown in v1.

### ACP client upgrade

The investigation of Rocket's current client found a text-only pipe. The composer needs this groundwork, built first (plan 1):

- **Typed updates.** Replace the per-prompt `String` channel with a per-session stream of an `AcpUpdate` enum defined in `rocket-acp` (no ACP crate types): text, tool call, tool call update, config options, usage, available commands. Today every other update is dropped, and updates between turns are lost.
- **Session info.** Keep the `InitializeResponse` and `NewSessionResponse` data (model and effort option lists, prompt capabilities) and return it from `start_session`.
- **Change an option.** A trait method and command that send `session/set_config_option`.
- **Stop.** A trait method and command that send `session/cancel` without taking the prompt lock. A `cancelled` stop reason ends the turn normally, and the session stays alive.
- **Prompt parts.** The prompt becomes a list of parts: text and embedded text resources. Parts are checked against the agent's `embeddedContext` capability. Images and audio are not used in v1.
- **Idle timeout.** The 120-second total-turn limit becomes an idle limit that resets on every update. A run-and-fix loop is not killed while it makes progress. Stop is the user's escape. A hung turn with no updates for 120 seconds still ends the session as it does today.
- **Permission requests.** The client answers `session/request_permission` with a deny, so a request can never hang a turn. With built-in tools off and the Rocket tools allowed in advance, none is expected, and plan 1 verifies this.
- **Plumbing.** New `DomainEvent` variants, event bus mappings, Tauri commands and TypeScript listeners for the above.

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

### Modes

The assistant has three Rocket modes, chosen in the composer:

| Mode | Read tools | `propose_changes` | `run_request` |
|---|---|---|---|
| Ask | yes | no | no |
| Edit | yes | yes | no |
| Agent | yes | yes | yes, in collections with the switch on |

The mode is held by the session's MCP server and changed with `set_assistant_mode`, which needs no restart. The tool list stays the same in every mode, so the agent's prompt cache is not disturbed. A tool outside the current mode refuses with a clear message ("Not available in Ask mode"). The mode is also named in the first message, so the agent plans accordingly.

The adapter's own modes (Manual, Accept edits, Plan, Auto, Bypass permissions) govern Claude Code's file-edit permissions. They have no effect with built-in tools off and are not shown. Bypass permissions stays disabled.

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
- **Header.** End session, and a permissions popover listing the workspace's collections with the run switch. This reuses the toggle logic from `AgentAutonomyToggle`; its label becomes "Allow the agent to run requests in this collection".
- **Proposal cards.** Each card shows a summary and Accept and Reject. Script and request edits show a before and after diff in a Monaco diff editor (Monaco is the project's editor for multi-line content). Creates show the new item's definition. A `Stale` or `Failed` card says why.
- **Focus.** A chip shows the open request. When the user sends a message, the focused request's definition, including unsaved script edits, is added to that message as context. If nothing is open, no chip is shown.
- **Request tab.** The AI Assist button in the Scripts tab opens the docked panel with that request as focus. The per-tab chat panel and per-tab session state are removed.
- **State.** `assistant-store` is a Zustand store with narrow selectors. The event bridge becomes a single workspace-level subscription. This removes the tab-lifecycle leak paths (parked tabs, deleted collections) that the per-tab design had.
- **Tool activity.** While the agent works, the chat shows one muted line per tool call ("Reading GET /orders", "Running Login") with its status, from the tool call updates.

### Composer (the prompt area)

```
+------------------------------------------------+
| [# GET /orders x] [# env: dev x]      chips    |
| Ask about this workspace...                    |
| (multi-line, grows to a max height, then       |
|  scrolls)                                      |
|------------------------------------------------|
| [Mode v] [Model v] [Effort v]       (12%)  [>] |
+------------------------------------------------+
```

- **`PromptEditor`.** A CodeMirror 6 component for the AI Assistant only (see decision 7). It reuses the existing editor pieces: `rocketTheme`, `rocketThemeDark`, `rocketTooltipBase`, tooltips attached to the document body, the placeholder, and optional `{{variable}}` highlighting. It leaves out `singleLineFilter`, and adds line wrapping, a maximum height (about 12 lines) after which it scrolls, and an accessible label. Because CodeMirror draws its own autocomplete list, no new dependency is needed.
- **Keys.** Enter sends. Shift+Enter inserts a newline. Esc stops a running turn. Up and Down at the first or last line recall earlier prompts (last 50 per workspace, kept in `localStorage` with safe access). The editor stops key events from reaching the global Cmd+Enter shortcut, which sends the active request.
- **Chips row.** The focus chip (the open request) is added by default and can be removed. Typing `#` opens a list of requests, folders, collections, environments and the last response. Choosing one adds a chip. On send, each chip becomes an embedded text resource that Rocket generates from its own data, with secrets masked, so no file reading is involved. Limits: 8 chips per message and 8 KB per chip.
- **Toolbar.** Mode dropdown (Ask, Edit, Agent). Model dropdown, from the agent's model option, with the agent's default first. Effort dropdown, shown only when the model supports it. A small context-used indicator from the usage updates, with the cost on hover. The Send button turns into Stop while a turn runs.
- **Slash commands.** Rocket's own prompt templates: `/explain`, `/tests`, `/fix`, `/scaffold`, `/doc`. Choosing one puts the template text in the editor for the user to complete and send. The adapter's commands are not shown in v1.
- **Not in v1.** `@` participants, drag and drop, images, queue and steer while running, a thinking display, voice, checkpoints.

## 6. Tokens and context

- The first message of a session carries the workspace outline. It is capped at 400 entries. Beyond that, the outline gives per-collection counts and the agent pages through `get_workspace_outline(collection, folder)`.
- Tool results are truncated: response bodies to 8 KB, history to 10 entries. Script bodies are returned in full.
- Success check for plan 2: compare input tokens for a first "hello" turn before and after the isolation change, if the adapter reports usage. The current cost is not measured, so the target is a large reduction, not a fixed number.

## 7. Safety and error handling

- API responses are untrusted input to the agent (prompt injection). The limits are: edits need approval, runs are gated per collection, the SSRF guard applies, and secrets are masked.
- Tool errors name what failed without echoing request content or credentials.
- A proposal that cannot apply never partially writes. Multi-change proposals apply one change at a time, and a failure stops the rest and reports which applied.
- Lifecycle: the session, its MCP server and its scratch directories end on workspace switch, End session, the idle timeout and app exit. The timeout and failed-prompt paths currently skip the MCP server and cache cleanup. Plan 2 moves that cleanup into `AcpSessionService` so every path ends the MCP server.
- An agent that dies while idle is not detected today. It is out of scope for v1. The panel shows the failure on the next message.

## 8. Testing

The user runs all tests manually. Unit and integration tests are still written, and run only when asked.

- Rust unit tests: workspace scope checks, masking, outline cap, proposal apply, stale detection, failure handling.
- Integration tests with `MockRuntime`: session start with the isolation options, proposal events, accept and reject.
- Vitest: `assistant-store`, panel views, proposal cards, model dropdown, focus context, and the composer (`PromptEditor` keys and growth, `#` and `/` lists, chips, history, Stop).
- Rust: typed updates, option changes, cancel, idle timeout, mode gating in the tool server.
- Manual checklist, written with each plan:
  - Token comparison before and after.
  - Model switching mid-session.
  - Accept, reject and stale proposals.
  - Workspace switch ends the session.
  - A second workspace is unreachable.

## 9. Build order

Six plans, built in order. Each plan has at most three tasks and its own review.

1. **ACP client upgrade.** Typed updates, session info, option changes (model and effort), Stop, prompt parts, idle timeout, permission deny.
2. **Isolation and lifecycle.** `_meta` options, empty config and scratch directories, MCP cleanup on every end path, stale-session sweep on webview load, token comparison.
3. **Workspace-scoped read tools and modes.** The workspace scope on the MCP server, the read tools, masking, outline and truncation, Ask/Edit/Agent gating and `set_assistant_mode`.
4. **Proposals backend.** Types, `ProposalService`, `propose_changes`, events and commands, removal of the direct write tools.
5. **Panel UI.** `assistant-store`, `AssistantPanel`, proposal cards with diffs, permissions popover, tool activity lines, the request-tab shortcut, removal of the per-tab chat.
6. **Composer.** `PromptEditor`, chips and `#` references, mode, model and effort pickers, slash templates, history, usage indicator, Send and Stop.

## 10. Risks

- The adapter may not honor every isolation option. Plan 2 verifies this first, with the fallbacks listed in section 2.
- The model, effort and fast-mode lists depend on the model and the credential. The UI renders what the agent reports and never assumes a fixed set.
- `PromptEditor` is a deliberate exception to the Monaco rule. Keeping it from spreading to other multi-line fields needs a new approval each time, as the rules file says.
- With built-in tools off, the agent depends entirely on Rocket's tools. A missing read tool shows up as a weaker answer. The tool list is meant to be extended.
- Proposals live in memory. A crash loses pending proposals, which is acceptable because nothing was written.
