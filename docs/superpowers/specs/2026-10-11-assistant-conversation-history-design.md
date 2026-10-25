# Assistant conversation history and resume: design

Date: 2026-10-11. Status: approved design decisions, spec ready for the user's final review. Roadmap row R-17.
Read first: `docs/superpowers/specs/opencollection-spec-reference.md`, `docs/features/assistant-composer.md`, `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`.

## 1. Summary

The workspace assistant keeps its messages in memory only. They are lost when the session ends or the app restarts. This design stores finished conversations per workspace, shows them read-only (stage 1), and lets the user continue one by starting a new session seeded with the stored transcript (stage 2).

It does not use the agent's own session load or resume. See section 2.

## 2. Decisions already made with the user

- Purpose: resume a conversation. Chosen design: archive first, then replay into a new session.
- Storage: per workspace, inside the workspace folder, never committed to git.
- The design must follow the OpenCollection specification (section 4 lists what that means here).

Why not native `session/load` or `session/resume`: the protocol has both (`agent-client-protocol-schema` 1.9.1), but Rocket's client never reads the capability or calls them, and the assistant runs each session with an empty per-session `CLAUDE_CONFIG_DIR` and a scratch folder that is deleted at the end. A Claude-style agent keeps its transcript in that folder, so native resume would fail even where the agent supports it. Keeping the folder would weaken the isolation guarantee ("no transcripts left behind"). Support in `claude-agent-acp` could not be verified offline. Replay needs none of this.

## 3. Goals and non-goals

Goals:
1. A finished or ended conversation survives an app restart, per workspace.
2. The user can list, open (read-only), delete and clear conversations.
3. The user can continue a conversation in a new session seeded with a bounded copy of the old transcript.
4. No change to agent isolation, the MCP tool server or the proposal safety rules.

Non-goals:
- Native agent session resume.
- Syncing, exporting or sharing transcripts. Search across conversations. Editing a stored conversation.
- Replaying tool results or the agent's internal state.
- Live proposals from a stored conversation. Old proposals are history only.

## 4. OpenCollection compatibility rules

The OpenCollection spec governs collections: `opencollection.yml`, `folder.yml`, request files, environments and the `extensions` object. It says nothing about workspace-level data, extra directories, or history, cookie and template stores. Those stores are Rocket-private. This feature must keep the collection format untouched:

1. Location is `<workspace>/assistant/conversations/<id>.yml` only. Never inside `<workspace>/collections/`, a collection, a folder, or the directory of an externally linked collection.
2. No transcript data and no new keys go into `opencollection.yml`, `folder.yml`, request files, environment files or `workspace.yml`. `workspace.yml` is rewritten from a domain struct and drops unknown keys, so it is not used. No `extensions.*` keys are used.
3. Files are `.yml` (never JSON), named `<id>.yml` with a generated UUID, so no user text appears in a file name. None of the reserved names (`opencollection.yml`, `folder.yml`, `workspace.yml`, `_order.yml`, `collection.json`) is used. Writes use `atomic_write`.
4. `<workspace>/assistant/.gitignore` contains `*`. It is created with `create_new` before the first write, never overwritten, and a failure to create it is only logged (same as the cookie store in `fs_cookie_repo.rs`). Rocket git actions on a workspace repository therefore never stage transcripts.
5. The path is resolved on every call from the active (or the session's pinned) workspace, in the style of `SharedPathHistoryRepo`, not pinned at startup. Directories are created lazily. The directory is outside the collections file watcher, so transcript writes raise no `collection-changed` events.
6. A collection is referenced by its `uid` from `opencollection.yml` (the identity `trust.yml` uses) as optional metadata inside a transcript, never by a path. Deleting a workspace removes its transcripts with it (`delete_workspace` removes the directory).
7. Reads tolerate missing and unknown keys and skip an unparseable file with a warning. A bad file never breaks the History view or the collection sidebar.

Key casing ruling (decided by the user on 2026-10-11): the persistence structs follow the repo rule, so YAML keys are snake_case with no `rename_all`. camelCase is used only on the IPC DTOs. Existing stores such as history and cookies write camelCase, but this store does not copy that.

## 5. Data model

One file per conversation.

```yaml
version: 1
id: 0b6f3f4e-...              # UUID, same as the file name
title: "Why does login return 401?"   # first user message, trimmed to 80 chars
agent_config_id: claude-code
mode: ask                      # ask | edit | agent, as when the conversation ended
workspace_id: default           # informational
created_at: 2026-10-11T09:12:00Z
updated_at: 2026-10-11T09:20:41Z
collections:                   # optional, uids of collections the chat focused on
  - uid: 3c1d...
    name: my-api
messages:
  - kind: user
    text: "..."
  - kind: agent
    text: "..."
  - kind: tool
    title: "run_request: login"
    status: completed          # pending | in_progress | completed | failed
  - kind: notice
    text: "..."
  - kind: proposal
    summary: "Change header X in login"
    status: accepted           # pending | accepted | rejected | stale | failed
```

Not stored: raw tool results, the ACP session id, usage numbers, model options, the MCP token or port, proposal change payloads.

Caps (constants, easy to change; approved by the user): 50 conversations per workspace (the oldest by `updated_at` is removed when a 51st is saved), 500 messages per conversation, 16 KB of text per message (cut with a visible marker). The whole file is read and written as a unit.

## 6. Architecture

Follows the repo layering: traits in domain crates, I/O in `rocket-infra`, orchestration in `rocket-app`, thin IPC commands.

- `rocket-acp`: `ConversationRecord`, `ConversationSummary`, `StoredMessage` types and a `ConversationRepository` trait (`list`, `get`, `save`, `delete`, `clear`). No I/O.
- `rocket-infra`: `FsConversationRepo` (one directory, `atomic_write`, caps, `ensure_gitignore`) and `SharedPathConversationRepo` (builds a short-lived `FsConversationRepo` from the shared active workspace path, with `pinned()` like the other shared repos for a read-modify-write).
- `rocket-app`: `ConversationService`: validates input, enforces caps, applies masking (section 8), and publishes `DomainEvent::AssistantConversationSaved { conversation_id }` after a save and `AssistantConversationDeleted { conversation_id }` (or `AssistantConversationsCleared`) after a delete. `TauriEventBus` maps them to the channels `assistant-conversation-saved`, `assistant-conversation-deleted` and `assistant-conversations-cleared`. The payloads carry ids only, never message text.
- `src-tauri`: commands `list_conversations`, `get_conversation`, `save_conversation`, `delete_conversation`, `clear_conversations`. camelCase DTOs, async commands (`spawn_blocking` around file work, as for proposals). Registered in `lib.rs`; wiring as for `agent_proposals.rs`.

## 7. Frontend

Stage 1:
- `assistant-store` gets a `conversationId` (generated at the first user message) and a small save routine in the assistant event bridge: it saves the current messages after a turn finishes (debounced, 500 ms), when the session ends, and when the panel closes. A conversation with no agent reply yet is not saved. A save error shows one quiet notice and never blocks chat.
- Streaming messages are saved only after `completeMessage` or `failMessage`. A save never includes `streaming: true`.
- A "History" view in the panel: list (title, relative date, message count, agent name), open read-only (reuses the chat message components with actions disabled), delete one, clear all (with a confirm step). A short notice states that conversations are stored on this computer in this workspace.
- Stored proposals render as plain history lines. They never show Accept or Reject buttons.
- The History view listens to those three channels and refreshes its list, so a second window or a finished save shows up without reopening. On a workspace switch the view reloads and the open read-only conversation closes.

Stage 2 (replay):
- "Continue" in a read-only conversation starts a normal new session with the same agent config. The mode always starts in Ask (approved by the user) and the user can raise it with the existing mode control, so a stored Agent mode is never re-applied silently.
- The first prompt is prefixed with a preface block built by a pure function `buildReplayPreface(record, limits)`: a clear header ("This is a previous conversation with the user, for context only. Do not repeat its actions."), then the last messages that fit in 12,000 characters (user and agent text, tool lines as one-line summaries, proposals as one-line summaries), oldest first, with a marker when older messages were dropped. Tool results are not included. The agent calls the read tools again if it needs data.
- The new session saves as a new conversation that records `continued_from: <id>` for the list view. The old conversation is not changed.
- The preface counts toward the normal prompt size, and the composer shows how large it is before sending.

## 8. Privacy and masking

- Tool output the agent saw is already masked by the read views. Agent text can only echo masked values.
- A secret the user typed or pasted into a prompt would be stored, as it is in prompt history today. Before writing, `ConversationService` replaces the values of secret variables (collection, folder, request and environment variables flagged secret, across the collections of the workspace) with the existing redaction marker, using the same helper the chips use. Vault values are not resolved here, because that needs keychain access and has a timeout. This limit is documented in the UI notice.
- Transcripts are workspace-scoped, capped, deletable, ignored by git, and never part of any export or sync.
- Stage 2 sends stored text to the agent, which is the same exposure as the original conversation.

## 9. Errors and edge cases

- Missing directory: created lazily; `list` of a missing directory returns empty.
- Corrupt or unknown-version file: skipped in `list` with a warning; `get` returns a clear error.
- Concurrent saves of the same conversation: the last write wins (atomic rename); one conversation is written by one panel.
- Workspace switch during a save: the save uses the workspace pinned when the session started, not the live one.
- Disk full or read-only workspace: the save fails with an error; the UI shows the quiet notice.
- A deleted agent config: a stored conversation still opens; "Continue" asks the user to pick an agent.

## 10. Testing

- `rocket-infra`: round trip, caps (oldest evicted, message and text limits), `.gitignore` created once and never overwritten, unknown keys ignored, corrupt file skipped, per-call workspace resolution, file names are the UUID only.
- `rocket-app`: masking of secret variable values and marker output, validation, caps, and the events published after save, delete and clear.
- `rocket-shared`: the three new `DomainEvent` variants and their wire shape; `TauriEventBus` channel names.
- Command tests: DTO casing, error mapping.
- Vitest: save triggers (turn end, session end, panel close, no save while streaming), History view states, read-only rendering (no action buttons on proposals), clear confirm, workspace switch reload; `buildReplayPreface` limits, ordering, drop marker, and that tool results never appear.
- Manual: restart the app, reopen the History view, continue a conversation.

## 11. Plans

Plans hold at most 3 tasks each.

- Plan 1 (stage 1 backend): types and trait in `rocket-acp`; `FsConversationRepo` and `SharedPathConversationRepo`; `ConversationService` with masking and the five IPC commands.
- Plan 2 (stage 1 frontend): API wrappers and types; save routine in the event bridge; History view with read-only rendering, delete and clear.
- Plan 3 (stage 2): `buildReplayPreface`, the Continue action and composer size hint, `continued_from` in the list.

Each task runs the usual loop: implementer, review, fix. The masking task gets an Opus review.

## 12. Review decisions

Decided by the user on 2026-10-11:
1. Key casing: snake_case persistence keys, camelCase only on IPC DTOs.
2. Caps: 50 conversations, 500 messages, 16 KB per message.
3. Continue always starts in Ask mode.
5. The "conversation saved" events are added now (section 6), not deferred to R-55.

4. Option A: vault values are not scrubbed in the first version (section 8). The History view states that a secret pasted into a prompt is stored, and Delete and Clear all are always available. Fetching vault values (best effort or strict) can be added later if needed.

No open questions remain.
