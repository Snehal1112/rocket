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
