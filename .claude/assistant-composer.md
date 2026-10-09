# AI Assistant composer

Code: `src/components/assistant/composer/` and `src/lib/assistant/`.

## PromptEditor

- CodeMirror 6, the one approved multi-line CodeMirror exception (see `.claude/rules/frontend-component-guardrails.md`). Do not reuse it for other fields.
- Reuses `rocketTheme`, `rocketThemeDark`, `rocketTooltipBase` and tooltips on `document.body`. Leaves out `singleLineFilter`. `promptEditorTheme` (loaded with `Prec.high`) grows the box to 12 lines, then scrolls.
- Keys: Enter sends, Shift+Enter adds a line, Esc stops a running turn, Up and Down recall history only on the first or last line. Enter is stopped from reaching the window's Cmd/Ctrl+Enter shortcut.
- One `autocompletion()` holds all sources (`#` references, `/` commands, optional `{{variable}}`). CodeMirror cannot merge two different `override` lists.
- jsdom cannot lay out CodeMirror. Keep logic in the pure modules (`prompt-triggers`, `prompt-history-nav`, `prompt-completions`) and test it there.

## Chips and resources

- The focus chip names the open request and is added by default. `#` adds request, folder, collection, environment and last-response chips. At most 8 chips per message.
- Chip text is built and masked by the backend (`build_assistant_chip_resource`, `mask_assistant_response`), capped at 8 KB per chip. `tryChipResource` never rejects and reports `{ ok: false }` for a chip that cannot load; `Composer` then refuses to send and keeps the draft. `chipToResource` turns the same failure into a short placeholder.
- A response chip fails closed: when the collection has vault bindings that cannot be resolved (or take over 8 s), the backend refuses it with "could not resolve vault secrets to mask this response; try again", so the message is not sent. The environment is sent only for a tab of the active collection, and the backend resolves every environment of the collection anyway.
- URIs: `rocket://<kind>/<collection>/<path>`.
- Chip text is masked in the backend by name, shape and known secret values. A value that exists only at run time in the frontend, such as a script `setVar`, cannot be recognised (see `crates/rocket-app/CLAUDE.md`).

## Storage

- Prompt history: `rocket-api:assistant-prompt-history:<workspaceId>`, last 50. History is kept on a workspace switch and cleared only when that workspace is deleted.
- Remembered model: `rocket-api:assistant-model:<agentConfigId>`, applied when the next session starts.
- Every access is in try/catch. Broken storage only loses the convenience.

## Toolbar

- Mode (Ask, Edit, Agent) calls `setAssistantMode` and then the store's `setMode`. A one-line summary of the mode shows under the pickers. A change applies from the next tool call.
- Model and Effort come from the session's config options with ids `model` and `effort`, and a picker is hidden when its option is absent. `setAgentConfigOption` returns the full list, which replaces the store's list.
- Pickers are disabled with a title explaining why (no session, or a reply is running).
- Send becomes Stop while a turn runs. Stop right after Send may do nothing until the reply starts.
- The context indicator shows `used / size` as a percentage, with tokens and cost on hover. It is hidden when the size is 0.
