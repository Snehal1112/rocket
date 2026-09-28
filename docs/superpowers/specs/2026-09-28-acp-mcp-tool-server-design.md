# ACP MCP Tool Server — Design Spec

**Subproject D of the ACP AI-assist feature.** Depends on subproject A (agent
config & credentials, merged) and subproject B (ACP transport, merged).
Builds the in-process MCP tool server that lets the ACP agent actually act on
a Rocket collection — run requests, edit scripts, read/write non-secret env
vars, inspect test results — rather than only exchanging chat text (which
subproject C already ships).

See `project_acp_ai_assist_feature.md` memory for full program context
(subprojects A–E, locked decisions, credential/security lessons from A/B/C).

## Why

Subprojects A–C give the user a chat panel that can talk to a pluggable ACP
agent, but the agent can only suggest text — it cannot run a request, edit a
script, or read/write an environment variable itself. Subproject D is what
makes the loop "fully agentic" per the original brainstorm's locked decision.

## Scope

In scope:

- A localhost HTTP MCP server, hosted in-process, exposing 5 tools:
  `list_collection_requests`, `run_request`, `edit_script`, `get_env_var`,
  `set_env_var`, `get_test_results`.
- A Stdio-transport shim for ACP agents that don't advertise
  `mcp_capabilities.http`.
- A minimal safety valve folded into this subproject (rather than deferred
  entirely to subproject E): a per-collection opt-in flag, checked on every
  tool call, plus an audit trail distinguishing agent-driven actions from
  manual ones.

Out of scope (subproject E, not started):

- The full settings UI/visible "agent running" indicator beyond the
  chat-panel checkbox described here.
- Anything beyond the minimal opt-in gate and audit event described below.

Out of scope (not part of this program at all):

- The true zero-process `McpServer::Acp` transport (gated behind the
  `unstable_mcp_over_acp` feature in `agent-client-protocol` 2.2.0, and
  additionally requires the spawned agent binary to support it). Flagged as
  a future upgrade once both the feature and third-party agent support
  mature.
- Persisting test results into `rocket-history` (they are cached in-memory
  for the lifetime of the ACP session instead — see "Test results" below).

## Architecture

Two transports, one implementation of the actual tool logic — never two
independent tool-handling code paths, to avoid the class of bug where a fix
or a security check lands in one transport's handler but not the other's.

- **HTTP backend (primary).** An async task inside the existing Tauri
  process, started per ACP session (not global), bound to `127.0.0.1` on a
  randomly chosen port, guarded by a random per-session bearer token. Lives
  in `src-tauri` (new module, e.g. `src-tauri/src/mcp/tool_server.rs`) since
  it needs `AppHandle` access to already-managed services — this is Tauri
  wiring, not domain logic, and Tauri-specific code does not belong in
  `rocket-infra`.
- **Stdio shim (fallback).** Rocket's own executable gains a hidden startup
  mode (`--acp-mcp-stdio-bridge`) that does nothing but proxy MCP JSON-RPC
  frames stdio↔HTTP against the already-running HTTP backend. The *agent*
  spawns this process per the `McpServer::Stdio` config Rocket declares; the
  shim is given the port and token via environment variables (never argv —
  matching subproject B's lesson that no credential/secret-bearing value may
  appear in a process's command-line args, since those are visible via
  `ps`). The shim has no direct access to app state; it can only do what the
  token-gated HTTP endpoint already allows.
- **Capability negotiation.** `AcpAgentClient::start_session`
  (`crates/rocket-infra/src/acp_agent_client.rs`) currently awaits but
  discards `InitializeResponse.agent_capabilities.mcp_capabilities`
  (`start_session`, ~line 267-286). It must start reading that response and
  choose `McpServer::Http` when the agent advertises `mcp_capabilities.http`,
  otherwise fall back to `McpServer::Stdio` (spec-mandated: "all agents MUST
  support this transport").
- **Trait boundary.** `AcpSessionClient::start_session`
  (`crates/rocket-acp/src/session.rs`) gains an `mcp_servers: &[McpServerSpec]`
  parameter, where `McpServerSpec` is a small Rocket-owned enum/struct
  defined in `rocket-acp` — not the `agent-client-protocol` crate's own
  `McpServer` type, since `rocket-acp` must not depend on that crate (its
  existing DDD boundary). `AcpAgentClient` (the `rocket-infra` impl) maps
  `McpServerSpec` to the real `agent-client-protocol::McpServer` when it
  builds `NewSessionRequest`.
- **Lifecycle.** The per-session HTTP listener and token are created when
  `AcpSessionService::start_session` runs and torn down when that session
  ends — folded into the same exit-sweep machinery subproject B already
  built (`Weak`-reference in-flight registry + `shutting_down` flag + Unix
  signal listener for `AcpSessionClient::end_all_sessions`), so a crashed or
  killed app can't leave a bound port or a live token behind.

## Tool wiring

The dispatcher holds an `AppHandle` and reaches into already-managed
services at call time via `app_handle.state::<T>()`, rather than threading
`Arc<CollectionService>` / `Arc<RequestExecutionService>` /
`Arc<EnvironmentService>` through `AcpSessionService`'s constructor — that
service is built in `lib.rs` before those services exist in the current
wiring order, so call-time `AppHandle` lookup (the same pattern already used
at `lib.rs:110` and `:712`) is the lower-friction choice.

Every tool call first re-checks the active collection's
`extensions.rocketapi.agentAutonomyEnabled` flag (see "Safety valve" below)
and refuses with an MCP tool-error if it is off. This is checked on every
call, not just once at session start, so toggling the checkbox mid-session
takes effect immediately.

- **`list_collection_requests(collection)`** — calls
  `CollectionRepository::get_summaries` (`crates/rocket-collection/src/repository.rs:19`,
  the lightweight variant with `RequestSummary` leaves, no bodies/auth),
  then walks the `Folder`/`CollectionItem` tree building each request's
  relative path the same way existing frontend sidebar code does (no path
  field exists on `RequestSummary` itself).
- **`run_request(collection, request_path, environment_name)`** — calls
  `CollectionRepository::get_request`, maps the result to
  `ExecuteRequestInput` reusing the field-mapping logic already factored out
  in `runner_sequence::build_step_input`
  (`crates/rocket-app/src/runner_sequence.rs:112-142`), then calls
  `RequestExecutionService::execute`
  (`crates/rocket-app/src/execution_service.rs:1423-1438`). A new
  `run_source: RunSource` field (`enum RunSource { Manual, Runner, LoadTest,
  Flow, Agent }`, `#[serde(default)]` for back-compat) is threaded through
  `ExecuteRequestInput` and into `HistoryEntry`
  (`crates/rocket-history/src/entry.rs`), set to `Agent` by this tool. Every
  other existing call site that builds `ExecuteRequestInput`
  (`build_step_input`, `flow_execution_service.rs`, the Tauri
  `execute_request` command) is updated to pass `RunSource::Manual` /
  `::Runner` / `::Flow` explicitly, so the new field's default only applies
  to code that hasn't been touched. The tool's response caches
  `ExecuteRequestOutput.test_results` in-memory, keyed by (session id,
  request path), for a later `get_test_results` call in the same session.
- **`edit_script(collection, request_path, phase, script_body)`** — a new
  narrow repository method, `save_request_script(collection, path, phase,
  body)`, added to `CollectionRepository`
  (`crates/rocket-collection/src/repository.rs`) and implemented in
  `rocket-infra`, mirroring the existing `save_request_variables` /
  `update_request_docs` pattern of single-field mutation — deliberately
  **not** a full read-modify-write of the whole `Request` (which risks
  clobbering a concurrent manual edit to unrelated fields, and there is no
  optimistic-concurrency mechanism on `Request` to detect that today).
  `phase` maps to the three existing script fields
  (`pre_request_script`/`post_response_script`/`tests`,
  `crates/rocket-collection/src/request.rs:39-43`).
- **`get_env_var(environment_name, key)` / `set_env_var(environment_name,
  key, value)`** — `EnvironmentRepository::get`/`save`
  (`crates/rocket-environment/src/repository.rs:5-10`) only expose
  whole-`Environment` operations today, so both tools fetch the full
  `Environment`, operate on one `Variable` by key, and re-save the whole
  environment. **Any `Variable` with `secret: true` is refused for both
  read and write** — today `secret` is only a display/audit flag
  (`crates/rocket-app/src/env_audit.rs` fires an audit event on secret
  writes but never blocks access), so this tool is the first place in the
  codebase that actually enforces `secret` as an access boundary. This
  boundary also implicitly covers `ExternalSecretBinding`-resolved values —
  those never appear as plain `Variable` entries in the first place, so
  there is nothing additional to filter there.
- **`get_test_results(request_path)`** — reads the in-memory cache
  populated by this session's most recent `run_request` call for that path.
  No `rocket-history` persistence is added for this; `HistoryEntry` has no
  test-result field today and adding one is out of scope for D.

## Safety valve (folded into D from subproject E's original scope)

Subproject E (full settings UI, visible "agent running" indicator) is not
started. Shipping D with no gate at all would mean the agent gets real write
power — run requests, edit scripts, write env vars — with no way to opt out
and no way to tell an agent-driven history entry from a manual one. A
minimal version of E's safety valve is therefore built as part of D:

- **Opt-in flag:** `extensions.rocketapi.agentAutonomyEnabled: bool`
  (default `false`), stored git-shared inside `opencollection.yml`'s
  `extensions` block — the same mechanism and precedent already used for
  the existing Rocket-only `sandbox_mode` setting
  (`crates/rocket-infra/src/fs_collection/settings.rs:14-62`,
  `extensions.rocketapi.sandboxMode`). `extensions` is the OpenCollection
  spec's own designated escape hatch for vendor-specific fields — it is not
  a violation of the spec's `additionalProperties: false` rule, since
  `extensions` itself is a declared, free-form field.
  (Trade-off, explicitly accepted: because this is git-shared, cloning or
  pulling a collection with this flag already set to `true` enables
  autonomous agent writes for whoever opens it next, the same trust model
  already implied by `sandbox_mode`.)
- **UI:** a checkbox in `AgentChatPanel`
  (`src/components/request/AgentChatPanel.tsx`), labeled to the effect of
  "Allow this agent to run requests and edit files", off by default,
  writing the setting above via the existing `save_settings` /
  `CollectionSettings` plumbing.
- **Audit trail:** one new generic domain event,
  `DomainEvent::AcpToolInvoked { session_id, tool, summary, timestamp }`
  (`rocket-shared/src/events.rs`), published by the tool dispatcher for
  *every* tool call regardless of kind (list/run/edit/get/set) — this
  satisfies "audit log" generically without needing bespoke handling in
  every existing domain event type. Additionally, `HistoryEntry.run_source
  = RunSource::Agent` (see above) makes agent-executed *requests*
  specifically distinguishable in history, per the original program
  decomposition's explicit ask ("every agent-executed request is tagged
  distinctly in rocket-history").

## Error handling

- All tool handlers map domain errors to MCP tool-call errors; no `unwrap`
  in any handler, matching the existing IPC-boundary rule.
- The HTTP backend checks the bearer token on every request using a
  constant-time comparison, to avoid a timing side-channel on the token
  value.
- A tool call against a collection with `agentAutonomyEnabled` off returns a
  clear, agent-visible refusal (not a generic 403/500) so the agent can
  explain to the user why it can't act, rather than failing silently.

## Testing

- Per-tool integration tests (`tempfile` collection fixtures, `wiremock`
  where HTTP execution is involved) covering both `agentAutonomyEnabled`
  on and off.
- A focused test for the Stdio shim, asserting it correctly proxies a
  request/response pair against a real (test-bound) instance of the HTTP
  backend and does nothing else.
- A security-focused test asserting that no input to `get_env_var` /
  `set_env_var` can read or write a `Variable` where `secret == true`,
  including edge cases like case-sensitivity of the key and an environment
  containing duplicate-looking keys.
- `cargo check` / scoped `cargo test -p rocket-acp -p rocket-app -p
  rocket-infra -p rocket -j4` (per this repo's `-j4` convention) and `yarn
  tsc --noEmit` for the new chat-panel checkbox.

## Open items intentionally deferred to subproject E

- Full settings-page UI for the opt-in toggle (beyond the chat-panel
  checkbox here) and any visible persistent "agent is running / has run N
  actions" indicator outside the chat panel itself.
- Any richer audit UI (a log viewer) beyond the raw `AcpToolInvoked` event
  and the `HistoryEntry.run_source` tag being queryable.
