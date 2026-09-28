# ACP MCP Tool Server (Subproject D) — Plan Index

**Spec:** [../../specs/2026-09-28-acp-mcp-tool-server-design.md](../../specs/2026-09-28-acp-mcp-tool-server-design.md)

**Context:** subproject D of the ACP AI-assist feature (see project memory
`project_acp_ai_assist_feature.md`). Depends on subproject A (agent config &
credentials) and subproject B (ACP transport) — both merged to `main`.
Delivers the in-process MCP tool server that lets the ACP agent actually run
requests, edit scripts, and read/write non-secret env vars, plus a minimal
safety valve (opt-in flag + audit trail) folded in from subproject E's
otherwise-deferred scope.

## Plan breakdown — 6 plans

| # | Plan | Crate/area | Depends on |
|---|---|---|---|
| 01 | Domain contracts | `rocket-shared`, `rocket-acp`, `rocket-collection`, `rocket-history` | — |
| 02 | Infra implementations | `rocket-infra` | 01 |
| 03 | App orchestration (`McpToolService`, `RunSource` threading) | `rocket-app` | 01, 02 |
| 04 | HTTP MCP backend (`rmcp` + `axum`) | `src-tauri` | 01, 02, 03 |
| 05 | Stdio shim + full wiring | `src-tauri` | 01, 02, 03, 04 |
| 06 | Frontend checkbox + integration/security tests | frontend, all backend crates | 01–05 |

Each plan file ends with a **Next Plan** section and a **Post-Implementation
Review** section (an Opus-model subagent reviewing that plan's own diff for
interface gaps, code quality, and DDD boundary conformance, with authority to
fix what it finds directly) — the same process this project used for
subprojects A and B. Within a plan, `superpowers:subagent-driven-development`
dispatches a complexity-appropriate subagent per task.

**Cross-plan compilation rule:** each plan must leave `cargo check --workspace -j4`
green at the end of its own tasks, even before later plans land. Where a
plan's trait/type change would otherwise break a not-yet-updated caller in a
later plan's scope, that plan's own tasks include the minimal compiling stub
needed (e.g. accepting-but-ignoring a new parameter in the one existing impl)
— never leave the workspace non-compiling between plans.

## Locked interface contracts

### `rocket-shared` (Plan 01)

```rust
// crates/rocket-shared/src/events.rs — new DomainEvent variant (no timestamp
// field — no existing variant has one; ordering comes from emission order)
DomainEvent::AcpToolInvoked { session_id: String, tool: String, summary: String }
```

```rust
// New file, exact path decided by Plan 01 after checking rocket-shared's
// existing module layout (crates/rocket-shared/src/lib.rs) for where
// small shared enums like this already live — re-export from lib.rs either
// way so rocket-app and rocket-history can both use `rocket_shared::RunSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    #[default]
    Manual,
    Runner,
    LoadTest,
    Flow,
    Agent,
}
```

### `rocket-acp` (Plan 01)

```rust
// crates/rocket-acp/src/mcp_server_spec.rs (new file) — Rocket-owned type,
// deliberately NOT agent_client_protocol::McpServer (rocket-acp must not
// depend on that crate — existing DDD boundary). rocket-infra maps this to
// the real agent-client-protocol type in Plan 02.
#[derive(Debug, Clone)]
pub enum McpServerSpec {
    Http { name: String, url: String, token: String },
    Stdio { name: String, command: String, args: Vec<String>, env: Vec<(String, String)> },
}
```

```rust
// crates/rocket-acp/src/session.rs — AcpSessionClient::start_session gains
// a new trailing parameter. Every other method is unchanged.
#[async_trait::async_trait]
pub trait AcpSessionClient: Send + Sync {
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
    ) -> DomainResult<String>;

    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: String,
        chunk_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> DomainResult<String>;

    async fn end_session(&self, session_id: &str) -> DomainResult<()>;
    async fn end_all_sessions(&self) -> DomainResult<()>;
}
```

Plan 01 must update the sole existing impl, `AcpAgentClient` in
`crates/rocket-infra/src/acp_agent_client.rs`, to accept (but may ignore
until Plan 02) the new `mcp_servers` parameter, and update
`AcpSessionService::start_session` (`crates/rocket-app/src/acp_session_service.rs:65-81`)
to pass `&[]` for it (Plan 03 replaces that empty slice with a real one) —
purely so the workspace keeps compiling; Plan 02 is where real MCP-server
attachment happens.

### `rocket-collection` (Plan 01)

```rust
// crates/rocket-collection/src/settings.rs:35-55 — new field
pub struct CollectionSettings {
    pub docs: Option<String>,
    pub auth: Option<Auth>,
    pub headers: Vec<Header>,
    pub variables: Vec<CollectionVariable>,
    pub sandbox_mode: SandboxMode,
    #[serde(default)]
    pub agent_autonomy_enabled: bool,
}
```

```rust
// crates/rocket-collection/src/repository.rs — new trait method + new type.
// Plan 01 checks whether rocket-scripting's existing phase enum
// (crates/rocket-scripting/src/phase.rs) is reachable from rocket-collection
// without violating that crate's "no rocket-infra/rocket-app" DDD rule; if
// not cleanly reusable, define a local RequestScriptPhase enum instead —
// either way, name it exactly `RequestScriptPhase` with these 3 variants so
// every later plan can rely on it.
pub enum RequestScriptPhase { PreRequest, PostResponse, Tests }

fn save_request_script(
    &self,
    collection: &str,
    request_path: &str,
    phase: RequestScriptPhase,
    body: String,
) -> DomainResult<()>;
```

This is small and self-contained enough that Plan 01 implements it fully
end-to-end (trait method + both real infra implementations), rather than
splitting one method's trait declaration and body across two plans. It must
be implemented on **both** existing `CollectionRepository` implementors:
`FsCollectionRepo` (`crates/rocket-infra/src/fs_collection/mod.rs`)
and `SharedPathCollectionRepo` (`crates/rocket-infra/src/shared_path_collection_repo.rs`)
— following the `save_request_variables` pattern at
`crates/rocket-infra/src/fs_collection/variables.rs:166-189` exactly:
validate name → per-collection mutex guard → resolve request path → read+parse
the OC YAML → mutate the relevant script field → re-serialize → `atomic_write`).
Confirm the real `OcHttpRequest` script field names in
`crates/rocket-infra/src/opencollection/*.rs` before writing this — they
should already be `pre_request_script`/`post_response_script`/`tests` (or
their OC YAML equivalents) since `rocket_collection::Request` already
round-trips these fields today.

### `rocket-history` (Plan 01)

```rust
// crates/rocket-history/src/entry.rs — new field + builder method, NOT a
// new constructor-arg (keeps every existing HistoryEntry::new call site
// compiling unchanged; run_source defaults to RunSource::Manual there).
pub struct HistoryEntry {
    // ...existing fields unchanged...
    #[serde(default)]
    pub run_source: rocket_shared::RunSource,
}
impl HistoryEntry {
    // existing `new(...)` unchanged, sets run_source: RunSource::Manual
    pub fn with_run_source(mut self, source: rocket_shared::RunSource) -> Self {
        self.run_source = source;
        self
    }
}
```

### `rocket-app` (Plan 03)

```rust
// crates/rocket-app/src/execution_service.rs — new field on the existing
// struct, #[serde(default)] so old callers/tests deserializing without it
// still compile and behave as Manual.
pub struct ExecuteRequestInput {
    // ...existing fields unchanged...
    #[serde(default)]
    pub run_source: rocket_shared::RunSource,
}
```

`finish_phases` (`execution_service.rs:1393-1405`) always chains
`.with_run_source(input.run_source)` onto the `HistoryEntry` it builds.

**CORRECTED (found by Plan 03):** `runner_sequence::build_step_input` is
*not* Runner-exclusive as originally stated here — `flow_execution_service`'s
`build_execute_request_input` also calls it, to avoid duplicating the
mapping, so hard-coding `RunSource::Runner` inside `build_step_input` would
mislabel every Flow-executed request's history entry. The real, final
signature gains a trailing `run_source: rocket_shared::RunSource` parameter:

```rust
pub fn build_step_input(
    item: &RunItem,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    request_guard_policy: RequestGuardPolicy,
    run_source: rocket_shared::RunSource,
) -> ExecuteRequestInput
```

`collection_runner_service::run_step` passes `RunSource::Runner`;
`flow_execution_service::build_execute_request_input` passes `RunSource::Flow`.
`McpToolService::run_request` (Plan 03) calls this same function, passing
`RunSource::Agent`. The
Tauri `execute_request` command / manual-send / load-test paths need no
change — `#[serde(default)]` already gives them `RunSource::Manual`.

**CORRECTED (found by Plan 03):** load test never gets `RunSource::LoadTest`
in practice — `run_load_test_command`/`run_load_test_v2_command`
(`src-tauri/src/commands/load_test.rs`) take `ExecuteRequestInput` as a raw
Tauri IPC parameter, exactly like the manual `execute_request` command, and
`RequestExecutionService::run_load_test`/`LoadTestService::run` never call
`finish_phases` or save a `HistoryEntry` for a load-test run — there is no
Rust call site where a `RunSource::LoadTest` tag would have any observable
effect. `RunSource::LoadTest` remains a defined enum variant (for future use)
but nothing in this subproject's plans actually sets it anywhere.

```rust
// crates/rocket-app/src/mcp_tool_service.rs (new file)
pub struct McpToolService {
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
    execution_svc: Arc<RequestExecutionService>,
    event_publisher: Arc<dyn rocket_shared::EventPublisher>,
    test_result_cache: std::sync::Mutex<std::collections::HashMap<(String, String), Vec<rocket_scripting::TestResult>>>,
}

impl McpToolService {
    pub fn new(
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
        execution_svc: Arc<RequestExecutionService>,
        event_publisher: Arc<dyn rocket_shared::EventPublisher>,
    ) -> Self;

    pub fn list_collection_requests(&self, session_id: &str, collection: &str) -> DomainResult<Vec<McpRequestEntry>>;
    pub async fn run_request(&self, session_id: &str, collection: &str, request_path: &str, environment_name: Option<&str>) -> DomainResult<McpRunResult>;
    pub fn edit_script(&self, session_id: &str, collection: &str, request_path: &str, phase: rocket_collection::RequestScriptPhase, body: String) -> DomainResult<()>;
    // CORRECTED (post-Plan-03): these 3 methods gained an explicit `collection: &str`
    // parameter this index originally omitted. Environments are resolved
    // per-(collection, name) everywhere in this codebase, and the autonomy
    // gate itself needs a collection to check `get_settings(collection)`
    // against — there is no way to implement these against a bare
    // `environment_name`/`request_path` alone. Plan 04's tool handlers and
    // Plan 06's tests were reconciled to this real, final shape.
    pub fn get_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str) -> DomainResult<String>;
    pub fn set_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str, value: String) -> DomainResult<()>;
    pub fn get_test_results(&self, session_id: &str, collection: &str, request_path: &str) -> DomainResult<Vec<rocket_scripting::TestResult>>;
}

// Simple DTOs this service returns — plain data, not IPC types (no camelCase
// rename here; that only applies at the src-tauri IPC boundary per this
// repo's serde rule).
pub struct McpRequestEntry { pub path: String, pub name: String, pub method: String, pub url: String }
pub struct McpRunResult { pub status: u16, pub duration_ms: u64, pub test_pass_count: usize, pub test_fail_count: usize }
```

Every method above: (1) first calls a private
`self.check_autonomy_enabled(collection)` — `collection_repo.get_settings(collection)?.agent_autonomy_enabled`, else return a `DomainError` the MCP layer maps to a clear tool-error string; (2) on success, publishes
`DomainEvent::AcpToolInvoked { session_id: session_id.to_string(), tool: "<name>".to_string(), summary: <one-line description of what happened> }`
via `event_publisher`. `get_env_var`/`set_env_var` additionally reject any
`Variable` where `secret == true` (for `get_env_var`, key-not-found and
key-is-secret must both be errors, and must not be distinguishable from each
other in the returned message — otherwise the tool becomes an oracle for
enumerating which env var names are secret-flagged; use one generic "variable
not accessible" error for both cases).

`AcpSessionService::start_session` (`crates/rocket-app/src/acp_session_service.rs`)
gains logic to build a `Vec<McpServerSpec>` (real content is Plan 04/05's
listener/port/token; Plan 03 only defines *where* this list is assembled and
threads it into the now-6-argument `session_client.start_session(...)` call)
gated on the target collection's `agent_autonomy_enabled` — if disabled, pass
`&[]` (chat-only mode, matching subproject C's existing behavior exactly).

**CORRECTED — real, final `AcpSessionService` shape (composed across Plans
03 and 05):**

```rust
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>, // Plan 03
    mcp_sweeper: Box<dyn McpServerSweeper>,                            // Plan 05
    prompt_timeout: Duration,
}

impl AcpSessionService {
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        mcp_sweeper: Box<dyn McpServerSweeper>,
    ) -> Self;

    // Plan 03 first landed this with `collection: Option<&str>` and no
    // `mcp_http` parameter (an always-empty Vec<McpServerSpec> placeholder).
    // Plan 05 changed `collection` to a required `&str` (every real call
    // site always has one by then) and added `mcp_http`, which carries the
    // already-spawned HTTP server's port/token in from the Tauri command
    // layer (spawning the server itself is `src-tauri`'s job, not this
    // service's — see McpHttpServerCredentials below).
    pub async fn start_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        collection: &str,
        mcp_http: Option<McpHttpServerCredentials>,
    ) -> DomainResult<String>;
}

// Plan 05 — plain data, no Tauri types, built by the Tauri command layer.
pub struct McpHttpServerCredentials { pub port: u16, pub token: String }

// Plan 05 — lets AcpSessionService sweep a Tauri-side MCP HTTP server
// registry from every place it already ends a session (normal end_session,
// the bulk end_all_sessions exit sweep, and send_prompt's internal
// timeout-kill branch), without depending on any Tauri type itself.
#[async_trait::async_trait]
pub trait McpServerSweeper: Send + Sync {
    async fn sweep(&self, session_id: &str);
    async fn sweep_all(&self);
}
```

`start_session` always builds *both* an `Http` and a `Stdio` `McpServerSpec`
when autonomy is enabled and `mcp_http` is `Some(..)` — it does not filter by
the agent's advertised capability; that selection happens one layer down,
inside `AcpAgentClient` (`rocket-infra`), which reads
`InitializeResponse.agent_capabilities.mcp_capabilities.http` before mapping
into the real `agent_client_protocol::McpServer` and deciding which variant(s)
the agent actually receives.

### `src-tauri` (Plans 04–05)

```rust
// src-tauri/src/mcp/tool_server.rs (new, Plan 04) — one rmcp ServerHandler
// wired to McpToolService via AppHandle::state, hosted over rmcp's
// Streamable HTTP transport (axum integration), behind a bearer-token
// axum middleware. Bound 127.0.0.1:<random port>, one instance per ACP
// session, torn down when that session ends (folded into the existing
// AcpSessionClient::end_all_sessions / exit-sweep machinery from subproject B).
pub struct McpHttpServerHandle {
    pub port: u16,
    pub token: String,
    // shutdown handle, whatever rmcp/axum's graceful-shutdown mechanism is
}
// CORRECTED (post-Plan-04): gains a `session_id: String` second parameter
// this index originally omitted. Every McpToolService call needs a
// session_id, and RocketMcpToolServer fixes its own copy at construction, so
// there is nowhere else to attach one after this function returns.
// `McpHttpServerHandle::shutdown` is synchronous (`pub fn shutdown(&self)`,
// fire-and-forget via `Notify::notify_one`) — not `async`, not by-value.
pub async fn spawn_mcp_http_server(app_handle: tauri::AppHandle, session_id: String) -> std::io::Result<McpHttpServerHandle>;

// Plan 04 also defines the one and only McpServerRegistry this subproject
// uses (src-tauri/src/mcp/registry.rs), managed as Arc<McpServerRegistry>
// Tauri state so Plan 05's TauriMcpServerSweeper can hold a clone of it:
pub struct McpServerRegistry { /* ... */ }
impl McpServerRegistry {
    pub fn new() -> Self;
    pub fn register(&self, session_id: String, handle: McpHttpServerHandle);
    pub fn end_session(&self, session_id: &str);
    pub fn shutdown_all(&self);
}
```

**Resolving "which session_id" (Plan 05):** the real ACP-protocol session id
is only known after `AcpSessionService::start_session` returns, but the HTTP
server must already be listening before that (its port/token go into the
handshake's `NewSessionRequest`). Plan 05's `start_agent_session` Tauri
command mints a throwaway UUID for `spawn_mcp_http_server`'s `session_id`
parameter (used only to tag that server's own `McpToolService` calls/audit
events for its whole lifetime), then registers the resulting handle in
`McpServerRegistry` keyed by the *real* post-handshake session id once
`start_session` returns — since that real id is what `end_agent_session`/
`send_agent_prompt` address a session by everywhere else.

**`rmcp`/`axum` dependency location:** despite the spec's original prose (and
this index's initial framing), neither crate is a dependency of
`crates/rocket-infra/Cargo.toml` — nothing in `rocket-infra`'s scope (Plan 02)
ends up using either one; `AcpAgentClient`'s MCP-server mapping uses only the
pre-existing `agent-client-protocol` crate. Plan 02 grounds the real, current
versions (`rmcp` `3.5.0`, `axum` `0.8.9`, confirmed via a dry-run `cargo add`
against crates.io) without keeping either as a dependency; Plan 04 adds the
one real `rmcp`/`axum` dependency pair, to `src-tauri/Cargo.toml`, with the
union of features both Plan 04's HTTP server and Plan 05's stdio bridge need
(`server`, `macros`, `transport-streamable-http-server`, `transport-io`,
`client`, `transport-streamable-http-client-reqwest`). Plan 05 does not add a
second `rmcp` line.

```rust
// src-tauri/src/mcp/stdio_bridge.rs (new, Plan 05) — invoked when the
// process is started with the hidden `--acp-mcp-stdio-bridge` flag (checked
// at the very top of `main()`, before normal Tauri bootstrap). Reads
// ROCKET_MCP_PORT/ROCKET_MCP_TOKEN from env (never argv). Runs an rmcp
// stdio-transport server whose tool handlers forward 1:1 to an rmcp
// HTTP-transport client against `http://127.0.0.1:<port>` with that token —
// a generic pass-through, implementing no tool logic of its own.
pub async fn run_stdio_bridge() -> std::io::Result<()>;
```

`TauriEventBus` (`src-tauri/src/tauri_event_bus.rs:36-80`) gains a match arm:
`DomainEvent::AcpToolInvoked { .. } => "agent-tool-invoked"`.

`src-tauri/src/lib.rs` wiring (Plan 05): construct `McpToolService` after
`collection_svc`/`exec_svc` exist (after line ~406) using `Arc`-wrapped
clones of the same repository/service instances already built there —
verify during this task whether those are already `Arc`-wrapped or need
wrapping — then `app.manage(Arc::new(mcp_tool_svc))` alongside the existing
`app.manage(...)` block (lines 471-489). `AcpAgentClient::start_session`
(`crates/rocket-infra/src/acp_agent_client.rs`) reads the (currently
discarded) `InitializeResponse.agent_capabilities.mcp_capabilities.http` and
picks `McpServerSpec::Http` vs `::Stdio` accordingly before mapping to the
real `agent_client_protocol::McpServer` — this decision point technically
lives in Plan 02's file but its *trigger* (spawning the HTTP server first to
get a port/token to put in the spec) depends on Plan 04, so Plan 05 is where
this gets threaded end-to-end and verified working.

### Frontend (Plan 06)

```typescript
// src/lib/tauri-api.ts:62-68 — new optional field
export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
  sandboxMode: SandboxMode;
  agentAutonomyEnabled?: boolean;
}
```

`AgentChatPanel.tsx` gains: on mount (or collection change), call
`getCollectionSettings(collectionName)` to read `agentAutonomyEnabled`; a
checkbox labeled "Allow this agent to run requests and edit files" that calls
`saveCollectionSettings(collectionName, { ...currentSettings, agentAutonomyEnabled: checked })`
on toggle. This is new plumbing for this component (it touches no settings
today) — Plan 06 must read the component's current structure first to place
this without breaking its existing per-tab session-lifecycle logic (see
`project_acp_ai_assist_feature.md` memory's subproject-C lessons about
per-tab state not being safe to key off component-mount lifecycle).

## Review Focus (carries into every plan's tests)

- A tool call against a collection with `agentAutonomyEnabled` false must be
  refused by **every** one of the 6 tools, not just the mutating ones —
  verified by a parametrized test, not one-off per tool.
- `get_env_var`/`set_env_var` must return the *same* error for "key not
  found" and "key is secret" — a test asserting the messages are
  indistinguishable, closing the oracle risk called out above.
- Toggling `agentAutonomyEnabled` mid-session (not just at session start)
  must take effect on the very next tool call — a test that starts a session
  while enabled, disables it, then asserts the next call is refused.
- The Stdio bridge process must never receive the token via argv (only env)
  — a test asserting the spawned command's argument list never contains the
  token string.
- Concurrent `run_request` calls from the agent and a manual UI send against
  the same request file must not corrupt either write — reuses whatever
  per-collection locking `save_request_script`/`save_request` already rely
  on; a test exercising both call paths concurrently against one fixture
  file.

## Execution note for whoever runs these plans

Run in numeric order (01 → 06). Use `superpowers:subagent-driven-development`
or `superpowers:executing-plans` per plan, per each file's own header. After
each plan's tasks are done, run that plan's Post-Implementation Review step
before starting the next plan.
