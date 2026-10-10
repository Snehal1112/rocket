# Assistant Conversation History, Plan 01: Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store finished workspace-assistant conversations per workspace at `<workspace>/assistant/conversations/<id>.yml`, with secret variable values masked, caps applied, and five IPC commands plus three id-only events, so Plan 02 can save and show them.

**Architecture:** `rocket-acp` gets pure conversation types, caps and the `ConversationRepository` trait; `rocket-shared` gets three `DomainEvent` variants that `TauriEventBus` maps to channels. `rocket-infra` writes one YAML file per conversation (`FsConversationRepo`) and resolves the active workspace per call (`SharedPathConversationRepo`, with `pinned()`). `rocket-app`'s `ConversationService` pins the workspace repositories when an assistant session starts (called from `start_workspace_assistant_inner`), masks secret variable values of the pinned workspace with the chips' `mask_secret_text`, applies the caps and publishes the events; `src-tauri` exposes it through thin async commands.

**Tech Stack:** Rust (`rocket-acp`, `rocket-shared`, `rocket-infra`, `rocket-app`, `src-tauri`), `serde`, `serde_yaml`, `chrono` (already a `rocket-app` dependency), `tempfile` in tests.

**Spec:** [`docs/superpowers/specs/2026-10-11-assistant-conversation-history-design.md`](../../specs/2026-10-11-assistant-conversation-history-design.md), sections 4 to 6, 8 to 10. Plan index: [`00-index.md`](00-index.md).

## Verified facts

Line numbers are from `worktree-acp-mcp-tool-server` at `e9f01191`. Anchor every edit by content as well as by line.

- `.gitignore` precedent: `FsCookieRepo::ensure_gitignore` (`crates/rocket-infra/src/fs_cookie_repo.rs:26-43`) uses `create_new`, writes `*`, and only logs a failure; its tests are at `:143-178`. `atomic_write` creates parent directories (`crates/rocket-infra/src/atomic_write.rs:15-20`).
- One-file-per-entry precedent: `FsHistoryRepo` (`crates/rocket-infra/src/fs_history_repo.rs:10-68`). `yaml_io::delete_if_exists(path, label)` returns `NotFound` for a missing file (`crates/rocket-infra/src/yaml_io.rs:55-61`).
- Per-call workspace precedent: `SharedPathHistoryRepo` (`crates/rocket-infra/src/shared_path_history_repo.rs:13-32`) has **no** `pinned()`. The `pinned()` precedent is `SharedPathCookieRepo::pinned` (`crates/rocket-infra/src/shared_path_cookie_repo.rs:54-56`) and `SharedPathEnvironmentRepo::pinned` (`crates/rocket-infra/src/shared_path_environment_repo.rs:63-65`).
- Persisted key-set guard precedent: `persisted_yaml_never_contains_a_raw_credential_field` (`crates/rocket-infra/src/fs_agent_config_repo.rs:218-269`). Domain struct persisted directly with plain snake_case serde: `AgentConfig` (`crates/rocket-acp/src/agent_config.rs:29-50`).
- `rocket-acp` depends on `async-trait`, `rocket-shared`, `serde`, `serde_json`, `tokio` only (`crates/rocket-acp/Cargo.toml`). Module list and re-exports: `crates/rocket-acp/src/lib.rs:1-19`.
- `DomainEvent` is `#[serde(tag = "type", rename_all = "camelCase")]` with snake_case fields (`crates/rocket-shared/src/events.rs:276-278`). The last ACP variant `AcpUsage` ends at `:594-599`. Unit-variant precedent: `HistoryCleared` (`:675`). Wire-shape test precedent: `acp_tool_invoked_wire_shape` (`:1948-1960`).
- `TauriEventBus::publish` holds an exhaustive inline `match` (`src-tauri/src/tauri_event_bus.rs:16-102`) and has no tests. It is the only exhaustive match on `DomainEvent` outside `rocket-shared`.
- Masking helper used by the chips: `mcp_read_views::mask_secret_text(text, &HashSet<String>)` (`crates/rocket-app/src/mcp_read_views.rs:754-760`), which expands `redaction_forms` and replaces with `redaction::REDACTED` (`"••••••"`, `crates/rocket-app/src/redaction.rs:7`). Both are `pub(crate)`.
- The chips collect secret values with `McpToolService::chip_secrets` (`crates/rocket-app/src/mcp_tool_service/chips.rs:230-306`) for **one** collection and request path, through the **live** active workspace. There is no workspace-wide collector, so Task 3 adds one over pinned repositories. Scope order and the "value or initial value" rule come from `RequestExecutionService::scopes_inner` (`crates/rocket-app/src/execution_service.rs:740-835`).
- `McpToolService` pins the workspace per session (`workspace_pins`, `crates/rocket-app/src/mcp_tool_service.rs:154`, `open_session` `:305-320`) but `forget_session` drops the pin on every session end path (`:826-840`). The final save runs after the session ended, so that pin cannot be reused; `ConversationService` keeps its own.
- Repositories expose `pinned()`: `CollectionRepository::pinned` (`crates/rocket-collection/src/repository.rs:387-389`), `EnvironmentRepository::pinned` and `EnvironmentRepositoryFactory::pinned` (`crates/rocket-environment/src/repository.rs:16-18`, `:39-41`). `with_send_pins` uses them the same way (`crates/rocket-app/src/execution_service/send_pins.rs:72-86`).
- `CollectionRepository::list` lists only folders under `<workspace>/collections` (`crates/rocket-infra/src/fs_collection/folders.rs:19-60`); `collection_identity(name).uid` is the `opencollection.yml` uid (`crates/rocket-collection/src/trust.rs:139-142`). `CollectionItem` variants: `crates/rocket-collection/src/folder.rs:23-50`. `runner_sequence::folder_dir_name` is `pub(crate)` (`crates/rocket-app/src/runner_sequence.rs:158-160`).
- `start_workspace_assistant_inner` (`src-tauri/src/commands/acp_sessions.rs:144-235`) calls `mcp_tool_svc.begin_assistant_session` right after the handshake (`:191-195`). Its integration test is `src-tauri/tests/acp_mcp_start_workspace_assistant.rs` (fixture `:243-305`, call `:330-342`).
- Async command precedent: `src-tauri/src/commands/agent_proposals.rs:243-317` (`run_blocking` with `tauri::async_runtime::spawn_blocking`, `_inner` functions, tests with `#[tokio::test]` at `:356-390`). Command modules: `src-tauri/src/commands/mod.rs:1-31`.
- `src-tauri/src/lib.rs`: `mcp_tool_svc` built at `:646-662`, `env_secret_store()` at `:39-41`, `app.manage(...)` block `:766-794` (`app.manage(active_workspace_path)` at `:790` moves the path, so build the service before it), assistant commands in `generate_handler!` at `:1046-1057`. `SharedPathEnvironmentRepo` is already imported (`:26`).
- `rocket-app` test doubles: `ConfigurableCollectionRepo` (`crates/rocket-app/src/test_doubles.rs:416-502`, `get` reads `with_summaries`, `get_folder_settings` returns `NotFound` for an unconfigured folder at `:628-639`), `RecordingPublisher::new() -> Arc<Self>` and `events()` (`:1024-1046`). `rocket-infra` and `tempfile` are dev-dependencies of `rocket-app` (`crates/rocket-app/Cargo.toml`).
- Package names for `cargo -p`: `rocket-acp`, `rocket-shared`, `rocket-infra`, `rocket-app`, and `rocket` for `src-tauri` (library crate `rocket_lib`).

## Global Constraints

- OC-1: Location is `<workspace>/assistant/conversations/<id>.yml` only, never inside `<workspace>/collections/`, a collection, a folder or a linked collection's directory.
- OC-2: No transcript data and no new keys go into `opencollection.yml`, `folder.yml`, request files, environment files or `workspace.yml`; no `extensions.*` keys.
- OC-3: Files are `.yml`, named `<id>.yml` with a generated lowercase UUID, never a reserved name; writes use `atomic_write`.
- OC-4: `<workspace>/assistant/.gitignore` holds `*`, is created with `create_new` before the first write, never overwritten, and a failure is only logged.
- OC-5: The path is resolved on every call from the active workspace (or the session's pinned one), directories are created lazily, and the folder is outside the collections watcher.
- OC-6: A collection is referenced only by its `opencollection.yml` uid (plus name) as metadata, never by a path.
- OC-7: Reads tolerate missing and unknown keys and skip an unparseable or other-version file with a warning.
- Persistence keys are snake_case with no `rename_all = "camelCase"`; camelCase only on the IPC DTOs in `src-tauri/src/commands/assistant_conversations.rs`.
- Caps: 50 conversations per workspace (oldest `updated_at` evicted), 500 messages per conversation (newest kept), 16 KB (16384 bytes) of text per message, cut with `TEXT_CUT_MARKER`.
- A continued conversation always starts in Ask mode (Plan 03).
- Events carry ids only: `AssistantConversationSaved { conversation_id }`, `AssistantConversationDeleted { conversation_id }`, `AssistantConversationsCleared`.
- No vault scrubbing in v1: only secret variable values are masked; vault values are never fetched here.
- Always pass `-j4` to `cargo test|check|clippy|build`; never `cargo test --workspace` or `--all`.
- No `unwrap()` in production code; tests use `.expect("message")`.
- Frontend (not touched here): shadcn/ui primitives and lucide-react icons only; narrow Zustand selectors.
- Harness: no bare `git stash`/`git stash pop`; no `git add -A`, `--all` or `.`; staged `.rs` runs `cargo check -j4` before commit (`HARNESS_GATE=0` skips).
- Commits: every commit goes through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and a pathspec commit (`git commit --only -- <paths>`), never `git add -A`.
- Comments are short full sentences ending with a period.

## Review Focus

1. **A secret that straddles the 16 KB cut.** Masking must run before the text cap, or the cut keeps half a secret. Test: `masking_runs_before_the_text_cap` (Task 3).
2. **A workspace switch between session start and the final save.** The save must land in, and be masked with, the workspace pinned at session start, even after `McpToolService::forget_session` ran. Tests: `save_after_a_workspace_switch_writes_into_the_pinned_workspace` (Task 3) and `calls_follow_the_active_workspace_and_pinned_stays` (Task 2).
3. **Secret values in every scope and every text field.** Collection value and initial value, folder, request, collection environment and global environment secrets are masked in user, agent, tool, notice and proposal text and in the title; plain values stay. A scope that cannot be read refuses the save. Tests: `save_masks_secret_values_of_every_scope_in_every_text_field`, `an_unreadable_variable_scope_refuses_the_save` (Task 3).
4. **A corrupt, hand-edited or other-version file in the folder.** `list` skips it with a warning, `get` returns a clear error, and eviction never removes the conversation just saved. Tests: `list_skips_corrupt_and_other_version_files_and_get_reports_them`, `eviction_never_removes_the_conversation_just_saved` (Task 2).
5. **An id that is not a lowercase UUID** (`../escape`, upper case, a `.yml` suffix). Every repository and service entry point refuses it and nothing is written. Tests: `ids_must_be_lowercase_uuids` (Task 1), `an_invalid_id_is_refused_without_writing` (Task 2), `a_save_needs_a_known_session_and_an_agent_reply` (Task 3).

---

### Task 1: Conversation types, repository trait and events

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-acp/src/conversation.rs`
- Modify: `crates/rocket-acp/src/lib.rs` (module list `:1-7`, re-exports `:8-19`)
- Modify: `crates/rocket-acp/CLAUDE.md` (Module Map table, after the `proposal.rs` row `:38`)
- Modify: `crates/rocket-shared/src/events.rs` (variants after `AcpUsage` `:594-599`; tests after `acp_tool_invoked_wire_shape` `:1948-1960`)
- Modify: `src-tauri/src/tauri_event_bus.rs` (extract `channel_for`, `:14-113`)
- Test: `crates/rocket-acp/src/conversation.rs` (`mod tests`), `crates/rocket-shared/src/events.rs` (`mod tests`), `src-tauri/src/tauri_event_bus.rs` (new `mod tests`)

**Interfaces:**
- Consumes: `rocket_shared::error::{DomainError, DomainResult}`.
- Produces (exact):
  ```rust
  // rocket_acp::conversation (re-exported at the crate root)
  pub const CONVERSATION_FORMAT_VERSION: u32 = 1;
  pub const MAX_CONVERSATIONS: usize = 50;
  pub const MAX_MESSAGES: usize = 500;
  pub const MAX_MESSAGE_TEXT_BYTES: usize = 16 * 1024;
  pub const MAX_TITLE_CHARS: usize = 80;
  pub const TEXT_CUT_MARKER: &str = "\n\n[cut: longer than 16 KB]";
  pub enum ConversationMode { Ask, Edit, Agent }                       // serde "ask" | "edit" | "agent", Default = Ask
  pub enum StoredToolStatus { Pending, InProgress, Completed, Failed } // serde snake_case
  pub enum StoredProposalStatus { Pending, Accepted, Rejected, Stale, Failed }
  pub enum StoredMessage {                                             // serde tag "kind", snake_case
      User { text: String }, Agent { text: String },
      Tool { title: String, status: StoredToolStatus },
      Notice { text: String },
      Proposal { summary: String, status: StoredProposalStatus },
  }
  impl StoredMessage { pub fn map_text(self, f: impl Fn(&str) -> String) -> Self; }
  pub struct ConversationCollection { pub uid: String, pub name: String }
  pub struct ConversationRecord { pub version: u32, pub id: String, pub title: String,
      pub agent_config_id: String, pub mode: ConversationMode, pub workspace_id: String,
      pub created_at: String, pub updated_at: String, pub collections: Vec<ConversationCollection>,
      pub continued_from: Option<String>, pub messages: Vec<StoredMessage> }
  impl ConversationRecord { pub fn summary(&self) -> ConversationSummary; pub fn apply_caps(&mut self); }
  pub struct ConversationSummary { pub id: String, pub title: String, pub agent_config_id: String,
      pub mode: ConversationMode, pub created_at: String, pub updated_at: String,
      pub message_count: usize, pub continued_from: Option<String> }
  pub fn validate_conversation_id(id: &str) -> DomainResult<()>;
  pub fn cap_message_text(text: &str) -> String;
  pub fn conversation_title(messages: &[StoredMessage]) -> String;
  pub fn has_agent_reply(messages: &[StoredMessage]) -> bool;
  pub trait ConversationRepository: Send + Sync {
      fn list(&self) -> DomainResult<Vec<ConversationSummary>>;
      fn get(&self, id: &str) -> DomainResult<ConversationRecord>;
      fn save(&self, record: &ConversationRecord) -> DomainResult<()>;
      fn delete(&self, id: &str) -> DomainResult<()>;
      fn clear(&self) -> DomainResult<()>;
      fn pinned(&self) -> Option<Box<dyn ConversationRepository>> { None }
  }

  // rocket_shared::events::DomainEvent
  AssistantConversationSaved { conversation_id: String }    // channel "assistant-conversation-saved"
  AssistantConversationDeleted { conversation_id: String }  // channel "assistant-conversation-deleted"
  AssistantConversationsCleared                             // channel "assistant-conversations-cleared"

  // src-tauri/src/tauri_event_bus.rs
  pub(crate) fn channel_for(event: &DomainEvent) -> &'static str;
  ```

- [ ] **Step 1: 📖 Read the OpenCollection reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md` and spec section 4. Confirm nothing in this task touches a collection, folder, request or environment file.

- [ ] **Step 2: Write the failing domain tests**

Create `crates/rocket-acp/src/conversation.rs` with only this content:

```rust
//! Stored assistant conversations. Pure data and rules: the files live in
//! `rocket-infra` (`FsConversationRepo`), and masking and workspace pins in
//! `rocket-app` (`ConversationService`). The YAML keys are the snake_case
//! field names, because this is a Rocket-private store, not an
//! OpenCollection file.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn user(text: &str) -> StoredMessage {
        StoredMessage::User { text: text.to_string() }
    }

    fn agent(text: &str) -> StoredMessage {
        StoredMessage::Agent { text: text.to_string() }
    }

    fn record(messages: Vec<StoredMessage>) -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: ID.to_string(),
            title: String::new(),
            agent_config_id: "claude-code".to_string(),
            mode: ConversationMode::Ask,
            workspace_id: "default".to_string(),
            created_at: "2026-10-11T09:12:00Z".to_string(),
            updated_at: "2026-10-11T09:20:41Z".to_string(),
            collections: Vec::new(),
            continued_from: None,
            messages,
        }
    }

    #[test]
    fn messages_serialize_with_a_kind_tag_and_snake_case_values() {
        let tool = StoredMessage::Tool {
            title: "run_request: login".into(),
            status: StoredToolStatus::InProgress,
        };
        assert_eq!(
            serde_json::to_string(&tool).expect("serialize"),
            r#"{"kind":"tool","title":"run_request: login","status":"in_progress"}"#
        );
        let proposal = StoredMessage::Proposal {
            summary: "Change header X".into(),
            status: StoredProposalStatus::Accepted,
        };
        assert_eq!(
            serde_json::to_string(&proposal).expect("serialize"),
            r#"{"kind":"proposal","summary":"Change header X","status":"accepted"}"#
        );
        assert_eq!(
            serde_json::to_string(&ConversationMode::Agent).expect("serialize"),
            r#""agent""#
        );
    }

    #[test]
    fn a_record_with_unknown_and_missing_keys_still_loads() {
        let json = format!(
            r#"{{"version":1,"id":"{ID}","future_key":true,"messages":[{{"kind":"user","text":"hi"}}]}}"#
        );
        let loaded: ConversationRecord = serde_json::from_str(&json).expect("parse");
        assert_eq!(loaded.mode, ConversationMode::Ask);
        assert!(loaded.collections.is_empty());
        assert_eq!(loaded.continued_from, None);
        assert_eq!(loaded.messages, vec![user("hi")]);
    }

    #[test]
    fn ids_must_be_lowercase_uuids() {
        assert!(validate_conversation_id(ID).is_ok());
        for bad in [
            "",
            "../etc/passwd",
            "0B6F3F4E-0000-4000-8000-000000000001",
            "0b6f3f4e00004000800000000000000001xx",
            "0b6f3f4e-0000-4000-8000-00000000000g",
            "0b6f3f4e-0000-4000-8000-000000000001.yml",
        ] {
            assert!(
                matches!(validate_conversation_id(bad), Err(DomainError::InvalidInput(_))),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary_with_the_marker() {
        let text = "é".repeat(MAX_MESSAGE_TEXT_BYTES);
        let cut = cap_message_text(&text);
        assert!(cut.len() <= MAX_MESSAGE_TEXT_BYTES);
        assert!(cut.ends_with(TEXT_CUT_MARKER));
        let exact = "a".repeat(MAX_MESSAGE_TEXT_BYTES);
        assert_eq!(cap_message_text(&exact), exact);
    }

    #[test]
    fn apply_caps_keeps_the_newest_messages_and_cuts_every_text_field() {
        let mut messages: Vec<StoredMessage> =
            (0..MAX_MESSAGES + 3).map(|i| user(&format!("m{i}"))).collect();
        messages.push(StoredMessage::Tool {
            title: "t".repeat(MAX_MESSAGE_TEXT_BYTES + 1),
            status: StoredToolStatus::Completed,
        });
        let mut r = record(messages);
        r.apply_caps();
        assert_eq!(r.messages.len(), MAX_MESSAGES);
        assert_eq!(r.messages[0], user("m4"));
        let StoredMessage::Tool { title, .. } = &r.messages[MAX_MESSAGES - 1] else {
            panic!("the tool line stays last");
        };
        assert!(title.ends_with(TEXT_CUT_MARKER));
    }

    #[test]
    fn the_title_is_the_first_user_message_on_one_line_and_at_most_80_chars() {
        let messages = vec![agent("hello"), user("  Why does\nlogin   return 401?  "), user("second")];
        assert_eq!(conversation_title(&messages), "Why does login return 401?");
        let long = vec![user(&"x".repeat(200))];
        assert_eq!(conversation_title(&long).chars().count(), MAX_TITLE_CHARS);
        assert_eq!(conversation_title(&[agent("only")]), "");
    }

    #[test]
    fn an_agent_reply_needs_text() {
        assert!(!has_agent_reply(&[user("hi"), agent("  ")]));
        assert!(has_agent_reply(&[user("hi"), agent("Hello")]));
    }

    #[test]
    fn map_text_reaches_every_text_field() {
        let upper = |t: &str| t.to_uppercase();
        assert_eq!(user("a").map_text(upper), user("A"));
        let tool = StoredMessage::Tool { title: "t".into(), status: StoredToolStatus::Failed };
        assert_eq!(
            tool.map_text(upper),
            StoredMessage::Tool { title: "T".into(), status: StoredToolStatus::Failed }
        );
        let proposal = StoredMessage::Proposal { summary: "s".into(), status: StoredProposalStatus::Stale };
        assert_eq!(
            proposal.map_text(upper),
            StoredMessage::Proposal { summary: "S".into(), status: StoredProposalStatus::Stale }
        );
    }

    #[test]
    fn the_summary_counts_messages() {
        let r = record(vec![user("a"), agent("b")]);
        let s = r.summary();
        assert_eq!(s.message_count, 2);
        assert_eq!(s.id, r.id);
        assert_eq!(s.updated_at, r.updated_at);
    }

    #[test]
    fn the_repository_trait_is_object_safe() {
        fn _assert(_: Box<dyn ConversationRepository>) {}
    }
}
```

Add `pub mod conversation;` to `crates/rocket-acp/src/lib.rs` after `pub mod agent_config;` (`:1`).

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p rocket-acp -j4 conversation::`
Expected: compile errors such as `cannot find type 'StoredMessage' in this scope` and `cannot find function 'validate_conversation_id'`.

- [ ] **Step 4: Implement the types**

Insert between the `use` lines and `#[cfg(test)]` in `crates/rocket-acp/src/conversation.rs`:

```rust
/// The `version` this build writes and reads. Other versions are skipped.
pub const CONVERSATION_FORMAT_VERSION: u32 = 1;
/// Conversations kept per workspace. Saving one more removes the oldest.
pub const MAX_CONVERSATIONS: usize = 50;
/// Messages kept per conversation. The newest ones are kept.
pub const MAX_MESSAGES: usize = 500;
/// Bytes of text kept per message field, marker included.
pub const MAX_MESSAGE_TEXT_BYTES: usize = 16 * 1024;
/// Characters kept for the title.
pub const MAX_TITLE_CHARS: usize = 80;
/// Ends a text that was cut.
pub const TEXT_CUT_MARKER: &str = "\n\n[cut: longer than 16 KB]";

/// The assistant mode when the conversation was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMode {
    #[default]
    Ask,
    Edit,
    Agent,
}

/// A tool line's last status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// A proposal's status when the conversation was saved. History only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredProposalStatus {
    Pending,
    Accepted,
    Rejected,
    Stale,
    Failed,
}

/// One line of a stored conversation. Tool results, the ACP session id,
/// usage numbers and proposal payloads are never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredMessage {
    User { text: String },
    Agent { text: String },
    Tool { title: String, status: StoredToolStatus },
    Notice { text: String },
    Proposal { summary: String, status: StoredProposalStatus },
}

impl StoredMessage {
    /// Applies `f` to every free-text field. Used for masking and capping.
    pub fn map_text(self, f: impl Fn(&str) -> String) -> Self {
        match self {
            Self::User { text } => Self::User { text: f(&text) },
            Self::Agent { text } => Self::Agent { text: f(&text) },
            Self::Tool { title, status } => Self::Tool { title: f(&title), status },
            Self::Notice { text } => Self::Notice { text: f(&text) },
            Self::Proposal { summary, status } => Self::Proposal {
                summary: f(&summary),
                status,
            },
        }
    }
}

/// A collection the chat focused on, by its `opencollection.yml` uid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationCollection {
    pub uid: String,
    pub name: String,
}

/// One stored conversation, the whole file. Every field but `version` and
/// `id` has a default, so an older or hand-edited file still loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub version: u32,
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub agent_config_id: String,
    #[serde(default)]
    pub mode: ConversationMode,
    /// Informational. The file's location decides the workspace.
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub collections: Vec<ConversationCollection>,
    /// The conversation this one continued (Plan 03).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
}

/// What the History list shows for one conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub continued_from: Option<String>,
}

impl ConversationRecord {
    pub fn summary(&self) -> ConversationSummary {
        ConversationSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            agent_config_id: self.agent_config_id.clone(),
            mode: self.mode,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            message_count: self.messages.len(),
            continued_from: self.continued_from.clone(),
        }
    }

    /// Keeps the newest `MAX_MESSAGES` messages and cuts every text field to
    /// `MAX_MESSAGE_TEXT_BYTES`. Mask before calling this, so a cut never
    /// keeps part of a secret.
    pub fn apply_caps(&mut self) {
        if self.messages.len() > MAX_MESSAGES {
            let excess = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..excess);
        }
        let messages = std::mem::take(&mut self.messages);
        self.messages = messages
            .into_iter()
            .map(|message| message.map_text(cap_message_text))
            .collect();
    }
}

/// Refuses anything but a lowercase UUID, so an id is always a safe file name.
pub fn validate_conversation_id(id: &str) -> DomainResult<()> {
    let bytes = id.as_bytes();
    let shaped = bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(b),
        });
    if shaped {
        Ok(())
    } else {
        Err(DomainError::InvalidInput(
            "invalid conversation id".to_string(),
        ))
    }
}

/// Cuts `text` to `MAX_MESSAGE_TEXT_BYTES`, marker included, on a char boundary.
pub fn cap_message_text(text: &str) -> String {
    if text.len() <= MAX_MESSAGE_TEXT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_MESSAGE_TEXT_BYTES - TEXT_CUT_MARKER.len();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{TEXT_CUT_MARKER}", &text[..end])
}

/// The first user message on one line, at most `MAX_TITLE_CHARS` characters.
pub fn conversation_title(messages: &[StoredMessage]) -> String {
    let first = messages
        .iter()
        .find_map(|message| match message {
            StoredMessage::User { text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("");
    first
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

/// True when the agent has replied with some text.
pub fn has_agent_reply(messages: &[StoredMessage]) -> bool {
    messages
        .iter()
        .any(|message| matches!(message, StoredMessage::Agent { text } if !text.trim().is_empty()))
}

/// Stored conversations of one workspace. Implemented in `rocket-infra`.
pub trait ConversationRepository: Send + Sync {
    /// Every readable conversation, newest `updated_at` first. Unparseable
    /// and other-version files are skipped with a warning.
    fn list(&self) -> DomainResult<Vec<ConversationSummary>>;
    /// `NotFound` for a missing file, `InvalidInput` for one that cannot be read.
    fn get(&self, id: &str) -> DomainResult<ConversationRecord>;
    /// Writes the whole record, applies the caps and evicts beyond `MAX_CONVERSATIONS`.
    fn save(&self, record: &ConversationRecord) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
    /// Removes every conversation file of the workspace.
    fn clear(&self) -> DomainResult<()>;
    /// A repository fixed to the workspace this one points at now, or `None`
    /// for one with a fixed location.
    fn pinned(&self) -> Option<Box<dyn ConversationRepository>> {
        None
    }
}
```

In `crates/rocket-acp/src/lib.rs` add after `pub use agent_config::{...};`:

```rust
pub use conversation::{
    cap_message_text, conversation_title, has_agent_reply, validate_conversation_id,
    ConversationCollection, ConversationMode, ConversationRecord, ConversationRepository,
    ConversationSummary, StoredMessage, StoredProposalStatus, StoredToolStatus,
    CONVERSATION_FORMAT_VERSION, MAX_CONVERSATIONS, MAX_MESSAGES, MAX_MESSAGE_TEXT_BYTES,
    MAX_TITLE_CHARS, TEXT_CUT_MARKER,
};
```

- [ ] **Step 5: Run the domain tests to see them pass**

Run: `cargo test -p rocket-acp -j4 conversation::`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 6: Write the failing event tests**

In `crates/rocket-shared/src/events.rs`, after `acp_tool_invoked_wire_shape` (`:1948-1960`), add:

```rust
    #[test]
    fn assistant_conversation_events_carry_ids_only() {
        let saved = DomainEvent::AssistantConversationSaved {
            conversation_id: "c1".into(),
        };
        assert_eq!(
            serde_json::to_string(&saved).expect("serialize"),
            r#"{"type":"assistantConversationSaved","conversation_id":"c1"}"#
        );
        let deleted = DomainEvent::AssistantConversationDeleted {
            conversation_id: "c1".into(),
        };
        assert_eq!(
            serde_json::to_string(&deleted).expect("serialize"),
            r#"{"type":"assistantConversationDeleted","conversation_id":"c1"}"#
        );
        assert_eq!(
            serde_json::to_string(&DomainEvent::AssistantConversationsCleared).expect("serialize"),
            r#"{"type":"assistantConversationsCleared"}"#
        );
    }
```

At the end of `src-tauri/src/tauri_event_bus.rs` add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_conversation_events_have_their_own_channels() {
        let id = || "c1".to_string();
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationSaved { conversation_id: id() }),
            "assistant-conversation-saved"
        );
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationDeleted { conversation_id: id() }),
            "assistant-conversation-deleted"
        );
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationsCleared),
            "assistant-conversations-cleared"
        );
        assert_eq!(channel_for(&DomainEvent::HistoryCleared), "history-changed");
    }
}
```

- [ ] **Step 7: Run them to see them fail**

Run: `cargo test -p rocket-shared -j4 assistant_conversation_events`
Expected: `no variant named 'AssistantConversationSaved' found for enum 'DomainEvent'`.

- [ ] **Step 8: Add the variants and the channels**

In `crates/rocket-shared/src/events.rs`, after the `AcpUsage { ... },` variant (`:594-599`), add:

```rust
    /// Emitted after an assistant conversation was saved. Carries the id
    /// only, never message text.
    AssistantConversationSaved { conversation_id: String },
    /// Emitted after one stored assistant conversation was deleted.
    AssistantConversationDeleted { conversation_id: String },
    /// Emitted after every stored conversation of the workspace was deleted.
    AssistantConversationsCleared,
```

In `src-tauri/src/tauri_event_bus.rs`, move the whole `match &event { ... }` of `publish` (`:16-102`) into a free function, and add three arms after `DomainEvent::AcpUsage { .. } => "agent-session-usage",`:

```rust
/// The frontend channel for each event.
pub(crate) fn channel_for(event: &DomainEvent) -> &'static str {
    match event {
        // (every existing arm, unchanged, in the same order)
        DomainEvent::AcpUsage { .. } => "agent-session-usage",
        // Stored assistant conversations. The payloads hold ids only.
        DomainEvent::AssistantConversationSaved { .. } => "assistant-conversation-saved",
        DomainEvent::AssistantConversationDeleted { .. } => "assistant-conversation-deleted",
        DomainEvent::AssistantConversationsCleared => "assistant-conversations-cleared",
        // (the remaining existing arms, unchanged)
    }
}
```

and make `publish` start with:

```rust
    fn publish(&self, event: DomainEvent) {
        let event_name = channel_for(&event);
        let _ = self.app.emit(event_name, &event);
```

keeping the `BranchSwitched`/`BranchMerged` extra emit (`:104-112`) as it is. The existing arms move verbatim; only `match &event` becomes `match event`.

- [ ] **Step 9: Run the event tests to see them pass**

Run: `cargo test -p rocket-shared -j4 assistant_conversation_events`
Expected: `1 passed`.
Run: `cargo test -p rocket -j4 --lib tauri_event_bus`
Expected: `1 passed`.

- [ ] **Step 10: Update the crate guide**

In `crates/rocket-acp/CLAUDE.md`, Module Map, add after the `proposal.rs` row:

```markdown
| `conversation.rs` | Stored assistant conversations: `ConversationRecord`, `ConversationSummary`, `StoredMessage`, caps (`MAX_CONVERSATIONS`, `MAX_MESSAGES`, `MAX_MESSAGE_TEXT_BYTES`), `validate_conversation_id` (lowercase UUID) and the `ConversationRepository` trait. Persisted as snake_case YAML by `rocket-infra`; see `.claude/assistant-conversations.md`. |
```

- [ ] **Step 11: Check and commit**

Run: `cargo check --workspace --all-targets -j4`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths:

```bash
git add crates/rocket-acp/src/conversation.rs crates/rocket-acp/src/lib.rs crates/rocket-acp/CLAUDE.md crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs
git commit --only -m "feat(acp): add stored conversation types and events" -- crates/rocket-acp/src/conversation.rs crates/rocket-acp/src/lib.rs crates/rocket-acp/CLAUDE.md crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs
```

---

### Task 2: `FsConversationRepo` and `SharedPathConversationRepo`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/fs_conversation_repo.rs`
- Create: `crates/rocket-infra/src/shared_path_conversation_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (`pub mod` after `fs_contract_repo` `:16` and after `shared_path_collection_repo` `:38`; `pub use` after `:61` and after `:81`)
- Modify: `crates/rocket-infra/CLAUDE.md` (Public types table, after the `SharedPathEnvironmentRepo, ...` row `:36`)
- Test: both new files (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `rocket_acp::conversation::*`, `crate::atomic_write`, `crate::yaml_io::delete_if_exists`.
- Produces (exact):
  ```rust
  // rocket_infra (re-exported at the crate root)
  pub struct FsConversationRepo;            // new(assistant_dir: PathBuf) -> Self, where assistant_dir = <workspace>/assistant
  impl ConversationRepository for FsConversationRepo;
  pub struct SharedPathConversationRepo;    // new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self
  impl ConversationRepository for SharedPathConversationRepo;   // pinned() -> Some(FsConversationRepo of the current workspace)
  ```

- [ ] **Step 1: 📖 Read the OpenCollection reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md` and spec section 4 (OC-1 to OC-7 above). The store lives in `<workspace>/assistant/`, a sibling of `collections/`, so no collection file is touched.

- [ ] **Step 2: Write the failing repository tests**

Create `crates/rocket-infra/src/fs_conversation_repo.rs` with only this content:

```rust
//! Assistant conversations of one workspace, one YAML file per conversation
//! at `<workspace>/assistant/conversations/<id>.yml`. A Rocket-private store
//! next to `collections/`, so no OpenCollection file is read or written.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_acp::conversation::{
    validate_conversation_id, ConversationRecord, ConversationRepository, ConversationSummary,
    CONVERSATION_FORMAT_VERSION, MAX_CONVERSATIONS,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::yaml_io::delete_if_exists;

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::{
        ConversationMode, StoredMessage, MAX_MESSAGES, MAX_MESSAGE_TEXT_BYTES, TEXT_CUT_MARKER,
    };
    use tempfile::TempDir;

    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }

    fn record(n: u32, updated_at: &str) -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: id(n),
            title: format!("chat {n}"),
            agent_config_id: "claude-code".into(),
            mode: ConversationMode::Ask,
            workspace_id: "default".into(),
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
            collections: Vec::new(),
            continued_from: None,
            messages: vec![
                StoredMessage::User { text: "hi".into() },
                StoredMessage::Agent { text: "hello".into() },
            ],
        }
    }

    fn setup() -> (TempDir, PathBuf, FsConversationRepo) {
        let dir = TempDir::new().expect("tempdir");
        let assistant = dir.path().join("assistant");
        let repo = FsConversationRepo::new(assistant.clone());
        (dir, assistant, repo)
    }

    fn file(assistant: &Path, n: u32) -> PathBuf {
        assistant.join("conversations").join(format!("{}.yml", id(n)))
    }

    fn ids(repo: &FsConversationRepo) -> Vec<String> {
        repo.list().expect("list").into_iter().map(|s| s.id).collect()
    }

    #[test]
    fn list_of_a_missing_directory_is_empty() {
        let (_dir, assistant, repo) = setup();
        assert!(repo.list().expect("list").is_empty());
        assert!(!assistant.exists(), "reading creates nothing");
    }

    #[test]
    fn save_get_list_round_trip() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save 1");
        repo.save(&record(2, "2026-10-11T10:00:00Z")).expect("save 2");
        assert_eq!(repo.get(&id(1)).expect("get"), record(1, "2026-10-11T09:00:00Z"));
        assert_eq!(ids(&repo), vec![id(2), id(1)], "newest first");
        assert!(file(&assistant, 1).is_file(), "the file name is the id");
    }

    #[test]
    fn the_file_uses_snake_case_keys_only() {
        let (_dir, assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.continued_from = Some(id(9));
        repo.save(&r).expect("save");
        let raw = fs::read_to_string(file(&assistant, 1)).expect("read");
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(&raw).expect("parse");
        let mut keys: Vec<String> = parsed
            .keys()
            .map(|k| k.as_str().expect("string key").to_string())
            .collect();
        keys.sort();
        let mut expected = vec![
            "version", "id", "title", "agent_config_id", "mode", "workspace_id", "created_at",
            "updated_at", "collections", "continued_from", "messages",
        ];
        expected.sort();
        assert_eq!(keys, expected, "a new persisted key must be reviewed: snake_case, no secrets");
    }

    #[test]
    fn save_writes_gitignore_with_star_once() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save 1");
        repo.save(&record(2, "2026-10-11T09:01:00Z")).expect("save 2");
        let content = fs::read_to_string(assistant.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "*");
    }

    #[test]
    fn save_leaves_a_user_edited_gitignore_untouched() {
        let (_dir, assistant, repo) = setup();
        fs::create_dir_all(&assistant).expect("mkdir");
        fs::write(assistant.join(".gitignore"), "custom\n").expect("write");
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        let content = fs::read_to_string(assistant.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "custom\n");
    }

    #[test]
    fn save_succeeds_when_gitignore_cannot_be_written() {
        let (_dir, assistant, repo) = setup();
        // A directory named .gitignore cannot be replaced by a file.
        fs::create_dir_all(assistant.join(".gitignore")).expect("mkdir");
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        assert!(repo.get(&id(1)).is_ok());
    }

    #[test]
    fn the_51st_save_evicts_the_oldest() {
        let (_dir, _assistant, repo) = setup();
        for n in 0..MAX_CONVERSATIONS as u32 {
            repo.save(&record(n + 1, &format!("2026-10-11T09:{n:02}:00Z")))
                .expect("save");
        }
        repo.save(&record(100, "2026-10-12T00:00:00Z")).expect("save 51st");
        let ids = ids(&repo);
        assert_eq!(ids.len(), MAX_CONVERSATIONS);
        assert!(!ids.contains(&id(1)), "the oldest is gone");
        assert!(ids.contains(&id(100)));
    }

    #[test]
    fn eviction_never_removes_the_conversation_just_saved() {
        let (_dir, _assistant, repo) = setup();
        for n in 0..MAX_CONVERSATIONS as u32 {
            repo.save(&record(n + 1, &format!("2026-10-12T09:{n:02}:00Z")))
                .expect("save");
        }
        // Older than every other one, for example a clock that went back.
        repo.save(&record(100, "2026-10-01T00:00:00Z")).expect("save");
        let ids = ids(&repo);
        assert_eq!(ids.len(), MAX_CONVERSATIONS);
        assert!(ids.contains(&id(100)));
        assert!(!ids.contains(&id(1)), "the next oldest went instead");
    }

    #[test]
    fn save_caps_messages_and_text() {
        let (_dir, _assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.messages = (0..MAX_MESSAGES + 1)
            .map(|i| StoredMessage::User { text: format!("m{i}") })
            .collect();
        r.messages.push(StoredMessage::Agent {
            text: "a".repeat(MAX_MESSAGE_TEXT_BYTES * 2),
        });
        repo.save(&r).expect("save");
        let loaded = repo.get(&id(1)).expect("get");
        assert_eq!(loaded.messages.len(), MAX_MESSAGES);
        let Some(StoredMessage::Agent { text }) = loaded.messages.last() else {
            panic!("the agent message stays last");
        };
        assert!(text.len() <= MAX_MESSAGE_TEXT_BYTES);
        assert!(text.ends_with(TEXT_CUT_MARKER));
    }

    #[test]
    fn list_skips_corrupt_and_other_version_files_and_get_reports_them() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        fs::write(file(&assistant, 2), "version: [not yaml").expect("write corrupt");
        let mut future = record(3, "2026-10-11T09:00:00Z");
        future.version = 2;
        fs::write(file(&assistant, 3), serde_yaml::to_string(&future).expect("yaml"))
            .expect("write v2");
        assert_eq!(ids(&repo), vec![id(1)]);
        assert!(matches!(repo.get(&id(2)), Err(DomainError::InvalidInput(_))));
        assert!(matches!(repo.get(&id(3)), Err(DomainError::InvalidInput(_))));
        assert!(matches!(repo.get(&id(4)), Err(DomainError::NotFound(_))));
    }

    #[test]
    fn a_file_whose_id_differs_from_its_name_is_skipped() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        fs::copy(file(&assistant, 1), file(&assistant, 5)).expect("copy");
        assert_eq!(ids(&repo), vec![id(1)]);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save");
        let raw = fs::read_to_string(file(&assistant, 1)).expect("read");
        fs::write(file(&assistant, 1), format!("{raw}future_key: 1\n")).expect("write");
        assert_eq!(repo.get(&id(1)).expect("get").title, "chat 1");
    }

    #[test]
    fn an_invalid_id_is_refused_without_writing() {
        let (_dir, assistant, repo) = setup();
        let mut r = record(1, "2026-10-11T09:00:00Z");
        r.id = "../escape".into();
        assert!(matches!(repo.save(&r), Err(DomainError::InvalidInput(_))));
        assert!(!assistant.exists(), "nothing was written");
        assert!(matches!(repo.get("../escape"), Err(DomainError::InvalidInput(_))));
        assert!(matches!(repo.delete("../escape"), Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn delete_and_clear() {
        let (_dir, assistant, repo) = setup();
        repo.save(&record(1, "2026-10-11T09:00:00Z")).expect("save 1");
        repo.save(&record(2, "2026-10-11T09:01:00Z")).expect("save 2");
        repo.delete(&id(1)).expect("delete");
        assert_eq!(ids(&repo), vec![id(2)]);
        assert!(matches!(repo.delete(&id(1)), Err(DomainError::NotFound(_))));
        fs::write(file(&assistant, 3), "version: [not yaml").expect("write corrupt");
        repo.clear().expect("clear");
        assert!(repo.list().expect("list").is_empty());
        assert!(!file(&assistant, 3).exists(), "clear removes unreadable files too");
        assert!(assistant.join(".gitignore").is_file(), "clear keeps the .gitignore");
    }
}
```

Create `crates/rocket-infra/src/shared_path_conversation_repo.rs`:

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_acp::conversation::{ConversationRecord, ConversationRepository, ConversationSummary};
use rocket_shared::error::DomainResult;

use crate::FsConversationRepo;

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::{ConversationMode, StoredMessage, CONVERSATION_FORMAT_VERSION};

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn record() -> ConversationRecord {
        ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: ID.into(),
            title: "t".into(),
            agent_config_id: "a".into(),
            mode: ConversationMode::Ask,
            workspace_id: "default".into(),
            created_at: "2026-10-11T09:00:00Z".into(),
            updated_at: "2026-10-11T09:00:00Z".into(),
            collections: Vec::new(),
            continued_from: None,
            messages: vec![StoredMessage::Agent { text: "hello".into() }],
        }
    }

    #[test]
    fn calls_follow_the_active_workspace_and_pinned_stays() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(first.path().to_path_buf()));
        let repo = SharedPathConversationRepo::new(Arc::clone(&path));
        let pinned = repo.pinned().expect("a shared repo can pin");

        *path.lock().expect("lock") = second.path().to_path_buf();
        pinned.save(&record()).expect("pinned save");
        let in_first = first.path().join("assistant").join("conversations").join(format!("{ID}.yml"));
        assert!(in_first.is_file(), "the pinned repo writes into the first workspace");
        assert!(repo.list().expect("list").is_empty(), "the live repo reads the second one");

        repo.save(&record()).expect("live save");
        let in_second = second.path().join("assistant").join("conversations").join(format!("{ID}.yml"));
        assert!(in_second.is_file());
    }
}
```

Register both modules in `crates/rocket-infra/src/lib.rs` (`pub mod fs_conversation_repo;` after `pub mod fs_contract_repo;`, `pub mod shared_path_conversation_repo;` after `pub mod shared_path_collection_repo;`, plus `pub use fs_conversation_repo::FsConversationRepo;` and `pub use shared_path_conversation_repo::SharedPathConversationRepo;`).

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p rocket-infra -j4 conversation`
Expected: compile errors `cannot find struct ... 'FsConversationRepo'` and `'SharedPathConversationRepo'`.

- [ ] **Step 4: Implement `FsConversationRepo`**

Insert between the `use` lines and `#[cfg(test)]` in `fs_conversation_repo.rs`:

```rust
pub struct FsConversationRepo {
    assistant_dir: PathBuf,
}

impl FsConversationRepo {
    /// `assistant_dir` is `<workspace>/assistant`. Nothing is created until a save.
    pub fn new(assistant_dir: PathBuf) -> Self {
        Self { assistant_dir }
    }

    fn conversations_dir(&self) -> PathBuf {
        self.assistant_dir.join("conversations")
    }

    /// The file of a valid id. Any other id is refused, so no path can escape.
    fn file_path(&self, id: &str) -> DomainResult<PathBuf> {
        validate_conversation_id(id)?;
        Ok(self.conversations_dir().join(format!("{id}.yml")))
    }

    /// Keeps transcripts out of git when the workspace is a repository.
    /// Creates `<workspace>/assistant/.gitignore` only if it is missing and
    /// never fails the save, like `FsCookieRepo::ensure_gitignore`.
    fn ensure_gitignore(&self) {
        let result = fs::create_dir_all(&self.assistant_dir).and_then(|()| {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.assistant_dir.join(".gitignore"))
            {
                Ok(mut file) => file.write_all(b"*"),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(e) => Err(e),
            }
        });
        if let Err(e) = result {
            tracing::warn!("Could not write the assistant folder .gitignore: {e}");
        }
    }

    /// Reads one file. The error text never quotes file content.
    fn parse(path: &Path) -> DomainResult<ConversationRecord> {
        let content = fs::read_to_string(path)?;
        serde_yaml::from_str(&content).map_err(|_| {
            DomainError::InvalidInput("the conversation file could not be read".to_string())
        })
    }

    /// Every readable file of this version whose `id` matches its name.
    /// Others are skipped with a warning.
    fn read_all(&self) -> DomainResult<Vec<(PathBuf, ConversationRecord)>> {
        let dir = self.conversations_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if !path.extension().is_some_and(|e| e == "yml") {
                continue;
            }
            match Self::parse(&path) {
                Ok(record)
                    if record.version == CONVERSATION_FORMAT_VERSION
                        && path.file_stem().and_then(|s| s.to_str()) == Some(record.id.as_str()) =>
                {
                    out.push((path, record));
                }
                Ok(_) => {
                    tracing::warn!(path = %path.display(), "skipping a conversation file of another version or id");
                }
                Err(_) => {
                    tracing::warn!(path = %path.display(), "skipping a conversation file that could not be read");
                }
            }
        }
        Ok(out)
    }

    /// Removes the oldest conversations beyond `MAX_CONVERSATIONS`, never `keep_id`.
    fn evict_beyond_cap(&self, keep_id: &str) -> DomainResult<()> {
        let mut all = self.read_all()?;
        if all.len() <= MAX_CONVERSATIONS {
            return Ok(());
        }
        all.sort_by(|a, b| {
            a.1.updated_at
                .cmp(&b.1.updated_at)
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        let excess = all.len() - MAX_CONVERSATIONS;
        for (path, _) in all.into_iter().filter(|(_, r)| r.id != keep_id).take(excess) {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

impl ConversationRepository for FsConversationRepo {
    fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        let mut summaries: Vec<ConversationSummary> =
            self.read_all()?.iter().map(|(_, r)| r.summary()).collect();
        summaries.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(summaries)
    }

    fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        let path = self.file_path(id)?;
        if !path.exists() {
            return Err(DomainError::NotFound(format!("conversation '{id}'")));
        }
        let record = Self::parse(&path)?;
        if record.version != CONVERSATION_FORMAT_VERSION {
            return Err(DomainError::InvalidInput(
                "this conversation was saved by another version of Rocket".to_string(),
            ));
        }
        Ok(record)
    }

    fn save(&self, record: &ConversationRecord) -> DomainResult<()> {
        let path = self.file_path(&record.id)?;
        let mut capped = record.clone();
        capped.version = CONVERSATION_FORMAT_VERSION;
        capped.apply_caps();
        let yaml = serde_yaml::to_string(&capped).map_err(|e| {
            DomainError::Internal(format!("Failed to serialize the conversation: {e}"))
        })?;
        self.ensure_gitignore();
        atomic_write(&path, yaml.as_bytes())?;
        // The conversation is saved. A failed eviction only leaves one file too many.
        if let Err(e) = self.evict_beyond_cap(&record.id) {
            tracing::warn!("Could not remove old conversations: {e}");
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let path = self.file_path(id)?;
        delete_if_exists(&path, &format!("conversation '{id}'"))
    }

    fn clear(&self) -> DomainResult<()> {
        let dir = self.conversations_dir();
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "yml") {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Implement `SharedPathConversationRepo`**

Insert between the `use` lines and `#[cfg(test)]` in `shared_path_conversation_repo.rs`:

```rust
/// The assistant conversations of `<active workspace>/assistant`.
///
/// Every call builds a short-lived `FsConversationRepo` from the shared active
/// workspace path, so a workspace switch takes effect on the next call.
/// `pinned()` fixes one workspace, which `ConversationService` does when a
/// session starts, so a save after a switch still lands where the chat began.
pub struct SharedPathConversationRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathConversationRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    /// The repository for the workspace that is active right now.
    fn current(&self) -> FsConversationRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("assistant");
        FsConversationRepo::new(dir)
    }
}

impl ConversationRepository for SharedPathConversationRepo {
    fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        self.current().list()
    }

    fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        self.current().get(id)
    }

    fn save(&self, record: &ConversationRecord) -> DomainResult<()> {
        self.current().save(record)
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        self.current().delete(id)
    }

    fn clear(&self) -> DomainResult<()> {
        self.current().clear()
    }

    fn pinned(&self) -> Option<Box<dyn ConversationRepository>> {
        Some(Box::new(self.current()))
    }
}
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p rocket-infra -j4 conversation`
Expected: `test result: ok. 15 passed` (14 in `fs_conversation_repo`, 1 in `shared_path_conversation_repo`).

- [ ] **Step 7: Update the crate guide**

In `crates/rocket-infra/CLAUDE.md`, Public types table, after the `SharedPathEnvironmentRepo, SharedPathCookieRepo, SharedPathTemplateRepo` row, add:

```markdown
| `FsConversationRepo`, `SharedPathConversationRepo` | `rocket_acp::ConversationRepository` | Stored assistant conversations at `<ws>/assistant/conversations/<id>.yml` (snake_case YAML, `atomic_write`, `<ws>/assistant/.gitignore` = `*` via `create_new`, caps applied on save, oldest evicted beyond 50, unreadable or other-version files skipped with a warning). The shared one resolves the active workspace per call; `pinned()` fixes it for a session. See `.claude/assistant-conversations.md`. |
```

- [ ] **Step 8: Check and commit**

Run: `cargo check --workspace --all-targets -j4`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add crates/rocket-infra/src/fs_conversation_repo.rs crates/rocket-infra/src/shared_path_conversation_repo.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/CLAUDE.md
git commit --only -m "feat(infra): store assistant conversations per workspace" -- crates/rocket-infra/src/fs_conversation_repo.rs crates/rocket-infra/src/shared_path_conversation_repo.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/CLAUDE.md
```

---

### Task 3: `ConversationService`, IPC commands and wiring

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/conversation_secrets.rs`
- Create: `crates/rocket-app/src/conversation_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (`pub mod` near `collection_trust` `:11`; `pub(crate) mod conversation_secrets;`; `pub use` near `:70`)
- Modify: `crates/rocket-app/CLAUDE.md` (Public Types table, after the `ProposalService` row `:25`)
- Create: `src-tauri/src/commands/assistant_conversations.rs`
- Modify: `src-tauri/src/commands/mod.rs` (`pub mod assistant_conversations;` after `assistant_chips` `:6`)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (imports `:4-7`; command `:111-132`; inner `:144-153` and after `:191-195`)
- Modify: `src-tauri/src/lib.rs` (build after `mcp_tool_svc` `:646-662`; `app.manage` after `:794`; handlers after `:1054`)
- Modify: `src-tauri/tests/acp_mcp_start_workspace_assistant.rs` (imports `:19-33`, `Fixture` `:243-250`, `build_fixture` `:256-305`, `start` `:330-342`, one new test)
- Create: `.claude/assistant-conversations.md`
- Modify: `CLAUDE.md` (one "See" line in "Rules and conventions")
- Test: `crates/rocket-app/src/conversation_service.rs` (`mod tests`), `src-tauri/src/commands/assistant_conversations.rs` (`mod tests`), `src-tauri/tests/acp_mcp_start_workspace_assistant.rs`

**Interfaces:**
- Consumes: Task 1 types and events; Task 2 `SharedPathConversationRepo`; `crate::mcp_read_views::mask_secret_text`; `crate::runner_sequence::folder_dir_name`; `CollectionRepository::{list, get, get_settings, get_folder_settings, collection_identity, pinned}`; `EnvironmentRepositoryFactory::{for_collection, pinned}`; `EnvironmentRepository::{list, pinned}`.
- Produces (exact):
  ```rust
  // rocket_app (re-exported at the crate root)
  pub const MAX_SESSION_PINS: usize = 16;
  pub struct ConversationInput { pub id: String, pub agent_config_id: String,
      pub mode: ConversationMode, pub workspace_id: String,
      pub collections: Vec<String>,            // collection names; stored by uid
      pub messages: Vec<StoredMessage>, pub continued_from: Option<String> }
  impl ConversationService {
      pub fn new(conversations: Arc<dyn ConversationRepository>,
          collections: Arc<dyn CollectionRepository>,
          collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
          global_envs: Arc<dyn EnvironmentRepository>,
          events: Arc<dyn EventPublisher>) -> Self;
      pub fn with_clock(self, now: Arc<dyn Fn() -> String + Send + Sync>) -> Self;
      pub fn pin_session(&self, session_id: &str);
      pub fn list(&self) -> DomainResult<Vec<ConversationSummary>>;
      pub fn get(&self, id: &str) -> DomainResult<ConversationRecord>;
      pub fn save(&self, session_id: &str, input: ConversationInput) -> DomainResult<ConversationSummary>;
      pub fn delete(&self, id: &str) -> DomainResult<()>;
      pub fn clear(&self) -> DomainResult<()>;
  }
  // rocket_app::conversation_secrets (crate-private)
  pub(crate) fn workspace_secret_values(collections: &dyn CollectionRepository,
      collection_envs: &dyn EnvironmentRepositoryFactory,
      global_envs: &dyn EnvironmentRepository) -> DomainResult<HashSet<String>>;

  // src-tauri/src/commands/assistant_conversations.rs (camelCase DTOs)
  pub struct ConversationSummaryDto { id, title, agentConfigId, mode, createdAt, updatedAt, messageCount, continuedFrom? }
  pub struct ConversationCollectionDto { uid, name }
  pub struct ConversationRecordDto { id, title, agentConfigId, mode, workspaceId, createdAt, updatedAt,
      collections: Vec<ConversationCollectionDto>, messages: Vec<StoredMessage>, continuedFrom? }
  pub struct ConversationInputDto { id, agentConfigId, mode, workspaceId, collections: Vec<String>,
      messages: Vec<StoredMessage>, continuedFrom? }
  list_conversations() -> Vec<ConversationSummaryDto>
  get_conversation(id) -> ConversationRecordDto
  save_conversation(session_id, conversation: ConversationInputDto) -> ConversationSummaryDto
  delete_conversation(id) -> ()
  clear_conversations() -> ()

  // start_workspace_assistant_inner gains `conversations: &ConversationService` after `resources`
  // and calls `conversations.pin_session(&info.session_id)` right after `begin_assistant_session`.
  ```

**How the save picks its workspace (spec section 9):** `start_workspace_assistant_inner` calls `ConversationService::pin_session(&info.session_id)` right after `begin_assistant_session`, so both read the same active path. `pin_session` stores `pinned()` copies of the conversation, collection, collection-environment and global-environment repositories (falling back to the shared repository when `pinned()` is `None`) under the ACP session id. `save_conversation` takes that session id, looks up the pin, collects the secret values of the pinned workspace and writes through the pinned conversation repository. The pin is not dropped at session end, because the final save runs after `SessionCleanup`; at most `MAX_SESSION_PINS` (16) pins are kept and the oldest is dropped first. A save from an unknown session id is refused. `list`, `get`, `delete` and `clear` use the live active workspace.

- [ ] **Step 1: 📖 Read the OpenCollection reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`, sections on variables (collection, folder, request, environment, `secret`) and spec section 8. Secret variable values are read only to mask; nothing is written to a collection or environment file.

- [ ] **Step 2: Write the failing service tests**

Create `crates/rocket-app/src/conversation_service.rs` with only this content:

```rust
//! Stores the workspace assistant's conversations (spec
//! `2026-10-11-assistant-conversation-history-design.md`). Masks secret
//! variable values of the session's workspace, applies the caps, writes into
//! the workspace that was active when the session started and publishes
//! id-only events. No I/O of its own.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use rocket_acp::conversation::{
    conversation_title, has_agent_reply, validate_conversation_id, ConversationCollection,
    ConversationMode, ConversationRecord, ConversationRepository, ConversationSummary,
    StoredMessage, CONVERSATION_FORMAT_VERSION,
};
use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::conversation_secrets::workspace_secret_values;
use crate::mcp_read_views::mask_secret_text;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    use rocket_acp::conversation::{
        StoredProposalStatus, StoredToolStatus, MAX_MESSAGE_TEXT_BYTES, TEXT_CUT_MARKER,
    };
    use rocket_collection::{
        Collection, CollectionItem, CollectionSettings, CollectionVariable, Folder,
        FolderSettings, Request,
    };
    use rocket_environment::{Environment, Variable};
    use rocket_shared::types::HttpMethod;

    use crate::redaction::REDACTED;
    use crate::test_doubles::{ConfigurableCollectionRepo, RecordingPublisher};

    const COLLECTION_SECRET: &str = "col-secret-111111";
    const INITIAL_SECRET: &str = "col-initial-222222";
    const FOLDER_SECRET: &str = "folder-secret-333333";
    const REQUEST_SECRET: &str = "request-secret-444444";
    const ENV_SECRET: &str = "env-secret-555555";
    const GLOBAL_SECRET: &str = "global-secret-666666";
    const PLAIN_VALUE: &str = "plain-value-777777";
    const ALL_SECRETS: [&str; 6] = [
        COLLECTION_SECRET, INITIAL_SECRET, FOLDER_SECRET, REQUEST_SECRET, ENV_SECRET, GLOBAL_SECRET,
    ];
    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn var(key: &str, value: &str, initial: &str, secret: bool) -> CollectionVariable {
        CollectionVariable {
            key: key.into(),
            value: value.into(),
            initial_value: initial.into(),
            enabled: true,
            secret,
        }
    }

    struct MapEnvRepo(Vec<Environment>);

    impl EnvironmentRepository for MapEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .iter()
                .find(|env| env.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct MapEnvFactory(Vec<Environment>);

    impl EnvironmentRepositoryFactory for MapEnvFactory {
        fn for_collection(&self, _: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MapEnvRepo(self.0.clone()))
        }
    }

    /// A "demo" collection with a secret in the collection, a folder and a
    /// request, a plain variable, and a folder with no folder.yml.
    fn collections() -> Arc<ConfigurableCollectionRepo> {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings(
            "demo",
            CollectionSettings {
                variables: vec![
                    var("token", COLLECTION_SECRET, "", true),
                    var("seed", "", INITIAL_SECRET, true),
                    var("plain", PLAIN_VALUE, "", false),
                ],
                ..Default::default()
            },
        );
        let mut request = Request::new("login", HttpMethod::Get, "https://api.test/login");
        request.variables = vec![var("pw", REQUEST_SECRET, "", true)];
        let mut auth = Folder::new("auth");
        auth.items.push(CollectionItem::Request(Box::new(request)));
        let mut tree = Collection::new("demo");
        tree.root.items.push(CollectionItem::Folder(auth));
        tree.root.items.push(CollectionItem::Folder(Folder::new("empty")));
        repo.with_summaries("demo", tree);
        repo.with_folder_settings(
            "demo",
            "auth",
            FolderSettings {
                variables: vec![var("fk", FOLDER_SECRET, "", true)],
                ..Default::default()
            },
        );
        repo
    }

    struct Fixture {
        _ws_a: tempfile::TempDir,
        ws_b: tempfile::TempDir,
        ws_a_path: PathBuf,
        active: Arc<StdMutex<PathBuf>>,
        events: Arc<RecordingPublisher>,
        svc: ConversationService,
    }

    fn fixture_with(collections: Arc<ConfigurableCollectionRepo>) -> Fixture {
        let ws_a = tempfile::tempdir().expect("tempdir");
        let ws_b = tempfile::tempdir().expect("tempdir");
        let active = Arc::new(StdMutex::new(ws_a.path().to_path_buf()));
        let events = RecordingPublisher::new();
        let mut global = Environment::new("globals");
        global.variables.push(Variable::secret("g", GLOBAL_SECRET));
        let mut env = Environment::new("dev");
        env.variables.push(Variable::secret("e", ENV_SECRET));
        let svc = ConversationService::new(
            Arc::new(rocket_infra::SharedPathConversationRepo::new(Arc::clone(&active))),
            collections,
            Arc::new(MapEnvFactory(vec![env])),
            Arc::new(MapEnvRepo(vec![global])),
            events.clone(),
        )
        .with_clock(Arc::new(|| "2026-10-11T09:20:41Z".to_string()));
        Fixture {
            ws_a_path: ws_a.path().to_path_buf(),
            _ws_a: ws_a,
            ws_b,
            active,
            events,
            svc,
        }
    }

    fn user(text: &str) -> StoredMessage {
        StoredMessage::User { text: text.into() }
    }

    fn agent(text: &str) -> StoredMessage {
        StoredMessage::Agent { text: text.into() }
    }

    fn input(messages: Vec<StoredMessage>) -> ConversationInput {
        ConversationInput {
            id: ID.into(),
            agent_config_id: "claude-code".into(),
            mode: ConversationMode::Edit,
            workspace_id: "default".into(),
            collections: vec!["demo".into()],
            messages,
            continued_from: None,
        }
    }

    fn stored_file(fx: &Fixture) -> PathBuf {
        fx.ws_a_path.join("assistant").join("conversations").join(format!("{ID}.yml"))
    }

    #[test]
    fn save_masks_secret_values_of_every_scope_in_every_text_field() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        let all = ALL_SECRETS.join(" ");
        fx.svc
            .save(
                "s1",
                input(vec![
                    user(&format!("use {all}")),
                    agent(&format!("ok {ENV_SECRET} and {PLAIN_VALUE}")),
                    StoredMessage::Tool {
                        title: format!("get_request {REQUEST_SECRET}"),
                        status: StoredToolStatus::Completed,
                    },
                    StoredMessage::Notice { text: GLOBAL_SECRET.into() },
                    StoredMessage::Proposal {
                        summary: format!("Set token to {COLLECTION_SECRET}"),
                        status: StoredProposalStatus::Pending,
                    },
                ]),
            )
            .expect("save");
        let raw = std::fs::read_to_string(stored_file(&fx)).expect("read file");
        for secret in ALL_SECRETS {
            assert!(!raw.contains(secret), "{secret} reached disk");
        }
        let record = fx.svc.get(ID).expect("get");
        assert!(record.title.contains(REDACTED), "the title is built from masked text");
        let StoredMessage::Agent { text } = &record.messages[1] else {
            panic!("agent second");
        };
        assert!(text.contains(REDACTED));
        assert!(text.contains(PLAIN_VALUE), "plain variables are not masked");
    }

    #[test]
    fn masking_runs_before_the_text_cap() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        // The secret starts just before the cut, so capping first would keep "col-".
        let cut_at = MAX_MESSAGE_TEXT_BYTES - TEXT_CUT_MARKER.len();
        let text = format!("{}{COLLECTION_SECRET}{}", "a".repeat(cut_at - 4), "b".repeat(5_000));
        fx.svc.save("s1", input(vec![user("q"), agent(&text)])).expect("save");
        let record = fx.svc.get(ID).expect("get");
        let StoredMessage::Agent { text } = &record.messages[1] else {
            panic!("agent second");
        };
        assert!(text.ends_with(TEXT_CUT_MARKER));
        assert!(!text.contains("col-"), "no fragment of the secret is left");
    }

    #[test]
    fn save_after_a_workspace_switch_writes_into_the_pinned_workspace() {
        let fx = fixture_with(collections());
        fx.svc.pin_session("s1");
        *fx.active.lock().expect("lock") = fx.ws_b.path().to_path_buf();
        fx.svc.save("s1", input(vec![user("q"), agent("a")])).expect("save");
        assert!(stored_file(&fx).is_file());
        assert!(!fx.ws_b.path().join("assistant").exists());
        assert!(fx.svc.list().expect("list").is_empty(), "list follows the live workspace");
    }

    #[test]
    fn a_save_needs_a_known_session_and_an_agent_reply() {
        let fx = fixture_with(collections());
        let unknown = fx.svc.save("unknown", input(vec![user("q"), agent("a")]));
        assert!(matches!(unknown, Err(DomainError::InvalidInput(_))));
        fx.svc.pin_session("s1");
        let no_reply = fx.svc.save("s1", input(vec![user("q")]));
        assert!(matches!(no_reply, Err(DomainError::InvalidInput(_))));
        let mut bad_id = input(vec![user("q"), agent("a")]);
        bad_id.id = "../x".into();
        assert!(matches!(fx.svc.save("s1", bad_id), Err(DomainError::InvalidInput(_))));
        let mut bad_from = input(vec![user("q"), agent("a")]);
        bad_from.continued_from = Some("../y".into());
        assert!(matches!(fx.svc.save("s1", bad_from), Err(DomainError::InvalidInput(_))));
        assert!(!fx.ws_a_path.join("assistant").exists(), "nothing was written");
        assert!(fx.events.events().is_empty());
    }

    #[test]
    fn an_unreadable_variable_scope_refuses_the_save() {
        // "broken" is listed (it has settings) but its tree cannot be read.
        let repo = collections();
        repo.set_settings("broken", CollectionSettings::default());
        let fx = fixture_with(repo);
        fx.svc.pin_session("s1");
        let err = fx
            .svc
            .save("s1", input(vec![user("q"), agent("a")]))
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!fx.ws_a_path.join("assistant").exists());
    }

    #[test]
    fn a_resave_keeps_created_at_and_events_carry_ids_only() {
        let fx = fixture_with(collections());
        let tick = Arc::new(AtomicUsize::new(0));
        let svc = fx.svc.with_clock(Arc::new(move || {
            format!("2026-10-11T09:0{}:00Z", tick.fetch_add(1, Ordering::SeqCst))
        }));
        svc.pin_session("s1");
        svc.save("s1", input(vec![user("q"), agent("a")])).expect("first save");
        let summary = svc
            .save("s1", input(vec![user("q"), agent("a"), user("q2"), agent("a2")]))
            .expect("second save");
        assert_eq!(summary.created_at, "2026-10-11T09:00:00Z");
        assert_eq!(summary.updated_at, "2026-10-11T09:01:00Z");
        assert_eq!(summary.message_count, 4);
        assert_eq!(summary.mode, ConversationMode::Edit);
        svc.delete(ID).expect("delete");
        svc.clear().expect("clear");
        let events: Vec<String> = fx
            .events
            .events()
            .iter()
            .map(|e| serde_json::to_string(e).expect("json"))
            .collect();
        let saved = format!(r#"{{"type":"assistantConversationSaved","conversation_id":"{ID}"}}"#);
        assert_eq!(
            events,
            vec![
                saved.clone(),
                saved,
                format!(r#"{{"type":"assistantConversationDeleted","conversation_id":"{ID}"}}"#),
                r#"{"type":"assistantConversationsCleared"}"#.to_string(),
            ]
        );
    }

    #[test]
    fn only_the_newest_sixteen_sessions_stay_pinned() {
        let fx = fixture_with(collections());
        for n in 0..=MAX_SESSION_PINS {
            fx.svc.pin_session(&format!("s{n}"));
        }
        let oldest = fx.svc.save("s0", input(vec![user("q"), agent("a")]));
        assert!(matches!(oldest, Err(DomainError::InvalidInput(_))));
        fx.svc
            .save(&format!("s{MAX_SESSION_PINS}"), input(vec![user("q"), agent("a")]))
            .expect("the newest pin saves");
    }
}
```

Add to `crates/rocket-app/src/lib.rs`: `pub(crate) mod conversation_secrets;` and `pub mod conversation_service;` after `pub mod collection_trust;` (`:11`), and `pub use conversation_service::{ConversationInput, ConversationService, MAX_SESSION_PINS};` after `pub use collection_trust::{...};` (`:71`).

Create `crates/rocket-app/src/conversation_secrets.rs` with only:

```rust
//! Secret variable values of a whole workspace, for masking stored
//! conversations. Vault values are not fetched here (spec section 8).

use std::collections::HashSet;

use rocket_collection::{CollectionItem, CollectionRepository, CollectionVariable, Folder};
use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_shared::error::{DomainError, DomainResult};

use crate::runner_sequence::folder_dir_name;
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p rocket-app -j4 conversation_service::`
Expected: compile errors `cannot find struct 'ConversationService'`, `cannot find function 'workspace_secret_values'`.

- [ ] **Step 4: Implement the secret collector**

Append to `crates/rocket-app/src/conversation_secrets.rs`:

```rust
/// Every secret variable value of the workspace: global environments, and
/// for each collection its variables (value and initial value), its
/// environments, and the variables of every folder and request. A read
/// error is returned, because a skipped scope would leave its secrets
/// unmasked. A folder without `folder.yml` (`NotFound`) has no variables.
pub(crate) fn workspace_secret_values(
    collections: &dyn CollectionRepository,
    collection_envs: &dyn EnvironmentRepositoryFactory,
    global_envs: &dyn EnvironmentRepository,
) -> DomainResult<HashSet<String>> {
    let mut out = HashSet::new();
    add_env_secrets(&mut out, &global_envs.list()?);
    for summary in collections.list()? {
        let name = summary.name;
        add_variable_secrets(&mut out, &collections.get_settings(&name)?.variables);
        add_env_secrets(&mut out, &collection_envs.for_collection(&name).list()?);
        let tree = collections.get(&name)?;
        walk_folder(collections, &name, &tree.root, "", &mut out)?;
    }
    Ok(out)
}

fn add_variable_secrets(out: &mut HashSet<String>, vars: &[CollectionVariable]) {
    for var in vars.iter().filter(|v| v.secret) {
        for value in [&var.value, &var.initial_value] {
            if !value.is_empty() {
                out.insert(value.clone());
            }
        }
    }
}

fn add_env_secrets(out: &mut HashSet<String>, envs: &[Environment]) {
    for var in envs
        .iter()
        .flat_map(|env| env.variables.iter())
        .filter(|v| v.secret && !v.value.is_empty())
    {
        out.insert(var.value.clone());
    }
}

fn walk_folder(
    collections: &dyn CollectionRepository,
    collection: &str,
    folder: &Folder,
    path: &str,
    out: &mut HashSet<String>,
) -> DomainResult<()> {
    for item in &folder.items {
        match item {
            CollectionItem::Request(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::GraphQl(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::WebSocket(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::Grpc(r) => add_variable_secrets(out, &r.variables),
            CollectionItem::Folder(child) => {
                let child_path = if path.is_empty() {
                    folder_dir_name(child).to_string()
                } else {
                    format!("{path}/{}", folder_dir_name(child))
                };
                match collections.get_folder_settings(collection, &child_path) {
                    Ok(settings) => add_variable_secrets(out, &settings.variables),
                    Err(DomainError::NotFound(_)) => {}
                    Err(e) => return Err(e),
                }
                walk_folder(collections, collection, child, &child_path, out)?;
            }
            CollectionItem::OpaqueItem(_)
            | CollectionItem::Summary(_)
            | CollectionItem::ScriptFile(_) => {}
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `ConversationService`**

Insert between the `use` lines and `#[cfg(test)]` in `conversation_service.rs`:

```rust
/// Sessions whose workspace pin is kept. The oldest pin is dropped first.
pub const MAX_SESSION_PINS: usize = 16;

/// What the panel sends to save. `collections` are names; the record keeps
/// their `opencollection.yml` uid.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationInput {
    pub id: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub workspace_id: String,
    pub collections: Vec<String>,
    pub messages: Vec<StoredMessage>,
    pub continued_from: Option<String>,
}

/// The repositories of the workspace a session started in.
struct WorkspacePin {
    conversations: Arc<dyn ConversationRepository>,
    collections: Arc<dyn CollectionRepository>,
    collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
    global_envs: Arc<dyn EnvironmentRepository>,
}

type Clock = Arc<dyn Fn() -> String + Send + Sync>;

pub struct ConversationService {
    conversations: Arc<dyn ConversationRepository>,
    collections: Arc<dyn CollectionRepository>,
    collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
    global_envs: Arc<dyn EnvironmentRepository>,
    events: Arc<dyn EventPublisher>,
    now: Clock,
    /// Workspace pins by ACP session id, oldest first.
    pins: Mutex<VecDeque<(String, Arc<WorkspacePin>)>>,
}

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The `pinned()` repository, or the shared one when it has a fixed location.
fn pin_of<T: ?Sized>(shared: &Arc<T>, pinned: Option<Box<T>>) -> Arc<T> {
    pinned.map(Arc::from).unwrap_or_else(|| Arc::clone(shared))
}

impl ConversationService {
    pub fn new(
        conversations: Arc<dyn ConversationRepository>,
        collections: Arc<dyn CollectionRepository>,
        collection_envs: Arc<dyn EnvironmentRepositoryFactory>,
        global_envs: Arc<dyn EnvironmentRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            conversations,
            collections,
            collection_envs,
            global_envs,
            events,
            now: Arc::new(utc_now),
            pins: Mutex::new(VecDeque::new()),
        }
    }

    /// Replaces the clock, which returns an RFC 3339 UTC time. For tests.
    pub fn with_clock(mut self, now: Clock) -> Self {
        self.now = now;
        self
    }

    /// Fixes the active workspace for a session's saves. Called by
    /// `start_workspace_assistant` right after the handshake.
    pub fn pin_session(&self, session_id: &str) {
        let pin = Arc::new(WorkspacePin {
            conversations: pin_of(&self.conversations, self.conversations.pinned()),
            collections: pin_of(&self.collections, self.collections.pinned()),
            collection_envs: pin_of(&self.collection_envs, self.collection_envs.pinned()),
            global_envs: pin_of(&self.global_envs, self.global_envs.pinned()),
        });
        let mut pins = self.pins.lock().unwrap_or_else(|e| e.into_inner());
        pins.retain(|(id, _)| id != session_id);
        pins.push_back((session_id.to_string(), pin));
        while pins.len() > MAX_SESSION_PINS {
            pins.pop_front();
        }
    }

    fn pin(&self, session_id: &str) -> DomainResult<Arc<WorkspacePin>> {
        self.pins
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, pin)| Arc::clone(pin))
            .ok_or_else(|| {
                DomainError::InvalidInput(
                    "This assistant session is not known, so the conversation was not saved."
                        .to_string(),
                )
            })
    }

    /// The active workspace's conversations, newest first.
    pub fn list(&self) -> DomainResult<Vec<ConversationSummary>> {
        self.conversations.list()
    }

    pub fn get(&self, id: &str) -> DomainResult<ConversationRecord> {
        validate_conversation_id(id)?;
        self.conversations.get(id)
    }

    /// Masks, caps and writes one conversation into the session's pinned
    /// workspace, then publishes `AssistantConversationSaved`.
    pub fn save(
        &self,
        session_id: &str,
        input: ConversationInput,
    ) -> DomainResult<ConversationSummary> {
        validate_conversation_id(&input.id)?;
        if let Some(from) = &input.continued_from {
            validate_conversation_id(from)?;
        }
        if input.agent_config_id.trim().is_empty() {
            return Err(DomainError::InvalidInput("the agent config id is missing".to_string()));
        }
        if !has_agent_reply(&input.messages) {
            return Err(DomainError::InvalidInput(
                "a conversation is saved only after the assistant replied".to_string(),
            ));
        }
        let pin = self.pin(session_id)?;
        let secrets = workspace_secret_values(
            pin.collections.as_ref(),
            pin.collection_envs.as_ref(),
            pin.global_envs.as_ref(),
        )
        .map_err(|_| {
            DomainError::InvalidInput(
                "could not read the workspace variables to mask this conversation, so it was not saved"
                    .to_string(),
            )
        })?;
        let messages: Vec<StoredMessage> = input
            .messages
            .into_iter()
            .map(|message| message.map_text(|text| mask_secret_text(text, &secrets)))
            .collect();
        let title = conversation_title(&messages);
        let now = (self.now)();
        let created_at = match pin.conversations.get(&input.id) {
            Ok(existing) if !existing.created_at.is_empty() => existing.created_at,
            _ => now.clone(),
        };
        let mut record = ConversationRecord {
            version: CONVERSATION_FORMAT_VERSION,
            id: input.id,
            title,
            agent_config_id: input.agent_config_id,
            mode: input.mode,
            workspace_id: input.workspace_id,
            created_at,
            updated_at: now,
            collections: collection_refs(pin.collections.as_ref(), &input.collections),
            continued_from: input.continued_from,
            messages,
        };
        // Masking ran first, so a cut can never keep part of a secret.
        record.apply_caps();
        pin.conversations.save(&record)?;
        self.events.publish(DomainEvent::AssistantConversationSaved {
            conversation_id: record.id.clone(),
        });
        Ok(record.summary())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        validate_conversation_id(id)?;
        self.conversations.delete(id)?;
        self.events.publish(DomainEvent::AssistantConversationDeleted {
            conversation_id: id.to_string(),
        });
        Ok(())
    }

    pub fn clear(&self) -> DomainResult<()> {
        self.conversations.clear()?;
        self.events.publish(DomainEvent::AssistantConversationsCleared);
        Ok(())
    }
}

/// Collections by their `opencollection.yml` uid. A name without a uid is left out.
fn collection_refs(
    collections: &dyn CollectionRepository,
    names: &[String],
) -> Vec<ConversationCollection> {
    let mut seen = HashSet::new();
    names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .filter_map(|name| {
            let uid = collections.collection_identity(name).ok()?.uid?;
            Some(ConversationCollection {
                uid,
                name: name.clone(),
            })
        })
        .collect()
}
```

If `pin_of` does not infer for trait objects, write the four fields out as `self.collections.pinned().map(Arc::from).unwrap_or_else(|| Arc::clone(&self.collections))` (the `send_pins.rs:77-83` form).

- [ ] **Step 6: Run the service tests to see them pass**

Run: `cargo test -p rocket-app -j4 conversation_service::`
Expected: `test result: ok. 7 passed`.

- [ ] **Step 7: Write the failing command tests**

Create `src-tauri/src/commands/assistant_conversations.rs`:

```rust
//! IPC commands for stored assistant conversations. Thin: each calls
//! `ConversationService` off the main thread and maps to camelCase DTOs.
//! `StoredMessage` is passed through as is: its tag values and field names
//! are single lowercase words and its statuses snake_case, the same on disk
//! and on the wire (pinned by a test below).

use std::sync::Arc;

use rocket_acp::conversation::{
    ConversationMode, ConversationRecord, ConversationSummary, StoredMessage,
};
use rocket_app::{ConversationInput, ConversationService};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::conversation::StoredToolStatus;
    use rocket_shared::events::NullEventPublisher;

    const ID: &str = "0b6f3f4e-0000-4000-8000-000000000001";

    fn service() -> (tempfile::TempDir, Arc<ConversationService>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = Arc::new(std::sync::Mutex::new(dir.path().to_path_buf()));
        let svc = ConversationService::new(
            Arc::new(rocket_infra::SharedPathConversationRepo::new(Arc::clone(&ws))),
            Arc::new(rocket_infra::SharedPathCollectionRepo::new(Arc::clone(&ws))),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(Arc::clone(&ws))),
            Arc::new(rocket_infra::SharedPathEnvironmentRepo::with_secret_store(
                Arc::clone(&ws),
                Arc::new(rocket_environment::NullSecretStore),
            )),
            Arc::new(NullEventPublisher),
        );
        (dir, Arc::new(svc))
    }

    fn input() -> ConversationInputDto {
        serde_json::from_value(serde_json::json!({
            "id": ID,
            "agentConfigId": "claude-code",
            "mode": "edit",
            "workspaceId": "default",
            "collections": [],
            "messages": [
                { "kind": "user", "text": "Why 401?" },
                { "kind": "agent", "text": "Because." },
                { "kind": "tool", "title": "get_request", "status": "in_progress" }
            ]
        }))
        .expect("the camelCase input parses")
    }

    #[tokio::test]
    async fn save_list_get_delete_and_clear_through_the_commands() {
        let (dir, svc) = service();
        svc.pin_session("s1");
        let saved = save_conversation_inner(Arc::clone(&svc), "s1".into(), input())
            .await
            .expect("save");
        assert_eq!(saved.title, "Why 401?");
        assert!(dir.path().join("assistant").join(".gitignore").is_file());
        let listed = list_conversations_inner(Arc::clone(&svc)).await.expect("list");
        assert_eq!(listed, vec![saved.clone()]);
        let record = get_conversation_inner(Arc::clone(&svc), ID.into()).await.expect("get");
        assert_eq!(record.messages.len(), 3);
        delete_conversation_inner(Arc::clone(&svc), ID.into()).await.expect("delete");
        clear_conversations_inner(Arc::clone(&svc)).await.expect("clear");
        assert!(list_conversations_inner(svc).await.expect("list").is_empty());
    }

    #[test]
    fn dtos_are_camel_case_and_messages_keep_their_wire_shape() {
        let summary = ConversationSummaryDto::from(ConversationSummary {
            id: ID.into(),
            title: "t".into(),
            agent_config_id: "a".into(),
            mode: ConversationMode::Ask,
            created_at: "c".into(),
            updated_at: "u".into(),
            message_count: 2,
            continued_from: Some("x".into()),
        });
        assert_eq!(
            serde_json::to_value(&summary).expect("json"),
            serde_json::json!({
                "id": ID, "title": "t", "agentConfigId": "a", "mode": "ask",
                "createdAt": "c", "updatedAt": "u", "messageCount": 2, "continuedFrom": "x"
            })
        );
        let message = StoredMessage::Tool {
            title: "t".into(),
            status: StoredToolStatus::InProgress,
        };
        assert_eq!(
            serde_json::to_value(&message).expect("json"),
            serde_json::json!({ "kind": "tool", "title": "t", "status": "in_progress" })
        );
    }

    #[tokio::test]
    async fn errors_keep_their_domain_kind() {
        let (_dir, svc) = service();
        let err = get_conversation_inner(Arc::clone(&svc), "../x".into())
            .await
            .expect_err("invalid id");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = save_conversation_inner(svc, "never-started".into(), input())
            .await
            .expect_err("unknown session");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

Add `pub mod assistant_conversations;` to `src-tauri/src/commands/mod.rs` after `pub mod assistant_chips;`.

- [ ] **Step 8: Run them to see them fail**

Run: `cargo test -p rocket -j4 --lib assistant_conversations`
Expected: compile errors `cannot find type 'ConversationInputDto'` and `cannot find function 'save_conversation_inner'`.

- [ ] **Step 9: Implement the DTOs and commands**

Insert between the `use` lines and `#[cfg(test)]` in `assistant_conversations.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummaryDto {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCollectionDto {
    pub uid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecordDto {
    pub id: String,
    pub title: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    pub workspace_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub collections: Vec<ConversationCollectionDto>,
    pub messages: Vec<StoredMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInputDto {
    pub id: String,
    pub agent_config_id: String,
    pub mode: ConversationMode,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub collections: Vec<String>,
    pub messages: Vec<StoredMessage>,
    #[serde(default)]
    pub continued_from: Option<String>,
}

impl From<ConversationSummary> for ConversationSummaryDto {
    fn from(s: ConversationSummary) -> Self {
        Self {
            id: s.id,
            title: s.title,
            agent_config_id: s.agent_config_id,
            mode: s.mode,
            created_at: s.created_at,
            updated_at: s.updated_at,
            message_count: s.message_count,
            continued_from: s.continued_from,
        }
    }
}

impl From<ConversationRecord> for ConversationRecordDto {
    fn from(r: ConversationRecord) -> Self {
        Self {
            id: r.id,
            title: r.title,
            agent_config_id: r.agent_config_id,
            mode: r.mode,
            workspace_id: r.workspace_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collections: r
                .collections
                .into_iter()
                .map(|c| ConversationCollectionDto { uid: c.uid, name: c.name })
                .collect(),
            messages: r.messages,
            continued_from: r.continued_from,
        }
    }
}

impl From<ConversationInputDto> for ConversationInput {
    fn from(d: ConversationInputDto) -> Self {
        Self {
            id: d.id,
            agent_config_id: d.agent_config_id,
            mode: d.mode,
            workspace_id: d.workspace_id,
            collections: d.collections,
            messages: d.messages,
            continued_from: d.continued_from,
        }
    }
}

/// Runs a blocking service call off the UI thread. The service reads the
/// workspace's variables and writes a file.
async fn run_blocking<T, F>(svc: Arc<ConversationService>, call: F) -> Result<T, DomainError>
where
    T: Send + 'static,
    F: FnOnce(&ConversationService) -> Result<T, DomainError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || call(&svc))
        .await
        .map_err(|e| DomainError::Internal(format!("conversation task failed: {e}")))?
}

pub async fn list_conversations_inner(
    svc: Arc<ConversationService>,
) -> Result<Vec<ConversationSummaryDto>, DomainError> {
    run_blocking(svc, |svc| {
        Ok(svc.list()?.into_iter().map(ConversationSummaryDto::from).collect())
    })
    .await
}

pub async fn get_conversation_inner(
    svc: Arc<ConversationService>,
    id: String,
) -> Result<ConversationRecordDto, DomainError> {
    run_blocking(svc, move |svc| svc.get(&id).map(ConversationRecordDto::from)).await
}

pub async fn save_conversation_inner(
    svc: Arc<ConversationService>,
    session_id: String,
    conversation: ConversationInputDto,
) -> Result<ConversationSummaryDto, DomainError> {
    run_blocking(svc, move |svc| {
        svc.save(&session_id, conversation.into())
            .map(ConversationSummaryDto::from)
    })
    .await
}

pub async fn delete_conversation_inner(
    svc: Arc<ConversationService>,
    id: String,
) -> Result<(), DomainError> {
    run_blocking(svc, move |svc| svc.delete(&id)).await
}

pub async fn clear_conversations_inner(svc: Arc<ConversationService>) -> Result<(), DomainError> {
    run_blocking(svc, |svc| svc.clear()).await
}

#[tauri::command]
pub async fn list_conversations(
    svc: State<'_, Arc<ConversationService>>,
) -> Result<Vec<ConversationSummaryDto>, DomainError> {
    list_conversations_inner(Arc::clone(&svc)).await
}

#[tauri::command]
pub async fn get_conversation(
    id: String,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<ConversationRecordDto, DomainError> {
    get_conversation_inner(Arc::clone(&svc), id).await
}

#[tauri::command]
pub async fn save_conversation(
    session_id: String,
    conversation: ConversationInputDto,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<ConversationSummaryDto, DomainError> {
    save_conversation_inner(Arc::clone(&svc), session_id, conversation).await
}

#[tauri::command]
pub async fn delete_conversation(
    id: String,
    svc: State<'_, Arc<ConversationService>>,
) -> Result<(), DomainError> {
    delete_conversation_inner(Arc::clone(&svc), id).await
}

#[tauri::command]
pub async fn clear_conversations(
    svc: State<'_, Arc<ConversationService>>,
) -> Result<(), DomainError> {
    clear_conversations_inner(Arc::clone(&svc)).await
}
```

- [ ] **Step 10: Run the command tests to see them pass**

Run: `cargo test -p rocket -j4 --lib assistant_conversations`
Expected: `3 passed`.

- [ ] **Step 11: Pin the workspace when the assistant starts**

In `src-tauri/src/commands/acp_sessions.rs`:
- Add `ConversationService` to the `use rocket_app::{...}` list (`:4-7`).
- In `start_workspace_assistant` (`:111-132`) add the parameter `conversations: State<'_, Arc<ConversationService>>,` after `resources` and pass `&conversations` after `&resources`.
- In `start_workspace_assistant_inner` (`:144-153`) add the parameter `conversations: &ConversationService,` after `resources: &SessionResourceRegistry,`.
- Right after the `mcp_tool_svc.begin_assistant_session(...)` call (`:191-195`) add:

```rust
    // The same moment as the tool pin, so the conversation of this session
    // is saved into the workspace it started in, even after a switch.
    conversations.pin_session(&info.session_id);
```

In `src-tauri/tests/acp_mcp_start_workspace_assistant.rs`:
- Add `ConversationService` to the `use rocket_app::{...}` list and `SharedPathConversationRepo` to the `use rocket_infra::{...}` list.
- Add the field `conversation_svc: ConversationService,` to `Fixture`.
- In `build_fixture`, before `let app = ...`, add:

```rust
    let conversation_svc = ConversationService::new(
        Arc::new(SharedPathConversationRepo::new(Arc::clone(&ws_path))),
        Arc::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
        Arc::new(SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path))),
        Arc::new(FsEnvironmentRepo::new(tmp.path().join("environments"))),
        Arc::new(NullEventPublisher),
    );
```

  and `conversation_svc,` in the `Fixture { ... }` literal.
- In `start`, pass `&fx.conversation_svc,` after `&fx.resources,`.
- Add this test at the end of the file:

```rust
#[tokio::test]
async fn a_started_session_can_save_its_conversation_into_its_workspace() {
    let (client, _, _) = client(false, "acp-real-session-2");
    let fx = build_fixture(client);
    let session_id = start(&fx).await.expect("start").session_id;

    let id = "0b6f3f4e-0000-4000-8000-000000000001";
    fx.conversation_svc
        .save(
            &session_id,
            rocket_app::ConversationInput {
                id: id.into(),
                agent_config_id: "agent-1".into(),
                mode: rocket_acp::ConversationMode::Edit,
                workspace_id: "default".into(),
                collections: vec!["demo".into()],
                messages: vec![
                    rocket_acp::StoredMessage::User { text: "hi".into() },
                    rocket_acp::StoredMessage::Agent { text: "hello".into() },
                ],
                continued_from: None,
            },
        )
        .expect("the start pinned the workspace");
    assert!(fx
        ._tmp
        .path()
        .join("assistant")
        .join("conversations")
        .join(format!("{id}.yml"))
        .is_file());
}
```

- [ ] **Step 12: Wire the service and register the commands**

In `src-tauri/src/lib.rs`, right after the `mcp_tool_svc` statement (`:646-662`), add:

```rust
            // Stored assistant conversations of the active workspace. The
            // environment repositories read secret values from the keychain,
            // because masking needs the real values.
            let conversation_svc = Arc::new(rocket_app::ConversationService::new(
                Arc::new(rocket_infra::SharedPathConversationRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
                Arc::new(SharedCollectionEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    env_secret_store(),
                )),
                Arc::new(SharedPathEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    env_secret_store(),
                )),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            ));
```

After `app.manage(Arc::clone(&proposal_svc));` (`:794`) add `app.manage(conversation_svc);`.

In `generate_handler!`, after `commands::agent_proposals::reject_agent_proposal,` (`:1054`) add:

```rust
            commands::assistant_conversations::list_conversations,
            commands::assistant_conversations::get_conversation,
            commands::assistant_conversations::save_conversation,
            commands::assistant_conversations::delete_conversation,
            commands::assistant_conversations::clear_conversations,
```

- [ ] **Step 13: Run the start integration test**

Run: `cargo test -p rocket -j4 --test acp_mcp_start_workspace_assistant`
Expected: every test passes, including `a_started_session_can_save_its_conversation_into_its_workspace`.

- [ ] **Step 14: Document the store**

Create `.claude/assistant-conversations.md`:

```markdown
# Assistant conversations

Spec: `docs/superpowers/specs/2026-10-11-assistant-conversation-history-design.md`. Plans: `docs/superpowers/plans/assistant-conversation-history/`.

## Storage

- `<workspace>/assistant/conversations/<id>.yml`, one file per conversation, `<id>` a lowercase UUID. `<workspace>/assistant/.gitignore` holds `*` (created with `create_new`, never overwritten, a failure only logged).
- Rocket-private, not OpenCollection: no file under `collections/` and no key in `opencollection.yml`, `folder.yml`, request, environment or `workspace.yml` files. Collections are referenced by `opencollection.yml` uid only.
- Keys are snake_case (`ConversationRecord` in `rocket-acp`, no `rename_all`). IPC DTOs in `src-tauri/src/commands/assistant_conversations.rs` are camelCase.
- Caps: 50 conversations (oldest `updated_at` evicted, never the one just saved), 500 messages (newest kept), 16 KB per text field (`TEXT_CUT_MARKER`). Unparseable, other-version or misnamed files are skipped by `list` with a warning.

## Saving

- `start_workspace_assistant` calls `ConversationService::pin_session(session_id)`. The pin holds `pinned()` repositories of the workspace active at start; `save_conversation(session_id, ...)` writes there even after a switch. 16 pins are kept. The McpToolService pin is not used, because it is dropped at session end, before the final save.
- Masking: secret variable values of the pinned workspace (global environments, collection variables and initial values, collection environments, folder and request variables) are replaced with `••••••` by `mask_secret_text`, before the caps. A scope that cannot be read refuses the save. Vault values are not fetched (v1), and only collections under `<workspace>/collections` are walked (the same list the assistant's tools use).
- Every save walks every request file of the workspace, once per finished turn (debounced in the frontend).
- Events carry ids only: `assistant-conversation-saved`, `assistant-conversation-deleted`, `assistant-conversations-cleared`.
```

In the root `CLAUDE.md`, "Rules and conventions", after the `.claude/assistant-composer.md` line, add:

```markdown
See `.claude/assistant-conversations.md` for stored assistant conversations (storage, pins, masking, caps).
```

In `crates/rocket-app/CLAUDE.md`, Public Types table, after the `ProposalService` row, add:

```markdown
| `ConversationService` | Stored assistant conversations. `pin_session(session_id)` (called by `start_workspace_assistant`) keeps `pinned()` repositories of the workspace active at start (16 pins). `save(session_id, input)` validates (lowercase UUID ids, an agent reply), masks every secret variable value of the pinned workspace with `mask_secret_text` (`conversation_secrets.rs`, fails closed on a read error, no vault), then applies the caps and publishes `AssistantConversationSaved`. `list`/`get`/`delete`/`clear` use the live workspace. See `.claude/assistant-conversations.md`. |
```

- [ ] **Step 15: Check and commit**

Run: `cargo check --workspace --all-targets -j4`
Expected: no errors.
Run: `cargo clippy -p rocket-app -p rocket-infra -p rocket-acp -j4 -- -D warnings`
Expected: no warnings in the new files.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add crates/rocket-app/src/conversation_secrets.rs crates/rocket-app/src/conversation_service.rs crates/rocket-app/src/lib.rs crates/rocket-app/CLAUDE.md src-tauri/src/commands/assistant_conversations.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/acp_sessions.rs src-tauri/src/lib.rs src-tauri/tests/acp_mcp_start_workspace_assistant.rs .claude/assistant-conversations.md CLAUDE.md
git commit --only -m "feat(assistant): save masked conversations per workspace" -- crates/rocket-app/src/conversation_secrets.rs crates/rocket-app/src/conversation_service.rs crates/rocket-app/src/lib.rs crates/rocket-app/CLAUDE.md src-tauri/src/commands/assistant_conversations.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/acp_sessions.rs src-tauri/src/lib.rs src-tauri/tests/acp_mcp_start_workspace_assistant.rs .claude/assistant-conversations.md CLAUDE.md
```

---

## Next Plan

Plan 02 (`2026-10-11-assistant-conversation-history-plan-02-frontend.md`) adds the TypeScript wrappers, the save routine and the History view on top of these five commands and three channels.

## Post-Implementation Review

An Opus reviewer reads this plan's diff (`git diff <base>..HEAD -- crates src-tauri`) against the Review Focus list, with extra attention on Task 3: the masking order, the scopes `workspace_secret_values` walks, the fail-closed error mapping, and the session pin in `start_workspace_assistant_inner`. Findings and rulings go into the ledger as `Ruling:` lines.
