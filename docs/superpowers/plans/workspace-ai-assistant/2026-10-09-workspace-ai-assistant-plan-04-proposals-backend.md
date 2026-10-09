# Workspace AI Assistant — Plan 04: Proposals Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every change the workspace assistant wants to make becomes a proposal that the user accepts or rejects. Nothing is written when the agent proposes. An accepted proposal is applied through the same `CollectionService` and `EnvironmentService` paths as a manual edit. A proposal whose target changed since it was made becomes `Stale` and writes nothing. The direct-write MCP tools `edit_script` and `set_env_var` are removed.

**Architecture:** The proposal types are pure domain data in `rocket-acp` (`proposal.rs`, no I/O, no dependency on other domain crates). `ProposalService` in `rocket-app` keeps proposals in memory per session behind a `Mutex`, fingerprints the target item with SHA-256 when a change is proposed and again when it is accepted, and applies accepted changes through an owned `CollectionService` and a per-call `EnvironmentService`. `src-tauri` exposes it to the agent as the `propose_changes` and `list_proposals` MCP tools, and to the frontend as three IPC commands and two events.

**Tech Stack:** Rust (`rocket-acp`, `rocket-shared`, `rocket-app`, `src-tauri`), `sha2` (already a workspace dependency), `rmcp` 3.5 tool macros, `schemars` 1, `serde`; TypeScript (`src/lib/tauri-api.ts`), Vitest.

**Spec:** [`docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`](../../specs/2026-10-09-workspace-ai-assistant-design.md), section 4 (and sections 3 and 7). Locked cross-plan contracts: [`00-plan-index.md`](00-plan-index.md), "Plan 04 — proposals".

## Verified facts

Line numbers are from the worktree before Plans 01–03 land. Those plans edit some of the same files, so every step below anchors its edit by content, not only by line.

- `crates/rocket-acp/Cargo.toml` depends only on `async-trait`, `rocket-shared`, `serde`, `tokio`. `crates/rocket-acp/CLAUDE.md` ("Key Design Points") says the crate has no cross-domain-crate dependencies. So `ScriptPhase` is defined in `rocket-acp` and mapped to `rocket_collection::RequestScriptPhase` in `rocket-app`.
- `rocket_collection::RequestScriptPhase { PreRequest, PostResponse, Tests }` is at `crates/rocket-collection/src/repository.rs:18-23`. `CollectionRepository::save_request_script` has a default body that returns `DomainError::Internal` (`repository.rs:304-314`); `FsCollectionRepo` and `SharedPathCollectionRepo` implement it.
- `CollectionService` (`crates/rocket-app/src/collection_service.rs:14-18`) holds `Box<dyn CollectionRepository>` and publishes one event per mutation: `save_request` (`:165`, `RequestSaved`), `rename_request` (`:179`, changes only the `name` field), `create_folder` (`:290`, `FolderCreated`), `move_item` (`:344`, `ItemMoved`), `get_summaries` (`:50`), `list` (`:41`). It has **no** `save_request_script`; Task 1 adds one.
- `EnvironmentService::new(Box<dyn EnvironmentRepository>, Box<dyn EventPublisher>)` and `save` are at `crates/rocket-app/src/environment_service.rs:14` and `:42`. `save` publishes `EnvironmentSaved` through `env_audit::publish_env_write_events` (`crates/rocket-app/src/env_audit.rs:16-23`).
- Environments are per collection: `EnvironmentRepositoryFactory::for_collection` (`crates/rocket-environment/src/repository.rs:18-19`). `SharedCollectionEnvironmentRepo::with_secret_store` (`crates/rocket-infra/src/shared_collection_environment_repo.rs:43`) keeps secret values on a whole-environment write-back; `src-tauri/src/lib.rs:557-562` already builds `McpToolService` with it.
- Hashing: `rocket-app/Cargo.toml` has no hashing crate. `sha2 = "0.10"` is a workspace dependency (`Cargo.toml:38`, lockfile `0.10.9`) already used by `rocket-infra` (`crates/rocket-infra/Cargo.toml:37`), `rocket-http`, `rocket-git`, `rocket-audit`. Task 1 adds `sha2.workspace = true` to `rocket-app`; no new crate enters `Cargo.lock`.
- `crate::flow_run_cache::saved_request_text(&Request) -> String` (`crates/rocket-app/src/flow_run_cache.rs:434-442`, `pub(crate)`) is the canonical JSON of a request with `uid`, `file_name` and `seq` cleared, because a file without a stored uid gets a fresh one on every load. The request fingerprint reuses it.
- `crate::runner_sequence::folder_dir_name(&Folder) -> &str` is `pub(crate)` at `crates/rocket-app/src/runner_sequence.rs:158`.
- `FsCollectionRepo::get_summaries` fills `RequestSummary.file_name` and `kind` (`crates/rocket-infra/src/fs_collection/tree.rs:279-337`) and `Folder.dir_name` (`tree.rs:131`). `create_folder` rewrites `folder.yml` unconditionally (`crates/rocket-infra/src/fs_collection/folders.rs:181-202`), so creating over an existing folder would reset its settings: the free-target check below prevents that.
- There is no folder rename command. The sidebar renames a folder by `move_item` to a sibling path (`src/components/collections/FolderNode.tsx:116-122`), and `move_item` updates `folder.yml`'s name (`folders.rs:259-275`).
- `McpToolService::edit_script` (`crates/rocket-app/src/mcp_tool_service.rs:274-296`) and `set_env_var` (`:323-353`) write directly; their tests call them at `:615-645`, `:936-964`, `:1036-1130`. `VARIABLE_NOT_ACCESSIBLE` is at `:52`, the private `validate_environment_name` at `:132-147`.
- `src-tauri/src/mcp/tool_server.rs`: `mcp_tool_service()` state lookup `:143-153`, param structs `:157-198`, `to_tool_result<T: Serialize>(DomainResult<T>)` `:210-220`, `parse_phase` `:228-237`, tools `edit_script` `:270-292` and `set_env_var` `:309-323`, `get_info` `:347-357`, test `edit_script_rejects_an_unknown_phase_without_touching_the_service` `:670-686`.
- `rmcp` is `3.5.1` in `Cargo.lock`. `ToolRouter::has_route(&self, name: &str) -> bool` exists (`rmcp-3.5.0/src/handler/server/router/tool.rs:462`), and `#[tool]` methods may take only `&self` (`rmcp-3.5.0/tests/test_json_schema_detection.rs:38`). `schemars = "1"` (`src-tauri/Cargo.toml:86`, lockfile `1.2.1`). `serde` is `1.0.228`; `rename_all_fields` is already used at `src-tauri/src/commands/flow.rs:115`.
- Before Plan 03, the MCP server's `session_id` is a pre-handshake UUID (`src-tauri/src/commands/acp_sessions.rs:73`). Plan 03 Task 2 replaces that field with `binding: Arc<McpSessionBinding>`: every command that spawns a tool server calls `handle.binding.bind(&info.session_id)` right after the handshake, and every tool method passes `self.binding.session_id()`, which is the real ACP id from then on. The frontend and `SessionCleanup` use the real id too, so `ProposalService` needs no id mapping of its own. A call that arrives before `bind` carries the provisional id and runs in Ask mode, where `propose_changes` is refused.
- After Plan 02, `TauriSessionCleanup` lives in `src-tauri/src/agent_session/cleanup.rs` with the fields `mcp_registry: Arc<McpServerRegistry>`, `resources: Arc<SessionResourceRegistry>` and `forget_cache: Box<dyn Fn(&str) + Send + Sync>`. `TauriSessionCleanup::new(mcp_registry, mcp_tool_svc, resources)` builds `forget_cache` as a closure over `McpToolService::forget_session`; `on_session_ended` calls `forget_cache` with the real id and with the pre-handshake id. `src-tauri/src/lib.rs` builds it inside `AcpSessionService::new(...)`, after `mcp_tool_svc` and `mcp_server_registry`; it is not managed state.
- `src-tauri/src/lib.rs`: `collection_svc` built at `:329-335`, `audit_publisher` at `:319`, `app.manage(...)` block `:651-676`, ACP commands in `generate_handler!` at `:920-922`. `TauriEventBus::publish` is an exhaustive `match` with the `AcpToolInvoked` arm at `src-tauri/src/tauri_event_bus.rs:49`. `DomainEvent::AcpToolInvoked` is at `crates/rocket-shared/src/events.rs:550-554`, its wire test at `:1864-1875`.
- `src/lib/tauri-api.ts`: `Header` `:17`, `Body` `:41`, `QueryParam` `:531`, `onAgentSessionFailed` `:2627-2636`. Precedent for a wrapper test with a mocked `invoke`: `src/lib/__tests__/tauri-api.flow-lint.test.ts`.
- `crates/rocket-app/src/test_doubles.rs:791-822`: `RecordingPublisher::new() -> Arc<Self>`, `events()`, and `SharedPublisher(pub Arc<RecordingPublisher>)`. `rocket-app` tests already use `rocket_infra::FsCollectionRepo::new_standalone` (`collection_service.rs:1115`) and `tempfile` (dev-dependencies).
- The `src-tauri` package is named `rocket` (`src-tauri/Cargo.toml:2`).

## Global Constraints

- Always pass `-j4` to `cargo`. Never run `cargo test --workspace`.
- The agent runs only `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check`. Test commands below are for the user to run.
- `cargo check --workspace --all-targets -j4` and `yarn tsc --noEmit` are green at the end of every task.
- Rust: no unwrap calls in production code. Tests use `.expect("message")`.
- Serde `rename_all = "camelCase"` only on the IPC DTOs in `src-tauri/src/commands/agent_proposals.rs`. The `rocket-acp` proposal types carry no serde derives.
- `DomainEvent` new variants keep snake_case fields and the existing `#[serde(tag = "type", rename_all = "camelCase")]`.
- Pending cap: `MAX_PENDING_PER_SESSION = 50`. Resolved proposals kept per session: `MAX_RESOLVED_PER_SESSION = 100` (oldest resolved dropped first).
- Fingerprint: lowercase hex SHA-256 (64 characters) of `saved_request_text(&request)` for a request, and of `"folder\n" + folder_shape(folder)` for a folder.
- Tool result of `propose_changes`: `{"proposal_ids": [...], "status": "queued; awaiting user approval"}`.
- Event channels: `agent-proposal-created`, `agent-proposal-resolved`. Status strings: `pending`, `accepted`, `rejected`, `stale`, `failed`.
- Comments are short full sentences ending with a period.
- Commits: conventional commits through the `dev-workflow-skills:1-git-commit` skill, with explicit paths (never `git add -A`).

## Review Focus

1. **The target changed between propose and accept.** A manual edit of the request after the proposal must make Accept return `Stale` and write nothing, while a reload that only changes `uid`, `file_name` or `seq` must not. Covered in Task 1 by `accept_marks_stale_when_the_request_changed_after_proposing` and `request_fingerprint_ignores_uid_file_name_and_seq`.
2. **A batch with one bad change.** `propose` with one valid and one invalid change must store nothing and publish nothing, and the 50 cap counts only pending proposals. Covered in Task 1 by `a_batch_with_one_invalid_change_stores_nothing` and `the_pending_cap_is_fifty_and_counts_only_pending`.
3. **Secret variables.** Proposing a write to a secret variable is refused, and a variable that became secret after the proposal makes Accept `Failed` without writing. A failing environment save marks the proposal `Failed` with the message. Covered in Task 1 by `set_env_var_refuses_a_secret_variable`, `set_env_var_fails_without_writing_when_the_variable_became_secret`, `a_failed_write_marks_the_proposal_failed`.
4. **Auth sent by the agent.** An `auth` field in `create_request` or `update_request` must be refused, not silently dropped. Covered in Task 2 by `a_patch_or_new_request_with_an_auth_field_is_refused`.
5. **Session ids and cleanup.** Proposals made through the MCP server must be listed, accepted and cleared under the real ACP session id. Plan 03's `McpSessionBinding` makes the tool server report that id, so the tools pass `self.binding.session_id()`, and `TauriSessionCleanup` clears proposals on every end path. Covered in Task 1 by `clear_session_drops_only_that_sessions_proposals` and in Task 2 by `list_proposals_uses_the_bound_session_id` and `propose_changes_is_refused_in_ask_mode`.

---

### Task 1: Proposal domain types, events and `ProposalService`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-acp/src/proposal.rs`
- Modify: `crates/rocket-acp/src/lib.rs` (module list, lines 1-6 before Plan 01)
- Modify: `crates/rocket-acp/CLAUDE.md` (Module Map table)
- Modify: `crates/rocket-shared/src/events.rs` (after `AcpToolInvoked`, `:550-554`; tests after `acp_tool_invoked_wire_shape`, `:1864-1875`)
- Modify: `src-tauri/src/tauri_event_bus.rs` (after the `AcpToolInvoked` arm, `:49`)
- Modify: `crates/rocket-app/Cargo.toml` (after `ulid.workspace = true`, `:26`)
- Modify: `crates/rocket-app/src/collection_service.rs` (import `:5-8`; new method after `move_item`, `:344-360`)
- Create: `crates/rocket-app/src/proposal_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (`pub mod` after `oauth2_service`, `:38`; `pub use` after `oauth2_service`, `:85`)
- Modify: `crates/rocket-app/CLAUDE.md` (Public Types table)

**Interfaces:**
- Consumes: `CollectionService::{list, get_summaries, get_request, save_request, rename_request, create_folder, move_item}`, `EnvironmentService::{new, save}`, `EnvironmentRepositoryFactory::for_collection`, `saved_request_text`, `folder_dir_name`, `rocket_collection::request_filename_for`.
- Produces (exact signatures):
  ```rust
  // rocket_acp::proposal
  pub enum ScriptPhase { PreRequest, PostResponse, Tests }            // Copy, Eq; fn label(self) -> &'static str
  pub struct ProposedRequest { pub name: String, pub method: HttpMethod, pub url: String,
      pub headers: Vec<Header>, pub query_params: Vec<QueryParam>, pub body: Option<Body>,
      pub docs: Option<String>, pub pre_request_script: Option<String>,
      pub post_response_script: Option<String>, pub tests: Option<String> }
  pub struct RequestPatch { pub method: Option<HttpMethod>, pub url: Option<String>,
      pub headers: Option<Vec<Header>>, pub query_params: Option<Vec<QueryParam>>,
      pub body: Option<Body>, pub docs: Option<String> }                // Default; changed_fields(), is_empty()
  pub enum ProposedChange { /* the seven variants of the index, unchanged */ }
  impl ProposedChange { pub fn collection(&self) -> &str; pub fn base_fingerprint(&self) -> Option<&str>;
      pub fn set_base_fingerprint(&mut self, fingerprint: String); pub fn summary(&self) -> String; }
  pub enum ProposalStatus { Pending, Accepted, Rejected, Stale, Failed { message: String } }
  impl ProposalStatus { pub fn as_str(&self) -> &'static str; pub fn message(&self) -> Option<&str>; }
  pub struct AgentProposal { pub id: String, pub session_id: String, pub change: ProposedChange,
      pub summary: String, pub status: ProposalStatus, pub created_at_ms: i64 }
  impl AgentProposal { pub fn new(id: String, session_id: String, change: ProposedChange, created_at_ms: i64) -> Self; }

  // rocket_shared::events::DomainEvent
  AcpProposalCreated { session_id: String, proposal_id: String, summary: String }
  AcpProposalResolved { session_id: String, proposal_id: String, status: String }

  // rocket_app::CollectionService
  pub fn save_request_script(&self, collection: &str, request_path: &str,
      phase: RequestScriptPhase, body: String) -> DomainResult<()>;

  // rocket_app::ProposalService (re-exported at the crate root)
  pub fn new(collections: CollectionService,
      environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
      events: Arc<dyn EventPublisher>) -> Self;
  pub fn propose(&self, session_id: &str, changes: Vec<ProposedChange>) -> DomainResult<Vec<String>>;
  pub fn list(&self, session_id: &str) -> Vec<AgentProposal>;
  pub fn accept(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal>;
  pub fn reject(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal>;
  pub fn clear_session(&self, session_id: &str);
  pub const MAX_PENDING_PER_SESSION: usize = 50;
  ```

- [ ] **Step 1: Write the failing domain tests**

Create `crates/rocket-acp/src/proposal.rs` with only the imports and the test module:

```rust
//! Changes the workspace assistant proposes. A proposal is only data. It is
//! applied by `rocket-app`'s `ProposalService` after the user accepts it, so
//! this module has no I/O and no dependency on other domain crates.

use rocket_shared::types::{Body, Header, HttpMethod, QueryParam};

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_script() -> ProposedChange {
        ProposedChange::EditScript {
            collection: "demo".into(),
            request_path: "users/get-users.yml".into(),
            phase: ScriptPhase::Tests,
            body: "rok.test('secret-body-text', () => {});".into(),
            base_fingerprint: String::new(),
        }
    }

    #[test]
    fn a_new_proposal_is_pending_and_carries_the_change_summary() {
        let proposal = AgentProposal::new("p1".into(), "s1".into(), edit_script(), 42);
        assert_eq!(proposal.status, ProposalStatus::Pending);
        assert_eq!(proposal.created_at_ms, 42);
        assert_eq!(
            proposal.summary,
            "Edit the tests script of 'users/get-users.yml' in demo"
        );
    }

    #[test]
    fn summaries_never_contain_script_bodies_or_variable_values() {
        let set_var = ProposedChange::SetEnvVar {
            collection: "demo".into(),
            environment: "dev".into(),
            key: "HOST".into(),
            value: "value-that-must-not-leak".into(),
        };
        assert!(!set_var.summary().contains("value-that-must-not-leak"));
        assert!(!edit_script().summary().contains("secret-body-text"));
        assert_eq!(
            set_var.summary(),
            "Set variable 'HOST' in environment 'dev' of demo"
        );
    }

    #[test]
    fn set_base_fingerprint_only_touches_update_style_changes() {
        let mut edit = edit_script();
        edit.set_base_fingerprint("abc".into());
        assert_eq!(edit.base_fingerprint(), Some("abc"));

        let mut create = ProposedChange::CreateFolder {
            collection: "demo".into(),
            parent_path: String::new(),
            name: "reports".into(),
        };
        create.set_base_fingerprint("abc".into());
        assert_eq!(create.base_fingerprint(), None);
        assert_eq!(create.collection(), "demo");
        assert_eq!(create.summary(), "Create folder 'reports' in demo");
    }

    #[test]
    fn a_patch_lists_the_fields_it_sets_in_a_fixed_order() {
        assert!(RequestPatch::default().is_empty());
        let patch = RequestPatch {
            url: Some("https://x".into()),
            method: Some(HttpMethod::Post),
            ..RequestPatch::default()
        };
        assert!(!patch.is_empty());
        assert_eq!(patch.changed_fields(), vec!["method", "url"]);
    }

    #[test]
    fn move_to_the_root_names_the_root() {
        let change = ProposedChange::MoveItem {
            collection: "demo".into(),
            from_path: "old/a.yml".into(),
            to_folder: String::new(),
            base_fingerprint: String::new(),
        };
        assert_eq!(
            change.summary(),
            "Move 'old/a.yml' to the collection root in demo"
        );
    }

    #[test]
    fn status_names_are_stable_wire_strings() {
        assert_eq!(ProposalStatus::Pending.as_str(), "pending");
        assert_eq!(ProposalStatus::Accepted.as_str(), "accepted");
        assert_eq!(ProposalStatus::Rejected.as_str(), "rejected");
        assert_eq!(ProposalStatus::Stale.as_str(), "stale");
        let failed = ProposalStatus::Failed {
            message: "disk full".into(),
        };
        assert_eq!(failed.as_str(), "failed");
        assert_eq!(failed.message(), Some("disk full"));
        assert_eq!(ProposalStatus::Stale.message(), None);
    }

    #[test]
    fn script_phase_labels() {
        assert_eq!(ScriptPhase::PreRequest.label(), "pre-request");
        assert_eq!(ScriptPhase::PostResponse.label(), "post-response");
        assert_eq!(ScriptPhase::Tests.label(), "tests");
    }
}
```

Wire the module into `crates/rocket-acp/src/lib.rs`. Add the `pub mod` line between `mcp_server_spec` and `session` (Plan 01 adds `prompt`, `session_info` and `update`; keep the list alphabetical), and the re-export after `McpServerSpec`:

```rust
pub mod proposal;
```

```rust
pub use proposal::{
    AgentProposal, ProposalStatus, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
```

- [ ] **Step 2: Check that the tests fail to compile**

Run: `cargo check -p rocket-acp --all-targets -j4`
Expected: FAIL with `cannot find type 'ProposedChange' in this scope` (and the other new names).

- [ ] **Step 3: Implement the domain types**

In `crates/rocket-acp/src/proposal.rs`, insert this between the `use` line and `#[cfg(test)]`:

```rust
/// The script field an `EditScript` change targets. `rocket-app` maps it to
/// `rocket_collection::RequestScriptPhase`, because this crate must not
/// depend on `rocket-collection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}

impl ScriptPhase {
    /// Short label used in proposal summaries.
    pub fn label(self) -> &'static str {
        match self {
            ScriptPhase::PreRequest => "pre-request",
            ScriptPhase::PostResponse => "post-response",
            ScriptPhase::Tests => "tests",
        }
    }
}

/// A new HTTP request the agent wants to create. There is no auth field on
/// purpose: a new request inherits auth from its folder or collection.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedRequest {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub docs: Option<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests: Option<String>,
}

/// A partial update of an existing HTTP request. `None` keeps a field as it
/// is. Auth cannot be patched in v1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestPatch {
    pub method: Option<HttpMethod>,
    pub url: Option<String>,
    pub headers: Option<Vec<Header>>,
    pub query_params: Option<Vec<QueryParam>>,
    pub body: Option<Body>,
    pub docs: Option<String>,
}

impl RequestPatch {
    /// Names of the fields this patch sets, in a fixed order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.method.is_some() {
            fields.push("method");
        }
        if self.url.is_some() {
            fields.push("url");
        }
        if self.headers.is_some() {
            fields.push("headers");
        }
        if self.query_params.is_some() {
            fields.push("query params");
        }
        if self.body.is_some() {
            fields.push("body");
        }
        if self.docs.is_some() {
            fields.push("docs");
        }
        fields
    }

    /// True when the patch sets no field.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }
}

/// One proposed operation. Paths are relative to the collection root, and
/// `""` is the root. Update-style changes carry `base_fingerprint`, the hash
/// of the item's stored form when the change was proposed.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposedChange {
    CreateFolder {
        collection: String,
        parent_path: String,
        name: String,
    },
    CreateRequest {
        collection: String,
        folder_path: String,
        request: ProposedRequest,
    },
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatch,
        base_fingerprint: String,
    },
    EditScript {
        collection: String,
        request_path: String,
        phase: ScriptPhase,
        body: String,
        base_fingerprint: String,
    },
    MoveItem {
        collection: String,
        from_path: String,
        to_folder: String,
        base_fingerprint: String,
    },
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
        base_fingerprint: String,
    },
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

impl ProposedChange {
    /// The collection this change targets.
    pub fn collection(&self) -> &str {
        match self {
            ProposedChange::CreateFolder { collection, .. }
            | ProposedChange::CreateRequest { collection, .. }
            | ProposedChange::UpdateRequest { collection, .. }
            | ProposedChange::EditScript { collection, .. }
            | ProposedChange::MoveItem { collection, .. }
            | ProposedChange::RenameItem { collection, .. }
            | ProposedChange::SetEnvVar { collection, .. } => collection,
        }
    }

    /// The fingerprint of an update-style change, `None` for the others.
    pub fn base_fingerprint(&self) -> Option<&str> {
        match self {
            ProposedChange::UpdateRequest {
                base_fingerprint, ..
            }
            | ProposedChange::EditScript {
                base_fingerprint, ..
            }
            | ProposedChange::MoveItem {
                base_fingerprint, ..
            }
            | ProposedChange::RenameItem {
                base_fingerprint, ..
            } => Some(base_fingerprint),
            ProposedChange::CreateFolder { .. }
            | ProposedChange::CreateRequest { .. }
            | ProposedChange::SetEnvVar { .. } => None,
        }
    }

    /// Sets the fingerprint of an update-style change. Creates and
    /// environment changes have none, so the call does nothing for them.
    pub fn set_base_fingerprint(&mut self, fingerprint: String) {
        match self {
            ProposedChange::UpdateRequest {
                base_fingerprint, ..
            }
            | ProposedChange::EditScript {
                base_fingerprint, ..
            }
            | ProposedChange::MoveItem {
                base_fingerprint, ..
            }
            | ProposedChange::RenameItem {
                base_fingerprint, ..
            } => *base_fingerprint = fingerprint,
            ProposedChange::CreateFolder { .. }
            | ProposedChange::CreateRequest { .. }
            | ProposedChange::SetEnvVar { .. } => {}
        }
    }

    /// One line for the proposal card and the agent. It never contains a
    /// script body, a header value or a variable value.
    pub fn summary(&self) -> String {
        match self {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => format!(
                "Create folder '{}' in {collection}",
                display_path(parent_path, name)
            ),
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => format!(
                "Create request {} '{}' in {collection}",
                request.method,
                display_path(folder_path, &request.name)
            ),
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => format!(
                "Update {} of '{request_path}' in {collection}",
                patch.changed_fields().join(", ")
            ),
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                ..
            } => format!(
                "Edit the {} script of '{request_path}' in {collection}",
                phase.label()
            ),
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => {
                let target = match to_folder.trim_matches('/') {
                    "" => "the collection root".to_string(),
                    folder => format!("'{folder}'"),
                };
                format!("Move '{from_path}' to {target} in {collection}")
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => format!("Rename '{path}' to '{new_name}' in {collection}"),
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                ..
            } => format!("Set variable '{key}' in environment '{environment}' of {collection}"),
        }
    }
}

/// `parent/name`, or just `name` at the collection root.
fn display_path(parent: &str, name: &str) -> String {
    match parent.trim_matches('/') {
        "" => name.to_string(),
        parent => format!("{parent}/{name}"),
    }
}

/// Where a proposal stands. Only a `Pending` proposal can be accepted or
/// rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
    /// The target changed after the proposal was made. Nothing was written.
    Stale,
    /// Applying failed. Nothing was written.
    Failed { message: String },
}

impl ProposalStatus {
    /// The wire name used in events, DTOs and tool results.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pending",
            ProposalStatus::Accepted => "accepted",
            ProposalStatus::Rejected => "rejected",
            ProposalStatus::Stale => "stale",
            ProposalStatus::Failed { .. } => "failed",
        }
    }

    /// The failure message, if the proposal failed.
    pub fn message(&self) -> Option<&str> {
        match self {
            ProposalStatus::Failed { message } => Some(message),
            _ => None,
        }
    }
}

/// One proposed change and its state, owned by one assistant session.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentProposal {
    pub id: String,
    pub session_id: String,
    pub change: ProposedChange,
    pub summary: String,
    pub status: ProposalStatus,
    pub created_at_ms: i64,
}

impl AgentProposal {
    /// A new pending proposal. The summary is taken from the change.
    pub fn new(
        id: String,
        session_id: String,
        change: ProposedChange,
        created_at_ms: i64,
    ) -> Self {
        let summary = change.summary();
        Self {
            id,
            session_id,
            change,
            summary,
            status: ProposalStatus::Pending,
            created_at_ms,
        }
    }
}
```

- [ ] **Step 4: Write the failing event wire-shape tests**

In `crates/rocket-shared/src/events.rs`, add after the `acp_tool_invoked_wire_shape` test:

```rust
    #[test]
    fn acp_proposal_created_wire_shape() {
        let event = DomainEvent::AcpProposalCreated {
            session_id: "sess-1".into(),
            proposal_id: "p-1".into(),
            summary: "Create folder 'reports' in demo".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpProposalCreated","session_id":"sess-1","proposal_id":"p-1","summary":"Create folder 'reports' in demo"}"#
        );
    }

    #[test]
    fn acp_proposal_resolved_wire_shape() {
        let event = DomainEvent::AcpProposalResolved {
            session_id: "sess-1".into(),
            proposal_id: "p-1".into(),
            status: "stale".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpProposalResolved","session_id":"sess-1","proposal_id":"p-1","status":"stale"}"#
        );
    }
```

- [ ] **Step 5: Implement the event variants and their channels**

In `crates/rocket-shared/src/events.rs`, add directly after the `AcpToolInvoked { .. }` variant (and after any ACP variants Plan 01 placed there):

```rust
    /// Emitted once per proposal the assistant queues. Nothing was written.
    AcpProposalCreated {
        session_id: String,
        proposal_id: String,
        summary: String,
    },
    /// Emitted when a proposal leaves `pending`. `status` is `accepted`,
    /// `rejected`, `stale` or `failed`.
    AcpProposalResolved {
        session_id: String,
        proposal_id: String,
        status: String,
    },
```

In `src-tauri/src/tauri_event_bus.rs`, add after the `DomainEvent::AcpToolInvoked { .. } => "agent-tool-invoked",` arm:

```rust
            DomainEvent::AcpProposalCreated { .. } => "agent-proposal-created",
            DomainEvent::AcpProposalResolved { .. } => "agent-proposal-resolved",
```

- [ ] **Step 6: Check the domain and event changes**

Run: `cargo check -p rocket-acp -p rocket-shared -p rocket --all-targets -j4`
Expected: PASS.

- [ ] **Step 7: Add `CollectionService::save_request_script` and the `sha2` dependency**

In `crates/rocket-app/Cargo.toml`, after `ulid.workspace = true`:

```toml
sha2.workspace = true
```

In `crates/rocket-app/src/collection_service.rs`, add `RequestScriptPhase` to the `rocket_collection` import:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSummary, CollectionVariable, FolderSettings,
    GraphQlRequest, GrpcRequest, Request, RequestKind, RequestScriptPhase, WebSocketRequest,
};
```

Add after `move_item`:

```rust
    /// Overwrites one script phase of a request, then tells listeners the
    /// request changed. The other phases and fields stay as they are.
    pub fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.repo
            .save_request_script(collection, request_path, phase, body)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: request_path.to_string(),
        });
        Ok(())
    }
```

- [ ] **Step 8: Write the failing `ProposalService` tests**

Create `crates/rocket-app/src/proposal_service.rs` with the module doc, the imports and the test module. Register it in `crates/rocket-app/src/lib.rs`: `pub mod proposal_service;` after `pub mod oauth2_service;`, and `pub use proposal_service::ProposalService;` after `pub use oauth2_service::OAuth2Service;`.

```rust
//! Holds the assistant's proposed workspace changes until the user accepts
//! or rejects them. Nothing is written when a change is proposed. An
//! accepted change is applied through `CollectionService` or
//! `EnvironmentService`, the same paths as a manual edit, so name checks
//! and events stay the same. Proposals live in memory only.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rocket_acp::proposal::{
    AgentProposal, ProposalStatus, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_collection::{
    request_filename_for, CollectionItem, Folder, Request, RequestScriptPhase,
};
use rocket_environment::{EnvironmentRepositoryFactory, Variable};
use rocket_shared::description::Documentation;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::BodyMode;
use sha2::{Digest, Sha256};

use crate::collection_service::CollectionService;
use crate::environment_service::EnvironmentService;
use crate::flow_run_cache::saved_request_text;
use crate::runner_sequence::folder_dir_name;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;

    use rocket_collection::CollectionRepository;
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_shared::types::{Header, HttpMethod};

    use crate::test_doubles::{RecordingPublisher, SharedPublisher};

    /// Environments shared by every repository handle the factory gives out.
    #[derive(Default)]
    struct MemoryEnvs {
        envs: StdMutex<HashMap<String, Environment>>,
        fail_saves: AtomicBool,
    }

    struct MemoryEnvFactory(Arc<MemoryEnvs>);
    struct MemoryEnvRepo(Arc<MemoryEnvs>);

    impl EnvironmentRepositoryFactory for MemoryEnvFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MemoryEnvRepo(Arc::clone(&self.0)))
        }
    }

    impl EnvironmentRepository for MemoryEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.envs.lock().expect("lock").values().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .envs
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.to_string()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            if self.0.fail_saves.load(Ordering::SeqCst) {
                return Err(DomainError::Io("disk full".to_string()));
            }
            self.0
                .envs
                .lock()
                .expect("lock")
                .insert(env.name.clone(), env.clone());
            Ok(())
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.envs.lock().expect("lock").remove(name);
            Ok(())
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        repo: rocket_infra::FsCollectionRepo,
        envs: Arc<MemoryEnvs>,
        events: Arc<RecordingPublisher>,
        svc: ProposalService,
    }

    /// A "demo" collection with one request, `get-users.yml`, and a "dev"
    /// environment with a plain `HOST` and a secret `TOKEN`.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("demo").expect("create collection");
        repo.save_request(
            "demo",
            "get-users.yml",
            &Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users"),
        )
        .expect("save request");

        let envs = Arc::new(MemoryEnvs::default());
        let mut dev = Environment::new("dev");
        dev.set_variable(Variable::new("HOST", "api.example.com"));
        let mut token = Variable::new("TOKEN", "s3cr3t");
        token.secret = true;
        dev.set_variable(token);
        envs.envs.lock().expect("lock").insert("dev".into(), dev);

        let events = RecordingPublisher::new();
        let svc = ProposalService::new(
            CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    dir.path().to_path_buf(),
                )),
                Box::new(SharedPublisher(Arc::clone(&events))),
            ),
            Arc::new(MemoryEnvFactory(Arc::clone(&envs))),
            events.clone(),
        );
        Fixture {
            _dir: dir,
            repo,
            envs,
            events,
            svc,
        }
    }

    fn folder(name: &str) -> ProposedChange {
        ProposedChange::CreateFolder {
            collection: "demo".into(),
            parent_path: String::new(),
            name: name.into(),
        }
    }

    fn url_patch(url: &str) -> ProposedChange {
        ProposedChange::UpdateRequest {
            collection: "demo".into(),
            request_path: "get-users.yml".into(),
            patch: RequestPatch {
                url: Some(url.into()),
                ..RequestPatch::default()
            },
            base_fingerprint: String::new(),
        }
    }

    fn set_var(key: &str, value: &str) -> ProposedChange {
        ProposedChange::SetEnvVar {
            collection: "demo".into(),
            environment: "dev".into(),
            key: key.into(),
            value: value.into(),
        }
    }

    fn resolved_statuses(events: &RecordingPublisher) -> Vec<String> {
        events
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::AcpProposalResolved { status, .. } => Some(status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn propose_queues_a_pending_proposal_and_writes_nothing() {
        let f = fixture();
        let ids = f.svc.propose("s1", vec![folder("reports")]).expect("propose");
        assert_eq!(ids.len(), 1);
        let listed = f.svc.list("s1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, ids[0]);
        assert_eq!(listed[0].status, ProposalStatus::Pending);
        assert!(!f._dir.path().join("demo").join("reports").exists());
        assert!(f.events.events().iter().any(|e| matches!(
            e,
            DomainEvent::AcpProposalCreated { proposal_id, .. } if *proposal_id == ids[0]
        )));
    }

    #[test]
    fn accepting_a_create_folder_applies_it_through_collection_service() {
        let f = fixture();
        let ids = f.svc.propose("s1", vec![folder("reports")]).expect("propose");
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Accepted);
        assert!(f._dir.path().join("demo").join("reports").is_dir());
        assert!(f
            .events
            .events()
            .iter()
            .any(|e| matches!(e, DomainEvent::FolderCreated { path, .. } if path == "reports")));
        assert_eq!(resolved_statuses(&f.events), vec!["accepted".to_string()]);
    }

    #[test]
    fn creating_over_an_existing_folder_is_refused() {
        let f = fixture();
        f.repo.create_folder("demo", "reports").expect("create folder");
        let err = f
            .svc
            .propose("s1", vec![folder("reports")])
            .expect_err("the folder already exists");
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[test]
    fn accepting_an_update_changes_only_the_patched_fields() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![url_patch("https://api.example.com/v2/users")])
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.url, "https://api.example.com/v2/users");
        assert_eq!(saved.name, "Get Users");
        assert_eq!(saved.method, HttpMethod::Get);
    }

    #[test]
    fn accept_marks_stale_when_the_request_changed_after_proposing() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![url_patch("https://api.example.com/v2/users")])
            .expect("propose");
        let mut manual = f.repo.get_request("demo", "get-users.yml").expect("load");
        manual.url = "https://manual.example.com".into();
        f.repo
            .save_request("demo", "get-users.yml", &manual)
            .expect("manual save");

        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.url, "https://manual.example.com", "a stale proposal writes nothing");
        assert_eq!(resolved_statuses(&f.events), vec!["stale".to_string()]);
    }

    #[test]
    fn request_fingerprint_ignores_uid_file_name_and_seq() {
        let a = Request::new("A", HttpMethod::Get, "https://x/a");
        let mut b = a.clone();
        b.uid = "another-uid".into();
        b.file_name = Some("a.yml".into());
        b.seq = Some(3);
        assert_eq!(request_fingerprint(&a), request_fingerprint(&b));
        assert_eq!(request_fingerprint(&a).len(), 64);
        b.headers.push(Header::new("Accept", "application/json"));
        assert_ne!(request_fingerprint(&a), request_fingerprint(&b));
    }

    #[test]
    fn accepting_an_edit_script_saves_only_that_phase() {
        let f = fixture();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::EditScript {
                    collection: "demo".into(),
                    request_path: "get-users.yml".into(),
                    phase: ScriptPhase::Tests,
                    body: "rok.test('ok', () => {});".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_request("demo", "get-users.yml").expect("load");
        assert_eq!(saved.tests.as_deref(), Some("rok.test('ok', () => {});"));
        assert_eq!(saved.pre_request_script, None);
        assert!(f.events.events().iter().any(|e| matches!(
            e,
            DomainEvent::RequestSaved { path, .. } if path == "get-users.yml"
        )));
    }

    #[test]
    fn move_and_rename_are_applied() {
        let f = fixture();
        f.repo.create_folder("demo", "archive").expect("create folder");
        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::MoveItem {
                    collection: "demo".into(),
                    from_path: "get-users.yml".into(),
                    to_folder: "archive".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose move");
        f.svc.accept("s1", &ids[0]).expect("accept move");
        assert!(f.repo.get_request("demo", "archive/get-users.yml").is_ok());

        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::RenameItem {
                    collection: "demo".into(),
                    path: "archive/get-users.yml".into(),
                    new_name: "List Users".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose rename");
        f.svc.accept("s1", &ids[0]).expect("accept rename");
        let saved = f.repo.get_request("demo", "archive/get-users.yml").expect("load");
        assert_eq!(saved.name, "List Users");

        let ids = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::RenameItem {
                    collection: "demo".into(),
                    path: "archive".into(),
                    new_name: "old".into(),
                    base_fingerprint: String::new(),
                }],
            )
            .expect("propose folder rename");
        f.svc.accept("s1", &ids[0]).expect("accept folder rename");
        assert!(f.repo.get_request("demo", "old/get-users.yml").is_ok());
    }

    #[test]
    fn a_batch_with_one_invalid_change_stores_nothing() {
        let f = fixture();
        let missing = ProposedChange::UpdateRequest {
            collection: "demo".into(),
            request_path: "missing.yml".into(),
            patch: RequestPatch {
                url: Some("https://x".into()),
                ..RequestPatch::default()
            },
            base_fingerprint: String::new(),
        };
        let err = f
            .svc
            .propose("s1", vec![folder("reports"), missing])
            .expect_err("the second change targets a missing request");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert!(f.svc.list("s1").is_empty());
        assert!(f.events.events().is_empty());
    }

    #[test]
    fn the_pending_cap_is_fifty_and_counts_only_pending() {
        let f = fixture();
        let changes: Vec<ProposedChange> = (0..MAX_PENDING_PER_SESSION)
            .map(|i| folder(&format!("f{i}")))
            .collect();
        let ids = f.svc.propose("s1", changes).expect("fifty fit");
        let err = f
            .svc
            .propose("s1", vec![folder("one-more")])
            .expect_err("the fifty-first is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        f.svc.reject("s1", &ids[0]).expect("reject one");
        f.svc
            .propose("s1", vec![folder("one-more")])
            .expect("a rejected proposal frees a slot");
    }

    #[test]
    fn a_collection_outside_the_workspace_is_refused() {
        let f = fixture();
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "elsewhere".into(),
                    parent_path: String::new(),
                    name: "x".into(),
                }],
            )
            .expect_err("not in the workspace");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_path_that_climbs_out_of_the_collection_is_refused() {
        let f = fixture();
        let err = f
            .svc
            .propose(
                "s1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: "../other".into(),
                    name: "x".into(),
                }],
            )
            .expect_err("traversal");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn set_env_var_refuses_a_secret_variable() {
        let f = fixture();
        let err = f
            .svc
            .propose("s1", vec![set_var("TOKEN", "new")])
            .expect_err("secret");
        assert!(err.to_string().contains("not accessible"));
        assert!(f.svc.list("s1").is_empty());
    }

    #[test]
    fn set_env_var_writes_a_plain_variable_and_can_add_one() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com"), set_var("REGION", "eu")])
            .expect("propose");
        for id in &ids {
            f.svc.accept("s1", id).expect("accept");
        }
        let envs = f.envs.envs.lock().expect("lock");
        let dev = envs.get("dev").expect("dev");
        assert_eq!(dev.get_value("HOST"), Some("api2.example.com"));
        assert_eq!(dev.get_value("REGION"), Some("eu"));
        let token = dev.variables.iter().find(|v| v.key == "TOKEN").expect("token");
        assert_eq!(token.value, "s3cr3t", "other variables are kept");
    }

    #[test]
    fn set_env_var_fails_without_writing_when_the_variable_became_secret() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        {
            let mut envs = f.envs.envs.lock().expect("lock");
            let dev = envs.get_mut("dev").expect("dev");
            if let Some(host) = dev.variables.iter_mut().find(|v| v.key == "HOST") {
                host.secret = true;
            }
        }
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert!(matches!(resolved.status, ProposalStatus::Failed { .. }));
        assert!(resolved
            .status
            .message()
            .is_some_and(|m| m.contains("not accessible")));
        let envs = f.envs.envs.lock().expect("lock");
        let host = envs["dev"].variables.iter().find(|v| v.key == "HOST").expect("host");
        assert_eq!(host.value, "api.example.com");
    }

    #[test]
    fn a_failed_write_marks_the_proposal_failed() {
        let f = fixture();
        let ids = f
            .svc
            .propose("s1", vec![set_var("HOST", "api2.example.com")])
            .expect("propose");
        f.envs.fail_saves.store(true, Ordering::SeqCst);
        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert!(resolved.status.message().is_some_and(|m| m.contains("disk full")));
        assert_eq!(resolved_statuses(&f.events), vec!["failed".to_string()]);
    }

    #[test]
    fn reject_marks_rejected_and_a_second_answer_is_refused() {
        let f = fixture();
        let ids = f.svc.propose("s1", vec![folder("reports")]).expect("propose");
        let resolved = f.svc.reject("s1", &ids[0]).expect("reject");
        assert_eq!(resolved.status, ProposalStatus::Rejected);
        assert!(!f._dir.path().join("demo").join("reports").exists());
        let err = f.svc.accept("s1", &ids[0]).expect_err("already answered");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = f.svc.accept("s1", "no-such-id").expect_err("unknown id");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn clear_session_drops_only_that_sessions_proposals() {
        let f = fixture();
        f.svc.propose("acp-1", vec![folder("reports")]).expect("propose");
        f.svc.propose("acp-2", vec![folder("other")]).expect("propose");
        let listed = f.svc.list("acp-1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_id, "acp-1");

        f.svc.clear_session("acp-1");
        f.svc.clear_session("acp-1");
        assert!(f.svc.list("acp-1").is_empty());
        assert_eq!(f.svc.list("acp-2").len(), 1);
    }
}
```

- [ ] **Step 9: Check that the tests fail to compile**

Run: `cargo check -p rocket-app --all-targets -j4`
Expected: FAIL with `cannot find type 'ProposalService' in this scope` and `cannot find function 'request_fingerprint'`.

- [ ] **Step 10: Implement `ProposalService`**

In `crates/rocket-app/src/proposal_service.rs`, insert between the `use` block and `#[cfg(test)]`:

```rust
/// How many proposals may wait for the user in one session.
pub const MAX_PENDING_PER_SESSION: usize = 50;

/// How many answered proposals one session keeps for `list`. The oldest go
/// first.
pub const MAX_RESOLVED_PER_SESSION: usize = 100;

/// One error for a secret variable, matching the old MCP tool's wording.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

pub struct ProposalService {
    collections: CollectionService,
    environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
    events: Arc<dyn EventPublisher>,
    store: Mutex<Store>,
}

#[derive(Default)]
struct Store {
    /// Proposals per real ACP session id, oldest first. The MCP tool server
    /// reports that id through Plan 03's `McpSessionBinding`.
    sessions: HashMap<String, Vec<AgentProposal>>,
}

/// What a collection-relative path points at in the summaries tree.
enum Target {
    /// A folder, with its shape text for the fingerprint.
    Folder(String),
    HttpRequest,
    /// A GraphQL, WebSocket or gRPC request, or a script file.
    Other,
    Missing,
}

/// Lets an `Arc` publisher stand in where a service wants a `Box`.
struct ForwardEvents(Arc<dyn EventPublisher>);

impl EventPublisher for ForwardEvents {
    fn publish(&self, event: DomainEvent) {
        self.0.publish(event);
    }
}

impl ProposalService {
    pub fn new(
        collections: CollectionService,
        environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            collections,
            environment_repo_factory,
            events,
            store: Mutex::new(Store::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Checks every change, then queues each one as its own pending
    /// proposal. If any change is invalid, none is queued.
    pub fn propose(
        &self,
        session_id: &str,
        changes: Vec<ProposedChange>,
    ) -> DomainResult<Vec<String>> {
        if changes.is_empty() {
            return Err(DomainError::InvalidInput(
                "propose at least one change".to_string(),
            ));
        }
        let workspace: Vec<String> = self
            .collections
            .list()?
            .into_iter()
            .map(|summary| summary.name)
            .collect();
        let mut prepared = Vec::with_capacity(changes.len());
        for mut change in changes {
            self.prepare(&mut change, &workspace)?;
            prepared.push(change);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut store = self.lock();
        let session = session_id.to_string();
        let list = store.sessions.entry(session.clone()).or_default();
        let pending = list
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .count();
        if pending + prepared.len() > MAX_PENDING_PER_SESSION {
            return Err(DomainError::InvalidInput(format!(
                "too many pending proposals: at most {MAX_PENDING_PER_SESSION} can wait for \
                 the user; ask the user to accept or reject some first"
            )));
        }
        let mut created = Vec::with_capacity(prepared.len());
        for change in prepared {
            let proposal =
                AgentProposal::new(ulid::Ulid::new().to_string(), session.clone(), change, now);
            created.push((proposal.id.clone(), proposal.summary.clone()));
            list.push(proposal);
        }
        prune_resolved(list);
        drop(store);

        let mut ids = Vec::with_capacity(created.len());
        for (proposal_id, summary) in created {
            self.events.publish(DomainEvent::AcpProposalCreated {
                session_id: session.clone(),
                proposal_id: proposal_id.clone(),
                summary,
            });
            ids.push(proposal_id);
        }
        Ok(ids)
    }

    /// The session's proposals, oldest first. Unknown sessions have none.
    pub fn list(&self, session_id: &str) -> Vec<AgentProposal> {
        self.lock()
            .sessions
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Re-checks the target and applies the change. A changed target makes
    /// the proposal `Stale`; a failed write makes it `Failed`. Either way
    /// nothing is written.
    pub fn accept(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal> {
        let mut store = self.lock();
        let session = session_id.to_string();
        let change = pending_mut(&mut store, &session, proposal_id)?.change.clone();
        // The lock stays held while the change is applied, so a second
        // accept of the same proposal waits and then finds it answered.
        let status = match self.still_applies(&change) {
            Ok(false) => ProposalStatus::Stale,
            Ok(true) => match self.apply(&change) {
                Ok(()) => ProposalStatus::Accepted,
                Err(e) => ProposalStatus::Failed {
                    message: e.to_string(),
                },
            },
            Err(e) => ProposalStatus::Failed {
                message: e.to_string(),
            },
        };
        let resolved = finish(&mut store, &session, proposal_id, status)?;
        drop(store);
        self.publish_resolved(&resolved);
        Ok(resolved)
    }

    /// Marks a pending proposal rejected. Nothing is written.
    pub fn reject(&self, session_id: &str, proposal_id: &str) -> DomainResult<AgentProposal> {
        let mut store = self.lock();
        let resolved = finish(&mut store, session_id, proposal_id, ProposalStatus::Rejected)?;
        drop(store);
        self.publish_resolved(&resolved);
        Ok(resolved)
    }

    /// Drops the session's proposals. Called from `TauriSessionCleanup` when
    /// the session ends. Safe to call more than once.
    pub fn clear_session(&self, session_id: &str) {
        self.lock().sessions.remove(session_id);
    }

    fn publish_resolved(&self, proposal: &AgentProposal) {
        self.events.publish(DomainEvent::AcpProposalResolved {
            session_id: proposal.session_id.clone(),
            proposal_id: proposal.id.clone(),
            status: proposal.status.as_str().to_string(),
        });
    }

    fn tree(&self, collection: &str) -> DomainResult<Folder> {
        Ok(self.collections.get_summaries(collection)?.root)
    }

    /// Checks one change against the workspace as it is now and fills in
    /// its base fingerprint. Nothing is written.
    fn prepare(&self, change: &mut ProposedChange, workspace: &[String]) -> DomainResult<()> {
        normalize_paths(change);
        for path in paths_of(change) {
            validate_relative_path(path)?;
        }
        let collection = change.collection().to_string();
        if !workspace.iter().any(|name| *name == collection) {
            return Err(DomainError::InvalidInput(format!(
                "collection '{collection}' is not in this workspace"
            )));
        }
        let fingerprint = match &*change {
            ProposedChange::CreateFolder {
                parent_path, name, ..
            } => {
                validate_item_name(name)?;
                self.check_free_target(&collection, parent_path, name)?;
                None
            }
            ProposedChange::CreateRequest {
                folder_path,
                request,
                ..
            } => {
                validate_item_name(&request.name)?;
                self.check_free_target(
                    &collection,
                    folder_path,
                    &request_filename_for(&request.name),
                )?;
                None
            }
            ProposedChange::UpdateRequest {
                request_path,
                patch,
                ..
            } => {
                if patch.is_empty() {
                    return Err(DomainError::InvalidInput(
                        "the patch changes no field".to_string(),
                    ));
                }
                let root = self.tree(&collection)?;
                Some(self.http_request_fingerprint(&collection, &root, request_path)?)
            }
            ProposedChange::EditScript { request_path, .. } => {
                let root = self.tree(&collection)?;
                Some(self.http_request_fingerprint(&collection, &root, request_path)?)
            }
            ProposedChange::MoveItem {
                from_path,
                to_folder,
                ..
            } => {
                let root = self.tree(&collection)?;
                let destination = move_destination(from_path, to_folder)?;
                if !is_folder(&root, to_folder) {
                    return Err(DomainError::NotFound(format!(
                        "folder '{to_folder}' in '{collection}'"
                    )));
                }
                if !is_free(&root, &destination) {
                    return Err(DomainError::AlreadyExists(format!(
                        "'{destination}' in '{collection}'"
                    )));
                }
                Some(self.item_fingerprint(&collection, &root, from_path)?)
            }
            ProposedChange::RenameItem { path, new_name, .. } => {
                validate_item_name(new_name)?;
                let root = self.tree(&collection)?;
                if is_folder(&root, path) {
                    let destination = join_path(parent_of(path), new_name);
                    if !is_free(&root, &destination) {
                        return Err(DomainError::AlreadyExists(format!(
                            "'{destination}' in '{collection}'"
                        )));
                    }
                }
                Some(self.item_fingerprint(&collection, &root, path)?)
            }
            ProposedChange::SetEnvVar {
                environment, key, ..
            } => {
                validate_environment_name(environment)?;
                if key.trim().is_empty() {
                    return Err(DomainError::InvalidInput(
                        "variable name cannot be empty".to_string(),
                    ));
                }
                let env = self
                    .environment_repo_factory
                    .for_collection(&collection)
                    .get(environment)?;
                if env.variables.iter().any(|v| v.key == *key && v.secret) {
                    return Err(DomainError::InvalidInput(
                        VARIABLE_NOT_ACCESSIBLE.to_string(),
                    ));
                }
                None
            }
        };
        if let Some(fingerprint) = fingerprint {
            change.set_base_fingerprint(fingerprint);
        }
        Ok(())
    }

    /// The parent must be a folder and `parent/name` must be unused.
    fn check_free_target(&self, collection: &str, parent: &str, name: &str) -> DomainResult<()> {
        let root = self.tree(collection)?;
        if !is_folder(&root, parent) {
            return Err(DomainError::NotFound(format!(
                "folder '{parent}' in '{collection}'"
            )));
        }
        let target = join_path(parent, name);
        if !is_free(&root, &target) {
            return Err(DomainError::AlreadyExists(format!(
                "'{target}' in '{collection}'"
            )));
        }
        Ok(())
    }

    fn http_request_fingerprint(
        &self,
        collection: &str,
        root: &Folder,
        path: &str,
    ) -> DomainResult<String> {
        match locate(root, path) {
            Target::HttpRequest => {
                let request = self.collections.get_request(collection, path)?;
                Ok(request_fingerprint(&request))
            }
            Target::Folder(_) => Err(DomainError::InvalidInput(format!(
                "'{path}' is a folder, not a request"
            ))),
            Target::Other => Err(DomainError::InvalidInput(format!(
                "'{path}' is not an HTTP request; proposals change HTTP requests and folders only"
            ))),
            Target::Missing => Err(DomainError::NotFound(format!(
                "'{path}' in '{collection}'"
            ))),
        }
    }

    fn item_fingerprint(&self, collection: &str, root: &Folder, path: &str) -> DomainResult<String> {
        if path.is_empty() {
            return Err(DomainError::InvalidInput(
                "the collection root cannot be moved or renamed".to_string(),
            ));
        }
        match locate(root, path) {
            Target::Folder(shape) => Ok(sha256_hex(&format!("folder\n{shape}"))),
            _ => self.http_request_fingerprint(collection, root, path),
        }
    }

    /// True when the target is still as it was when the change was proposed.
    fn still_applies(&self, change: &ProposedChange) -> DomainResult<bool> {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => {
                let root = self.tree(collection)?;
                Ok(is_folder(&root, parent_path) && is_free(&root, &join_path(parent_path, name)))
            }
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let root = self.tree(collection)?;
                let target = join_path(folder_path, &request_filename_for(&request.name));
                Ok(is_folder(&root, folder_path) && is_free(&root, &target))
            }
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                base_fingerprint,
                ..
            }
            | ProposedChange::EditScript {
                collection,
                request_path,
                base_fingerprint,
                ..
            } => {
                let root = self.tree(collection)?;
                unchanged(
                    self.http_request_fingerprint(collection, &root, request_path),
                    base_fingerprint,
                )
            }
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                base_fingerprint,
            } => {
                let root = self.tree(collection)?;
                let destination = move_destination(from_path, to_folder)?;
                if !is_folder(&root, to_folder) || !is_free(&root, &destination) {
                    return Ok(false);
                }
                unchanged(
                    self.item_fingerprint(collection, &root, from_path),
                    base_fingerprint,
                )
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                base_fingerprint,
            } => {
                let root = self.tree(collection)?;
                if is_folder(&root, path) && !is_free(&root, &join_path(parent_of(path), new_name))
                {
                    return Ok(false);
                }
                unchanged(
                    self.item_fingerprint(collection, &root, path),
                    base_fingerprint,
                )
            }
            // Environment writes carry no fingerprint. `apply_env_var`
            // re-checks the secret flag itself.
            ProposedChange::SetEnvVar { .. } => Ok(true),
        }
    }

    /// Applies one change with one write through the manual-edit services.
    fn apply(&self, change: &ProposedChange) -> DomainResult<()> {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => self
                .collections
                .create_folder(collection, &join_path(parent_path, name)),
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                let path = join_path(folder_path, &request_filename_for(&request.name));
                self.collections
                    .save_request(collection, &path, &build_request(request))
                    .map(|_| ())
            }
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => {
                let mut request = self.collections.get_request(collection, request_path)?;
                apply_patch(&mut request, patch);
                self.collections
                    .save_request(collection, request_path, &request)
                    .map(|_| ())
            }
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                body,
                ..
            } => self.collections.save_request_script(
                collection,
                request_path,
                collection_phase(*phase),
                body.clone(),
            ),
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => {
                let destination = move_destination(from_path, to_folder)?;
                self.collections
                    .move_item(collection, from_path, collection, &destination)
            }
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => {
                let root = self.tree(collection)?;
                if is_folder(&root, path) {
                    // A folder is renamed by moving it, like the sidebar does.
                    self.collections.move_item(
                        collection,
                        path,
                        collection,
                        &join_path(parent_of(path), new_name),
                    )
                } else {
                    self.collections.rename_request(collection, path, new_name)
                }
            }
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                value,
            } => self.apply_env_var(collection, environment, key, value),
        }
    }

    /// Writes one non-secret variable through `EnvironmentService`, so the
    /// usual validation and `EnvironmentSaved` event apply.
    fn apply_env_var(
        &self,
        collection: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> DomainResult<()> {
        let mut env = self
            .environment_repo_factory
            .for_collection(collection)
            .get(environment)?;
        match env.variables.iter_mut().find(|v| v.key == key) {
            Some(variable) if variable.secret => {
                return Err(DomainError::InvalidInput(
                    VARIABLE_NOT_ACCESSIBLE.to_string(),
                ));
            }
            Some(variable) => variable.value = value.to_string(),
            None => env.variables.push(Variable::new(key, value)),
        }
        EnvironmentService::new(
            self.environment_repo_factory.for_collection(collection),
            Box::new(ForwardEvents(Arc::clone(&self.events))),
        )
        .save(&env)
    }
}

/// The pending proposal with this id, or an error that says why not.
fn pending_mut<'a>(
    store: &'a mut Store,
    session: &str,
    proposal_id: &str,
) -> DomainResult<&'a mut AgentProposal> {
    let proposal = store
        .sessions
        .get_mut(session)
        .and_then(|list| list.iter_mut().find(|p| p.id == proposal_id))
        .ok_or_else(|| DomainError::NotFound(format!("proposal '{proposal_id}'")))?;
    if proposal.status != ProposalStatus::Pending {
        return Err(DomainError::InvalidInput(format!(
            "proposal '{proposal_id}' is already {}",
            proposal.status.as_str()
        )));
    }
    Ok(proposal)
}

/// Sets the final status of a pending proposal and returns a copy.
fn finish(
    store: &mut Store,
    session: &str,
    proposal_id: &str,
    status: ProposalStatus,
) -> DomainResult<AgentProposal> {
    let proposal = pending_mut(store, session, proposal_id)?;
    proposal.status = status;
    let resolved = proposal.clone();
    if let Some(list) = store.sessions.get_mut(session) {
        prune_resolved(list);
    }
    Ok(resolved)
}

/// Drops the oldest answered proposals beyond `MAX_RESOLVED_PER_SESSION`.
fn prune_resolved(list: &mut Vec<AgentProposal>) {
    let resolved = list
        .iter()
        .filter(|p| p.status != ProposalStatus::Pending)
        .count();
    let mut excess = resolved.saturating_sub(MAX_RESOLVED_PER_SESSION);
    list.retain(|p| {
        if excess > 0 && p.status != ProposalStatus::Pending {
            excess -= 1;
            false
        } else {
            true
        }
    });
}

/// Compares a fresh fingerprint with the proposal's. A target that is gone
/// or changed kind counts as changed; other errors are real failures.
fn unchanged(current: DomainResult<String>, base: &str) -> DomainResult<bool> {
    match current {
        Ok(fingerprint) => Ok(fingerprint == base),
        Err(DomainError::NotFound(_)) | Err(DomainError::InvalidInput(_)) => Ok(false),
        Err(other) => Err(other),
    }
}

/// Lowercase hex SHA-256 of `text`.
fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The fingerprint of a request's stored form. `uid`, `file_name` and `seq`
/// are left out, so a reload of a file without a stored uid is not a change.
fn request_fingerprint(request: &Request) -> String {
    sha256_hex(&saved_request_text(request))
}

/// A folder's direct children, sorted, one per line.
fn folder_shape(folder: &Folder) -> String {
    let mut lines: Vec<String> = folder
        .items
        .iter()
        .filter_map(|item| match item {
            CollectionItem::Folder(sub) => Some(format!("folder:{}", folder_dir_name(sub))),
            CollectionItem::Summary(summary) => Some(format!(
                "request:{}:{}:{}:{}",
                summary.file_name.as_deref().unwrap_or_default(),
                summary.name,
                summary.method,
                summary.url
            )),
            CollectionItem::ScriptFile(file) => Some(format!("script:{}", file.file_name)),
            _ => None,
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

fn child_folder<'a>(folder: &'a Folder, name: &str) -> Option<&'a Folder> {
    folder.items.iter().find_map(|item| match item {
        CollectionItem::Folder(sub) if folder_dir_name(sub) == name => Some(sub),
        _ => None,
    })
}

/// Finds what `path` points at. `""` is the collection root.
fn locate(root: &Folder, path: &str) -> Target {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return Target::Folder(folder_shape(root));
    };
    let mut folder = root;
    for segment in parents {
        match child_folder(folder, segment) {
            Some(next) => folder = next,
            None => return Target::Missing,
        }
    }
    if let Some(found) = child_folder(folder, last) {
        return Target::Folder(folder_shape(found));
    }
    for item in &folder.items {
        match item {
            CollectionItem::Summary(summary) if summary.file_name.as_deref() == Some(*last) => {
                return if summary.kind.is_http() {
                    Target::HttpRequest
                } else {
                    Target::Other
                };
            }
            CollectionItem::ScriptFile(file) if file.file_name == *last => return Target::Other,
            _ => {}
        }
    }
    Target::Missing
}

fn is_folder(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Folder(_))
}

fn is_free(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Missing)
}

/// `parent/name`, or `name` at the root. Both inputs are normalized.
fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// The folder that holds `path`, `""` for the root.
fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// Where `from_path` lands when moved into `to_folder`.
fn move_destination(from_path: &str, to_folder: &str) -> DomainResult<String> {
    if from_path.is_empty() {
        return Err(DomainError::InvalidInput(
            "the collection root cannot be moved".to_string(),
        ));
    }
    if to_folder == from_path || to_folder.starts_with(&format!("{from_path}/")) {
        return Err(DomainError::InvalidInput(
            "a folder cannot be moved into itself".to_string(),
        ));
    }
    if parent_of(from_path) == to_folder {
        return Err(DomainError::InvalidInput(
            "the item is already in that folder".to_string(),
        ));
    }
    let name = from_path.rsplit('/').next().unwrap_or(from_path);
    Ok(join_path(to_folder, name))
}

/// Strips leading and trailing slashes from every path in the change.
fn normalize_paths(change: &mut ProposedChange) {
    let trim = |path: &mut String| *path = path.trim_matches('/').to_string();
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => trim(parent_path),
        ProposedChange::CreateRequest { folder_path, .. } => trim(folder_path),
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => trim(request_path),
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => {
            trim(from_path);
            trim(to_folder);
        }
        ProposedChange::RenameItem { path, .. } => trim(path),
        ProposedChange::SetEnvVar { .. } => {}
    }
}

fn paths_of(change: &ProposedChange) -> Vec<&str> {
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => vec![parent_path.as_str()],
        ProposedChange::CreateRequest { folder_path, .. } => vec![folder_path.as_str()],
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => vec![request_path.as_str()],
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => vec![from_path.as_str(), to_folder.as_str()],
        ProposedChange::RenameItem { path, .. } => vec![path.as_str()],
        ProposedChange::SetEnvVar { .. } => vec![],
    }
}

/// Refuses paths that could leave the collection. The repository checks
/// again, so this only gives the agent a clear message early.
fn validate_relative_path(path: &str) -> DomainResult<()> {
    if path.contains(['\\', '\0']) || path.split('/').any(|s| s == ".." || s == ".") {
        return Err(DomainError::InvalidInput(
            "paths must stay inside the collection".to_string(),
        ));
    }
    Ok(())
}

/// A folder or request name must be one path segment.
fn validate_item_name(name: &str) -> DomainResult<()> {
    if name.trim().is_empty() || name.contains(['/', '\\', '\0']) || name.starts_with('.') {
        return Err(DomainError::InvalidInput(format!(
            "'{name}' is not a valid name"
        )));
    }
    Ok(())
}

/// Same rule as `McpToolService::validate_environment_name`.
fn validate_environment_name(name: &str) -> DomainResult<()> {
    if name.is_empty()
        || name.contains('\0')
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.starts_with('.')
        || std::path::Path::new(name)
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(DomainError::InvalidInput(
            "invalid environment name".to_string(),
        ));
    }
    Ok(())
}

fn collection_phase(phase: ScriptPhase) -> RequestScriptPhase {
    match phase {
        ScriptPhase::PreRequest => RequestScriptPhase::PreRequest,
        ScriptPhase::PostResponse => RequestScriptPhase::PostResponse,
        ScriptPhase::Tests => RequestScriptPhase::Tests,
    }
}

/// A body of mode `none` is stored as no body, like the request editor does.
fn stored_body(body: &rocket_shared::types::Body) -> Option<rocket_shared::types::Body> {
    if matches!(body.mode, BodyMode::None) {
        None
    } else {
        Some(body.clone())
    }
}

fn build_request(proposed: &ProposedRequest) -> Request {
    let mut request = Request::new(
        proposed.name.clone(),
        proposed.method.clone(),
        proposed.url.clone(),
    );
    request.headers = proposed.headers.clone();
    request.query_params = proposed.query_params.clone();
    request.body = proposed.body.as_ref().and_then(stored_body);
    request.docs = proposed.docs.clone().map(Documentation::text);
    request.pre_request_script = proposed.pre_request_script.clone();
    request.post_response_script = proposed.post_response_script.clone();
    request.tests = proposed.tests.clone();
    request
}

fn apply_patch(request: &mut Request, patch: &RequestPatch) {
    if let Some(method) = &patch.method {
        request.method = method.clone();
    }
    if let Some(url) = &patch.url {
        request.url = url.clone();
    }
    if let Some(headers) = &patch.headers {
        request.headers = headers.clone();
    }
    if let Some(query_params) = &patch.query_params {
        request.query_params = query_params.clone();
    }
    if let Some(body) = &patch.body {
        request.body = stored_body(body);
    }
    if let Some(docs) = &patch.docs {
        request.docs = Some(Documentation::text(docs.clone()));
    }
}
```

- [ ] **Step 11: Update the crate docs**

In `crates/rocket-acp/CLAUDE.md`, add a row to the Module Map table:

```markdown
| `proposal.rs` | `AgentProposal`, `ProposalStatus`, `ProposedChange`, `ProposedRequest`, `RequestPatch`, `ScriptPhase`: pure proposal data, applied by `rocket-app`'s `ProposalService` |
```

In `crates/rocket-app/CLAUDE.md`, add a row to the Public Types table after `AcpSessionService`:

```markdown
| `ProposalService` | In-memory, per-session queue of the assistant's proposed changes (cap 50 pending). `accept` re-reads the target, compares a SHA-256 fingerprint (`Stale` on mismatch) and applies through `CollectionService`/`EnvironmentService`; a failed write is `Failed`. Publishes `AcpProposalCreated`/`AcpProposalResolved`. Proposals are keyed by the real ACP session id; `clear_session` runs from `TauriSessionCleanup`. |
```

- [ ] **Step 12: Verify**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS with no new warnings.

Run: `yarn tsc --noEmit`
Expected: PASS (no TypeScript changed).

For the user to run:
- `cargo test -p rocket-acp proposal -j4`
- `cargo test -p rocket-shared acp_proposal -j4`
- `cargo test -p rocket-app proposal_service -j4`

- [ ] **Step 13: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat: add agent proposal types and ProposalService` for exactly these paths:
- `crates/rocket-acp/src/proposal.rs`
- `crates/rocket-acp/src/lib.rs`
- `crates/rocket-acp/CLAUDE.md`
- `crates/rocket-shared/src/events.rs`
- `src-tauri/src/tauri_event_bus.rs`
- `crates/rocket-app/Cargo.toml`
- `Cargo.lock` (only if `cargo check` changed it)
- `crates/rocket-app/src/collection_service.rs`
- `crates/rocket-app/src/proposal_service.rs`
- `crates/rocket-app/src/lib.rs`
- `crates/rocket-app/CLAUDE.md`

---

### Task 2: `propose_changes` and `list_proposals` MCP tools; remove the direct-write tools

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (delete `edit_script` `:274-296`, `set_env_var` `:323-353` and their tests; trim Plan 03's two mode tests)
- Modify: `src-tauri/src/mcp/tool_server.rs` (params `:157-198`, `parse_phase` `:228-237`, tools `:270-323`, `get_info` `:347-357`, tests, including Plan 03's `the_tool_list_is_the_same_in_every_mode`)
- Modify: `src-tauri/tests/mcp_http_server_integration.rs` (Plan 03's sorted tool-name list in `real_mcp_client_lists_and_calls_tools_over_http`)
- Modify: `src-tauri/src/lib.rs` (build `proposal_svc` after `collection_svc`, `:329-335`; manage it after `app.manage(mcp_tool_svc);`, `:676`)

`src-tauri/src/commands/acp_sessions.rs` needs no change: Plan 03 already binds every tool server to the real ACP session id (`handle.binding.bind(&info.session_id)` in `start_agent_session_inner` and `start_workspace_assistant_inner`).

**Interfaces:**
- Consumes: `ProposalService::{propose, list}` (Task 1), `McpSessionBinding` and the `binding` field of `RocketMcpToolServer` (Plan 03 Task 2), `McpToolService::check_mode(&self, session_id: &str, required: AssistantMode) -> DomainResult<()>` and `rocket_app::mcp_tool_service::AssistantMode::Edit` (Plan 03), `to_tool_result` and `mcp_tool_service()` (existing).
- Produces: MCP tools `propose_changes(changes: [ProposedChangeParams])` and `list_proposals()`; `Arc<ProposalService>` in Tauri managed state; the tool list no longer has `edit_script` or `set_env_var`.

Tool input (JSON, snake_case, tag `op`):

```json
{ "changes": [
  { "op": "create_folder",  "collection": "demo", "parent_path": "", "name": "reports" },
  { "op": "create_request", "collection": "demo", "folder_path": "users",
    "request": { "name": "List Users", "method": "GET", "url": "{{base}}/users",
                 "headers": [{ "key": "Accept", "value": "application/json" }],
                 "query_params": [], "body": { "mode": "json", "content": "{}" },
                 "docs": "...", "pre_request_script": "...", "post_response_script": "...", "tests": "..." } },
  { "op": "update_request", "collection": "demo", "request_path": "users/list-users.yml",
    "patch": { "method": "POST", "url": "...", "headers": [...], "query_params": [...], "body": {...}, "docs": "..." } },
  { "op": "edit_script", "collection": "demo", "request_path": "a.yml", "phase": "tests", "body": "..." },
  { "op": "move_item", "collection": "demo", "from_path": "a.yml", "to_folder": "archive" },
  { "op": "rename_item", "collection": "demo", "path": "a.yml", "new_name": "New name" },
  { "op": "set_env_var", "collection": "demo", "environment": "dev", "key": "HOST", "value": "x" }
] }
```

- [ ] **Step 1: Remove the direct-write methods from `McpToolService`**

In `crates/rocket-app/src/mcp_tool_service.rs`:
- Delete `pub fn edit_script(...)` and `pub fn set_env_var(...)` entirely.
- Delete every test whose body calls `.edit_script(` or `.set_env_var(` (today `edit_script_saves_via_the_repository_and_publishes_audit_event`, `set_env_var_writes_a_non_secret_variable_and_it_is_readable_back`, `set_env_var_refuses_a_secret_variable_and_does_not_create_missing_keys`, `set_env_var_refuses_a_traversal_shaped_environment_name`), except Plan 03's two mode tests. In a test that lists several refused calls in one table (today the block at `:610-645`), delete only the `edit_script` and `set_env_var` entries and the assertion that names `edit_script`.
- In Plan 03's `ask_mode_refuses_running_and_writing_but_allows_reading`, delete the two `assert_refused_by_mode("edit_script", ...)` and `assert_refused_by_mode("set_env_var", ...)` calls and keep the rest. Rename `edit_mode_allows_writing_but_not_running` to `edit_mode_does_not_allow_running` and delete its `svc.edit_script(...)` statement, keeping the `run_request` refusal. Mode gating of `propose_changes` is tested in `tool_server.rs` (Step 2).
- In the module doc comment (`:1-8`) and the `FakeEnvRepoFactory` doc comment, drop the words about editing scripts and writing variables. Replace the first module doc line with: `//! Lets an ACP agent read a Rocket workspace and run requests. Writes go through ProposalService instead.`
- Reword every other comment that still names the removed methods (today the `McpRequestEntry` doc at `:23`, the `VARIABLE_NOT_ACCESSIBLE` doc at `:48` and the `FakeEnvRepoFactory` doc at `:501`), for example "`get_env_var`" alone instead of "`get_env_var`/`set_env_var`", and "`run_request` expects back" instead of "`run_request` and `edit_script` expect back".

Run: `grep -n "edit_script\|set_env_var" crates/rocket-app/src/mcp_tool_service.rs`
Expected: no output.

Run: `cargo check -p rocket-app --all-targets -j4`
Expected: PASS. If it warns that `VARIABLE_NOT_ACCESSIBLE`, `RequestScriptPhase` (test import) or another item is now unused, delete that item or import, and run it again.

- [ ] **Step 2: Write the failing tool-server tests**

In `src-tauri/src/mcp/tool_server.rs`, delete the test `edit_script_rejects_an_unknown_phase_without_touching_the_service` and add these tests at the end of the `tests` module:

```rust
    #[test]
    fn the_tool_list_has_the_propose_tools_and_no_direct_write_tools() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        assert!(server.tool_router.has_route("propose_changes"));
        assert!(server.tool_router.has_route("list_proposals"));
        assert!(!server.tool_router.has_route("edit_script"));
        assert!(!server.tool_router.has_route("set_env_var"));
    }

    fn parse_one(change: serde_json::Value) -> ProposedChangeParams {
        let params: ProposeChangesParams =
            serde_json::from_value(serde_json::json!({ "changes": [change] })).expect("parse");
        params.changes.into_iter().next().expect("one change")
    }

    #[test]
    fn a_patch_or_new_request_with_an_auth_field_is_refused() {
        let patch = serde_json::from_value::<ProposeChangesParams>(serde_json::json!({
            "changes": [{
                "op": "update_request", "collection": "demo", "request_path": "ping.yml",
                "patch": { "url": "https://x", "auth": { "authType": "bearer", "token": "t" } }
            }]
        }));
        assert!(patch.is_err(), "auth must not be dropped silently");
        let create = serde_json::from_value::<ProposeChangesParams>(serde_json::json!({
            "changes": [{
                "op": "create_request", "collection": "demo",
                "request": { "name": "A", "method": "GET", "url": "https://x",
                             "auth": { "authType": "none" } }
            }]
        }));
        assert!(create.is_err(), "auth must not be dropped silently");
    }

    #[test]
    fn create_request_params_convert_to_the_domain_change() {
        let change = to_domain_change(parse_one(serde_json::json!({
            "op": "create_request", "collection": "demo",
            "request": {
                "name": "List Users", "method": "get", "url": "https://x/users",
                "headers": [{ "key": "Accept", "value": "application/json" }],
                "body": { "mode": "json", "content": "{}" }
            }
        })))
        .expect("convert");
        match change {
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                assert_eq!(collection, "demo");
                assert_eq!(folder_path, "");
                assert_eq!(request.method, HttpMethod::Get);
                assert!(request.headers[0].enabled);
                assert_eq!(request.body.map(|b| b.mode), Some(BodyMode::Json));
            }
            other => panic!("expected CreateRequest, got {other:?}"),
        }
    }

    #[test]
    fn a_bad_phase_method_or_body_mode_is_refused_with_a_message() {
        let refuse = |change: serde_json::Value| {
            to_domain_change(parse_one(change)).expect_err("must be refused")
        };
        assert!(refuse(serde_json::json!({
            "op": "edit_script", "collection": "demo", "request_path": "a.yml",
            "phase": "before", "body": ""
        }))
        .contains("script phase"));
        assert!(refuse(serde_json::json!({
            "op": "update_request", "collection": "demo", "request_path": "a.yml",
            "patch": { "method": "GET POST" }
        }))
        .contains("HTTP method"));
        assert!(refuse(serde_json::json!({
            "op": "update_request", "collection": "demo", "request_path": "a.yml",
            "patch": { "body": { "mode": "binary" } }
        }))
        .contains("body mode"));
    }

    #[tokio::test]
    async fn list_proposals_returns_the_sessions_proposals_with_their_status() {
        use rocket_collection::CollectionRepository;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let collections_dir = tmp.path().join("collections");
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())
            .create("demo")
            .expect("create collection");
        let ws_path = Arc::new(std::sync::Mutex::new(tmp.path().to_path_buf()));
        let proposals = Arc::new(ProposalService::new(
            rocket_app::CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir)),
                Box::new(rocket_shared::events::NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(ws_path)),
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        let ids = proposals
            .propose(
                "session-1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: String::new(),
                    name: "reports".into(),
                }],
            )
            .expect("propose");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(proposals);
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(app.handle().clone(), "session-1".to_string());

        let result = server.list_proposals().await.expect("tool call");
        assert!(!result.is_error.unwrap_or(false));
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        assert!(text.contains(&ids[0]));
        assert!(text.contains("pending"));
    }

    #[tokio::test]
    async fn list_proposals_uses_the_bound_session_id() {
        use rocket_collection::CollectionRepository;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let collections_dir = tmp.path().join("collections");
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())
            .create("demo")
            .expect("create collection");
        let ws_path = Arc::new(std::sync::Mutex::new(tmp.path().to_path_buf()));
        let proposals = Arc::new(ProposalService::new(
            rocket_app::CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir)),
                Box::new(rocket_shared::events::NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(ws_path)),
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        let ids = proposals
            .propose(
                "acp-1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: String::new(),
                    name: "reports".into(),
                }],
            )
            .expect("propose");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(proposals);
        // Plan 03's binding: provisional until the handshake returns the
        // real ACP session id.
        let binding = Arc::new(McpSessionBinding::new("provisional-1".to_string()));
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::with_binding(app.handle().clone(), Arc::clone(&binding));
        let text_of = |result: &CallToolResult| {
            result
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.clone())
                .unwrap_or_default()
        };

        let before = server.list_proposals().await.expect("tool call");
        assert!(!text_of(&before).contains(&ids[0]), "unbound: the provisional id has none");

        binding.bind("acp-1");
        let after = server.list_proposals().await.expect("tool call");
        assert!(text_of(&after).contains(&ids[0]), "bound: the real session's proposals");
    }

    #[tokio::test]
    async fn propose_changes_is_refused_in_ask_mode() {
        let fixture = TestFixture::new(true);
        let params: ProposeChangesParams = serde_json::from_value(serde_json::json!({
            "changes": [{ "op": "create_folder", "collection": "demo", "name": "reports" }]
        }))
        .expect("parse");
        let result = fixture
            .server_in(AssistantMode::Ask)
            .propose_changes(Parameters(params))
            .await
            .expect("tool call");
        assert!(tool_is_error(&result));
        assert!(tool_text(&result).contains("Not available in Ask mode"));
    }
```

Also update Plan 03's `the_tool_list_is_the_same_in_every_mode` in the same module: its `expected` list becomes the index's ten names, sorted:

```rust
        let expected = vec![
            "get_collection_settings",
            "get_environment",
            "get_history",
            "get_request",
            "get_test_results",
            "get_workspace_outline",
            "list_collections",
            "list_proposals",
            "propose_changes",
            "run_request",
        ];
```

In `src-tauri/tests/mcp_http_server_integration.rs`, replace the sorted `names` list that Plan 03 wrote in `real_mcp_client_lists_and_calls_tools_over_http` with the same ten names.

- [ ] **Step 3: Check that the tests fail to compile**

Run: `cargo check -p rocket --all-targets -j4`
Expected: FAIL with `cannot find type 'ProposeChangesParams'`, `cannot find function 'to_domain_change'` and `no method named 'list_proposals'`.

- [ ] **Step 4: Implement the tools and remove the direct-write tools**

In `src-tauri/src/mcp/tool_server.rs`:

1. Delete `EditScriptParams`, `SetEnvVarParams`, `parse_phase`, and the `edit_script` and `set_env_var` tool methods. If Plan 03 left placeholder `propose_changes` or `list_proposals` methods, delete them too; the versions below replace them.

2. Add these imports next to `use rocket_app::McpToolService;`:

```rust
use rocket_acp::proposal::{
    AgentProposal, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_app::mcp_tool_service::AssistantMode;
use rocket_app::ProposalService;
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod, QueryParam};
```

3. Add the state lookup after `mcp_tool_service()`:

```rust
/// Looks up the `Arc<ProposalService>` this app manages. A missing
/// registration is a wiring bug, so it is a protocol-level error.
fn proposal_service<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Arc<ProposalService>, McpError> {
    match app_handle.try_state::<Arc<ProposalService>>() {
        Some(state) => Ok(Arc::clone(state.inner())),
        None => Err(McpError::internal_error(
            "ProposalService is not managed on this AppHandle",
            None,
        )),
    }
}
```

4. Add the parameter types and conversions after the remaining param structs:

```rust
/// What `propose_changes` tells the agent.
const PROPOSALS_QUEUED: &str = "queued; awaiting user approval";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ProposeChangesParams {
    /// Each change becomes its own proposal that the user accepts or rejects.
    pub changes: Vec<ProposedChangeParams>,
}

/// One change. Paths are relative to the collection root; "" is the root.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProposedChangeParams {
    /// Create an empty folder.
    CreateFolder {
        collection: String,
        #[serde(default)]
        parent_path: String,
        name: String,
    },
    /// Create an HTTP request. It inherits auth from its folder or collection.
    CreateRequest {
        collection: String,
        #[serde(default)]
        folder_path: String,
        request: ProposedRequestParams,
    },
    /// Change some fields of an HTTP request. Omitted fields stay as they are.
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatchParams,
    },
    /// Replace one script: "pre_request", "post_response" or "tests".
    EditScript {
        collection: String,
        request_path: String,
        phase: String,
        body: String,
    },
    /// Move a request or folder into another folder of the same collection.
    MoveItem {
        collection: String,
        from_path: String,
        #[serde(default)]
        to_folder: String,
    },
    /// Rename a request (its display name) or a folder.
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
    },
    /// Set or add a non-secret environment variable.
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyValueParams {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BodyParams {
    /// One of "none", "json", "xml", "text", "sparql", "formurlencoded".
    pub mode: String,
    #[serde(default)]
    pub content: Option<String>,
}

/// Unknown fields, such as `auth`, are refused rather than dropped.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposedRequestParams {
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KeyValueParams>,
    #[serde(default)]
    pub query_params: Vec<KeyValueParams>,
    #[serde(default)]
    pub body: Option<BodyParams>,
    #[serde(default)]
    pub docs: Option<String>,
    #[serde(default)]
    pub pre_request_script: Option<String>,
    #[serde(default)]
    pub post_response_script: Option<String>,
    #[serde(default)]
    pub tests: Option<String>,
}

/// Unknown fields, such as `auth`, are refused rather than dropped.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestPatchParams {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Option<Vec<KeyValueParams>>,
    #[serde(default)]
    pub query_params: Option<Vec<KeyValueParams>>,
    #[serde(default)]
    pub body: Option<BodyParams>,
    #[serde(default)]
    pub docs: Option<String>,
}

/// What `propose_changes` returns.
#[derive(Debug, serde::Serialize)]
pub struct ProposeChangesResult {
    pub proposal_ids: Vec<String>,
    pub status: &'static str,
}

/// One proposal as `list_proposals` shows it to the agent.
#[derive(Debug, serde::Serialize)]
pub struct ProposalView {
    pub id: String,
    pub summary: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl From<AgentProposal> for ProposalView {
    fn from(proposal: AgentProposal) -> Self {
        Self {
            message: proposal.status.message().map(str::to_string),
            status: proposal.status.as_str(),
            id: proposal.id,
            summary: proposal.summary,
        }
    }
}

/// Parses the wire-format phase. The error is the message text only, so the
/// caller can return it as an agent-visible tool error.
fn parse_script_phase(phase: &str) -> Result<ScriptPhase, String> {
    match phase {
        "pre_request" => Ok(ScriptPhase::PreRequest),
        "post_response" => Ok(ScriptPhase::PostResponse),
        "tests" => Ok(ScriptPhase::Tests),
        other => Err(format!(
            "unknown script phase '{other}': expected pre_request, post_response or tests"
        )),
    }
}

fn parse_method(method: &str) -> Result<HttpMethod, String> {
    method
        .parse::<HttpMethod>()
        .map_err(|_| format!("'{method}' is not a valid HTTP method"))
}

/// Only text-like bodies can be proposed. Form data and files need the user.
fn to_body(body: BodyParams) -> Result<Body, String> {
    let mode: BodyMode = serde_json::from_value(serde_json::Value::String(body.mode.clone()))
        .map_err(|_| format!("unknown body mode '{}'", body.mode))?;
    if matches!(mode, BodyMode::FormData | BodyMode::Binary | BodyMode::GraphQl) {
        return Err(format!(
            "body mode '{}' cannot be proposed; use none, json, xml, text, sparql or formurlencoded",
            body.mode
        ));
    }
    Ok(Body {
        mode,
        content: body.content,
        form_data: None,
        file_path: None,
    })
}

fn to_headers(pairs: Vec<KeyValueParams>) -> Vec<Header> {
    pairs
        .into_iter()
        .map(|pair| Header {
            key: pair.key,
            value: pair.value,
            enabled: pair.enabled,
            description: None,
        })
        .collect()
}

fn to_query_params(pairs: Vec<KeyValueParams>) -> Vec<QueryParam> {
    pairs
        .into_iter()
        .map(|pair| QueryParam {
            key: pair.key,
            value: pair.value,
            enabled: pair.enabled,
            description: None,
        })
        .collect()
}

fn to_domain_request(request: ProposedRequestParams) -> Result<ProposedRequest, String> {
    Ok(ProposedRequest {
        method: parse_method(&request.method)?,
        body: request.body.map(to_body).transpose()?,
        name: request.name,
        url: request.url,
        headers: to_headers(request.headers),
        query_params: to_query_params(request.query_params),
        docs: request.docs,
        pre_request_script: request.pre_request_script,
        post_response_script: request.post_response_script,
        tests: request.tests,
    })
}

fn to_domain_patch(patch: RequestPatchParams) -> Result<RequestPatch, String> {
    Ok(RequestPatch {
        method: patch.method.as_deref().map(parse_method).transpose()?,
        body: patch.body.map(to_body).transpose()?,
        url: patch.url,
        headers: patch.headers.map(to_headers),
        query_params: patch.query_params.map(to_query_params),
        docs: patch.docs,
    })
}

/// Converts one tool-input change to the domain type. `ProposalService`
/// fills in the base fingerprint, so the agent never supplies one.
fn to_domain_change(params: ProposedChangeParams) -> Result<ProposedChange, String> {
    Ok(match params {
        ProposedChangeParams::CreateFolder {
            collection,
            parent_path,
            name,
        } => ProposedChange::CreateFolder {
            collection,
            parent_path,
            name,
        },
        ProposedChangeParams::CreateRequest {
            collection,
            folder_path,
            request,
        } => ProposedChange::CreateRequest {
            collection,
            folder_path,
            request: to_domain_request(request)?,
        },
        ProposedChangeParams::UpdateRequest {
            collection,
            request_path,
            patch,
        } => ProposedChange::UpdateRequest {
            collection,
            request_path,
            patch: to_domain_patch(patch)?,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::EditScript {
            collection,
            request_path,
            phase,
            body,
        } => ProposedChange::EditScript {
            collection,
            request_path,
            phase: parse_script_phase(&phase)?,
            body,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::MoveItem {
            collection,
            from_path,
            to_folder,
        } => ProposedChange::MoveItem {
            collection,
            from_path,
            to_folder,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::RenameItem {
            collection,
            path,
            new_name,
        } => ProposedChange::RenameItem {
            collection,
            path,
            new_name,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::SetEnvVar {
            collection,
            environment,
            key,
            value,
        } => ProposedChange::SetEnvVar {
            collection,
            environment,
            key,
            value,
        },
    })
}
```

5. Add the two tools inside `#[tool_router] impl<R: tauri::Runtime> RocketMcpToolServer<R>`:

```rust
    #[tool(
        description = "Propose workspace changes for the user to review: create_folder, create_request, update_request, edit_script, move_item, rename_item or set_env_var (non-secret only). Nothing is written until the user accepts each proposal. Returns the new proposal ids. Not available in Ask mode."
    )]
    async fn propose_changes(
        &self,
        Parameters(params): Parameters<ProposeChangesParams>,
    ) -> Result<CallToolResult, McpError> {
        let tools = mcp_tool_service(&self.app_handle)?;
        if let Err(refusal) = tools.check_mode(self.binding.session_id(), AssistantMode::Edit) {
            return Ok(to_tool_result::<()>(Err(refusal)));
        }
        let mut changes = Vec::with_capacity(params.changes.len());
        for change in params.changes {
            match to_domain_change(change) {
                Ok(change) => changes.push(change),
                Err(message) => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(message)]));
                }
            }
        }
        let proposals = proposal_service(&self.app_handle)?;
        let result = proposals
            .propose(self.binding.session_id(), changes)
            .map(|proposal_ids| ProposeChangesResult {
                proposal_ids,
                status: PROPOSALS_QUEUED,
            });
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "List this session's proposals and their status: pending, accepted, rejected, stale (the item changed after it was proposed; read it again and propose again) or failed (with a message)."
    )]
    async fn list_proposals(&self) -> Result<CallToolResult, McpError> {
        let proposals = proposal_service(&self.app_handle)?;
        let views: Vec<ProposalView> = proposals
            .list(self.binding.session_id())
            .into_iter()
            .map(ProposalView::from)
            .collect();
        Ok(to_tool_result(Ok(views)))
    }
```

6. Replace the `.with_instructions(...)` text in `get_info` with:

```rust
            .with_instructions(
                "Rocket workspace assistant tools: read the workspace, propose changes \
                 for the user to accept, and run requests where the user allows it.",
            )
```

- [ ] **Step 5: Build and manage `ProposalService` in `src-tauri/src/lib.rs`**

Directly after the `collection_svc` construction (`let collection_svc = CollectionService::new_with_audit(...);`), add:

```rust
            // Holds the assistant's proposed changes until the user accepts
            // them. It applies them through its own CollectionService, built
            // like collection_svc above, so accepted changes follow workspace
            // switches and publish the same events as manual edits. The
            // environment factory keeps secret values on a write-back.
            let proposal_svc = Arc::new(rocket_app::ProposalService::new(
                CollectionService::new_with_audit(
                    Box::new(SharedPathCollectionRepo::new(Arc::clone(
                        &active_workspace_path,
                    ))),
                    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                    audit_publisher.clone(),
                ),
                Arc::new(SharedCollectionEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    env_secret_store(),
                )),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            ));
```

After `app.manage(mcp_tool_svc);`, add:

```rust
            app.manage(Arc::clone(&proposal_svc));
```

- [ ] **Step 6: Confirm every tool server is bound to the real session id**

No id linking is needed. Plan 03 Task 2 made every tool method pass `self.binding.session_id()` and made every command that spawns a tool server bind it right after the handshake. Check it:

```bash
grep -rn "spawn_mcp_http_server(" src-tauri/src/commands
grep -rn "binding.bind(" src-tauri/src/commands
grep -n "self.session_id" src-tauri/src/mcp/tool_server.rs
```

Expected: each command that calls `spawn_mcp_http_server` (`start_agent_session_inner` and `start_workspace_assistant_inner`) also calls `handle.binding.bind(&info.session_id)` before the handle is registered, and the last grep prints nothing. If a command lacks the `bind` call, stop and report it: it is a Plan 03 gap, and proposals made through that server would be stored under the provisional id.

- [ ] **Step 7: Verify**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS with no new warnings.

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `grep -rn "edit_script\|set_env_var" src-tauri/src crates/rocket-app/src`
Expected: matches only in `src-tauri/src/mcp/tool_server.rs`, in the `propose_changes` description string and in test JSON or `has_route` assertions. No `#[tool]` method and no `McpToolService` method has either name.

For the user to run:
- `cargo test -p rocket tool_server -j4`
- `cargo test -p rocket-app mcp_tool_service -j4`
- `cargo test -p rocket --test mcp_http_server_integration -j4`

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat: queue agent edits as proposals via propose_changes` for exactly these paths:
- `crates/rocket-app/src/mcp_tool_service.rs`
- `src-tauri/src/mcp/tool_server.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/tests/mcp_http_server_integration.rs`

---

### Task 3: IPC commands, DTOs, session cleanup and TypeScript bindings

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src-tauri/src/commands/agent_proposals.rs`
- Modify: `src-tauri/src/commands/mod.rs` (after `pub mod agent_configs;`, line 2)
- Modify: `src-tauri/src/lib.rs` (`generate_handler!`, after `commands::acp_sessions::end_agent_session,`, `:922`; the `TauriSessionCleanup::new(...)` call inside `AcpSessionService::new(...)`)
- Modify: `src-tauri/src/agent_session/cleanup.rs` (Plan 02's `TauriSessionCleanup::new`)
- Modify: `src/lib/tauri-api.ts` (after `onAgentSessionFailed`, `:2627-2636`)
- Create: `src/lib/__tests__/tauri-api.agent-proposals.test.ts`

**Interfaces:**
- Consumes: `ProposalService::{list, accept, reject, clear_session}` and `Arc<ProposalService>` managed state (Tasks 1-2); `SessionCleanup::on_session_ended` (Plan 02).
- Produces:
  ```rust
  #[tauri::command] list_agent_proposals(session_id: String) -> Result<Vec<AgentProposalDto>, DomainError>
  #[tauri::command] accept_agent_proposal(session_id: String, proposal_id: String) -> Result<AgentProposalDto, DomainError>
  #[tauri::command] reject_agent_proposal(session_id: String, proposal_id: String) -> Result<AgentProposalDto, DomainError>
  ```
  ```ts
  export type AgentProposalStatus = 'pending' | 'accepted' | 'rejected' | 'stale' | 'failed';
  export type AgentProposedChange = { op: 'createFolder' | ... };   // full union below
  export interface AgentProposal { id; sessionId; change; summary; status; statusMessage?; createdAtMs }
  export const listAgentProposals: (sessionId: string) => Promise<AgentProposal[]>;
  export const acceptAgentProposal: (sessionId: string, proposalId: string) => Promise<AgentProposal>;
  export const rejectAgentProposal: (sessionId: string, proposalId: string) => Promise<AgentProposal>;
  export const onAgentProposalCreated: (handler: (e: AgentProposalCreatedEvent) => void) => Promise<UnlistenFn>;
  export const onAgentProposalResolved: (handler: (e: AgentProposalResolvedEvent) => void) => Promise<UnlistenFn>;
  ```

- [ ] **Step 1: Write the failing DTO tests**

Create `src-tauri/src/commands/agent_proposals.rs` with the module doc, imports and tests, and register it in `src-tauri/src/commands/mod.rs` after `pub mod agent_configs;`:

```rust
pub mod agent_proposals;
```

```rust
//! IPC commands and DTOs for the assistant's proposals. The commands stay
//! thin: they call `ProposalService` and map its output to camelCase DTOs.
//! The base fingerprint stays in the backend.

use std::sync::Arc;

use rocket_acp::proposal::{
    AgentProposal, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_app::ProposalService;
use rocket_shared::error::DomainError;
use rocket_shared::types::{Body, Header, QueryParam};
use serde::Serialize;
use tauri::State;

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::proposal::ProposalStatus;

    #[test]
    fn a_failed_edit_script_proposal_serializes_in_camel_case() {
        let mut proposal = AgentProposal::new(
            "p1".into(),
            "s1".into(),
            ProposedChange::EditScript {
                collection: "demo".into(),
                request_path: "get-users.yml".into(),
                phase: ScriptPhase::PreRequest,
                body: "console.log(1);".into(),
                base_fingerprint: "abc".into(),
            },
            42,
        );
        proposal.status = ProposalStatus::Failed {
            message: "disk full".into(),
        };
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert_eq!(json["id"], "p1");
        assert_eq!(json["sessionId"], "s1");
        assert_eq!(json["createdAtMs"], 42);
        assert_eq!(json["status"], "failed");
        assert_eq!(json["statusMessage"], "disk full");
        assert_eq!(json["change"]["op"], "editScript");
        assert_eq!(json["change"]["requestPath"], "get-users.yml");
        assert_eq!(json["change"]["phase"], "preRequest");
        assert_eq!(json["change"]["body"], "console.log(1);");
        assert!(
            json["change"].get("baseFingerprint").is_none(),
            "the fingerprint stays in the backend"
        );
    }

    #[test]
    fn a_pending_create_request_has_no_status_message_and_camel_case_fields() {
        let proposal = AgentProposal::new(
            "p2".into(),
            "s1".into(),
            ProposedChange::CreateRequest {
                collection: "demo".into(),
                folder_path: "users".into(),
                request: ProposedRequest {
                    name: "List Users".into(),
                    method: rocket_shared::types::HttpMethod::Get,
                    url: "https://x/users".into(),
                    headers: vec![Header::new("Accept", "application/json")],
                    query_params: vec![],
                    body: None,
                    docs: None,
                    pre_request_script: None,
                    post_response_script: None,
                    tests: Some("rok.test('ok', () => {});".into()),
                },
            },
            7,
        );
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert!(json.get("statusMessage").is_none());
        assert_eq!(json["status"], "pending");
        assert_eq!(json["change"]["op"], "createRequest");
        assert_eq!(json["change"]["folderPath"], "users");
        assert_eq!(json["change"]["request"]["method"], "GET");
        assert_eq!(json["change"]["request"]["queryParams"], serde_json::json!([]));
        assert_eq!(json["change"]["request"]["tests"], "rok.test('ok', () => {});");
        assert!(json["change"]["request"].get("body").is_none());
    }

    #[test]
    fn set_env_var_uses_the_set_env_var_tag() {
        let proposal = AgentProposal::new(
            "p3".into(),
            "s1".into(),
            ProposedChange::SetEnvVar {
                collection: "demo".into(),
                environment: "dev".into(),
                key: "HOST".into(),
                value: "api.example.com".into(),
            },
            1,
        );
        let json = serde_json::to_value(AgentProposalDto::from(proposal)).expect("serialize");
        assert_eq!(json["change"]["op"], "setEnvVar");
        assert_eq!(json["change"]["environment"], "dev");
    }
}
```

- [ ] **Step 2: Check that the tests fail to compile**

Run: `cargo check -p rocket --all-targets -j4`
Expected: FAIL with `cannot find type 'AgentProposalDto' in this scope`.

- [ ] **Step 3: Implement the DTOs and commands, and register them**

In `src-tauri/src/commands/agent_proposals.rs`, insert between the `use` block and `#[cfg(test)]`:

```rust
/// One proposal as the panel reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProposalDto {
    pub id: String,
    pub session_id: String,
    pub change: ProposedChangeDto,
    pub summary: String,
    /// `pending`, `accepted`, `rejected`, `stale` or `failed`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at_ms: i64,
}

/// The proposed operation. The tag is `op`, for example `editScript`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProposedChangeDto {
    CreateFolder {
        collection: String,
        parent_path: String,
        name: String,
    },
    CreateRequest {
        collection: String,
        folder_path: String,
        request: ProposedRequestDto,
    },
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatchDto,
    },
    EditScript {
        collection: String,
        request_path: String,
        /// `preRequest`, `postResponse` or `tests`.
        phase: &'static str,
        body: String,
    },
    MoveItem {
        collection: String,
        from_path: String,
        to_folder: String,
    },
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
    },
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedRequestDto {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
}

/// Only the fields the patch sets are present.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<Header>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<Vec<QueryParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

fn phase_name(phase: ScriptPhase) -> &'static str {
    match phase {
        ScriptPhase::PreRequest => "preRequest",
        ScriptPhase::PostResponse => "postResponse",
        ScriptPhase::Tests => "tests",
    }
}

impl From<ProposedRequest> for ProposedRequestDto {
    fn from(request: ProposedRequest) -> Self {
        Self {
            method: request.method.to_string(),
            name: request.name,
            url: request.url,
            headers: request.headers,
            query_params: request.query_params,
            body: request.body,
            docs: request.docs,
            pre_request_script: request.pre_request_script,
            post_response_script: request.post_response_script,
            tests: request.tests,
        }
    }
}

impl From<RequestPatch> for RequestPatchDto {
    fn from(patch: RequestPatch) -> Self {
        Self {
            method: patch.method.map(|method| method.to_string()),
            url: patch.url,
            headers: patch.headers,
            query_params: patch.query_params,
            body: patch.body,
            docs: patch.docs,
        }
    }
}

impl From<ProposedChange> for ProposedChangeDto {
    fn from(change: ProposedChange) -> Self {
        match change {
            ProposedChange::CreateFolder {
                collection,
                parent_path,
                name,
            } => Self::CreateFolder {
                collection,
                parent_path,
                name,
            },
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => Self::CreateRequest {
                collection,
                folder_path,
                request: request.into(),
            },
            ProposedChange::UpdateRequest {
                collection,
                request_path,
                patch,
                ..
            } => Self::UpdateRequest {
                collection,
                request_path,
                patch: patch.into(),
            },
            ProposedChange::EditScript {
                collection,
                request_path,
                phase,
                body,
                ..
            } => Self::EditScript {
                collection,
                request_path,
                phase: phase_name(phase),
                body,
            },
            ProposedChange::MoveItem {
                collection,
                from_path,
                to_folder,
                ..
            } => Self::MoveItem {
                collection,
                from_path,
                to_folder,
            },
            ProposedChange::RenameItem {
                collection,
                path,
                new_name,
                ..
            } => Self::RenameItem {
                collection,
                path,
                new_name,
            },
            ProposedChange::SetEnvVar {
                collection,
                environment,
                key,
                value,
            } => Self::SetEnvVar {
                collection,
                environment,
                key,
                value,
            },
        }
    }
}

impl From<AgentProposal> for AgentProposalDto {
    fn from(proposal: AgentProposal) -> Self {
        Self {
            status_message: proposal.status.message().map(str::to_string),
            status: proposal.status.as_str().to_string(),
            id: proposal.id,
            session_id: proposal.session_id,
            change: proposal.change.into(),
            summary: proposal.summary,
            created_at_ms: proposal.created_at_ms,
        }
    }
}

#[tauri::command]
pub fn list_agent_proposals(
    session_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<Vec<AgentProposalDto>, DomainError> {
    Ok(svc
        .list(&session_id)
        .into_iter()
        .map(AgentProposalDto::from)
        .collect())
}

#[tauri::command]
pub fn accept_agent_proposal(
    session_id: String,
    proposal_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<AgentProposalDto, DomainError> {
    svc.accept(&session_id, &proposal_id)
        .map(AgentProposalDto::from)
}

#[tauri::command]
pub fn reject_agent_proposal(
    session_id: String,
    proposal_id: String,
    svc: State<'_, Arc<ProposalService>>,
) -> Result<AgentProposalDto, DomainError> {
    svc.reject(&session_id, &proposal_id)
        .map(AgentProposalDto::from)
}
```

In `src-tauri/src/lib.rs`, inside `tauri::generate_handler![...]`, after `commands::acp_sessions::end_agent_session,` (and after any ACP commands Plans 01–03 added there):

```rust
            commands::agent_proposals::list_agent_proposals,
            commands::agent_proposals::accept_agent_proposal,
            commands::agent_proposals::reject_agent_proposal,
```

- [ ] **Step 4: Clear a session's proposals when it ends**

Plan 02's `TauriSessionCleanup` (`src-tauri/src/agent_session/cleanup.rs`) holds `Arc`s and a `forget_cache` closure; it has no `app_handle`. Clear proposals from that closure, so they go on every end path, next to `McpToolService::forget_session`.

In `src-tauri/src/agent_session/cleanup.rs`, change the `rocket_app` import to `use rocket_app::{McpToolService, ProposalService, SessionCleanup};` and replace `TauriSessionCleanup::new` with:

```rust
    pub fn new(
        mcp_registry: Arc<McpServerRegistry>,
        mcp_tool_svc: Arc<McpToolService>,
        resources: Arc<SessionResourceRegistry>,
        proposals: Arc<ProposalService>,
    ) -> Self {
        Self::with_cache_forgetter(mcp_registry, resources, move |id| {
            mcp_tool_svc.forget_session(id);
            // Pending proposals die with their session. Nothing was written.
            proposals.clear_session(id);
        })
    }
```

`with_cache_forgetter` and its tests do not change. `on_session_ended` calls the closure with the real id and with the pre-handshake id; `clear_session` is idempotent and a no-op for an id with no proposals, which matches the `SessionCleanup` contract.

In `src-tauri/src/lib.rs`, add the fourth argument where `AcpSessionService::new(...)` builds the cleanup (`proposal_svc` is built after `collection_svc` since Task 2, which is before this point):

```rust
                Arc::new(agent_session::cleanup::TauriSessionCleanup::new(
                    Arc::clone(&mcp_server_registry),
                    Arc::clone(&mcp_tool_svc),
                    Arc::clone(&session_resources),
                    Arc::clone(&proposal_svc),
                )),
```

Run `grep -rn "TauriSessionCleanup::new(" src-tauri` and confirm `lib.rs` is the only caller.

- [ ] **Step 5: Write the failing TypeScript test**

Create `src/lib/__tests__/tauri-api.agent-proposals.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  acceptAgentProposal,
  type AgentProposal,
  listAgentProposals,
  onAgentProposalCreated,
  onAgentProposalResolved,
  rejectAgentProposal,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const proposal: AgentProposal = {
  id: 'p1',
  sessionId: 's1',
  change: {
    op: 'editScript',
    collection: 'demo',
    requestPath: 'get-users.yml',
    phase: 'tests',
    body: "rok.test('ok', () => {});",
  },
  summary: "Edit the tests script of 'get-users.yml' in demo",
  status: 'pending',
  createdAtMs: 1,
};

describe('agent proposal commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('lists proposals by session id', async () => {
    vi.mocked(invoke).mockResolvedValue([proposal]);
    await expect(listAgentProposals('s1')).resolves.toEqual([proposal]);
    expect(invoke).toHaveBeenCalledWith('list_agent_proposals', { sessionId: 's1' });
  });

  it('accepts and rejects by session id and proposal id', async () => {
    vi.mocked(invoke).mockResolvedValue({ ...proposal, status: 'accepted' });
    await acceptAgentProposal('s1', 'p1');
    expect(invoke).toHaveBeenCalledWith('accept_agent_proposal', {
      sessionId: 's1',
      proposalId: 'p1',
    });
    await rejectAgentProposal('s1', 'p1');
    expect(invoke).toHaveBeenCalledWith('reject_agent_proposal', {
      sessionId: 's1',
      proposalId: 'p1',
    });
  });

  it('listens on the proposal channels and passes the payload through', async () => {
    vi.mocked(listen).mockResolvedValue(vi.fn());
    const created = vi.fn();
    await onAgentProposalCreated(created);
    expect(listen).toHaveBeenCalledWith('agent-proposal-created', expect.any(Function));
    const payload = {
      type: 'acpProposalCreated' as const,
      session_id: 's1',
      proposal_id: 'p1',
      summary: 'Create folder',
    };
    const handler = vi.mocked(listen).mock.calls[0][1];
    handler({ event: 'agent-proposal-created', id: 1, payload });
    expect(created).toHaveBeenCalledWith(payload);

    await onAgentProposalResolved(vi.fn());
    expect(listen).toHaveBeenCalledWith('agent-proposal-resolved', expect.any(Function));
  });
});
```

- [ ] **Step 6: Check that it fails to type-check**

Run: `yarn tsc --noEmit`
Expected: FAIL with `Module '"../tauri-api"' has no exported member 'listAgentProposals'` (and the other new names).

- [ ] **Step 7: Add the TypeScript types, wrappers and listeners**

In `src/lib/tauri-api.ts`, add directly after the `onAgentSessionFailed` export (and after any agent listeners Plan 01 added):

```ts
// ==== AI Assistant proposals ====
// Proposals are DTOs, so their fields are camelCase. The two events below
// are DomainEvent JSON, so their fields stay snake_case.

export type AgentProposalStatus = 'pending' | 'accepted' | 'rejected' | 'stale' | 'failed';

export interface AgentProposedRequest {
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  queryParams: QueryParam[];
  body?: Body;
  docs?: string;
  preRequestScript?: string;
  postResponseScript?: string;
  tests?: string;
}

/** Only the fields the patch sets are present. */
export interface AgentRequestPatch {
  method?: HttpMethod;
  url?: string;
  headers?: Header[];
  queryParams?: QueryParam[];
  body?: Body;
  docs?: string;
}

export type AgentProposedChange =
  | { op: 'createFolder'; collection: string; parentPath: string; name: string }
  | { op: 'createRequest'; collection: string; folderPath: string; request: AgentProposedRequest }
  | { op: 'updateRequest'; collection: string; requestPath: string; patch: AgentRequestPatch }
  | {
      op: 'editScript';
      collection: string;
      requestPath: string;
      phase: 'preRequest' | 'postResponse' | 'tests';
      body: string;
    }
  | { op: 'moveItem'; collection: string; fromPath: string; toFolder: string }
  | { op: 'renameItem'; collection: string; path: string; newName: string }
  | { op: 'setEnvVar'; collection: string; environment: string; key: string; value: string };

export interface AgentProposal {
  id: string;
  sessionId: string;
  change: AgentProposedChange;
  summary: string;
  status: AgentProposalStatus;
  /** Why a proposal failed. Present only when `status` is `failed`. */
  statusMessage?: string;
  createdAtMs: number;
}

export const listAgentProposals = (sessionId: string) =>
  invoke<AgentProposal[]>('list_agent_proposals', { sessionId });

export const acceptAgentProposal = (sessionId: string, proposalId: string) =>
  invoke<AgentProposal>('accept_agent_proposal', { sessionId, proposalId });

export const rejectAgentProposal = (sessionId: string, proposalId: string) =>
  invoke<AgentProposal>('reject_agent_proposal', { sessionId, proposalId });

export interface AgentProposalCreatedEvent {
  type: 'acpProposalCreated';
  session_id: string;
  proposal_id: string;
  summary: string;
}

export interface AgentProposalResolvedEvent {
  type: 'acpProposalResolved';
  session_id: string;
  proposal_id: string;
  status: Exclude<AgentProposalStatus, 'pending'>;
}

export const onAgentProposalCreated = (
  handler: (event: AgentProposalCreatedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentProposalCreatedEvent>('agent-proposal-created', (e) => handler(e.payload));

export const onAgentProposalResolved = (
  handler: (event: AgentProposalResolvedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentProposalResolvedEvent>('agent-proposal-resolved', (e) => handler(e.payload));
```

- [ ] **Step 8: Verify**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS with no new warnings.

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. If it reports only import order or formatting in the two new or edited TypeScript files, run `yarn lint` and `yarn format`, then run `yarn check` again.

For the user to run:
- `cargo test -p rocket agent_proposals -j4`
- `yarn test tauri-api.agent-proposals`

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat: add agent proposal IPC commands and TS bindings` for exactly these paths:
- `src-tauri/src/commands/agent_proposals.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/agent_session/cleanup.rs`
- `src/lib/tauri-api.ts`
- `src/lib/__tests__/tauri-api.agent-proposals.test.ts`

---

## Interface deviations

None of the locked names or shapes in the index change. These are additions the index did not spell out:

- `ProposalService::new(collections: CollectionService, environment_repo_factory: Arc<dyn EnvironmentRepositoryFactory>, events: Arc<dyn EventPublisher>)`. The constructor was not in the index. `ProposalService` owns its own `CollectionService` (built in `lib.rs` like `collection_svc`) because `collection_svc` is managed by value.
- No session-id mapping in `ProposalService` (an earlier draft had `link_session`). The tools use Plan 03's `McpSessionBinding`, so the MCP server reports the real ACP session id after the handshake. Resolved in the index under "Interface deviations resolved".
- `TauriSessionCleanup::new` (Plan 02) gains a fourth parameter, `proposals: Arc<ProposalService>`, and clears proposals in its cache-forgetter closure.
- `CollectionService::save_request_script`, so `EditScript` applies through the service and publishes `RequestSaved`.
- The fields of `ProposedRequest` and `RequestPatch`, the `ScriptPhase` enum in `rocket-acp`, and helper methods on `ProposedChange`, `ProposalStatus`, `RequestPatch` and `AgentProposal`.
- `AgentProposalDto.change` uses camelCase `op` tags (`editScript`) and leaves out `base_fingerprint`. `AgentProposalDto` adds `statusMessage` for `Failed { message }`.
- `set_env_var` proposals may add a missing non-secret variable. The removed direct tool refused missing keys; a proposal needs the user's Accept, and Plan 03's `get_environment` already lists secret names, so there is no name oracle to protect.

## Next Plan

**Plan 05 — Panel UI** (`docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-05-panel-ui.md`). It builds `assistant-store` (`proposals: AgentProposal[]`, `upsertProposal`, `resolveProposal`) on this plan's `listAgentProposals`, `acceptAgentProposal`, `rejectAgentProposal`, `onAgentProposalCreated` and `onAgentProposalResolved`, and renders `AssistantProposalCard` with a Monaco diff for `editScript` and `updateRequest` (the "before" side comes from `getRequest`), the new item for creates, and `statusMessage` for `failed` and a re-propose hint for `stale` cards. It also removes the per-tab chat.

## Post-Implementation Review

After every task above is checked off and `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green, dispatch a review subagent:

```
Agent({
  subagent_type: "general-purpose",
  model: "opus",
  description: "Plan 04 proposals-backend review",
  prompt: "Review the full diff this plan produced (the commits of all 3 tasks of
    docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-04-proposals-backend.md;
    write it to a file with git diff and review that file, not the live tree).
    Read that plan file, docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md
    and section 4 of docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md
    in full first. You may fix what you find directly (edit files, run
    cargo check --workspace --all-targets -j4, yarn tsc --noEmit and yarn check,
    commit through the dev-workflow-skills:1-git-commit skill with explicit paths).
    Do not run cargo test --workspace; the user runs tests. Check for:
    (1) Interface gaps against the index's locked Plan 04 contract: AgentProposal,
    ProposalStatus, the seven ProposedChange variants and their field names,
    ProposalService::{propose, list, accept, reject, clear_session}, the
    AcpProposalCreated/AcpProposalResolved fields and channels, the three IPC
    command names, AgentProposalDto camelCase, and that edit_script and
    set_env_var are gone from both the MCP tool list and McpToolService.
    (2) Safety: no proposal path writes before Accept; Accept writes at most once
    per proposal; a Stale or Failed proposal wrote nothing; set_env_var can never
    write a secret variable; an auth field from the agent is refused; tool and
    error messages never echo script bodies, header values or variable values;
    the Mutex is never unwrapped (poison-safe) and no unwrap calls exist in
    production code.
    (3) Session ids: the propose and list tools pass self.binding.session_id()
    (Plan 03's McpSessionBinding), so proposals are listed, accepted and cleared
    under the real ACP session id; every command that spawns an MCP server binds
    it right after the handshake; and TauriSessionCleanup::new's closure calls
    clear_session, so it runs on every end path.
    (4) DDD boundaries: rocket-acp still depends only on rocket-shared among
    workspace crates; rocket-app does no I/O and uses only traits and services;
    src-tauri commands stay thin; serde camelCase only on the IPC DTOs.
    (5) Code quality: comments are short full sentences ending with a period,
    no leftover placeholders, no duplicated path or fingerprint logic beyond
    validate_environment_name (copied on purpose from McpToolService).
    Report what you found and what you fixed, in under 400 words."
})
```
