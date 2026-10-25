# Assistant Conversation History: Plan Index

**Spec:** [../../specs/2026-10-11-assistant-conversation-history-design.md](../../specs/2026-10-11-assistant-conversation-history-design.md) (roadmap R-17).

**Context:** builds on the workspace AI assistant series ([../workspace-ai-assistant/00-plan-index.md](../workspace-ai-assistant/00-plan-index.md)), which lives on the unmerged branch `worktree-acp-mcp-tool-server`. Work on that branch (or a branch cut from it), never on `main`.

## Plans

| # | Plan | Areas | Depends on |
|---|---|---|---|
| 01 | [Backend](2026-10-11-assistant-conversation-history-plan-01-backend.md): types, trait, events, `FsConversationRepo`/`SharedPathConversationRepo`, `ConversationService` (masking, caps, pins), five IPC commands | `rocket-acp`, `rocket-shared`, `rocket-infra`, `rocket-app`, `src-tauri` | — |
| 02 | [Frontend](2026-10-11-assistant-conversation-history-plan-02-frontend.md): wrappers, snapshot, save routine, History view | `src/lib`, `src/stores`, `src/components/assistant` | 01 |
| 03 | [Replay](2026-10-11-assistant-conversation-history-plan-03-replay.md): `buildReplayPreface`, Continue in Ask mode, size hint, continued-from label | `src/lib/assistant`, `src/stores`, `src/components/assistant` | 01, 02 |

Build in order 01, 02, 03, one plan at a time. Per the user's standing preference, the next plan starts without asking once the current one is complete. Never push; merging is the user's decision.

## Model hints (one line per task)

| Task | Implementer | Reviewer |
|---|---|---|
| P01 T1 Types, trait, events, `channel_for` | sonnet | sonnet |
| P01 T2 `FsConversationRepo`, `SharedPathConversationRepo` (incl. `pinned()`) | sonnet | sonnet |
| P01 T3 `ConversationService` masking and caps, session pin in `start_workspace_assistant_inner`, commands, wiring | sonnet | **opus** (masking order and scopes, fail-closed mapping, save-path workspace pinning) |
| P02 T1 Wrappers, timeline move, snapshot | sonnet | haiku |
| P02 T2 `conversationId`, save routine | sonnet | sonnet |
| P02 T3 History view, read-only view, panel toggle | sonnet | sonnet |
| P03 T1 `buildReplayPreface` | sonnet | sonnet |
| P03 T2 Continue (store seed, `continueConversation`, preface once, Continue footer) | sonnet | sonnet |
| P03 T3 Composer size hint, continued-from label, docs | sonnet | haiku |

The final whole-branch review after Plan 03 runs on opus.

## Execution notes

- **Ledger:** one per plan at `.superpowers/sdd/2026-10-11-assistant-conversation-history-plan-0N/progress.md`. "Task N: complete" means done. Log decisions as `Ruling:` lines. After `/clear` or compaction, use the `resume-plan` skill.
- **One implementer per task, one at a time per worktree.** Reviewers read a diff file, not the live tree. Re-reviews cover only the fixes since the last review.
- **Cross-plan compile rule:** `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green at the end of every task.
- **Tests:** each task writes its tests and lists the exact command. The previous series had the rule "the user runs the tests; the agent runs only compile and lint checks". The controller confirms with the user before Plan 01 whether that rule still applies; if it does, implementers skip the "run" steps for tests and still run the checks.
- **Cargo:** always `-j4`; never `cargo test --workspace` or `--all`. Give the worktree its own `target/`.
- **Commits:** every commit through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and `git commit --only -- <paths>`; never `git add -A`, `--all` or `.`; no bare `git stash`. Dispatched implementers get this rule in their prompt.
- **OpenCollection:** every backend task starts with "📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`." The store is Rocket-private (spec section 4); no collection, folder, request, environment or `workspace.yml` file changes.
- **Docs:** tasks update `crates/rocket-acp/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/features/assistant-conversations.md` (new), `docs/features/assistant-composer.md` and one pointer line in the root `CLAUDE.md`. Tasks do not edit the roadmap.

## Rulings (controller, 2026-10-11)

- Ruling: the session-end save uses `ConversationService::pin_session`, not the `McpToolService` pin — `forget_session` drops that pin before the final save — cost if wrong: a signature change in the start command and its integration test.
- Ruling: `workspace_secret_values` walks the workspace on each save on a blocking thread, and it fails closed: if any collection file cannot be read, that save is skipped with a visible notice, the conversation stays in memory and the next turn retries — cost if wrong: a broken file blocks saving until fixed.
- Ruling: externally linked collections are not scrubbed (the collection list covers `<ws>/collections` only); the History view notice says so next to the vault limit — cost if wrong: a secret variable value from a linked collection that is pasted into a prompt is stored.
- Ruling: the composer size hint is informational; there is no prompt text limit to enforce.
- Ruling: `continued_from` is stored by Plan 01 (one backend change); Plan 03 only sets and shows it. `StoredMessage` is reused as the wire type, and a test pins the shape. The message cap keeps the newest 500.
- Ruling: tests — implementers write the tests and run only the focused commands named in each task plus the compile and lint checks. They do not run the full suites; the user runs those.

## Locked contracts

```rust
// rocket_acp::conversation
pub struct ConversationRecord { version, id, title, agent_config_id, mode: ConversationMode, workspace_id,
    created_at, updated_at, collections: Vec<ConversationCollection>, continued_from: Option<String>,
    messages: Vec<StoredMessage> }                     // snake_case YAML, no rename_all
pub enum StoredMessage { User{text}, Agent{text}, Tool{title, status}, Notice{text}, Proposal{summary, status} } // tag "kind"
pub trait ConversationRepository { list, get, save, delete, clear, pinned }
// rocket_shared::events::DomainEvent
AssistantConversationSaved { conversation_id }, AssistantConversationDeleted { conversation_id }, AssistantConversationsCleared
// rocket_app
ConversationService::{new, with_clock, pin_session, list, get, save(session_id, ConversationInput), delete, clear}
// src-tauri commands (camelCase DTOs)
list_conversations, get_conversation(id), save_conversation(session_id, conversation), delete_conversation(id), clear_conversations
```

```ts
// src/lib/tauri-api.ts
listConversations(), getConversation(id), saveConversation(sessionId, conversation), deleteConversation(id), clearConversations()
onAssistantConversationSaved / onAssistantConversationDeleted / onAssistantConversationsCleared
// channels: assistant-conversation-saved, assistant-conversation-deleted, assistant-conversations-cleared
// src/lib/assistant: buildConversationInput, startConversationSaver, buildReplayPreface, continueConversation
// src/stores/assistant-store.ts: conversationId?, replay?: ReplaySeed & { sent }, markReplaySent(sessionId)
```

**Workspace of a save (spec section 9):** `start_workspace_assistant_inner` calls `ConversationService::pin_session(session_id)` right after `begin_assistant_session`. The pin keeps `pinned()` repositories (conversations, collections, collection environments, global environments) of the workspace active at start. `save_conversation(session_id, ...)` writes and masks there. The `McpToolService` pin is not reused, because `forget_session` drops it before the final save.

**Interface note:** `continued_from` is carried end to end by Plan 01 (record, summary, input, DTOs) so the backend changes once; Plan 03 only sets it from the frontend and shows it.

## Self-Review

### Spec coverage

| Spec item | Where |
|---|---|
| §3.1 Survives restart, per workspace | P01 T2 (files), P02 T2 (save routine) |
| §3.2 List, open read-only, delete, clear | P01 T3 (commands), P02 T3 (view) |
| §3.3 Continue with a bounded transcript | P03 T1, T2 |
| §3.4 No change to isolation, MCP server, proposals | P01 T3 adds only a pin call in the start command; no tool or isolation code changes |
| §4.1 Location `<ws>/assistant/conversations/<id>.yml` | P01 T2 `SharedPathConversationRepo::current`, test `calls_follow_the_active_workspace_and_pinned_stays` |
| §4.2 No OpenCollection files or keys | P01 T2 (separate folder), Global Constraints |
| §4.3 `.yml`, UUID names, `atomic_write` | P01 T1 `validate_conversation_id`, P01 T2 `file_path`, `save_get_list_round_trip` |
| §4.4 `.gitignore` `*` via `create_new`, logged failure | P01 T2 three `.gitignore` tests |
| §4.5 Per-call resolution, lazy dirs, outside watcher | P01 T2, `list_of_a_missing_directory_is_empty` |
| §4.6 Collection by uid | P01 T3 `collection_refs` |
| §4.7 Tolerant reads | P01 T1 `a_record_with_unknown_and_missing_keys_still_loads`, P01 T2 `unknown_keys_are_ignored`, `list_skips_corrupt_...` |
| §4 key casing ruling | P01 T2 `the_file_uses_snake_case_keys_only`, P01 T3 `dtos_are_camel_case_...` |
| §5 Data model and "not stored" list | P01 T1 types |
| §5 Caps 50 / 500 / 16 KB | P01 T1 `apply_caps`, P01 T2 eviction and cap tests |
| §6 Events and channels | P01 T1, P01 T3 `a_resave_keeps_created_at_and_events_carry_ids_only` |
| §6 Five commands, async, `spawn_blocking` | P01 T3 |
| §7 `conversationId` at first user message | P02 T2 store test |
| §7 Save after turn (500 ms), session end, panel close; no save without a reply; no streaming | P02 T2 saver tests |
| §7 Quiet notice on save error | P02 T2 `shows one quiet notice per conversation when saves fail` |
| §7 History list, read-only, delete, clear confirm, privacy notice | P02 T3 |
| §7 Proposals as plain lines | P02 T3 `opens a conversation read-only, with proposals as plain lines` |
| §7 Events refresh, workspace switch reload and close | P02 T3 |
| §7 Continue in Ask mode, same agent config | P03 T2 |
| §7 `buildReplayPreface` header, 12,000 chars, order, marker, no tool results | P03 T1 |
| §7 `continued_from` recorded, old one unchanged | P01 T3 (stored), P03 T2 (sent), P03 T3 (shown) |
| §7 Composer size hint | P03 T3 |
| §8 Secret variable masking with the chips' helper | P01 T3 (`mask_secret_text`, `workspace_secret_values`) |
| §8 No vault scrubbing, UI says so | P01 T3 (none fetched), P02 T3 `HISTORY_PRIVACY_NOTICE` |
| §9 Missing dir, corrupt file, last write wins, switch during save, disk error, deleted agent config | P01 T2, P01 T3, P02 T2, P03 T2 |
| §10 Testing list | Each item above names its test |

### Placeholder scan

No "TBD", "TODO", "similar to task N" or "add error handling" in the three plans. Two steps describe a mechanical move instead of repeating code: P01 T1 Step 8 (move the existing `match` arms of `TauriEventBus::publish` verbatim into `channel_for`) and P02 T1 Step 4 (cut `TimelineEntry` and `buildTimeline` unchanged into `timeline.ts`); both name the exact lines.

### Type consistency

Checked across plans: command argument names (`sessionId`/`session_id`, `conversation`, `id`), DTO fields (`agentConfigId`, `workspaceId`, `messageCount`, `continuedFrom`), `StoredMessage` kinds and status values (`in_progress`, `pending|accepted|rejected|stale|failed`), mode values (`ask|edit|agent`), channel names, event payload field `conversation_id`, and the store names `conversationId`, `replay`, `markReplaySent`, `ReplaySeed`. `AssistantHistoryView` has no props in Plan 02 and an optional `onContinued` in Plan 03, so Plan 02's tests stay valid.

### Review Focus coverage

Each plan lists five Review Focus items, and each names a test in the owning task: P01 (masking order, switch pin, every scope, corrupt files, invalid ids), P02 (session-end proposals, streaming, save order, save failures, read-only and switch), P03 (preface once, limit, no tool results, Ask mode, deleted agent).
