# Assistant conversations

Spec: `docs/superpowers/specs/2026-10-11-assistant-conversation-history-design.md`. Plans: `docs/superpowers/plans/assistant-conversation-history/`.

## Storage

- `<workspace>/assistant/conversations/<id>.yml`, one file per conversation, `<id>` a lowercase UUID. `<workspace>/assistant/.gitignore` holds `*` (created with `create_new`, never overwritten, a failure only logged).
- Rocket-private, not OpenCollection: no file under `collections/` and no key in `opencollection.yml`, `folder.yml`, request, environment or `workspace.yml` files. Collections are referenced by `opencollection.yml` uid only.
- Keys are snake_case (`ConversationRecord` in `rocket-acp`, no `rename_all`). IPC DTOs in `src-tauri/src/commands/assistant_conversations.rs` are camelCase.
- Caps: 50 conversations (oldest `updated_at` evicted, never the one just saved), 500 messages (newest kept), 16 KB per text field (`TEXT_CUT_MARKER`). Unparseable, other-version or misnamed files are skipped by `list` with a warning.

## Saving

- `start_workspace_assistant` calls `ConversationService::pin_session(session_id)`. The pin holds `pinned()` repositories of the workspace active at start; `save_conversation(session_id, ...)` writes there even after a switch. 16 pins are kept. The McpToolService pin is not used, because it is dropped at session end, before the final save.
- Masking: secret variable values of the pinned workspace (global environments, collection variables and initial values, collection environments, folder and request variables of every request kind, nested folders included) are replaced with `••••••` by `mask_secret_text`, before the caps. The input is bounded first: a payload over `MAX_INPUT_TEXT_BYTES` (2 MB of text) is refused, and only the newest 500 messages are masked. The title comes from the first user message, masked.
- Fail closed: any unreadable source refuses the save with a generic `InvalidInput` and writes nothing: a secret store (keychain) read error, a corrupt environment file, a request file that does not parse, an unreadable `folder.yml` or settings file. This uses the strict reads `EnvironmentRepository::list_strict` and `CollectionRepository::get_strict` (lenient `list`/`get` skip or log these cases for every other caller). The log line names the scope and the error kind only, never the error text or a value.
- Limits of the masking (v1): values shorter than 6 characters (`MIN_REDACTION_LEN`) are not masked; only the exact value is matched, not case, base64 or other encodings; overlapping secrets can leave the tail of one visible; vault values are not fetched; externally linked collections are not scrubbed, because only collections under `<workspace>/collections` are walked (the same list the assistant's tools use).
- Side effect: the walk calls `CollectionRepository::get_strict`, which auto-migrates a legacy JSON collection to YAML on first read, like `get`. A save can therefore migrate a legacy collection.
- The pin is dropped when the session fails to start (`unpin_session`). It is not dropped at session end, because the final save runs after cleanup; it ages out with the 16-pin cap.
- Every save walks every request file of the workspace, once per finished turn (debounced in the frontend).
- Events carry ids only: `assistant-conversation-saved`, `assistant-conversation-deleted`, `assistant-conversations-cleared`.
