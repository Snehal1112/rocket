# Workspace AI Assistant — Plan Index

**Spec:** [../../specs/2026-10-09-workspace-ai-assistant-design.md](../../specs/2026-10-09-workspace-ai-assistant-design.md)

**Context:** follow-on to ACP subprojects A to D (agent configs, transport, chat panel, MCP tool server). Turns the per-request AI Assist chat into a workspace-level assistant with isolated agent sessions, workspace-scoped tools, preview-and-approve edits, and a Copilot-Chat-style composer.

## Plan breakdown — 6 plans

| # | Plan | Crates / areas | Depends on |
|---|---|---|---|
| 01 | ACP client upgrade | `rocket-acp`, `rocket-infra`, `rocket-app`, `rocket-shared`, `src-tauri`, `src/lib` | — |
| 02 | Isolation and lifecycle | `rocket-app`, `src-tauri` | 01 |
| 03 | Workspace-scoped read tools and modes | `rocket-app`, `src-tauri` | 01, 02 |
| 04 | Proposals backend | `rocket-acp`, `rocket-app`, `src-tauri` | 01, 03 |
| 05 | Panel UI | `src/` (React) | 01–04 |
| 06 | Composer | `src/` (React) | 01–05 |

Each plan has at most three tasks. Each plan file ends with a **Next Plan** section and a **Post-Implementation Review** section (an Opus-model subagent reviews that plan's own diff for interface gaps, code quality and DDD boundary conformance, and may fix what it finds directly).

**Cross-plan compilation rule:** every plan leaves `cargo check --workspace --all-targets -j4` and `yarn tsc --noEmit` green at the end of each of its own tasks, even before later plans land. When a trait or type change would break a caller that a later plan reworks, the earlier plan includes the minimal compiling adaptation (for example, the existing per-tab `AgentChatPanel` keeps working until Plan 05 removes it).

**Testing rule (user decision):** the user runs all tests manually. Each task still writes its tests. The agent runs only compile and lint checks: `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit`, `yarn check`. The agent runs the written tests only when the user asks. Never run `cargo test --workspace`.

**Commits:** conventional commits, created through the `dev-workflow-skills:1-git-commit` skill, never a freeform `git commit -m`.

**Rust rules:** no unwrap calls in production paths (tests may use them), no shelling out to `git`, `#[serde(rename_all = "camelCase")]` on IPC DTOs only. **Frontend rules:** shadcn primitives only (no raw `<button>`, `<input>`, `<select>`, `<form>`), `lucide-react` icons only, narrow Zustand selectors, Monaco for multi-line code editors. The one exception is `PromptEditor` (CodeMirror 6), used only by the AI Assistant (see `.claude/rules/frontend-component-guardrails.md`).

**OpenCollection rule:** every task that touches `.yml` files, collection/environment/request/auth models, the `rocket-collection`, `rocket-environment`, `rocket-http`, `rocket-import`, `rocket-workspace` or `rocket-infra` crates, or IPC commands that deal with collections or environments starts with: "📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Locked interface contracts

These names and shapes are fixed. A plan that needs to deviate says so in its own "Interface deviations" note, and the index is then updated.

### Plan 01 — `rocket-acp` (domain, no ACP crate types)

```rust
// crates/rocket-acp/src/update.rs (new)
pub enum AcpUpdate {
    Text { text: String },
    ToolCall { call_id: String, title: String, kind: String, status: ToolCallStatus },
    ToolCallUpdate { call_id: String, title: Option<String>, status: Option<ToolCallStatus> },
    ConfigOptions { options: Vec<ConfigOption> },
    Usage { used: u64, size: u64, cost_usd: Option<f64> },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus { Pending, InProgress, Completed, Failed }

// crates/rocket-shared/src/acp.rs (new; DomainEvent carries it), re-exported by
// crates/rocket-acp/src/session_info.rs as rocket_acp::{ConfigOption, ConfigChoice}
pub struct ConfigOption {
    pub id: String,                 // "model", "effort", "mode", "fast", ...
    pub name: String,
    pub category: Option<String>,   // "model", "thought_level", "mode", "model_config"
    pub current_value: String,
    pub choices: Vec<ConfigChoice>,
}
pub struct ConfigChoice { pub value: String, pub name: String, pub description: Option<String> }
// crates/rocket-acp/src/session_info.rs (new)
pub struct PromptCapabilities { pub embedded_context: bool, pub image: bool }
pub struct SessionInfo {
    pub session_id: String,
    pub config_options: Vec<ConfigOption>,
    pub prompt_capabilities: PromptCapabilities,
}

// crates/rocket-acp/src/prompt.rs (new)
pub enum PromptPart {
    Text(String),
    Resource { uri: String, mime_type: Option<String>, text: String },
}
```

`AcpSessionClient` after Plan 01 (one new parameter on `start_session`, a new return type, a new `send_prompt` shape, two new methods):

```rust
async fn start_session(&self, command: &str, args: &[String], cwd: &str,
    env: &[(String, String)], mcp_servers: &[McpServerSpec],
    meta: Option<serde_json::Value>) -> DomainResult<SessionInfo>;
async fn send_prompt(&self, session_id: &str, parts: Vec<PromptPart>,
    update_tx: UnboundedSender<AcpUpdate>) -> DomainResult<String>;   // stop reason
async fn cancel(&self, session_id: &str) -> DomainResult<()>;
async fn set_config_option(&self, session_id: &str, config_id: &str, value: &str)
    -> DomainResult<Vec<ConfigOption>>;
async fn end_session(&self, session_id: &str) -> DomainResult<()>;   // unchanged
async fn end_all_sessions(&self) -> DomainResult<()>;                // unchanged
```

`meta` is passed through as the ACP `_meta` of `session/new`. Updates stay per-prompt (`update_tx`), so the existing "all updates before the finished event" ordering guarantee is kept. Updates that arrive between turns are dropped in v1.

### Plan 01 — events, commands, DTOs

New `DomainEvent` variants (fields are snake_case on the wire, tag is camelCase, like the existing ACP events) and event-bus channels:

| Variant | Fields | Channel |
|---|---|---|
| `AcpToolActivity` | `session_id, call_id, title: String, status: String` (`pending`, `in_progress`, `completed`, `failed`) | `agent-session-tool-activity` |
| `AcpConfigOptionsChanged` | `session_id, options: Vec<ConfigOption>` | `agent-session-config-options` |
| `AcpUsage` | `session_id, used: u64, size: u64, cost_usd: Option<f64>` | `agent-session-usage` |

`AcpSessionService` timeout: `prompt_timeout` becomes `prompt_idle_timeout` (same constructor position), reset on every `AcpUpdate`, and the test seam `with_prompt_timeout` becomes `with_prompt_idle_timeout`. A cancelled turn (`stopReason == "cancelled"`) is a normal finish. After Plan 01: `AcpSessionService::new(session_client, event_publisher, agent_config_service, collection_repo)`, `start_session(..) -> DomainResult<SessionInfo>`, `send_prompt(session_id, parts: Vec<PromptPart>)`, `cancel(session_id)`, `set_config_option(session_id, config_id, value) -> DomainResult<Vec<ConfigOption>>`; `start_agent_session_inner` returns `Result<SessionInfo, DomainError>` and the command maps it with `AgentSessionStartedDto::from`.

IPC (camelCase DTOs, in `src-tauri/src/commands/acp_session_dto.rs`, with `From<ConfigOption> for ConfigOptionDto`, `From<SessionInfo> for AgentSessionStartedDto` and `prompt_parts(prompt, resources) -> DomainResult<Vec<PromptPart>>`, which enforces 8 resources of at most 8192 bytes):

```rust
pub struct ConfigChoiceDto { value: String, name: String, description: Option<String> }
pub struct ConfigOptionDto { id: String, name: String, category: Option<String>,
                             current_value: String, choices: Vec<ConfigChoiceDto> }
pub struct AgentSessionStartedDto { session_id: String, config_options: Vec<ConfigOptionDto> }
pub struct PromptResourceDto { uri: String, mime_type: Option<String>, text: String }

start_agent_session(agent_config_id, cwd, collection) -> AgentSessionStartedDto   // return type changes
send_agent_prompt(session_id, prompt: String, resources: Option<Vec<PromptResourceDto>>)
cancel_agent_prompt(session_id)
set_agent_config_option(session_id, config_id, value) -> Vec<ConfigOptionDto>
```

TypeScript (`src/lib/tauri-api.ts`): `startAgentSession(...)` returns `AgentSessionStarted { sessionId, configOptions }`; `sendAgentPrompt(sessionId, prompt, resources?)`; `cancelAgentPrompt(sessionId)`; `setAgentConfigOption(sessionId, configId, value)`; listeners `onAgentToolActivity`, `onAgentConfigOptions`, `onAgentUsage`.

### Plan 02 — isolation and lifecycle

```rust
// crates/rocket-app/src/acp_session_service.rs
pub trait SessionCleanup: Send + Sync {
    /// Called exactly once per session on EVERY end path: end_session, idle
    /// timeout, failed prompt, end_all_sessions. Idempotent.
    fn on_session_ended(&self, session_id: &str);
}
// AcpSessionService::new gains `cleanup: Arc<dyn SessionCleanup>` after `event_publisher`.

// crates/rocket-app/src/agent_isolation.rs (new, pure)
pub fn isolation_meta(system_prompt_append: &str) -> serde_json::Value;
pub const ISOLATION_ENV_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";
```

`isolation_meta` returns `{ "claudeCode": { "options": { "settingSources": [], "strictMcpConfig": true, "tools": [], "allowedTools": ["mcp__rocket__*"], "allowDangerouslySkipPermissions": false } }, "systemPrompt": { "append": <text> } }`. Scratch directories (session cwd and `CLAUDE_CONFIG_DIR`) are created by the command layer in `src-tauri` (`SessionScratch`), because `rocket-app` does no I/O, and removed by `TauriSessionCleanup`, which also ends the MCP registry entry and calls `McpToolService::forget_session`.

Additions Plan 02 defines (used by Plans 03 and 04):

```rust
// rocket_app::agent_isolation (re-exported at the crate root)
pub struct SessionIsolation { pub config_dir: String, pub system_prompt_append: String }  // new(config_dir), meta(), env_entry()
// rocket_app::AcpSessionService
pub fn new(session_client, event_publisher, cleanup: Arc<dyn SessionCleanup>, agent_config_service, collection_repo) -> Self;
pub fn with_prompt_idle_timeout(/* same five */, prompt_idle_timeout: Duration) -> Self;
pub async fn start_session(&self, agent_config_id, cwd, collection, mcp_http: Option<McpHttpServerCredentials>,
    isolation: Option<SessionIsolation>) -> DomainResult<SessionInfo>;   // does NOT track; the caller registers its resources, then calls track()
pub fn track(&self, session_id: &str) -> bool;          // false when the session was refused (shutdown); caller then calls end_untracked()
pub async fn end_untracked(&self, session_id: &str);
pub async fn end_tracked_sessions(&self) -> usize;
// src-tauri/src/agent_session/scratch.rs
pub struct SessionScratch;   // create() -> std::io::Result<Self>; isolation() -> Result<(String, SessionIsolation), DomainError>
// src-tauri/src/agent_session/cleanup.rs
pub struct SessionResources { pub scratch: SessionScratch, pub mcp_session_id: Option<String> }
pub struct SessionResourceRegistry;   // managed as Arc<SessionResourceRegistry>; register(session_id, resources)
pub struct TauriSessionCleanup;       // new(mcp_registry, mcp_tool_svc, resources [, proposals from Plan 04]); not managed state
```

IPC: `end_stale_assistant_sessions()` ends every backend session the backend still tracks. The webview calls it once per load, from Plan 05's app-lifetime assistant event bridge, and every assistant start waits for it to finish.

### Plan 03 — workspace scope and modes

```rust
// crates/rocket-app/src/mcp_tool_service.rs
pub enum AssistantMode { Ask, Edit, Agent }          // serde: "ask" | "edit" | "agent"
// McpToolService read methods (all take session_id; collection arguments are checked
// against the active workspace's collection list; secrets masked):
fn get_workspace_outline(&self, session_id: &str, collection: Option<&str>, folder: Option<&str>) -> DomainResult<String>;
fn list_collections(&self, session_id: &str) -> DomainResult<Vec<CollectionBrief>>;
fn get_request(&self, session_id: &str, collection: &str, request_path: &str) -> DomainResult<MaskedRequest>;
fn get_collection_settings(&self, session_id: &str, collection: &str) -> DomainResult<MaskedSettings>;
fn get_environment(&self, session_id: &str, collection: &str, environment: &str) -> DomainResult<MaskedEnvironment>;
fn get_history(&self, session_id: &str, collection: &str, request_path: &str, limit: usize) -> DomainResult<Vec<HistoryBrief>>;
fn check_mode(&self, session_id: &str, required: AssistantMode) -> DomainResult<()>;
// McpToolService::new gains a seventh parameter, history_repo: Box<dyn rocket_history::HistoryRepository>
// (Plan 03 Task 1 updates every call site). HistoryBrief carries no response body; run_request's
// McpRunResult gains body (masked, cut to 8 KB) and body_truncated.
// McpToolService::peek_outline_preamble(session_id) -> Option<PromptPart> keeps the stored outline;
// McpToolService::discard_outline(session_id) drops it. send_agent_prompt peeks, then discards once the
// prompt was accepted (take_outline_preamble = peek + discard).

// crates/rocket-app/src/acp_session_service.rs
pub async fn start_workspace_session(&self, agent_config_id: &str, cwd: &str,
    mcp_http: McpHttpServerCredentials, isolation: SessionIsolation) -> DomainResult<SessionInfo>;  // not tracked, like start_session: the caller registers the MCP handle and SessionResources, calls svc.track(id), and on false calls svc.end_untracked(id)

// src-tauri/src/mcp/tool_server.rs
pub struct McpSessionBinding;   // new(provisional_id), bind(&self, acp_id) (first bind wins), session_id(&self) -> &str
// RocketMcpToolServer holds binding: Arc<McpSessionBinding> (no session_id field); tools pass self.binding.session_id().
// McpHttpServerHandle gains pub binding: Arc<McpSessionBinding>; every command that spawns a tool server
// calls handle.binding.bind(&info.session_id) right after the handshake.
```

IPC: `start_workspace_assistant(agent_config_id, mode: AssistantMode, model: Option<String>) -> AgentSessionStartedDto` (replaces `start_agent_session` for the assistant; the per-tab command stays until Plan 05 removes its last caller) and `set_assistant_mode(session_id, mode)`. The workspace outline is stored per session and prepended once, as an embedded resource with uri `rocket://workspace/outline`, to the first `send_agent_prompt` of that session. MCP tool names: `get_workspace_outline`, `list_collections`, `get_request`, `get_collection_settings`, `get_environment`, `get_history`, `get_test_results`, `list_proposals`, `propose_changes`, `run_request`.

### Plan 04 — proposals

```rust
// crates/rocket-acp/src/proposal.rs (new, domain)
pub struct AgentProposal { pub id: String, pub session_id: String, pub change: ProposedChange,
    pub summary: String, pub status: ProposalStatus, pub created_at_ms: i64 }
pub enum ProposalStatus { Pending, Accepted, Rejected, Stale, Failed { message: String } }
pub enum ProposedChange {
    CreateFolder { collection: String, parent_path: String, name: String },
    CreateRequest { collection: String, folder_path: String, request: ProposedRequest },
    UpdateRequest { collection: String, request_path: String, patch: RequestPatch, base_fingerprint: String },
    EditScript { collection: String, request_path: String, phase: ScriptPhase, body: String, base_fingerprint: String },
    MoveItem { collection: String, from_path: String, to_folder: String, base_fingerprint: String },
    RenameItem { collection: String, path: String, new_name: String, base_fingerprint: String },
    SetEnvVar { collection: String, environment: String, key: String, value: String },
}
// crates/rocket-app/src/proposal_service.rs
impl ProposalService {
    pub fn propose(&self, session_id: &str, changes: Vec<ProposedChange>) -> DomainResult<Vec<String>>;  // ids
    pub fn list(&self, session_id: &str) -> Vec<AgentProposal>;
    pub fn accept(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal>;
    pub fn reject(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal>;
    pub fn clear_session(&self, session_id: &str);   // called from SessionCleanup
}
// ProposalService::new(collections: CollectionService, environment_repo_factory, events). No session-id
// mapping: proposals are keyed by the id the tool server reports through McpSessionBinding (Plan 03).
// TauriSessionCleanup::new gains a fourth parameter, proposals: Arc<ProposalService>, and calls
// clear_session from its cache-forgetter closure.
```

Events: `AcpProposalCreated { session_id, proposal_id, summary }` → `agent-proposal-created`; `AcpProposalResolved { session_id, proposal_id, status: String }` → `agent-proposal-resolved` (snake_case fields on the wire). IPC: `list_agent_proposals(session_id)`, `accept_agent_proposal(session_id, proposal_id)`, `reject_agent_proposal(session_id, proposal_id)`; DTO `AgentProposalDto { id, sessionId, change, summary, status, statusMessage?, createdAtMs }` (camelCase). `change` is tagged by `op` with camelCase values (`createFolder`, `createRequest`, `updateRequest`, `editScript`, `moveItem`, `renameItem`, `setEnvVar`), its fields are camelCase, `editScript.phase` is `preRequest`, `postResponse` or `tests`, and it carries no `baseFingerprint` and no "before" text (the panel fetches the current request with `getRequest`). Cap: 50 pending per session. The direct-write tools `edit_script` and `set_env_var` are removed from the MCP server in this plan.

### Plan 05 — panel UI (TypeScript)

```ts
// src/stores/assistant-store.ts
interface AssistantState {
  session?: { sessionId: string; agentConfigId: string; status: 'starting' | 'active' | 'ended' | 'error';
              configOptions: ConfigOption[]; mode: 'ask' | 'edit' | 'agent'; error?: string };
  messages: AssistantMessage[];            // user text, agent text (streaming), tool activity lines
  proposals: AgentProposal[];
  usage?: { used: number; size: number; costUsd?: number };
  focus?: { collection: string; path: string };
  panelOpen: boolean;
}
// actions: openPanel, closePanel, setFocus, beginSession, activateSession, appendUserMessage,
// appendChunk, completeMessage, failMessage, upsertToolActivity, setConfigOptions, setUsage,
// upsertProposal, resolveProposal, endSession, reset (+ failStart; + setMode from Plan 06)
// Signatures (Plan 05 deviations): beginSession(agentConfigId, mode) -> token; activateSession(token,
// sessionId, configOptions) -> boolean; appendUserMessage(text) -> boolean (false while a turn runs);
// event-driven actions take the session id first: appendChunk(sessionId, text), completeMessage(sessionId),
// failMessage(sessionId, error), upsertToolActivity(sessionId, activity), setConfigOptions(sessionId, options),
// setUsage(sessionId, usage), resolveProposal(sessionId, proposalId, status); endSession(notice?).
// src/lib/assistant/assistant-session.ts: startAssistant(agentConfigId, mode?), sendAssistantMessage(text,
// resources? [added by Plan 06]), stopAssistantTurn(), endAssistantSession(notice?), sweepStaleAssistantSessions().
```

Components: `AssistantPanel`, `AssistantProposalCard`, `AssistantPermissionsPopover` under `src/components/assistant/`. The title bar gets a Sparkles toggle. The per-tab chat (`AgentChatPanel`, `tab.agentSession` and its pane-store actions, the tab-keyed parts of `agent-session-event-bridge.ts`) is removed in this plan.

### Plan 06 — composer (TypeScript)

```ts
// src/components/assistant/composer/PromptEditor.tsx
interface PromptEditorProps {
  value: string; onChange(v: string): void; onSubmit(): void; onStop(): void;
  running: boolean; placeholder?: string; disabled?: boolean;
  history: string[]; onHistoryCommit(v: string): void;
  referenceSource: (query: string) => ReferenceItem[];   // '#' list
  commandSource: (query: string) => SlashCommandItem[];  // '/' list
  onReferencePicked(item: ReferenceItem): void;
  variableContext?: Map<string, VariableScopeEntry>;
  'aria-label': string;
}
interface ReferenceItem { kind: 'request' | 'folder' | 'collection' | 'environment' | 'last-response';
                          collection: string; path?: string; label: string }
interface SlashCommandItem { name: string; description: string; template: string }
// src/lib/assistant/chip-resources.ts
export function chipToResource(chip: ReferenceItem): Promise<PromptResourceDto>;   // masked text, 8 KB cap
```

## Interface deviations resolved

Cross-plan review of 2026-10-09. Each item names the one shape every plan now uses.

1. **Session id of the MCP tool server.** The tool server starts before the ACP handshake. One mechanism: Plan 03's `McpSessionBinding` (provisional id, then `bind(real ACP id)` right after the handshake, shared by `RocketMcpToolServer` and `McpHttpServerHandle`). Plan 02's cleanup forgets both ids; that is an interim measure Plan 03 supersedes and it stays as a harmless backstop. Plan 04 has no `ProposalService::link_session`; its tools pass `self.binding.session_id()`.
2. **Idle-timeout test seam.** `AcpSessionService::with_prompt_timeout` is renamed `with_prompt_idle_timeout` (field `prompt_idle_timeout`) in Plan 01. Plans 02 and 03 use the new name. Plan 02's own test helper is `service_with_cleanup`, because Plan 01's test module already has `service_with`.
3. **`ConfigOption` / `ConfigChoice` location.** Defined in `rocket_shared::acp` (because `DomainEvent` carries them), re-exported as `rocket_acp::{ConfigOption, ConfigChoice}`. DTOs live in `src-tauri/src/commands/acp_session_dto.rs`.
4. **`start_agent_session_inner` return type.** Stays `Result<SessionInfo, DomainError>` after Plans 01 and 02; the command maps with `AgentSessionStartedDto::from`.
5. **Workspace assistant start.** `start_workspace_session(agent_config_id, cwd, mcp_http, isolation: SessionIsolation)` uses Plan 02's `SessionIsolation`. Neither `start_session` nor `start_workspace_session` tracks the id: the caller registers the MCP handle and `SessionResources`, then calls `svc.track(id)`, and on `false` calls `svc.end_untracked(id)`. `AcpSessionService::{track, end_untracked}` are part of the Plan 02 contract. Once tracked, `SessionCleanup` runs for the session. `start_workspace_assistant` creates `SessionScratch`, uses `scratch.isolation()` with `system_prompt_append = WORKSPACE_ASSISTANT_INSTRUCTIONS`, and registers `SessionResources` in the managed `SessionResourceRegistry` (no `adopt_scratch`, no `session_lifecycle` module).
6. **Outline preamble and prompt limits.** `send_agent_prompt` keeps Plan 01's `prompt_parts` (8 resources of 8 KB, user parts only) and inserts the outline in front afterwards.
7. **`McpHttpServerHandle.binding`.** Plan 03 adds the field and updates every literal, including Plan 02's test helper in `src-tauri/src/agent_session/cleanup.rs`.
8. **Proposal cleanup.** `TauriSessionCleanup::new(mcp_registry, mcp_tool_svc, resources, proposals)` clears proposals in its cache-forgetter closure (Plan 04 Task 3). Plan 04 also updates Plan 03's tool-list tests and mode tests when it removes `edit_script` and `set_env_var`.
9. **`get_history` body.** `HistoryEntry` stores no response body, so `get_history` returns none; `run_request` returns the masked body cut to 8 KB (spec updated).
10. **Stale-session sweep timing.** Once per webview load from Plan 05's bridge, and every start waits for it (Plan 02 wording and spec updated).
11. **Plan 05 store shapes.** Event-driven actions take the session id first (`setConfigOptions(sessionId, options)`), `appendUserMessage` returns `boolean`. Plan 06 uses these, adds `setMode`, and sends through `sendAssistantMessage(text, resources?)` instead of calling `sendAgentPrompt` from the composer. The only `startWorkspaceAssistant(` call is in `src/lib/assistant/assistant-session.ts`.

## Plan order and handoff

Build in order 01 → 06. Per the user's standing preference, once a plan finishes, the next plan starts without asking for consent (one plan at a time). Merging to `main` is a separate step the user decides on.
