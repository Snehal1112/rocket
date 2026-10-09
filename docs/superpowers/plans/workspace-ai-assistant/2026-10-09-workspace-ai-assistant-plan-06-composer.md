# Workspace AI Assistant — Plan 06: Composer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Plan 05 input stub with a Copilot-Chat-style composer: a growing CodeMirror 6 `PromptEditor`, context chips with `#` references and `/` templates, Mode, Model and Effort pickers, a context-used indicator, prompt history, and a Send button that turns into Stop.

**Architecture:** The pure logic (trigger matching, history recall, chip limits, reference lists, masking and size caps, usage text, storage) lives in small modules with thorough unit tests, because jsdom cannot lay out CodeMirror. `PromptEditor` reuses the `SingleLineEditor` pieces (theme, tooltip base, tooltips on `document.body`, controlled-value sync) without `singleLineFilter`. `Composer` turns chips into masked text resources and hands them, with the text, to Plan 05's `sendAssistantMessage(text, resources)` (this plan adds the `resources` argument), so the store's double-send guard and failure path stay in one place. The toolbar is presentational and receives callbacks.

**Tech Stack:** React 19, TypeScript (ES2020 lib), Zustand 5, `@tanstack/react-query` 5, CodeMirror 6 (`@codemirror/autocomplete` 6.20.1, `commands` 6.10.3, `state` 6.6.0, `view` 6.41.0, all already dependencies), shadcn/ui (`Badge`, `Button`, `DropdownMenu`, `Tooltip`), `lucide-react`, `sonner`, Vitest + Testing Library (jsdom). No new dependency.

**Spec:** `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`: section 5 "Composer (the prompt area)", decisions 6 to 8, section 8 (Vitest list). Index: `docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md` ("Plan 06 — composer").

## Verified facts

Checked by reading the code in this worktree on 2026-10-09:

- `src/components/editor/SingleLineEditor.tsx:109` declares `isSyncingRef`. `:128-214` builds the extension list in a `useMemo` with a `biome-ignore lint/correctness/useExhaustiveDependencies` comment and boolean deps. `:131` puts `singleLineFilter` first. `:138` uses `tooltips({ parent: document.body })`. `:145` forwards `aria-label` with `EditorView.contentAttributes`. `:152-167` binds Enter with `Prec.high(keymap.of(...))`. `:222-250` creates the view in an effect keyed on `extensions` and seeds the variable context. `:252-264` syncs `value` into the editor with `isSyncingRef`.
- `src/components/editor/extensions/variable-autocomplete.ts:66` uses `context.matchBefore(/\{\{[$\w.-]*/)`. `:94` returns `from: before.from + 2`. `:106-112` wraps it in `autocompletion({ override: [...], activateOnTyping: true, icons: false })`.
- `src/components/editor/extensions/index.ts:3` exports `rocketTheme`, `rocketThemeDark`, `rocketTooltipBase`. `:5` exports `variableCompletionSource`. `:6` exports `setVariableContextEffect`, `variableContextField`. `:7` exports `variableHighlight`.
- `src/components/editor/extensions/theme.ts:22-45`: `rocketTheme` sets `.cm-scroller { overflow: hidden; align-items: center !important }` and `.cm-content { min-height: auto; padding: 0 }`, and `'&' { height: 100% }` (`:13-18`). `extensions/__tests__/theme.test.ts:39-56` checks the cascade by reading the mounted `<style>` text.
- `node_modules/@codemirror/autocomplete/dist/index.js:2058-2069`: the completion keymap (Enter, Escape, ArrowUp, ArrowDown) runs at `Prec.highest`, so an open list takes those keys first. `:376-401`: the config combiner has no merge rule for `override`. Two `autocompletion()` calls with different `override` values conflict, so the editor uses one `autocompletion()` with all sources. `completionStatus` is exported.
- `node_modules/@codemirror/commands/dist/index.js:1731` binds Enter and Shift-Enter to `insertNewlineAndIndent`. `:1775` binds Mod-Enter to `insertBlankLine`. `insertNewline`, `history` and `historyKeymap` are exported.
- `node_modules/@codemirror/view/dist/index.js:8211` sets `aria-multiline="true"` on the content element by default. `:8998` runs keymaps as a `Prec.default` `domEventHandlers` keydown handler, so a `Prec.highest` handler runs first.
- `src/hooks/useKeyboardShortcuts.ts:21-23` returns unless Ctrl or Meta is held. `:30-46` sends the active request tab on Cmd/Ctrl+Enter. `:141` registers it with `window.addEventListener('keydown', handler)` in the bubble phase, so `stopPropagation()` inside the editor keeps it from firing.
- `src/components/editor/__tests__/SingleLineEditor.submit.test.tsx:20-29` drives CodeMirror keymaps in jsdom by dispatching `keydown` on `.cm-content`.
- `src/lib/queries/collection-queries.ts:8` `useCollections()` and `src/lib/queries/environment-queries.ts:16,24` `environmentKeys`, `useEnvironments()`. There is no query hook for collection trees. The sidebar loads trees with `getCollectionSummaries` (`src/components/collections/CollectionNode.tsx:155-159`).
- Tree paths: `CollectionNode.tsx:589-597` uses `item.dirName ?? item.name` as a top-level folder path and `:634` uses `item.fileName ?? item.name` as a top-level request path. `FolderNode.tsx:381-382,419-420` joins nested paths with `/`. Only `request` and `summary` items are shown as requests (`FolderNode.tsx:412-418` and `CollectionNode.tsx:619-625` skip GraphQL, WebSocket, gRPC and opaque items).
- `src/lib/tauri-api.ts:866` `listCollections`, `:870` `getCollectionSummaries(name): Promise<Collection>`, `:873` `getRequest(collection, path): Promise<Request>`, `:952` `getCollectionSettings(name)`, `:962` `listEnvironments(collection)`, `:965` `getEnvironment(collection, name)`, `:1561` `getFolderSettings(collection, folderPath)`. `:2522` is today's `sendAgentPrompt(sessionId, prompt)`, which Plan 01 extends with `resources?`.
- Types in `src/lib/tauri-api.ts`: `Auth` union with `authType` (`:48-54`), `CollectionSettings` (`:86-102`, has `agentAutonomyEnabled`), `FolderSettings` (`:105-116`), `CollectionVariable { key, value, initialValue, enabled, secret }` (`:73-79`), `Request` (`:166-185`, scripts are `preRequestScript`, `postResponseScript`, `tests`), `Folder` (`:261-267`), `RequestSummary` (`:270-278`, `kind?`), `CollectionItem` (`:330-338`), `Collection { name, root, settings }` (`:340-344`), `Variable { key, value, enabled, secret }` (`:346-351`), `Environment` (`:434-443`), `QueryParam` (`:531-535`), `TestResult { name, status, error }` (`:514-518`).
- `src/types/pane-types.ts:21-26` `BaseTab { id, title, isDirty, source?: { collection, path } }`. `:43-48` `RequestTab { request: RequestState; response: ResponseState | null }`. `:298` `isRequestTab`. `:322-345` `RequestState` with `preRequestScript`, `postResponseScript`, `testsScript` (the editor writes unsaved edits here). `:416-420` `KeyValueEntry`. `:427-433` `BodyState`. `:454` `AuthState.authType`. `:534-550` `ResponseState`.
- `src/lib/pane-utils.ts:85` `createDefaultRequest()`, `:209` `createDefaultLeaf(groupId?)`, `:332` `collectAllTabs(node)`. `src/stores/pane-store.ts:282` `PaneState.root`, `:415` `usePaneStore`. `src/stores/workspace-store.ts:4` `activeWorkspaceId`.
- Masking helpers: `src/lib/flow-export.ts:13` `REDACTED = '<redacted>'`, `:53` `redactKnownSecrets(text)` (header lines, Bearer and Basic tokens, URL passwords, JSON pairs and `name=value` pairs whose name holds a credential, keeps `{{var}}`). `src/lib/flow-secrets.ts:7` `isVariableReference(value)`. `src/lib/sensitive-headers.ts:15` `isSensitiveHeader(name)`.
- `src/lib/execute-request.ts:668-676` fills `ResponseState.headers` from the backend's header list, `enabled` included.
- UI primitives present in `src/components/ui/`: `badge.tsx` (variants include `secondary`), `button.tsx` (sizes `sm`, `icon`), `dropdown-menu.tsx` (exports `DropdownMenuRadioGroup`, `DropdownMenuRadioItem`, `DropdownMenuLabel`, `:182-197`), `tooltip.tsx` (no app-wide provider; components wrap their own `TooltipProvider`, for example `src/components/contract/ContractBadge.tsx:55`). There is no `command.tsx` or `toggle-group.tsx`, and `cmdk` is not in `package.json`.
- `lucide-react` exports `Crosshair`, `FileText`, `Folder`, `Globe`, `Inbox`, `Layers`, `Send`, `Square`, `X`, `ChevronDown` and the `LucideIcon` type.
- `tsconfig.json`: `lib` is ES2020 (no `Array.prototype.at`, `findLast`, `String.prototype.replaceAll`), `include: ["src"]` so test files are type-checked, `noUnusedParameters: true`.
- `biome.json`: test files are excluded from lint and format. `a11y/useAriaPropsSupportedByRole` is an error (no `aria-label` on a plain `div` or `span`). JSX uses single quotes.
- `vite.config.ts:7-11`: jsdom, `globals: true` (Testing Library cleans up automatically), setup file `src/test-setup.ts`. `src/test/deferred.ts` exports `createDeferred<T>()`. `src/components/git/__tests__/git-failure-contracts.test.tsx:24-28` shows the partial `vi.mock('@/lib/tauri-api', async (importOriginal) => ...)` pattern.
- `localStorage` keys use the `rocket-api:` prefix (`src/stores/env-store.ts:17`). `localStorage` access in `src/` today has no try/catch, so this plan adds its own guarded helpers.
- The `PromptEditor` CodeMirror exception is already recorded: `.claude/rules/frontend-component-guardrails.md:12` and `CLAUDE.md:95`. `.claude/frontend.md`, referenced by `CLAUDE.md:60`, does not exist.

**Not verifiable today (written by Plans 01, 03 and 05 in parallel):** `src/stores/assistant-store.ts`, `src/components/assistant/AssistantPanel.tsx`, `src/components/assistant/AssistantInputStub.tsx`, and the new `src/lib/tauri-api.ts` exports. Task 1 Step 1 checks the names this plan consumes before any code is written.

## Global Constraints

- Frontend only. No Rust, no new Tauri command, no new npm dependency.
- CodeMirror for multi-line text only in `src/components/assistant/composer/PromptEditor.tsx`.
- shadcn/ui primitives only outside the editor: chips are `Badge` + `Button`, pickers are `DropdownMenu` radio groups, the usage hover is `Tooltip`. No raw `<button>`, `<input>`, `<select>`, `<form>`, `<textarea>`.
- Icons from `lucide-react` only.
- Zustand: one narrow selector per value or action. No destructuring of a whole store.
- Chip limits: at most 8 chips per message (focus chip included), at most 8192 UTF-8 bytes of text per chip, truncation marker included.
- Prompt history: last 50 prompts per workspace, key `rocket-api:assistant-prompt-history:<workspaceId>`, every `localStorage` access in try/catch.
- Remembered model: key `rocket-api:assistant-model:<agentConfigId>`, every access in try/catch.
- Config option ids: `model` and `effort`. A picker is hidden when its option is absent or has no choices.
- Resource URIs: `rocket://<kind>/<collection>/<path segments>`, each segment URI-encoded, `mimeType: 'text/plain'`.
- The chip text never contains a secret variable's value, a literal credential in an auth field, a sensitive header value or a `name=value` credential. `{{variable}}` references are kept.
- Enter sends, Shift+Enter inserts a newline, Esc stops a running turn, Up and Down recall history only on the first or last line, Enter never reaches the window's Cmd/Ctrl+Enter shortcut.
- Agent-run verification is only `yarn tsc --noEmit` and `yarn check`. Test commands are listed for the user to run. Never run `cargo test --workspace`.
- Commits: stage explicit paths only (never `git add -A` or `.`), then use the `dev-workflow-skills:1-git-commit` skill with the conventional-commit message given in the step.

## Review Focus

- **Double send.** Enter while a turn runs, or two Enters before React re-renders, must start exactly one `sendAgentPrompt`. Tests: Task 1 "ignores Enter while a turn runs"; Task 3 "sends once when Enter is pressed twice quickly" (`Composer` guards with a ref, not only state).
- **Cmd/Ctrl+Enter leaking to the global shortcut.** A Ctrl+Enter or Meta+Enter typed in the prompt must not reach `useKeyboardShortcuts`, which would also send the active HTTP request. Test: Task 1 "keeps Cmd/Ctrl+Enter away from the window shortcut".
- **Credentials inside chip text.** A Bearer token in a header and in the auth, an `api_key=` query value, a secret environment variable, a `Set-Cookie` response header and a backend error message that echoes a token must all be absent from the resource text, while `{{variable}}` references stay. Tests: Task 2 `chip-resources.test.ts` cases "masks literal credentials of a saved request", "shares environment names but not secret values", "masks response cookies", "does not echo a load error".
- **History recall away from the edges.** In a multi-line draft, Up on line 2 must move the cursor and not replace the draft, and typing after a recall must start a new draft. Tests: Task 1 `atRecallEdge` cases and "starts a new draft after the user types".
- **Broken storage.** `localStorage` that throws (private mode, blocked site data) or holds junk (bad JSON, a non-array, non-string entries) must not throw from the composer. Tests: Task 2 `prompt-history.test.ts` "survives storage that throws" and "ignores junk"; Task 3 `model-memory.test.ts` "survives storage that throws".

---

## Task 1: `PromptEditor` and its pure logic

**Files:**
- Create: `src/lib/assistant/types.ts`
- Create: `src/components/assistant/composer/prompt-triggers.ts`
- Create: `src/components/assistant/composer/prompt-history-nav.ts`
- Create: `src/components/assistant/composer/prompt-completions.ts`
- Create: `src/components/assistant/composer/prompt-editor-theme.ts`
- Create: `src/components/assistant/composer/PromptEditor.tsx`
- Test: `src/components/assistant/composer/__tests__/prompt-triggers.test.ts`
- Test: `src/components/assistant/composer/__tests__/prompt-history-nav.test.ts`
- Test: `src/components/assistant/composer/__tests__/prompt-completions.test.ts`
- Test: `src/components/assistant/composer/__tests__/PromptEditor.test.tsx`

**Interfaces:**
- Consumes: `rocketTheme`, `rocketThemeDark`, `rocketTooltipBase`, `setVariableContextEffect`, `variableCompletionSource`, `variableContextField`, `variableHighlight` from `@/components/editor/extensions` (index `:3-7`); `VariableScopeEntry` from `@/lib/url-variables`; `cn` from `@/lib/utils`.
- Produces (index contract, unchanged):
  - `src/lib/assistant/types.ts`: `type ReferenceKind = 'request' | 'folder' | 'collection' | 'environment' | 'last-response'`; `interface ReferenceItem { kind: ReferenceKind; collection: string; path?: string; label: string }`; `interface SlashCommandItem { name: string; description: string; template: string }`.
  - `PromptEditor.tsx`: `interface PromptEditorProps` exactly as in the index, `function PromptEditor(props: PromptEditorProps): JSX.Element`, and re-exports `ReferenceItem`, `SlashCommandItem`.
  - `prompt-triggers.ts`: `matchTrigger(textBefore: string, lineFrom: number): TriggerMatch | null`, `interface TriggerMatch { kind: 'reference' | 'command'; from: number; query: string }`.
  - `prompt-history-nav.ts`: `interface HistoryCursor { index: number | null; draft: string }`, `IDLE_HISTORY_CURSOR`, `type HistoryDirection = 'up' | 'down'`, `stepHistory(history, cursor, current, direction): { cursor: HistoryCursor; text: string } | null`, `atRecallEdge(state: EditorState, direction): boolean`.
  - `prompt-completions.ts`: `interface Latest<T> { readonly current: T }`, `referenceCompletions(source, onPicked): CompletionSource`, `commandCompletions(source): CompletionSource`.
  - `prompt-editor-theme.ts`: `PROMPT_MAX_LINES = 12`, `PROMPT_LINE_HEIGHT = 1.5`, `promptEditorTheme`.

- [ ] **Step 1: Confirm the names this plan consumes from Plans 01, 03 and 05**

Run:

```bash
grep -n "export const sendAgentPrompt\|export const cancelAgentPrompt\|export const setAgentConfigOption\|export const setAssistantMode\|export const startWorkspaceAssistant\|export interface ConfigOption\|export interface ConfigChoice\|export interface PromptResourceDto" src/lib/tauri-api.ts
grep -n "export const useAssistantStore\|export function selectTurnRunning\|setConfigOptions\|appendUserMessage\|setMode\|focus?:\|usage?:\|configOptions\|mode:" src/stores/assistant-store.ts
grep -n "export async function sendAssistantMessage\|export async function startAssistant\|startWorkspaceAssistant(" src/lib/assistant/assistant-session.ts
grep -rn "AssistantInputStub" src
```

Expected: `sendAgentPrompt(sessionId, prompt, resources?)`, `cancelAgentPrompt(sessionId)`, `setAgentConfigOption(sessionId, configId, value)` returning `Promise<ConfigOption[]>`, `setAssistantMode(sessionId, mode)`, `startWorkspaceAssistant(agentConfigId, mode, model?)`, the `ConfigOption` type with `id`, `name`, `category`, `currentValue`, `choices: { value, name, description }[]`, the `PromptResourceDto` type with `uri`, `mimeType`, `text`; the store hook `useAssistantStore` with `session.configOptions`, `session.mode`, `session.agentConfigId`, `usage`, `focus`, `setConfigOptions(sessionId, options)` (session id first; it drops options for another session), `appendUserMessage(text): boolean` (it also opens the streaming reply and returns `false` while a turn runs), and `selectTurnRunning(state)`; in `src/lib/assistant/assistant-session.ts`, `sendAssistantMessage(text)`, `startAssistant(agentConfigId, mode?)` and the only `startWorkspaceAssistant(agentConfigId, mode)` call; and the stub file used by `AssistantPanel.tsx`.

If a name differs (for example the DTO type is called `PromptResource`), use the real name at every call site in this plan. Do not rename the other plan's export. Record each difference as a `Ruling:` line in `.superpowers/sdd/workspace-ai-assistant-plan-06/progress.md`. `setMode` may be missing; Task 3 adds it.

- [ ] **Step 2: Write the failing tests for the pure modules**

Create `src/components/assistant/composer/__tests__/prompt-triggers.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { matchTrigger } from '../prompt-triggers';

describe('matchTrigger', () => {
  it('matches a bare # at the start of the prompt', () => {
    expect(matchTrigger('#', 0)).toEqual({ kind: 'reference', from: 0, query: '' });
  });

  it('matches a # after a space and reports where it starts', () => {
    expect(matchTrigger('see #ord', 0)).toEqual({ kind: 'reference', from: 4, query: 'ord' });
  });

  it('adds the line offset on a later line', () => {
    expect(matchTrigger('#env', 6)).toEqual({ kind: 'reference', from: 6, query: 'env' });
  });

  it('ignores a # inside a word', () => {
    expect(matchTrigger('issue#12', 0)).toBeNull();
  });

  it('ignores a finished reference followed by a space', () => {
    expect(matchTrigger('#orders now', 0)).toBeNull();
  });

  it('matches a / command at the very start of the prompt', () => {
    expect(matchTrigger('/ex', 0)).toEqual({ kind: 'command', from: 0, query: 'ex' });
  });

  it('ignores a / command on a later line', () => {
    expect(matchTrigger('/ex', 5)).toBeNull();
  });

  it('ignores a / after other text', () => {
    expect(matchTrigger('say /ex', 0)).toBeNull();
  });

  it('ignores a / command once a space follows it', () => {
    expect(matchTrigger('/tests now', 0)).toBeNull();
  });
});
```

Create `src/components/assistant/composer/__tests__/prompt-history-nav.test.ts`:

```ts
import { EditorSelection, EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { atRecallEdge, IDLE_HISTORY_CURSOR, stepHistory } from '../prompt-history-nav';

const HISTORY = ['first', 'second', 'third'];

describe('stepHistory', () => {
  it('returns null on Up with an empty history', () => {
    expect(stepHistory([], IDLE_HISTORY_CURSOR, 'draft', 'up')).toBeNull();
  });

  it('returns null on Down while the user edits the draft', () => {
    expect(stepHistory(HISTORY, IDLE_HISTORY_CURSOR, 'draft', 'down')).toBeNull();
  });

  it('keeps the draft and shows the newest prompt on the first Up', () => {
    expect(stepHistory(HISTORY, IDLE_HISTORY_CURSOR, 'draft', 'up')).toEqual({
      cursor: { index: 2, draft: 'draft' },
      text: 'third',
    });
  });

  it('walks to older prompts and stops at the oldest', () => {
    const second = stepHistory(HISTORY, { index: 2, draft: 'd' }, 'third', 'up');
    expect(second).toEqual({ cursor: { index: 1, draft: 'd' }, text: 'second' });
    expect(stepHistory(HISTORY, { index: 0, draft: 'd' }, 'first', 'up')).toBeNull();
  });

  it('walks forward and brings the draft back after the newest prompt', () => {
    expect(stepHistory(HISTORY, { index: 1, draft: 'd' }, 'second', 'down')).toEqual({
      cursor: { index: 2, draft: 'd' },
      text: 'third',
    });
    expect(stepHistory(HISTORY, { index: 2, draft: 'd' }, 'third', 'down')).toEqual({
      cursor: IDLE_HISTORY_CURSOR,
      text: 'd',
    });
  });

  it('clamps an index left over from a longer history', () => {
    expect(stepHistory(['only'], { index: 5, draft: 'd' }, 'x', 'up')).toEqual({
      cursor: { index: 0, draft: 'd' },
      text: 'only',
    });
  });
});

describe('atRecallEdge', () => {
  const at = (doc: string, anchor: number, head = anchor) =>
    EditorState.create({ doc, selection: EditorSelection.single(anchor, head) });

  it('allows Up only on the first line', () => {
    expect(atRecallEdge(at('one\ntwo', 0), 'up')).toBe(true);
    expect(atRecallEdge(at('one\ntwo', 7), 'up')).toBe(false);
  });

  it('allows Down only on the last line', () => {
    expect(atRecallEdge(at('one\ntwo', 7), 'down')).toBe(true);
    expect(atRecallEdge(at('one\ntwo', 0), 'down')).toBe(false);
  });

  it('allows both on a single line', () => {
    expect(atRecallEdge(at('one', 1), 'up')).toBe(true);
    expect(atRecallEdge(at('one', 1), 'down')).toBe(true);
  });

  it('refuses while text is selected', () => {
    expect(atRecallEdge(at('one', 0, 3), 'up')).toBe(false);
  });
});
```

Create `src/components/assistant/composer/__tests__/prompt-completions.test.ts`:

```ts
import {
  type Completion,
  CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';
import { commandCompletions, referenceCompletions } from '../prompt-completions';

const ORDERS: ReferenceItem = {
  kind: 'request',
  collection: 'shop',
  path: 'orders/list.yml',
  label: 'GET List orders',
};
const EXPLAIN: SlashCommandItem = {
  name: 'explain',
  description: 'Explain a request',
  template: 'Explain this request: ',
};

type Apply = (view: EditorView, completion: Completion, from: number, to: number) => void;

let view: EditorView | null = null;

afterEach(() => {
  const parent = view?.dom.parentElement;
  view?.destroy();
  parent?.remove();
  view = null;
});

function makeView(doc: string): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  view = new EditorView({ state: EditorState.create({ doc }), parent });
  return view;
}

function complete(source: CompletionSource, v: EditorView): CompletionResult | null {
  const result = source(new CompletionContext(v.state, v.state.doc.length, false));
  if (result instanceof Promise) throw new Error('sync result expected');
  return result;
}

function applyFirst(result: CompletionResult, v: EditorView) {
  const option = result.options[0];
  if (typeof option.apply !== 'function') throw new Error('apply missing');
  (option.apply as Apply)(v, option, result.from, v.state.doc.length);
}

describe('referenceCompletions', () => {
  it('lists the source items for the typed query, starting at the #', () => {
    const source = vi.fn(() => [ORDERS]);
    const v = makeView('see #ord');
    const result = complete(referenceCompletions({ current: source }, { current: vi.fn() }), v);
    expect(source).toHaveBeenCalledWith('ord');
    expect(result?.from).toBe(4);
    expect(result?.filter).toBe(false);
    expect(result?.options.map((o) => o.label)).toEqual(['GET List orders']);
  });

  it('removes the typed #query and reports the picked item', () => {
    const onPicked = vi.fn();
    const v = makeView('see #ord');
    const result = complete(
      referenceCompletions({ current: () => [ORDERS] }, { current: onPicked }),
      v,
    );
    if (!result) throw new Error('result expected');
    applyFirst(result, v);
    expect(v.state.doc.toString()).toBe('see ');
    expect(onPicked).toHaveBeenCalledWith(ORDERS);
  });

  it('does not open for a # inside a word', () => {
    const source = vi.fn(() => [ORDERS]);
    const v = makeView('issue#12');
    expect(complete(referenceCompletions({ current: source }, { current: vi.fn() }), v)).toBeNull();
    expect(source).not.toHaveBeenCalled();
  });

  it('does not open when nothing matches', () => {
    const v = makeView('#zzz');
    expect(
      complete(referenceCompletions({ current: () => [] }, { current: vi.fn() }), v),
    ).toBeNull();
  });
});

describe('commandCompletions', () => {
  it('lists commands for a / at the start and inserts the template', () => {
    const source = vi.fn(() => [EXPLAIN]);
    const v = makeView('/ex');
    const result = complete(commandCompletions({ current: source }), v);
    expect(source).toHaveBeenCalledWith('ex');
    expect(result?.from).toBe(0);
    expect(result?.options[0].label).toBe('/explain');
    if (!result) throw new Error('result expected');
    applyFirst(result, v);
    expect(v.state.doc.toString()).toBe('Explain this request: ');
    expect(v.state.selection.main.head).toBe('Explain this request: '.length);
  });

  it('does not open for a / after other text', () => {
    const v = makeView('hi /ex');
    expect(complete(commandCompletions({ current: () => [EXPLAIN] }), v)).toBeNull();
  });
});
```

- [ ] **Step 3: Write the failing `PromptEditor` component test**

Create `src/components/assistant/composer/__tests__/PromptEditor.test.tsx`:

```tsx
import { EditorView } from '@codemirror/view';
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PromptEditor, type PromptEditorProps } from '../PromptEditor';

const KEY_CODES: Record<string, number> = { Enter: 13, Escape: 27, ArrowUp: 38, ArrowDown: 40 };

function press(target: HTMLElement, key: string, init: KeyboardEventInit = {}) {
  target.dispatchEvent(
    new KeyboardEvent('keydown', {
      key,
      code: key,
      keyCode: KEY_CODES[key],
      bubbles: true,
      cancelable: true,
      ...init,
    }),
  );
}

function setup(overrides: Partial<PromptEditorProps> = {}) {
  const props: PromptEditorProps = {
    value: '',
    onChange: vi.fn(),
    onSubmit: vi.fn(),
    onStop: vi.fn(),
    running: false,
    history: [],
    onHistoryCommit: vi.fn(),
    referenceSource: () => [],
    commandSource: () => [],
    onReferencePicked: vi.fn(),
    'aria-label': 'Prompt',
    ...overrides,
  };
  const utils = render(<PromptEditor {...props} />);
  const content = utils.container.querySelector('.cm-content') as HTMLElement;
  const view = EditorView.findFromDOM(
    utils.container.querySelector('.cm-editor') as HTMLElement,
  ) as EditorView;
  return { ...utils, props, content, view };
}

describe('PromptEditor keys', () => {
  it('sends on Enter and commits the prompt to history', () => {
    const { content, props, view } = setup({ value: 'hello' });
    press(content, 'Enter');
    expect(props.onHistoryCommit).toHaveBeenCalledWith('hello');
    expect(props.onSubmit).toHaveBeenCalledTimes(1);
    expect(view.state.doc.toString()).toBe('hello');
  });

  it('ignores Enter while a turn runs', () => {
    const { content, props } = setup({ value: 'hello', running: true });
    press(content, 'Enter');
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(props.onHistoryCommit).not.toHaveBeenCalled();
  });

  it('ignores Enter on a blank prompt', () => {
    const { content, props } = setup({ value: '   ' });
    press(content, 'Enter');
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it('inserts a newline on Shift+Enter', () => {
    const { content, props, view } = setup({ value: 'a' });
    view.dispatch({ selection: { anchor: 1 } });
    press(content, 'Enter', { shiftKey: true });
    expect(view.state.doc.toString()).toBe('a\n');
    expect(props.onChange).toHaveBeenCalledWith('a\n');
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it('stops a running turn on Escape', () => {
    const { content, props } = setup({ running: true });
    press(content, 'Escape');
    expect(props.onStop).toHaveBeenCalledTimes(1);
  });

  it('does not stop anything on Escape when idle', () => {
    const { content, props } = setup();
    press(content, 'Escape');
    expect(props.onStop).not.toHaveBeenCalled();
  });

  it('keeps Cmd/Ctrl+Enter away from the window shortcut', () => {
    const windowKeydown = vi.fn();
    window.addEventListener('keydown', windowKeydown);
    try {
      const { content, props } = setup({ value: 'hello' });
      press(content, 'Enter', { ctrlKey: true });
      press(content, 'Enter', { metaKey: true });
      press(content, 'Enter');
      expect(windowKeydown).not.toHaveBeenCalled();
      expect(props.onSubmit).toHaveBeenCalled();
    } finally {
      window.removeEventListener('keydown', windowKeydown);
    }
  });
});

describe('PromptEditor history', () => {
  it('walks back through history and returns to the draft', () => {
    const { content, view, props } = setup({ value: 'draft', history: ['first', 'second'] });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('second');
    expect(props.onChange).toHaveBeenLastCalledWith('second');
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('second');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('draft');
  });

  it('starts a new draft after the user types', () => {
    const { content, view } = setup({ history: ['first'] });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    view.dispatch({ changes: { from: 5, insert: '!' } });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('first!');
  });
});

describe('PromptEditor setup', () => {
  it('labels the editor as a multi-line textbox', () => {
    const { content } = setup();
    expect(content.getAttribute('aria-label')).toBe('Prompt');
    expect(content.getAttribute('aria-multiline')).toBe('true');
  });

  it('applies a new value prop without echoing it through onChange', () => {
    const { props, rerender, view } = setup({ value: 'old' });
    rerender(<PromptEditor {...props} value='next' />);
    expect(view.state.doc.toString()).toBe('next');
    expect(props.onChange).not.toHaveBeenCalled();
  });

  it('caps the height at twelve lines and lets the text start at the top', () => {
    setup();
    const css = Array.from(document.head.querySelectorAll('style'))
      .map((el) => el.textContent ?? '')
      .join('\n');
    expect(css).toContain('max-height: calc(18em + 16px)');
    expect(css.lastIndexOf('align-items: flex-start !important')).toBeGreaterThan(
      css.indexOf('align-items: center !important'),
    );
  });
});
```

- [ ] **Step 4: Run the tests to see them fail (for the user to run)**

```bash
yarn test src/components/assistant/composer
```

Expected: FAIL, the modules under test do not exist yet.

- [ ] **Step 5: Write the shared types**

Create `src/lib/assistant/types.ts`:

```ts
/** What a `#` reference points at. */
export type ReferenceKind = 'request' | 'folder' | 'collection' | 'environment' | 'last-response';

/**
 * One item of the `#` list, and the chip it becomes. `path` is the request or folder
 * path inside the collection, or the environment name when `kind` is `environment`.
 */
export interface ReferenceItem {
  kind: ReferenceKind;
  collection: string;
  path?: string;
  label: string;
}

/** One Rocket prompt template in the `/` list. */
export interface SlashCommandItem {
  name: string;
  description: string;
  template: string;
}
```

- [ ] **Step 6: Write the trigger matcher**

Create `src/components/assistant/composer/prompt-triggers.ts`:

```ts
/** A `#` reference or `/` command the user is typing at the cursor. */
export interface TriggerMatch {
  kind: 'reference' | 'command';
  /** Document offset of the `#` or `/` character. */
  from: number;
  /** Text typed after the trigger character. */
  query: string;
}

// A `#` at the start of the line or after whitespace, then text without spaces.
const REFERENCE = /(?:^|\s)#([^\s#]*)$/;
// A `/` at the very start of the prompt, then a command name.
const COMMAND = /^\/([\w-]*)$/;

/**
 * Finds the trigger being typed. `textBefore` is the line text up to the cursor and
 * `lineFrom` is the document offset where that line starts. A `/` command counts
 * only on the first line, at the start of the prompt.
 */
export function matchTrigger(textBefore: string, lineFrom: number): TriggerMatch | null {
  const reference = REFERENCE.exec(textBefore);
  if (reference) {
    const query = reference[1];
    return {
      kind: 'reference',
      from: lineFrom + textBefore.length - query.length - 1,
      query,
    };
  }
  if (lineFrom === 0) {
    const command = COMMAND.exec(textBefore);
    if (command) return { kind: 'command', from: 0, query: command[1] };
  }
  return null;
}
```

- [ ] **Step 7: Write the history recall logic**

Create `src/components/assistant/composer/prompt-history-nav.ts`:

```ts
import type { EditorState } from '@codemirror/state';

/** Where Up and Down recall stands. `index` is null while the user edits their own draft. */
export interface HistoryCursor {
  index: number | null;
  draft: string;
}

export type HistoryDirection = 'up' | 'down';

export const IDLE_HISTORY_CURSOR: HistoryCursor = { index: null, draft: '' };

/**
 * One Up or Down step through `history`, which is ordered oldest first. Returns the
 * new cursor and the text to show, or null when there is nothing in that direction.
 * The first Up keeps `current` as the draft, and Down past the newest prompt brings
 * the draft back.
 */
export function stepHistory(
  history: readonly string[],
  cursor: HistoryCursor,
  current: string,
  direction: HistoryDirection,
): { cursor: HistoryCursor; text: string } | null {
  if (direction === 'up') {
    if (history.length === 0) return null;
    if (cursor.index === null) {
      const index = history.length - 1;
      return { cursor: { index, draft: current }, text: history[index] };
    }
    if (cursor.index === 0) return null;
    // The history can shrink between steps, so the index is clamped.
    const index = Math.min(cursor.index - 1, history.length - 1);
    return { cursor: { index, draft: cursor.draft }, text: history[index] };
  }
  if (cursor.index === null) return null;
  if (cursor.index >= history.length - 1) {
    return { cursor: IDLE_HISTORY_CURSOR, text: cursor.draft };
  }
  const index = cursor.index + 1;
  return { cursor: { index, draft: cursor.draft }, text: history[index] };
}

/**
 * True when Up (first line) or Down (last line) should recall history instead of
 * moving the cursor. A selection always moves the cursor.
 */
export function atRecallEdge(state: EditorState, direction: HistoryDirection): boolean {
  const { main } = state.selection;
  if (!main.empty) return false;
  const line = state.doc.lineAt(main.head);
  return direction === 'up' ? line.number === 1 : line.number === state.doc.lines;
}
```

- [ ] **Step 8: Write the `#` and `/` completion sources**

Create `src/components/assistant/composer/prompt-completions.ts`:

```ts
import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import type { ReferenceItem, ReferenceKind, SlashCommandItem } from '@/lib/assistant/types';
import { matchTrigger, type TriggerMatch } from './prompt-triggers';

/** A value read at call time, such as a React ref, so the editor never holds a stale callback. */
export interface Latest<T> {
  readonly current: T;
}

const KIND_LABEL: Record<ReferenceKind, string> = {
  request: 'request',
  folder: 'folder',
  collection: 'collection',
  environment: 'environment',
  'last-response': 'response',
};

function triggerAt(context: CompletionContext): TriggerMatch | null {
  const line = context.state.doc.lineAt(context.pos);
  return matchTrigger(line.text.slice(0, context.pos - line.from), line.from);
}

/**
 * The `#` list. The source filters, so CodeMirror shows the items in the given order.
 * Picking an item removes the typed `#query` text and hands the item to `onPicked`,
 * which turns it into a chip.
 */
export function referenceCompletions(
  source: Latest<(query: string) => ReferenceItem[]>,
  onPicked: Latest<(item: ReferenceItem) => void>,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const match = triggerAt(context);
    if (!match || match.kind !== 'reference') return null;
    const items = source.current(match.query);
    if (items.length === 0) return null;
    const options: Completion[] = items.map((item) => ({
      label: item.label,
      detail:
        item.kind === 'collection'
          ? KIND_LABEL[item.kind]
          : `${KIND_LABEL[item.kind]} · ${item.collection}`,
      type: 'reference',
      apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
        view.dispatch({ changes: { from, to, insert: '' }, selection: { anchor: from } });
        onPicked.current(item);
      },
    }));
    return { from: match.from, options, filter: false };
  };
}

/** The `/` list. Picking a command replaces the typed `/query` with its template. */
export function commandCompletions(
  source: Latest<(query: string) => SlashCommandItem[]>,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const match = triggerAt(context);
    if (!match || match.kind !== 'command') return null;
    const commands = source.current(match.query);
    if (commands.length === 0) return null;
    const options: Completion[] = commands.map((command) => ({
      label: `/${command.name}`,
      detail: command.description,
      type: 'command',
      apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
        view.dispatch({
          changes: { from, to, insert: command.template },
          selection: { anchor: from + command.template.length },
        });
      },
    }));
    return { from: match.from, options, filter: false };
  };
}
```

- [ ] **Step 9: Write the prompt theme**

Create `src/components/assistant/composer/prompt-editor-theme.ts`:

```ts
import { EditorView } from '@codemirror/view';

/** Lines the prompt grows to before it scrolls. */
export const PROMPT_MAX_LINES = 12;
export const PROMPT_LINE_HEIGHT = 1.5;

/**
 * Turns the single-line `rocketTheme` into a growing prose box. It must load with a
 * higher precedence than `rocketTheme`, so its rules come later in the cascade:
 * height follows the content up to the cap, the text starts at the top instead of
 * the vertical center, and the scroller scrolls instead of clipping.
 */
export const promptEditorTheme = EditorView.theme({
  '&': {
    height: 'auto',
    maxHeight: `calc(${PROMPT_MAX_LINES * PROMPT_LINE_HEIGHT}em + 16px)`,
    fontFamily: 'inherit',
    fontSize: '13px',
  },
  '.cm-scroller': {
    overflow: 'auto',
    alignItems: 'flex-start !important',
    lineHeight: String(PROMPT_LINE_HEIGHT),
    fontFamily: 'inherit',
  },
  '.cm-content': {
    padding: '8px 0',
    minHeight: `${PROMPT_LINE_HEIGHT}em`,
  },
  '.cm-line': {
    padding: '0 10px',
  },
});
```

- [ ] **Step 10: Write `PromptEditor`**

Create `src/components/assistant/composer/PromptEditor.tsx`:

```tsx
import { autocompletion, completionStatus } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, insertNewline } from '@codemirror/commands';
import { Annotation, EditorState, type Extension, Prec } from '@codemirror/state';
import { placeholder as cmPlaceholder, EditorView, keymap, tooltips } from '@codemirror/view';
import { useEffect, useMemo, useRef } from 'react';
import {
  rocketTheme,
  rocketThemeDark,
  rocketTooltipBase,
  setVariableContextEffect,
  variableCompletionSource,
  variableContextField,
  variableHighlight,
} from '@/components/editor/extensions';
import type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { commandCompletions, referenceCompletions } from './prompt-completions';
import { promptEditorTheme } from './prompt-editor-theme';
import {
  atRecallEdge,
  type HistoryCursor,
  type HistoryDirection,
  IDLE_HISTORY_CURSOR,
  stepHistory,
} from './prompt-history-nav';

export type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';

export interface PromptEditorProps {
  value: string;
  onChange(v: string): void;
  onSubmit(): void;
  onStop(): void;
  running: boolean;
  placeholder?: string;
  disabled?: boolean;
  history: string[];
  onHistoryCommit(v: string): void;
  referenceSource: (query: string) => ReferenceItem[];
  commandSource: (query: string) => SlashCommandItem[];
  onReferencePicked(item: ReferenceItem): void;
  variableContext?: Map<string, VariableScopeEntry>;
  'aria-label': string;
}

// Marks the change a history recall makes, so it does not reset the recall cursor.
const historyRecall = Annotation.define<boolean>();

/**
 * The AI Assistant prompt box: CodeMirror 6, multi-line, growing to a maximum height.
 * This is the one approved CodeMirror exception for multi-line text (see
 * `.claude/rules/frontend-component-guardrails.md`). Do not reuse it elsewhere.
 */
export function PromptEditor({
  value,
  onChange,
  onSubmit,
  onStop,
  running,
  placeholder,
  disabled,
  history: promptHistory,
  onHistoryCommit,
  referenceSource,
  commandSource,
  onReferencePicked,
  variableContext,
  'aria-label': ariaLabel,
}: PromptEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // True while props.value is pushed into the editor, so onChange does not echo it back.
  const isSyncingRef = useRef(false);
  const historyCursorRef = useRef<HistoryCursor>(IDLE_HISTORY_CURSOR);

  // The extensions are built once, so they read the latest props through refs.
  const propsRef = useRef({ onChange, onSubmit, onStop, running, promptHistory, onHistoryCommit });
  propsRef.current = { onChange, onSubmit, onStop, running, promptHistory, onHistoryCommit };
  const referenceSourceRef = useRef(referenceSource);
  referenceSourceRef.current = referenceSource;
  const commandSourceRef = useRef(commandSource);
  commandSourceRef.current = commandSource;
  const onReferencePickedRef = useRef(onReferencePicked);
  onReferencePickedRef.current = onReferencePicked;
  const variableContextRef = useRef(variableContext);
  variableContextRef.current = variableContext;

  // biome-ignore lint/correctness/useExhaustiveDependencies: extensions rebuild only when presence toggles, not on identity change.
  const extensions = useMemo(() => {
    const submit = (view: EditorView): boolean => {
      const props = propsRef.current;
      // A running turn ignores Enter, so one turn never queues a second prompt.
      if (props.running) return true;
      const text = view.state.doc.toString();
      if (text.trim() === '') return true;
      historyCursorRef.current = IDLE_HISTORY_CURSOR;
      props.onHistoryCommit(text);
      props.onSubmit();
      return true;
    };

    const stop = (): boolean => {
      if (!propsRef.current.running) return false;
      propsRef.current.onStop();
      return true;
    };

    const recall =
      (direction: HistoryDirection) =>
      (view: EditorView): boolean => {
        if (completionStatus(view.state) !== null) return false;
        if (!atRecallEdge(view.state, direction)) return false;
        const step = stepHistory(
          propsRef.current.promptHistory,
          historyCursorRef.current,
          view.state.doc.toString(),
          direction,
        );
        if (!step) return false;
        historyCursorRef.current = step.cursor;
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: step.text },
          selection: { anchor: step.text.length },
          annotations: historyRecall.of(true),
        });
        return true;
      };

    const exts: Extension[] = [
      rocketTheme,
      rocketThemeDark,
      rocketTooltipBase,
      Prec.high(promptEditorTheme),
      EditorView.lineWrapping,
      history(),
      // Keep Enter inside the editor, so the window's Cmd/Ctrl+Enter shortcut does not
      // also send the active HTTP request.
      Prec.highest(
        EditorView.domEventHandlers({
          keydown(event) {
            if (event.key === 'Enter') event.stopPropagation();
            return false;
          },
        }),
      ),
      Prec.high(
        keymap.of([
          { key: 'Enter', run: submit },
          { key: 'Mod-Enter', run: submit },
          { key: 'Shift-Enter', run: insertNewline },
          { key: 'Escape', run: stop },
          { key: 'ArrowUp', run: recall('up') },
          { key: 'ArrowDown', run: recall('down') },
        ]),
      ),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      // Render the completion list at document root so it escapes the panel's overflow.
      tooltips({ parent: document.body }),
      // One autocompletion() only: CodeMirror cannot merge two different `override` lists.
      autocompletion({
        override: [
          referenceCompletions(referenceSourceRef, onReferencePickedRef),
          commandCompletions(commandSourceRef),
          ...(variableContext ? [variableCompletionSource] : []),
        ],
        activateOnTyping: true,
        icons: false,
      }),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || isSyncingRef.current) return;
        const recalled = update.transactions.some((tr) => tr.annotation(historyRecall));
        if (!recalled) historyCursorRef.current = IDLE_HISTORY_CURSOR;
        propsRef.current.onChange(update.state.doc.toString());
      }),
      EditorView.contentAttributes.of({ 'aria-label': ariaLabel, 'aria-multiline': 'true' }),
    ];

    if (placeholder) exts.push(cmPlaceholder(placeholder));
    if (variableContext) exts.push(variableContextField, variableHighlight());
    if (disabled) exts.push(EditorState.readOnly.of(true), EditorView.editable.of(false));
    return exts;
  }, [!!variableContext, !!disabled, placeholder, ariaLabel]);

  // Create the EditorView. It is rebuilt only when the extension set changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial doc only, live sync is in the value effect below.
  useEffect(() => {
    if (!containerRef.current) return;
    let state = EditorState.create({ doc: value, extensions });
    // Seed the variable context before the view exists, so highlighting starts correct.
    if (variableContextRef.current) {
      state = state.update({
        effects: setVariableContextEffect.of(variableContextRef.current),
      }).state;
    }
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [extensions]);

  // Push an outside value change (for example clearing after send) into the editor.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      isSyncingRef.current = true;
      view.dispatch({ changes: { from: 0, to: currentDoc.length, insert: value } });
      isSyncingRef.current = false;
    }
  }, [value]);

  // Keep the highlight context current.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !variableContext) return;
    view.dispatch({ effects: setVariableContextEffect.of(variableContext) });
  }, [variableContext]);

  return (
    <div
      ref={containerRef}
      className={cn('w-full', disabled && 'cursor-not-allowed opacity-50')}
    />
  );
}
```

- [ ] **Step 11: Run the tests (for the user to run)**

```bash
yarn test src/components/assistant/composer
```

Expected: PASS for `prompt-triggers`, `prompt-history-nav`, `prompt-completions` and `PromptEditor` (37 tests). jsdom cannot measure layout, so the growth itself (up to 12 lines, then scrolling) is covered by the CSS check and the manual check in Task 3.

- [ ] **Step 12: Verify types and lint**

```bash
yarn tsc --noEmit
yarn check
```

Expected: both clean. If `yarn check` reports only import order or formatting in the new files, run `yarn biome check --write <those files>` and re-run `yarn check`.

- [ ] **Step 13: Commit**

```bash
git add src/lib/assistant/types.ts src/components/assistant/composer/prompt-triggers.ts src/components/assistant/composer/prompt-history-nav.ts src/components/assistant/composer/prompt-completions.ts src/components/assistant/composer/prompt-editor-theme.ts src/components/assistant/composer/PromptEditor.tsx src/components/assistant/composer/__tests__/prompt-triggers.test.ts src/components/assistant/composer/__tests__/prompt-history-nav.test.ts src/components/assistant/composer/__tests__/prompt-completions.test.ts src/components/assistant/composer/__tests__/PromptEditor.test.tsx
```

Invoke the `dev-workflow-skills:1-git-commit` skill with the message `feat(assistant): add CodeMirror PromptEditor for the composer`.

---

## Task 2: Chips, references, resources, slash templates and prompt history

**Files:**
- Create: `src/lib/assistant/request-tabs.ts`
- Create: `src/lib/assistant/chip-resources.ts`
- Create: `src/lib/assistant/slash-commands.ts`
- Create: `src/lib/assistant/prompt-history.ts`
- Create: `src/components/assistant/composer/chips.ts`
- Create: `src/components/assistant/composer/reference-source.ts`
- Create: `src/components/assistant/composer/useReferenceItems.ts`
- Create: `src/components/assistant/composer/ComposerChips.tsx`
- Test: `src/lib/assistant/__tests__/chip-resources.test.ts`
- Test: `src/lib/assistant/__tests__/slash-commands.test.ts`
- Test: `src/lib/assistant/__tests__/prompt-history.test.ts`
- Test: `src/components/assistant/composer/__tests__/chips.test.ts`
- Test: `src/components/assistant/composer/__tests__/reference-source.test.ts`
- Test: `src/components/assistant/composer/__tests__/ComposerChips.test.tsx`

**Interfaces:**
- Consumes: `ReferenceItem`, `ReferenceKind`, `SlashCommandItem` (Task 1); `getRequest`, `getFolderSettings`, `getCollectionSettings`, `getEnvironment`, `getCollectionSummaries`, `listEnvironments`, types `Request`, `CollectionSettings`, `FolderSettings`, `Environment`, `Collection`, `CollectionItem`, `Folder`, `PromptResourceDto` from `@/lib/tauri-api`; `redactKnownSecrets`, `REDACTED` (`flow-export.ts:13,53`); `isVariableReference` (`flow-secrets.ts:7`); `isSensitiveHeader` (`sensitive-headers.ts:15`); `collectAllTabs` (`pane-utils.ts:332`); `usePaneStore` (`pane-store.ts:415`); `useCollections` (`collection-queries.ts:8`); `environmentKeys` (`environment-queries.ts:16`).
- Produces:
  - `chipToResource(chip: ReferenceItem): Promise<PromptResourceDto>` (index contract). Never rejects.
  - `chipUri(chip: ReferenceItem): string`, `capText(text: string, limit?: number): string`, `maskValue(name: string, value: string): string`, `RESOURCE_LIMIT_BYTES = 8192`.
  - `findRequestTab(root: PaneNode, collection: string, path: string): RequestTab | undefined`.
  - `SLASH_COMMANDS: readonly SlashCommandItem[]`, `filterSlashCommands(query: string): SlashCommandItem[]`.
  - `PROMPT_HISTORY_LIMIT = 50`, `loadPromptHistory(workspaceId: string): string[]`, `appendPromptHistory(history: readonly string[], prompt: string): string[]`, `savePromptHistory(workspaceId: string, history: readonly string[]): void`.
  - `MAX_CHIPS = 8`, `interface ComposerChip { key: string; item: ReferenceItem; focus: boolean }`, `chipKey(item)`, `addChip(chips, item, focus?)`, `removeChip(chips, key)`.
  - `flattenCollectionTree(collection, root)`, `environmentReferences(collection, environments)`, `filterReferences(items, query, limit?)`, `focusReference(focus, root)`, `lastResponseReference(focus, root)`, `MAX_REFERENCE_RESULTS = 50`.
  - `useReferenceItems(): ReferenceItem[]`.
  - `ComposerChips({ chips, onRemove }: { chips: ComposerChip[]; onRemove(key: string): void })`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

This task reads request, folder, collection and environment models and their auth fields to build chip text.

- [ ] **Step 2: Write the failing tests for the library modules**

Create `src/lib/assistant/__tests__/chip-resources.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  capText,
  chipToResource,
  chipUri,
  RESOURCE_LIMIT_BYTES,
} from '@/lib/assistant/chip-resources';
import type { ReferenceItem } from '@/lib/assistant/types';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import {
  type Environment,
  getEnvironment,
  getFolderSettings,
  getRequest,
  type Request,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: vi.fn(),
  getEnvironment: vi.fn(),
  getFolderSettings: vi.fn(),
  getCollectionSettings: vi.fn(),
}));

const REQUEST_CHIP: ReferenceItem = {
  kind: 'request',
  collection: 'shop',
  path: 'orders/list.yml',
  label: 'GET List orders',
};
const bytes = (text: string) => new TextEncoder().encode(text).length;

function openTab(tab: RequestTab) {
  const leaf = createDefaultLeaf('g1');
  usePaneStore.setState({ root: { ...leaf, tabs: [tab], activeTabId: tab.id }, activeGroupId: 'g1' });
}

function requestTab(overrides: Partial<RequestTab> = {}): RequestTab {
  return {
    id: 't1',
    title: 'List orders',
    isDirty: true,
    tabType: 'request',
    source: { collection: 'shop', path: 'orders/list.yml' },
    request: {
      ...createDefaultRequest(),
      url: 'https://api.test/orders',
      preRequestScript: 'console.log("unsaved edit");',
    },
    response: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(getRequest).mockReset();
  vi.mocked(getEnvironment).mockReset();
  vi.mocked(getFolderSettings).mockReset();
  usePaneStore.setState({ root: createDefaultLeaf('g1'), activeGroupId: 'g1' });
});

describe('chipToResource', () => {
  it('uses the open tab, so unsaved script edits are included', async () => {
    openTab(requestTab());
    const resource = await chipToResource(REQUEST_CHIP);
    expect(getRequest).not.toHaveBeenCalled();
    expect(resource.text).toContain('console.log("unsaved edit");');
    expect(resource.text).toContain('unsaved edits');
    expect(resource.uri).toBe('rocket://request/shop/orders/list.yml');
    expect(resource.mimeType).toBe('text/plain');
  });

  it('masks literal credentials of a saved request and keeps variable references', async () => {
    const saved: Request = {
      uid: 'u1',
      name: 'List orders',
      method: 'GET',
      url: 'https://api.test/orders?api_key=live123456',
      headers: [
        { key: 'Authorization', value: 'Bearer abcdefgh12345678', enabled: true },
        { key: 'X-Trace', value: '{{traceId}}', enabled: true },
      ],
      auth: { authType: 'bearer', token: 'abcdefgh12345678' },
      tests: 'test("ok", () => {});',
    };
    vi.mocked(getRequest).mockResolvedValue(saved);
    const resource = await chipToResource(REQUEST_CHIP);
    expect(getRequest).toHaveBeenCalledWith('shop', 'orders/list.yml');
    expect(resource.text).not.toContain('abcdefgh12345678');
    expect(resource.text).not.toContain('live123456');
    expect(resource.text).toContain('<redacted>');
    expect(resource.text).toContain('{{traceId}}');
    expect(resource.text).toContain('Auth: bearer');
    expect(resource.text).toContain('test("ok", () => {});');
  });

  it('shares environment names but not secret values', async () => {
    const env: Environment = {
      name: 'dev',
      variables: [
        { key: 'baseUrl', value: 'https://dev.test', enabled: true, secret: false },
        { key: 'clientSecret', value: 's3cr3t-value', enabled: true, secret: true },
      ],
    };
    vi.mocked(getEnvironment).mockResolvedValue(env);
    const resource = await chipToResource({
      kind: 'environment',
      collection: 'shop',
      path: 'dev',
      label: 'env: dev',
    });
    expect(getEnvironment).toHaveBeenCalledWith('shop', 'dev');
    expect(resource.text).toContain('https://dev.test');
    expect(resource.text).toContain('clientSecret');
    expect(resource.text).not.toContain('s3cr3t-value');
  });

  it('masks response cookies and shows the status and body', async () => {
    const response: ResponseState = {
      status: 200,
      statusText: 'OK',
      headers: [{ id: 'h1', key: 'Set-Cookie', value: 'sid=abc123xyz', enabled: true }],
      body: '{"ok":true}',
      durationMs: 12,
      ttfbMs: 5,
      sizeBytes: 11,
      activeView: 'pretty',
    };
    openTab(requestTab({ response }));
    const resource = await chipToResource({ ...REQUEST_CHIP, kind: 'last-response' });
    expect(resource.text).toContain('Status: 200 OK');
    expect(resource.text).toContain('{"ok":true}');
    expect(resource.text).not.toContain('abc123xyz');
    expect(resource.uri).toBe('rocket://last-response/shop/orders/list.yml');
  });

  it('does not echo a load error', async () => {
    vi.mocked(getFolderSettings).mockRejectedValue(new Error('boom Authorization: Bearer zzzzzzzz9'));
    const resource = await chipToResource({
      kind: 'folder',
      collection: 'shop',
      path: 'orders',
      label: 'orders',
    });
    expect(resource.text).toBe('Rocket could not load this folder: orders.');
  });

  it('caps a large chip at 8 KB with a marker', async () => {
    const tab = requestTab();
    tab.request = { ...tab.request, body: { mode: 'json', content: 'x'.repeat(20_000), formData: [] } };
    openTab(tab);
    const resource = await chipToResource(REQUEST_CHIP);
    expect(bytes(resource.text)).toBeLessThanOrEqual(RESOURCE_LIMIT_BYTES);
    expect(resource.text).toContain('[truncated:');
  });
});

describe('capText', () => {
  it('leaves short text alone', () => {
    expect(capText('hello')).toBe('hello');
  });

  it('never splits a multi-byte character', () => {
    const capped = capText('é'.repeat(5_000));
    expect(bytes(capped)).toBeLessThanOrEqual(RESOURCE_LIMIT_BYTES);
    expect(capped).not.toContain('\uFFFD');
    expect(capped).toContain('[truncated: 10000 bytes cut to 8192]');
  });
});

describe('chipUri', () => {
  it('encodes each segment', () => {
    expect(
      chipUri({ kind: 'request', collection: 'my shop', path: 'a b/list.yml', label: 'x' }),
    ).toBe('rocket://request/my%20shop/a%20b/list.yml');
    expect(chipUri({ kind: 'collection', collection: 'shop', label: 'shop' })).toBe(
      'rocket://collection/shop',
    );
  });
});
```

Create `src/lib/assistant/__tests__/slash-commands.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { filterSlashCommands, SLASH_COMMANDS } from '@/lib/assistant/slash-commands';

describe('slash commands', () => {
  it('offers the five Rocket templates in order', () => {
    expect(filterSlashCommands('').map((c) => c.name)).toEqual([
      'explain',
      'tests',
      'fix',
      'scaffold',
      'doc',
    ]);
  });

  it('filters by name prefix, ignoring case', () => {
    expect(filterSlashCommands('te').map((c) => c.name)).toEqual(['tests']);
    expect(filterSlashCommands('EX').map((c) => c.name)).toEqual(['explain']);
    expect(filterSlashCommands('zzz')).toEqual([]);
  });

  it('gives every command a description and a template', () => {
    for (const command of SLASH_COMMANDS) {
      expect(command.description.length).toBeGreaterThan(0);
      expect(command.template.trim().length).toBeGreaterThan(0);
    }
  });
});
```

Create `src/lib/assistant/__tests__/prompt-history.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  appendPromptHistory,
  loadPromptHistory,
  PROMPT_HISTORY_LIMIT,
  savePromptHistory,
} from '@/lib/assistant/prompt-history';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('appendPromptHistory', () => {
  it('trims and skips blank prompts', () => {
    expect(appendPromptHistory([], '  hi  ')).toEqual(['hi']);
    expect(appendPromptHistory(['a'], '   ')).toEqual(['a']);
  });

  it('moves a repeated prompt to the newest place', () => {
    expect(appendPromptHistory(['a', 'b', 'c'], 'a')).toEqual(['b', 'c', 'a']);
  });

  it('keeps the last 50 prompts', () => {
    const full = Array.from({ length: PROMPT_HISTORY_LIMIT }, (_, i) => `p${i}`);
    const next = appendPromptHistory(full, 'new');
    expect(next).toHaveLength(PROMPT_HISTORY_LIMIT);
    expect(next[0]).toBe('p1');
    expect(next[next.length - 1]).toBe('new');
  });
});

describe('load and save', () => {
  it('keeps one history per workspace', () => {
    savePromptHistory('w1', ['one']);
    savePromptHistory('w2', ['two']);
    expect(loadPromptHistory('w1')).toEqual(['one']);
    expect(loadPromptHistory('w2')).toEqual(['two']);
  });

  it('does nothing without a workspace id', () => {
    savePromptHistory('', ['x']);
    expect(localStorage.length).toBe(0);
    expect(loadPromptHistory('')).toEqual([]);
  });

  it('ignores junk', () => {
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '{not json');
    expect(loadPromptHistory('w1')).toEqual([]);
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '{"a":1}');
    expect(loadPromptHistory('w1')).toEqual([]);
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '["ok", 3, null, "fine"]');
    expect(loadPromptHistory('w1')).toEqual(['ok', 'fine']);
  });

  it('survives storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(loadPromptHistory('w1')).toEqual([]);
    expect(() => savePromptHistory('w1', ['x'])).not.toThrow();
  });
});
```

- [ ] **Step 3: Write the failing tests for the composer modules**

Create `src/components/assistant/composer/__tests__/chips.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { ReferenceItem } from '@/lib/assistant/types';
import { addChip, chipKey, type ComposerChip, MAX_CHIPS, removeChip } from '../chips';

const item = (n: number): ReferenceItem => ({
  kind: 'request',
  collection: 'shop',
  path: `r${n}.yml`,
  label: `GET r${n}`,
});

describe('chips', () => {
  it('adds a new chip', () => {
    const result = addChip([], item(1));
    expect(result.outcome).toBe('added');
    expect(result.chips).toEqual([{ key: chipKey(item(1)), item: item(1), focus: false }]);
  });

  it('refuses a duplicate', () => {
    const first = addChip([], item(1)).chips;
    const result = addChip(first, item(1));
    expect(result.outcome).toBe('duplicate');
    expect(result.chips).toBe(first);
  });

  it('refuses a ninth chip', () => {
    let chips: ComposerChip[] = [];
    for (let i = 0; i < MAX_CHIPS; i++) chips = addChip(chips, item(i)).chips;
    expect(chips).toHaveLength(MAX_CHIPS);
    const result = addChip(chips, item(99));
    expect(result.outcome).toBe('limit');
    expect(result.chips).toHaveLength(MAX_CHIPS);
  });

  it('tells a request apart from its last response', () => {
    expect(chipKey(item(1))).not.toBe(chipKey({ ...item(1), kind: 'last-response' }));
  });

  it('removes a chip by key', () => {
    const chips = addChip(addChip([], item(1)).chips, item(2)).chips;
    expect(removeChip(chips, chipKey(item(1))).map((c) => c.item)).toEqual([item(2)]);
  });
});
```

Create `src/components/assistant/composer/__tests__/reference-source.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { ReferenceItem } from '@/lib/assistant/types';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import type { Folder } from '@/lib/tauri-api';
import type { RequestTab, ResponseState } from '@/types/pane-types';
import {
  environmentReferences,
  filterReferences,
  flattenCollectionTree,
  focusReference,
  lastResponseReference,
} from '../reference-source';

const ROOT: Folder = {
  uid: 'root',
  name: 'shop',
  items: [
    { type: 'summary', uid: 's1', name: 'Health', method: 'GET', url: '/health', fileName: 'health.yml' },
    {
      type: 'folder',
      uid: 'f1',
      name: 'Orders',
      dirName: 'orders',
      items: [
        { type: 'summary', uid: 's2', name: 'List orders', method: 'GET', url: '/orders', fileName: 'list.yml' },
        { type: 'summary', uid: 's3', name: 'Stream', method: 'GET', url: '/ws', fileName: 'stream.yml', kind: 'websocket' },
      ],
    },
  ],
};

const tabWith = (response: ResponseState | null): RequestTab => ({
  id: 't1',
  title: 'List orders',
  isDirty: false,
  tabType: 'request',
  source: { collection: 'shop', path: 'orders/list.yml' },
  request: createDefaultRequest(),
  response,
});

const rootWith = (tab: RequestTab) => ({ ...createDefaultLeaf('g1'), tabs: [tab], activeTabId: tab.id });

const RESPONSE: ResponseState = {
  status: 200,
  statusText: 'OK',
  headers: [],
  body: '{}',
  durationMs: 1,
  ttfbMs: 1,
  sizeBytes: 2,
  activeView: 'pretty',
};

describe('flattenCollectionTree', () => {
  it('lists the collection, folders and HTTP requests with sidebar paths', () => {
    expect(flattenCollectionTree('shop', ROOT)).toEqual<ReferenceItem[]>([
      { kind: 'collection', collection: 'shop', label: 'shop' },
      { kind: 'request', collection: 'shop', path: 'health.yml', label: 'GET Health' },
      { kind: 'folder', collection: 'shop', path: 'orders', label: 'Orders' },
      { kind: 'request', collection: 'shop', path: 'orders/list.yml', label: 'GET List orders' },
    ]);
  });
});

describe('environmentReferences', () => {
  it('uses the environment name as the path', () => {
    expect(environmentReferences('shop', [{ name: 'dev', variables: [] }])).toEqual([
      { kind: 'environment', collection: 'shop', path: 'dev', label: 'env: dev' },
    ]);
  });
});

describe('filterReferences', () => {
  const items = flattenCollectionTree('shop', ROOT);

  it('returns everything up to the limit for an empty query', () => {
    expect(filterReferences(items, '')).toHaveLength(items.length);
    expect(filterReferences(items, '', 2)).toHaveLength(2);
  });

  it('ranks label prefix, then label match, then path match', () => {
    expect(filterReferences(items, 'orders').map((i) => i.label)).toEqual([
      'Orders',
      'GET List orders',
    ]);
    expect(filterReferences(items, 'list.yml').map((i) => i.label)).toEqual(['GET List orders']);
  });
});

describe('focus references', () => {
  it('labels the focus chip with the open tab title', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(focusReference(focus, rootWith(tabWith(null)))?.label).toBe('List orders');
  });

  it('falls back to the file name without .yml when no tab is open', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(focusReference(focus, createDefaultLeaf('g1'))?.label).toBe('list');
    expect(focusReference(undefined, createDefaultLeaf('g1'))).toBeNull();
  });

  it('offers the last response only when the focused tab has one', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(lastResponseReference(focus, rootWith(tabWith(null)))).toBeNull();
    expect(lastResponseReference(focus, rootWith(tabWith(RESPONSE)))).toEqual({
      kind: 'last-response',
      collection: 'shop',
      path: 'orders/list.yml',
      label: 'Last response: List orders',
    });
  });
});
```

Create `src/components/assistant/composer/__tests__/ComposerChips.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ComposerChip } from '../chips';
import { ComposerChips } from '../ComposerChips';

const CHIPS: ComposerChip[] = [
  {
    key: 'request:shop:list.yml',
    item: { kind: 'request', collection: 'shop', path: 'list.yml', label: 'List orders' },
    focus: true,
  },
  {
    key: 'environment:shop:dev',
    item: { kind: 'environment', collection: 'shop', path: 'dev', label: 'env: dev' },
    focus: false,
  },
];

describe('ComposerChips', () => {
  it('renders nothing without chips', () => {
    const { container } = render(<ComposerChips chips={[]} onRemove={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows each chip and removes one by key', async () => {
    const onRemove = vi.fn();
    render(<ComposerChips chips={CHIPS} onRemove={onRemove} />);
    expect(screen.getByText('List orders')).toBeInTheDocument();
    expect(screen.getByText('env: dev')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove env: dev' }));
    expect(onRemove).toHaveBeenCalledWith('environment:shop:dev');
  });
});
```

- [ ] **Step 4: Run the tests to see them fail (for the user to run)**

```bash
yarn test src/lib/assistant src/components/assistant/composer
```

Expected: FAIL for the six new test files (modules missing). Task 1 tests still pass.

- [ ] **Step 5: Write the request-tab lookup**

Create `src/lib/assistant/request-tabs.ts`:

```ts
import { collectAllTabs } from '@/lib/pane-utils';
import { isRequestTab, type PaneNode, type RequestTab } from '@/types/pane-types';

/**
 * The open request tab that shows `path` of `collection`, in any pane. Its request
 * state holds the user's unsaved edits, scripts included.
 */
export function findRequestTab(
  root: PaneNode,
  collection: string,
  path: string,
): RequestTab | undefined {
  return collectAllTabs(root).find(
    (tab): tab is RequestTab =>
      isRequestTab(tab) && tab.source?.collection === collection && tab.source.path === path,
  );
}
```

- [ ] **Step 6: Write `chipToResource` and its renderers**

Create `src/lib/assistant/chip-resources.ts`:

```ts
import { REDACTED, redactKnownSecrets } from '@/lib/flow-export';
import { isVariableReference } from '@/lib/flow-secrets';
import { isSensitiveHeader } from '@/lib/sensitive-headers';
import {
  type CollectionSettings,
  type Environment,
  type FolderSettings,
  getCollectionSettings,
  getEnvironment,
  getFolderSettings,
  getRequest,
  type PromptResourceDto,
  type Request,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';
import { findRequestTab } from './request-tabs';
import type { ReferenceItem } from './types';

/** Largest text one chip adds to a message, in UTF-8 bytes, marker included. */
export const RESOURCE_LIMIT_BYTES = 8 * 1024;

// A header, parameter or variable name that holds a credential.
const CREDENTIAL_NAME =
  /token|secret|password|passwd|api[_-]?key|credential|cookie|authorization|session/i;
const FENCE = '`'.repeat(3);

interface Row {
  key: string;
  value: string;
  enabled: boolean;
}

interface VariableRow extends Row {
  secret: boolean;
}

/** A value that is safe to share. `{{variable}}` references are kept as they are. */
export function maskValue(name: string, value: string): string {
  if (value.trim() === '' || isVariableReference(value)) return value;
  if (isSensitiveHeader(name) || CREDENTIAL_NAME.test(name)) return REDACTED;
  return redactKnownSecrets(value);
}

/** Cuts `text` to `limit` UTF-8 bytes, marker included, without splitting a character. */
export function capText(text: string, limit = RESOURCE_LIMIT_BYTES): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(text);
  if (bytes.length <= limit) return text;
  const marker = `\n[truncated: ${bytes.length} bytes cut to ${limit}]`;
  const keep = limit - encoder.encode(marker).length;
  // A cut inside a character decodes to U+FFFD, which is dropped.
  const head = new TextDecoder().decode(bytes.slice(0, keep)).replace(/\uFFFD+$/, '');
  return `${head}${marker}`;
}

/** The resource URI of a chip, such as `rocket://request/shop/orders/list.yml`. */
export function chipUri(chip: ReferenceItem): string {
  const segments = [chip.collection, ...(chip.path ? chip.path.split('/') : [])];
  return `rocket://${chip.kind}/${segments.map(encodeURIComponent).join('/')}`;
}

function rowsSection(title: string, rows: readonly Row[]): string[] {
  const shown = rows.filter((row) => row.enabled && row.key.trim() !== '');
  if (shown.length === 0) return [];
  return [`${title}:`, ...shown.map((row) => `  ${row.key}: ${maskValue(row.key, row.value)}`)];
}

function variablesSection(title: string, rows: readonly VariableRow[]): string[] {
  const shown = rows.filter((row) => row.enabled && row.key.trim() !== '');
  if (shown.length === 0) return [];
  return [
    `${title}:`,
    ...shown.map((row) =>
      row.secret
        ? `  ${row.key}: (secret, value not shared)`
        : `  ${row.key}: ${maskValue(row.key, row.value)}`,
    ),
  ];
}

function codeSection(title: string, code: string | null | undefined, language: string): string[] {
  if (!code || code.trim() === '') return [];
  return [`${title}:`, `${FENCE}${language}`, code, FENCE];
}

function authLine(authType: string | undefined): string[] {
  if (!authType || authType === 'none') return [];
  if (authType === 'inherit') return ['Auth: inherited from the folder or collection'];
  return [`Auth: ${authType} (credential values are not shared)`];
}

function bodyText(mode: string, content: string, formData: readonly Row[]): string {
  if (mode === 'none') return '';
  if (mode === 'binary') return '(binary file, not shared)';
  if (mode === 'formdata' || mode === 'formurlencoded') {
    return formData
      .filter((row) => row.enabled && row.key.trim() !== '')
      .map((row) => `${row.key}=${maskValue(row.key, row.value)}`)
      .join('\n');
  }
  return redactKnownSecrets(content);
}

/** The parts of a request a chip shows, from an open tab or from disk. */
interface RequestView {
  title: string;
  unsaved: boolean;
  kind: string;
  method: string;
  url: string;
  queryParams: readonly Row[];
  headers: readonly Row[];
  authType: string;
  bodyMode: string;
  body: string;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
  tests?: string | null;
  docs?: string | null;
}

function fromTab(tab: RequestTab): RequestView {
  const request = tab.request;
  return {
    title: tab.title,
    unsaved: tab.isDirty,
    kind: request.requestType,
    method: request.method,
    url: request.url,
    queryParams: request.queryParams,
    headers: request.headers,
    authType: request.auth.authType,
    bodyMode: request.body.mode,
    body: bodyText(request.body.mode, request.body.content, request.body.formData),
    preRequestScript: request.preRequestScript,
    postResponseScript: request.postResponseScript,
    tests: request.testsScript,
    docs: request.docs,
  };
}

function fromSaved(request: Request): RequestView {
  const mode = request.body?.mode ?? 'none';
  return {
    title: request.name,
    unsaved: false,
    kind: 'http',
    method: request.method,
    url: request.url,
    queryParams: request.queryParams ?? [],
    headers: request.headers,
    authType: request.auth.authType,
    bodyMode: mode,
    body: bodyText(mode, request.body?.content ?? '', request.body?.formData ?? []),
    preRequestScript: request.preRequestScript,
    postResponseScript: request.postResponseScript,
    tests: request.tests,
    docs: request.docs,
  };
}

function renderRequest(view: RequestView, collection: string, path: string): string {
  return [
    `Request: ${view.title}${view.unsaved ? ' (open in the editor, unsaved edits included)' : ''}`,
    `Collection: ${collection}`,
    `Path: ${path}`,
    `Type: ${view.kind}`,
    `${view.method} ${redactKnownSecrets(view.url)}`,
    ...rowsSection('Query parameters', view.queryParams),
    ...rowsSection('Headers', view.headers),
    ...authLine(view.authType),
    ...(view.body ? [`Body (${view.bodyMode}):`, view.body] : []),
    ...codeSection('Pre-request script', view.preRequestScript, 'javascript'),
    ...codeSection('Post-response script', view.postResponseScript, 'javascript'),
    ...codeSection('Tests', view.tests, 'javascript'),
    ...codeSection('Docs', view.docs, 'markdown'),
  ].join('\n');
}

function renderFolder(collection: string, path: string, settings: FolderSettings): string {
  return [
    `Folder: ${path}`,
    `Collection: ${collection}`,
    ...authLine(settings.auth?.authType),
    ...rowsSection('Headers', settings.headers),
    ...variablesSection('Variables', settings.variables),
    ...codeSection('Pre-request script', settings.preRequestScript, 'javascript'),
    ...codeSection('Post-response script', settings.postResponseScript, 'javascript'),
    ...codeSection('Tests', settings.testsScript, 'javascript'),
    ...codeSection('Docs', settings.docs, 'markdown'),
  ].join('\n');
}

function renderCollection(name: string, settings: CollectionSettings): string {
  return [
    `Collection: ${name}`,
    ...authLine(settings.auth?.authType),
    ...rowsSection('Headers', settings.headers),
    ...variablesSection('Variables', settings.variables),
    `The agent may run requests here: ${settings.agentAutonomyEnabled ? 'yes' : 'no'}`,
    ...codeSection('Docs', settings.docs, 'markdown'),
  ].join('\n');
}

function renderEnvironment(collection: string, env: Environment): string {
  return [
    `Environment: ${env.name}`,
    `Collection: ${collection}`,
    ...variablesSection('Variables', env.variables),
  ].join('\n');
}

function renderResponse(tab: RequestTab, response: ResponseState): string {
  const tests = response.testResults ?? [];
  const failed = tests.filter((test) => test.status === 'failed');
  // Response headers are all shown, whatever their enabled flag says.
  const headers = response.headers.map((header) => ({ ...header, enabled: true }));
  return [
    `Last response of: ${tab.title}`,
    `${tab.request.method} ${redactKnownSecrets(tab.request.url)}`,
    `Status: ${response.status} ${response.statusText}`,
    `Time: ${response.durationMs} ms, size: ${response.sizeBytes} bytes`,
    ...rowsSection('Headers', headers),
    ...(tests.length > 0
      ? [
          `Tests: ${tests.length - failed.length} passed, ${failed.length} failed`,
          ...failed.map(
            (test) => `  failed: ${test.name}${test.error ? ` (${test.error})` : ''}`,
          ),
        ]
      : []),
    'Body:',
    response.isBinary ? '(binary body, not shared)' : response.body,
  ].join('\n');
}

async function renderChip(chip: ReferenceItem): Promise<string> {
  const path = chip.path ?? '';
  const root = usePaneStore.getState().root;
  switch (chip.kind) {
    case 'request': {
      const tab = findRequestTab(root, chip.collection, path);
      const view = tab ? fromTab(tab) : fromSaved(await getRequest(chip.collection, path));
      return renderRequest(view, chip.collection, path);
    }
    case 'folder':
      return renderFolder(chip.collection, path, await getFolderSettings(chip.collection, path));
    case 'collection':
      return renderCollection(chip.collection, await getCollectionSettings(chip.collection));
    case 'environment':
      return renderEnvironment(chip.collection, await getEnvironment(chip.collection, path));
    case 'last-response': {
      const tab = findRequestTab(root, chip.collection, path);
      if (!tab?.response) return `No response is available for ${chip.label}.`;
      return renderResponse(tab, tab.response);
    }
  }
}

/**
 * Turns a chip into an embedded text resource that Rocket builds from its own data.
 * Secrets are masked, a final `redactKnownSecrets` pass covers free text such as
 * bodies and scripts, and the text is capped at 8 KB. Never rejects: a chip that
 * cannot load becomes a short notice that does not echo the error.
 */
export async function chipToResource(chip: ReferenceItem): Promise<PromptResourceDto> {
  let text: string;
  try {
    text = await renderChip(chip);
  } catch {
    // The error text can echo request content, so it is not passed on.
    text = `Rocket could not load this ${chip.kind}: ${chip.label}.`;
  }
  return { uri: chipUri(chip), mimeType: 'text/plain', text: capText(redactKnownSecrets(text)) };
}
```

- [ ] **Step 7: Write the slash templates and the prompt history store**

Create `src/lib/assistant/slash-commands.ts`:

```ts
import type { SlashCommandItem } from './types';

/** Rocket's own prompt templates. Choosing one puts the template text in the editor. */
export const SLASH_COMMANDS: readonly SlashCommandItem[] = [
  {
    name: 'explain',
    description: 'Explain what a request and its scripts do',
    template: 'Explain what this request does, including its scripts and tests: ',
  },
  {
    name: 'tests',
    description: 'Write tests for a request',
    template:
      'Write tests for this request that check the status code, the important response fields and the response time: ',
  },
  {
    name: 'fix',
    description: 'Find and fix a failing script or test',
    template: 'This request or one of its scripts fails. Find the cause and propose a fix: ',
  },
  {
    name: 'scaffold',
    description: 'Create requests and folders',
    template: 'Create folders and requests for the following API: ',
  },
  {
    name: 'doc',
    description: 'Write documentation for a request',
    template:
      'Write short Markdown documentation for this request: what it does, its parameters and an example response: ',
  },
];

/** The commands whose name starts with `query`, ignoring case. */
export function filterSlashCommands(query: string): SlashCommandItem[] {
  const prefix = query.trim().toLowerCase();
  return SLASH_COMMANDS.filter((command) => command.name.startsWith(prefix));
}
```

Create `src/lib/assistant/prompt-history.ts`:

```ts
/** Prompts kept per workspace for Up and Down recall. */
export const PROMPT_HISTORY_LIMIT = 50;

const keyFor = (workspaceId: string) => `rocket-api:assistant-prompt-history:${workspaceId}`;

/** The workspace's saved prompts, oldest first. Empty when storage is missing or broken. */
export function loadPromptHistory(workspaceId: string): string[] {
  if (!workspaceId) return [];
  try {
    const raw = localStorage.getItem(keyFor(workspaceId));
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((entry): entry is string => typeof entry === 'string')
      .slice(-PROMPT_HISTORY_LIMIT);
  } catch {
    // Storage can be blocked or hold bad JSON. History is a convenience, so start empty.
    return [];
  }
}

/** `history` with `prompt` added as the newest entry. A repeat moves to the end. */
export function appendPromptHistory(history: readonly string[], prompt: string): string[] {
  const text = prompt.trim();
  if (text === '') return [...history];
  return [...history.filter((entry) => entry !== text), text].slice(-PROMPT_HISTORY_LIMIT);
}

/** Saves the history. A storage failure only loses the history. */
export function savePromptHistory(workspaceId: string, history: readonly string[]): void {
  if (!workspaceId) return;
  try {
    localStorage.setItem(keyFor(workspaceId), JSON.stringify(history.slice(-PROMPT_HISTORY_LIMIT)));
  } catch {
    // Storage can be full or blocked. The prompt was still sent.
  }
}
```

- [ ] **Step 8: Write the chip and reference helpers**

Create `src/components/assistant/composer/chips.ts`:

```ts
import type { ReferenceItem } from '@/lib/assistant/types';

/** Chips one message can carry, the focus chip included. */
export const MAX_CHIPS = 8;

export interface ComposerChip {
  key: string;
  item: ReferenceItem;
  /** True for the chip that names the open request. */
  focus: boolean;
}

export type AddChipOutcome = 'added' | 'duplicate' | 'limit';

/** Identifies what a chip points at, so the same item is not added twice. */
export function chipKey(item: ReferenceItem): string {
  return `${item.kind}:${item.collection}:${item.path ?? ''}`;
}

/** Adds a chip unless it is already there or the message is full. */
export function addChip(
  chips: ComposerChip[],
  item: ReferenceItem,
  focus = false,
): { chips: ComposerChip[]; outcome: AddChipOutcome } {
  const key = chipKey(item);
  if (chips.some((chip) => chip.key === key)) return { chips, outcome: 'duplicate' };
  if (chips.length >= MAX_CHIPS) return { chips, outcome: 'limit' };
  return { chips: [...chips, { key, item, focus }], outcome: 'added' };
}

export function removeChip(chips: readonly ComposerChip[], key: string): ComposerChip[] {
  return chips.filter((chip) => chip.key !== key);
}
```

Create `src/components/assistant/composer/reference-source.ts`:

```ts
import { findRequestTab } from '@/lib/assistant/request-tabs';
import type { ReferenceItem } from '@/lib/assistant/types';
import type { CollectionItem, Environment, Folder } from '@/lib/tauri-api';
import type { PaneNode } from '@/types/pane-types';

/** Items the `#` list shows at once. */
export const MAX_REFERENCE_RESULTS = 50;

interface Focus {
  collection: string;
  path: string;
}

/**
 * The collection, its folders and its HTTP requests, with the same paths the sidebar
 * uses (`CollectionNode` and `FolderNode`). Other protocols are left out, as in the sidebar.
 */
export function flattenCollectionTree(collection: string, root: Folder): ReferenceItem[] {
  const out: ReferenceItem[] = [{ kind: 'collection', collection, label: collection }];
  const walk = (items: readonly CollectionItem[], basePath: string) => {
    for (const item of items) {
      if (item.type === 'folder') {
        const dir = item.dirName ?? item.name;
        const path = basePath ? `${basePath}/${dir}` : dir;
        out.push({ kind: 'folder', collection, path, label: item.name });
        walk(item.items, path);
      } else if (
        item.type === 'request' ||
        (item.type === 'summary' && (item.kind ?? 'http') === 'http')
      ) {
        const fileName = item.fileName ?? item.name;
        const path = basePath ? `${basePath}/${fileName}` : fileName;
        out.push({ kind: 'request', collection, path, label: `${item.method} ${item.name}` });
      }
    }
  };
  walk(root.items, '');
  return out;
}

/** One item per environment. The environment name goes in `path`. */
export function environmentReferences(
  collection: string,
  environments: readonly Environment[],
): ReferenceItem[] {
  return environments.map(
    (env): ReferenceItem => ({
      kind: 'environment',
      collection,
      path: env.name,
      label: `env: ${env.name}`,
    }),
  );
}

// Lower is better. -1 means no match.
function score(item: ReferenceItem, query: string): number {
  const label = item.label.toLowerCase();
  if (label.startsWith(query)) return 0;
  if (label.includes(query)) return 1;
  if (`${item.collection}/${item.path ?? ''}`.toLowerCase().includes(query)) return 2;
  return -1;
}

/** The items matching `query`, best first. Equal scores keep the tree order. */
export function filterReferences(
  items: readonly ReferenceItem[],
  query: string,
  limit = MAX_REFERENCE_RESULTS,
): ReferenceItem[] {
  const q = query.trim().toLowerCase();
  if (q === '') return items.slice(0, limit);
  return items
    .map((item) => ({ item, rank: score(item, q) }))
    .filter((entry) => entry.rank >= 0)
    .sort((a, b) => a.rank - b.rank)
    .slice(0, limit)
    .map((entry) => entry.item);
}

function fileLabel(path: string): string {
  const last = path.split('/').pop() ?? path;
  return last.replace(/\.yml$/, '');
}

/** The chip for the focused request, labelled with its tab title when the tab is open. */
export function focusReference(focus: Focus | undefined, root: PaneNode): ReferenceItem | null {
  if (!focus) return null;
  const tab = findRequestTab(root, focus.collection, focus.path);
  return {
    kind: 'request',
    collection: focus.collection,
    path: focus.path,
    label: tab?.title ?? fileLabel(focus.path),
  };
}

/** The focused request's last response, when its open tab holds one. */
export function lastResponseReference(
  focus: Focus | undefined,
  root: PaneNode,
): ReferenceItem | null {
  if (!focus) return null;
  const tab = findRequestTab(root, focus.collection, focus.path);
  if (!tab?.response) return null;
  return {
    kind: 'last-response',
    collection: focus.collection,
    path: focus.path,
    label: `Last response: ${tab.title}`,
  };
}
```

Create `src/components/assistant/composer/useReferenceItems.ts`:

```ts
import { type UseQueryResult, useQueries } from '@tanstack/react-query';
import { useMemo } from 'react';
import type { ReferenceItem } from '@/lib/assistant/types';
import { useCollections } from '@/lib/queries/collection-queries';
import { environmentKeys } from '@/lib/queries/environment-queries';
import {
  type Collection,
  type Environment,
  getCollectionSummaries,
  listEnvironments,
} from '@/lib/tauri-api';
import { environmentReferences, flattenCollectionTree } from './reference-source';

// Module-level combiners keep the combined arrays stable while the data is unchanged.
const treesOf = (results: UseQueryResult<Collection>[]) => results.map((result) => result.data);
const environmentsOf = (results: UseQueryResult<Environment[]>[]) =>
  results.map((result) => result.data);

/**
 * Every collection, folder, HTTP request and environment of the workspace, for the
 * `#` list. Trees load with the same lightweight summaries call as the sidebar, and
 * environments share the `useEnvironments` cache key.
 */
export function useReferenceItems(): ReferenceItem[] {
  const { data: collections } = useCollections();
  const names = useMemo(() => (collections ?? []).map((c) => c.name), [collections]);
  const trees = useQueries({
    queries: names.map((name) => ({
      queryKey: ['assistant', 'reference-tree', name],
      queryFn: () => getCollectionSummaries(name),
    })),
    combine: treesOf,
  });
  const environments = useQueries({
    queries: names.map((name) => ({
      queryKey: environmentKeys.collection(name),
      queryFn: () => listEnvironments(name),
    })),
    combine: environmentsOf,
  });
  return useMemo(
    () =>
      names.flatMap((name, i) => {
        const tree = trees[i];
        const envs = environments[i];
        const fallback: ReferenceItem = { kind: 'collection', collection: name, label: name };
        return [
          ...(tree ? flattenCollectionTree(name, tree.root) : [fallback]),
          ...(envs ? environmentReferences(name, envs) : []),
        ];
      }),
    [names, trees, environments],
  );
}
```

- [ ] **Step 9: Write `ComposerChips`**

Create `src/components/assistant/composer/ComposerChips.tsx`:

```tsx
import {
  Crosshair,
  FileText,
  Folder,
  Globe,
  Inbox,
  Layers,
  type LucideIcon,
  X,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { ReferenceKind } from '@/lib/assistant/types';
import type { ComposerChip } from './chips';

const KIND_ICON: Record<ReferenceKind, LucideIcon> = {
  request: FileText,
  folder: Folder,
  collection: Layers,
  environment: Globe,
  'last-response': Inbox,
};

interface ComposerChipsProps {
  chips: ComposerChip[];
  onRemove(key: string): void;
}

/** The context chips above the prompt. The focus chip shows a crosshair. */
export function ComposerChips({ chips, onRemove }: ComposerChipsProps) {
  if (chips.length === 0) return null;
  return (
    <div className='flex flex-wrap gap-1 px-2 pt-2'>
      {chips.map((chip) => {
        const Icon = chip.focus ? Crosshair : KIND_ICON[chip.item.kind];
        return (
          <Badge key={chip.key} variant='secondary' className='gap-1 pr-0.5 font-normal'>
            <Icon className='size-3 shrink-0' />
            <span className='max-w-40 truncate'>{chip.item.label}</span>
            <Button
              variant='ghost'
              size='icon'
              className='size-4 p-0'
              aria-label={`Remove ${chip.item.label}`}
              onClick={() => onRemove(chip.key)}
            >
              <X className='size-3' />
            </Button>
          </Badge>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 10: Run the tests (for the user to run)**

```bash
yarn test src/lib/assistant src/components/assistant/composer
```

Expected: PASS, the six new files (33 tests) plus the Task 1 files.

- [ ] **Step 11: Verify types and lint**

```bash
yarn tsc --noEmit
yarn check
```

Expected: both clean. If the `combine` functions in `useReferenceItems.ts` fail to type-check against the installed `@tanstack/react-query`, inline them as `combine: (results) => results.map((result) => result.data)` and note the change as a `Ruling:` line in the ledger. If `yarn check` reports only import order or formatting, run `yarn biome check --write <those files>`.

- [ ] **Step 12: Commit**

```bash
git add src/lib/assistant/request-tabs.ts src/lib/assistant/chip-resources.ts src/lib/assistant/slash-commands.ts src/lib/assistant/prompt-history.ts src/components/assistant/composer/chips.ts src/components/assistant/composer/reference-source.ts src/components/assistant/composer/useReferenceItems.ts src/components/assistant/composer/ComposerChips.tsx src/lib/assistant/__tests__/chip-resources.test.ts src/lib/assistant/__tests__/slash-commands.test.ts src/lib/assistant/__tests__/prompt-history.test.ts src/components/assistant/composer/__tests__/chips.test.ts src/components/assistant/composer/__tests__/reference-source.test.ts src/components/assistant/composer/__tests__/ComposerChips.test.tsx
```

Invoke the `dev-workflow-skills:1-git-commit` skill with the message `feat(assistant): add composer chips, references and prompt history`.

---

## Task 3: Toolbar, `Composer` assembly and the stub replacement

**Files:**
- Create: `src/components/assistant/composer/usage-format.ts`
- Create: `src/lib/assistant/model-memory.ts`
- Create: `src/components/assistant/composer/ComposerToolbar.tsx`
- Create: `src/components/assistant/composer/Composer.tsx`
- Modify: `src/stores/assistant-store.ts` (add `setMode` only if Task 1 Step 1 found it missing; the file is written by Plan 05, so locate the interface and the `create(...)` body by `grep -n "setConfigOptions" src/stores/assistant-store.ts`)
- Modify: `src/components/assistant/AssistantPanel.tsx` (the stub import and the `<AssistantInputStub` element, found with `grep -n "AssistantInputStub" src/components/assistant/AssistantPanel.tsx`)
- Modify: `src/lib/assistant/assistant-session.ts` (Plan 05: `sendAssistantMessage` gains a `resources` argument; `startAssistant` passes the remembered model to its `startWorkspaceAssistant(` call)
- Modify: `src/lib/assistant/__tests__/assistant-session.test.ts` and `src/components/assistant/__tests__/AssistantPanel.test.tsx` (Plan 05 tests whose call expectations or stub queries change)
- Delete: `src/components/assistant/AssistantInputStub.tsx`
- Create: `.claude/assistant-composer.md`
- Modify: `CLAUDE.md:62` (add one pointer line after the folder-settings line)
- Test: `src/components/assistant/composer/__tests__/usage-format.test.ts`
- Test: `src/lib/assistant/__tests__/model-memory.test.ts`
- Test: `src/components/assistant/composer/__tests__/ComposerToolbar.test.tsx`
- Test: `src/components/assistant/composer/__tests__/Composer.test.tsx`
- Test: `src/stores/__tests__/assistant-store.set-mode.test.ts`

**Interfaces:**
- Consumes: everything from Tasks 1 and 2; `sendAgentPrompt(sessionId, prompt, resources?)`, `cancelAgentPrompt(sessionId)`, `setAgentConfigOption(sessionId, configId, value): Promise<ConfigOption[]>`, `setAssistantMode(sessionId, mode)`, type `ConfigOption` (Plans 01 and 03); `useAssistantStore` with `session.{sessionId, agentConfigId, status, configOptions, mode}`, `usage`, `focus`, `setConfigOptions(sessionId, options)`, `completeMessage(sessionId)`, `selectTurnRunning`, and `sendAssistantMessage(text)`, `startAssistant(agentConfigId, mode?)` from `src/lib/assistant/assistant-session.ts` (Plan 05); `useWorkspaceStore` (`workspace-store.ts:4`); `toast` from `sonner`.
- Produces:
  - `formatUsage(usage?: UsageInput): UsageView | null`, `interface UsageInput { used: number; size: number; costUsd?: number }`, `interface UsageView { percent: number; text: string; detail: string }`.
  - `loadRememberedModel(agentConfigId: string): string | undefined`, `rememberModel(agentConfigId: string, model: string): void`.
  - `type AssistantModeValue = 'ask' | 'edit' | 'agent'`, `MODE_OPTIONS`, `MODEL_OPTION_ID = 'model'`, `EFFORT_OPTION_ID = 'effort'`, `ComposerToolbar(props: ComposerToolbarProps)`.
  - `Composer(): JSX.Element` (no props), mounted by `AssistantPanel` in place of the stub.
  - Store action `setMode(mode: 'ask' | 'edit' | 'agent'): void` (additive, see Interface deviations).

- [ ] **Step 1: Write the failing tests**

Create `src/components/assistant/composer/__tests__/usage-format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatUsage } from '../usage-format';

describe('formatUsage', () => {
  it('hides the indicator without usage or with a zero size', () => {
    expect(formatUsage(undefined)).toBeNull();
    expect(formatUsage({ used: 10, size: 0 })).toBeNull();
  });

  it('rounds the percentage and lists the tokens', () => {
    expect(formatUsage({ used: 24_000, size: 200_000 })).toEqual({
      percent: 12,
      text: '12%',
      detail: '24,000 of 200,000 tokens',
    });
  });

  it('adds the cost when the agent reports it', () => {
    expect(formatUsage({ used: 1, size: 100, costUsd: 0.01234 })?.detail).toBe(
      '1 of 100 tokens, $0.0123',
    );
  });

  it('never shows more than 100%', () => {
    expect(formatUsage({ used: 300, size: 100 })?.text).toBe('100%');
  });
});
```

Create `src/lib/assistant/__tests__/model-memory.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRememberedModel, rememberModel } from '@/lib/assistant/model-memory';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('model memory', () => {
  it('remembers one model per agent config', () => {
    rememberModel('a1', 'opus');
    rememberModel('a2', 'sonnet');
    expect(loadRememberedModel('a1')).toBe('opus');
    expect(loadRememberedModel('a2')).toBe('sonnet');
    expect(loadRememberedModel('a3')).toBeUndefined();
  });

  it('survives storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(() => rememberModel('a1', 'opus')).not.toThrow();
    expect(loadRememberedModel('a1')).toBeUndefined();
  });
});
```

Create `src/components/assistant/composer/__tests__/ComposerToolbar.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ConfigOption } from '@/lib/tauri-api';
import { ComposerToolbar, type ComposerToolbarProps } from '../ComposerToolbar';

const MODEL = {
  id: 'model',
  name: 'Model',
  currentValue: 'sonnet',
  choices: [
    { value: 'sonnet', name: 'Sonnet' },
    { value: 'opus', name: 'Opus' },
  ],
} as ConfigOption;
const EFFORT = {
  id: 'effort',
  name: 'Effort',
  currentValue: 'high',
  choices: [
    { value: 'low', name: 'Low' },
    { value: 'high', name: 'High' },
  ],
} as ConfigOption;

function setup(overrides: Partial<ComposerToolbarProps> = {}) {
  const props: ComposerToolbarProps = {
    mode: 'ask',
    onModeChange: vi.fn(),
    configOptions: [MODEL],
    onConfigChange: vi.fn(),
    usage: undefined,
    running: false,
    canSend: true,
    onSend: vi.fn(),
    onStop: vi.fn(),
    ...overrides,
  };
  render(<ComposerToolbar {...props} />);
  return { props, user: userEvent.setup() };
}

describe('ComposerToolbar', () => {
  it('changes the mode', async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Mode: Ask' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Edit/ }));
    expect(props.onModeChange).toHaveBeenCalledWith('edit');
  });

  it('changes the model through its config option', async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Model: Sonnet' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Opus/ }));
    expect(props.onConfigChange).toHaveBeenCalledWith('model', 'opus');
  });

  it('shows Effort only when the agent reports it', () => {
    setup();
    expect(screen.queryByRole('button', { name: /^Effort/ })).toBeNull();
  });

  it('shows Effort when present and hides Model when absent', () => {
    setup({ configOptions: [EFFORT] });
    expect(screen.getByRole('button', { name: 'Effort: High' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Model/ })).toBeNull();
  });

  it('shows the context used with tokens and cost for screen readers', () => {
    setup({ usage: { used: 12, size: 100, costUsd: 0.5 } });
    expect(screen.getByText('12%')).toBeInTheDocument();
    expect(screen.getByText(/12 of 100 tokens, \$0\.5000/)).toBeInTheDocument();
  });

  it('hides the context indicator for a zero size', () => {
    setup({ usage: { used: 0, size: 0 } });
    expect(screen.queryByText('0%')).toBeNull();
  });

  it('disables Send when there is nothing to send', () => {
    setup({ canSend: false });
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('turns Send into Stop while a turn runs', async () => {
    const { props, user } = setup({ running: true });
    expect(screen.queryByRole('button', { name: 'Send' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(props.onStop).toHaveBeenCalledTimes(1);
  });
});
```

Create `src/components/assistant/composer/__tests__/Composer.test.tsx`:

```tsx
import { EditorView } from '@codemirror/view';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRememberedModel } from '@/lib/assistant/model-memory';
import { loadPromptHistory } from '@/lib/assistant/prompt-history';
import { createDefaultLeaf } from '@/lib/pane-utils';
import {
  cancelAgentPrompt,
  type ConfigOption,
  getRequest,
  listCollections,
  listEnvironments,
  sendAgentPrompt,
  setAgentConfigOption,
  setAssistantMode,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { createDeferred } from '@/test/deferred';
import { Composer } from '../Composer';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  listCollections: vi.fn(),
  getCollectionSummaries: vi.fn(),
  listEnvironments: vi.fn(),
  getRequest: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  setAgentConfigOption: vi.fn(),
  setAssistantMode: vi.fn(),
}));

const MODEL = {
  id: 'model',
  name: 'Model',
  currentValue: 'sonnet',
  choices: [
    { value: 'sonnet', name: 'Sonnet' },
    { value: 'opus', name: 'Opus' },
  ],
} as ConfigOption;

function renderComposer() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const utils = render(
    <QueryClientProvider client={qc}>
      <Composer />
    </QueryClientProvider>,
  );
  const content = utils.container.querySelector('.cm-content') as HTMLElement;
  const view = EditorView.findFromDOM(
    utils.container.querySelector('.cm-editor') as HTMLElement,
  ) as EditorView;
  return { ...utils, content, view, user: userEvent.setup() };
}

function typeInto(view: EditorView, text: string) {
  act(() => {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
  });
}

function pressEnter(content: HTMLElement) {
  act(() => {
    content.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        code: 'Enter',
        keyCode: 13,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(listCollections).mockResolvedValue([]);
  vi.mocked(listEnvironments).mockResolvedValue([]);
  vi.mocked(getRequest).mockResolvedValue({
    uid: 'u1',
    name: 'List orders',
    method: 'GET',
    url: 'https://api.test/orders',
    headers: [],
    auth: { authType: 'none' },
  });
  vi.mocked(sendAgentPrompt).mockReset().mockResolvedValue('end_turn');
  vi.mocked(cancelAgentPrompt).mockReset().mockResolvedValue(undefined);
  vi.mocked(setAgentConfigOption).mockReset();
  vi.mocked(setAssistantMode).mockReset().mockResolvedValue(undefined);
  usePaneStore.setState({ root: createDefaultLeaf('g1'), activeGroupId: 'g1' });
  useWorkspaceStore.setState({ activeWorkspaceId: 'w1' });
  useAssistantStore.setState({
    session: {
      sessionId: 's1',
      agentConfigId: 'a1',
      status: 'active',
      configOptions: [MODEL],
      mode: 'ask',
    },
    focus: { collection: 'shop', path: 'orders/list.yml' },
    usage: undefined,
    // A streaming reply left by an earlier test would count as a running turn.
    messages: [],
    proposals: [],
  });
});

describe('Composer', () => {
  it('shows the focus chip and lets the user remove it', async () => {
    const { user } = renderComposer();
    expect(screen.getByText('list')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove list' }));
    expect(screen.queryByText('list')).toBeNull();
  });

  it('sends the prompt with the focus chip as a masked resource', async () => {
    const { content, view } = renderComposer();
    typeInto(view, 'Explain it');
    pressEnter(content);
    await waitFor(() =>
      expect(sendAgentPrompt).toHaveBeenCalledWith('s1', 'Explain it', [
        expect.objectContaining({
          uri: 'rocket://request/shop/orders/list.yml',
          mimeType: 'text/plain',
        }),
      ]),
    );
    expect(loadPromptHistory('w1')).toEqual(['Explain it']);
    await waitFor(() => expect(view.state.doc.toString()).toBe(''));
  });

  it('sends without resources when no chip is left', async () => {
    const { content, view, user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Remove list' }));
    typeInto(view, 'hello');
    pressEnter(content);
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalledWith('s1', 'hello', undefined));
  });

  it('sends once when Enter is pressed twice quickly', async () => {
    const turn = createDeferred<string>();
    vi.mocked(sendAgentPrompt).mockReturnValue(turn.promise);
    const { content, view } = renderComposer();
    typeInto(view, 'hello');
    act(() => {
      for (let i = 0; i < 2; i++) {
        content.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13, bubbles: true, cancelable: true }),
        );
      }
    });
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalledTimes(1));
    await act(async () => {
      turn.resolve('end_turn');
    });
  });

  it('shows Stop while the turn runs and cancels it', async () => {
    const turn = createDeferred<string>();
    vi.mocked(sendAgentPrompt).mockReturnValue(turn.promise);
    const { content, view, user } = renderComposer();
    typeInto(view, 'hello');
    pressEnter(content);
    await user.click(await screen.findByRole('button', { name: 'Stop' }));
    expect(cancelAgentPrompt).toHaveBeenCalledWith('s1');
    await act(async () => {
      turn.resolve('cancelled');
      // The bridge does this on agent-session-finished; no bridge runs here.
      useAssistantStore.getState().completeMessage('s1');
    });
    expect(await screen.findByRole('button', { name: 'Send' })).toBeInTheDocument();
  });

  it('changes the model, stores the new options and remembers the choice', async () => {
    const effort = {
      id: 'effort',
      name: 'Effort',
      currentValue: 'high',
      choices: [{ value: 'high', name: 'High' }],
    } as ConfigOption;
    vi.mocked(setAgentConfigOption).mockResolvedValue([{ ...MODEL, currentValue: 'opus' }, effort]);
    const { user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Model: Sonnet' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Opus/ }));
    expect(setAgentConfigOption).toHaveBeenCalledWith('s1', 'model', 'opus');
    expect(await screen.findByRole('button', { name: 'Effort: High' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Model: Opus' })).toBeInTheDocument();
    expect(loadRememberedModel('a1')).toBe('opus');
  });

  it('switches the mode in the backend and the store', async () => {
    const { user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Mode: Ask' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Edit/ }));
    expect(setAssistantMode).toHaveBeenCalledWith('s1', 'edit');
    expect(await screen.findByRole('button', { name: 'Mode: Edit' })).toBeInTheDocument();
  });
});
```

Create `src/stores/__tests__/assistant-store.set-mode.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';

beforeEach(() => {
  useAssistantStore.setState({ session: undefined });
});

describe('assistant-store setMode', () => {
  it('changes the mode of the current session', () => {
    useAssistantStore.setState({
      session: { sessionId: 's1', agentConfigId: 'a1', status: 'active', configOptions: [], mode: 'ask' },
    });
    useAssistantStore.getState().setMode('agent');
    expect(useAssistantStore.getState().session?.mode).toBe('agent');
  });

  it('does nothing without a session', () => {
    useAssistantStore.getState().setMode('edit');
    expect(useAssistantStore.getState().session).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run the tests to see them fail (for the user to run)**

```bash
yarn test src/components/assistant/composer src/lib/assistant src/stores/__tests__/assistant-store.set-mode.test.ts
```

Expected: FAIL for the five new files (modules or `setMode` missing).

- [ ] **Step 3: Write the usage text and the model memory**

Create `src/components/assistant/composer/usage-format.ts`:

```ts
export interface UsageInput {
  used: number;
  size: number;
  costUsd?: number;
}

export interface UsageView {
  percent: number;
  /** Short text shown in the toolbar, such as `12%`. */
  text: string;
  /** Token counts and cost, shown on hover and to screen readers. */
  detail: string;
}

/** The context-used indicator, or null to hide it (no usage yet, or a zero size). */
export function formatUsage(usage?: UsageInput): UsageView | null {
  if (!usage || usage.size <= 0) return null;
  const percent = Math.min(100, Math.round((usage.used / usage.size) * 100));
  const tokens = `${usage.used.toLocaleString('en-US')} of ${usage.size.toLocaleString('en-US')} tokens`;
  const cost = typeof usage.costUsd === 'number' ? `, $${usage.costUsd.toFixed(4)}` : '';
  return { percent, text: `${percent}%`, detail: `${tokens}${cost}` };
}
```

Create `src/lib/assistant/model-memory.ts`:

```ts
const keyFor = (agentConfigId: string) => `rocket-api:assistant-model:${agentConfigId}`;

/** The model last chosen for this agent config, used when the next session starts. */
export function loadRememberedModel(agentConfigId: string): string | undefined {
  try {
    return localStorage.getItem(keyFor(agentConfigId)) ?? undefined;
  } catch {
    // Blocked storage means no remembered model. The agent's default is used.
    return undefined;
  }
}

export function rememberModel(agentConfigId: string, model: string): void {
  try {
    localStorage.setItem(keyFor(agentConfigId), model);
  } catch {
    // Blocked storage only loses the remembered choice.
  }
}
```

- [ ] **Step 4: Write `ComposerToolbar`**

Create `src/components/assistant/composer/ComposerToolbar.tsx`:

```tsx
import { ChevronDown, Send, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { ConfigOption } from '@/lib/tauri-api';
import { formatUsage, type UsageInput } from './usage-format';

export type AssistantModeValue = 'ask' | 'edit' | 'agent';

export const MODEL_OPTION_ID = 'model';
export const EFFORT_OPTION_ID = 'effort';

export const MODE_OPTIONS: readonly {
  value: AssistantModeValue;
  label: string;
  description: string;
}[] = [
  { value: 'ask', label: 'Ask', description: 'Reads the workspace and answers.' },
  { value: 'edit', label: 'Edit', description: 'Also proposes changes for you to accept.' },
  {
    value: 'agent',
    label: 'Agent',
    description: 'Also runs requests in collections that allow it.',
  },
];

interface PickerChoice {
  value: string;
  label: string;
  description?: string | null;
}

interface PickerProps {
  title: string;
  value: string;
  choices: readonly PickerChoice[];
  onChange(value: string): void;
  disabled?: boolean;
}

function Picker({ title, value, choices, onChange, disabled }: PickerProps) {
  const shown = choices.find((choice) => choice.value === value)?.label ?? value;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant='ghost'
          size='sm'
          className='h-7 gap-1 px-2 text-xs'
          aria-label={`${title}: ${shown}`}
          disabled={disabled}
        >
          <span className='max-w-32 truncate'>{shown}</span>
          <ChevronDown className='size-3 opacity-60' />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='start' side='top'>
        <DropdownMenuLabel className='text-xs'>{title}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          {choices.map((choice) => (
            <DropdownMenuRadioItem
              key={choice.value}
              value={choice.value}
              className='flex-col items-start'
            >
              <span>{choice.label}</span>
              {choice.description ? (
                <span className='text-xs text-muted-foreground'>{choice.description}</span>
              ) : null}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function toChoices(option: ConfigOption): PickerChoice[] {
  return option.choices.map((choice) => ({
    value: choice.value,
    label: choice.name,
    description: choice.description,
  }));
}

export interface ComposerToolbarProps {
  mode: AssistantModeValue;
  onModeChange(mode: AssistantModeValue): void;
  configOptions: readonly ConfigOption[];
  onConfigChange(configId: string, value: string): void;
  usage?: UsageInput;
  running: boolean;
  canSend: boolean;
  disabled?: boolean;
  onSend(): void;
  onStop(): void;
}

/** Mode, Model and Effort pickers, the context-used indicator and Send or Stop. */
export function ComposerToolbar({
  mode,
  onModeChange,
  configOptions,
  onConfigChange,
  usage,
  running,
  canSend,
  disabled,
  onSend,
  onStop,
}: ComposerToolbarProps) {
  const model = configOptions.find((option) => option.id === MODEL_OPTION_ID);
  const effort = configOptions.find((option) => option.id === EFFORT_OPTION_ID);
  const usageView = formatUsage(usage);
  const pickersDisabled = disabled || running;

  return (
    <div className='flex items-center gap-1 border-t px-1.5 py-1'>
      <Picker
        title='Mode'
        value={mode}
        choices={MODE_OPTIONS}
        disabled={pickersDisabled}
        onChange={(value) => {
          const next = MODE_OPTIONS.find((option) => option.value === value);
          if (next) onModeChange(next.value);
        }}
      />
      {model && model.choices.length > 0 ? (
        <Picker
          title='Model'
          value={model.currentValue}
          choices={toChoices(model)}
          disabled={pickersDisabled}
          onChange={(value) => onConfigChange(model.id, value)}
        />
      ) : null}
      {effort && effort.choices.length > 0 ? (
        <Picker
          title='Effort'
          value={effort.currentValue}
          choices={toChoices(effort)}
          disabled={pickersDisabled}
          onChange={(value) => onConfigChange(effort.id, value)}
        />
      ) : null}
      <div className='flex-1' />
      {usageView ? (
        <TooltipProvider delayDuration={200}>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className='px-1 text-xs tabular-nums text-muted-foreground'>
                <span>{usageView.text}</span>
                <span className='sr-only'>{` context used, ${usageView.detail}`}</span>
              </span>
            </TooltipTrigger>
            <TooltipContent>{usageView.detail}</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      ) : null}
      {running ? (
        <Button
          variant='secondary'
          size='icon'
          className='size-7'
          aria-label='Stop'
          onClick={onStop}
        >
          <Square className='size-3.5' />
        </Button>
      ) : (
        <Button
          size='icon'
          className='size-7'
          aria-label='Send'
          disabled={!canSend}
          onClick={onSend}
        >
          <Send className='size-3.5' />
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Add `setMode` to the assistant store (only if missing)**

Run `grep -n "setMode" src/stores/assistant-store.ts`. If it prints nothing, add to the state interface, next to `setConfigOptions`:

```ts
  setMode: (mode: 'ask' | 'edit' | 'agent') => void;
```

and to the `create(...)` body, next to the `setConfigOptions` implementation:

```ts
  setMode: (mode) => set((s) => (s.session ? { session: { ...s.session, mode } } : {})),
```

If Plan 05 already added an equivalent action under another name, use that name in `Composer.tsx` and in the test file above instead, and record a `Ruling:` line.

- [ ] **Step 6: Write `Composer`**

First give Plan 05's send flow a `resources` argument. In `src/lib/assistant/assistant-session.ts`, change `sendAssistantMessage` to:

```ts
export async function sendAssistantMessage(
  text: string,
  resources?: PromptResourceDto[],
): Promise<void> {
  const trimmed = text.trim();
  const store = useAssistantStore.getState();
  const session = store.session;
  if (!trimmed || session?.status !== 'active') return;
  // The store refuses a second turn while one runs, so a double send stops here.
  if (!store.appendUserMessage(trimmed)) return;
  try {
    await sendAgentPrompt(session.sessionId, trimmed, resources);
  } catch (err) {
    useAssistantStore.getState().failMessage(session.sessionId, String(err));
  }
}
```

and add `type PromptResourceDto` to its `@/lib/tauri-api` import. In `src/lib/assistant/__tests__/assistant-session.test.ts`, the test `sends one prompt per turn` now expects `toHaveBeenCalledWith('s1', 'hi', undefined)`.

Create `src/components/assistant/composer/Composer.tsx`. It calls `sendAssistantMessage` (never `sendAgentPrompt` directly), and treats a turn as running while its own send is in flight or while the store's reply still streams (`selectTurnRunning`), so Stop also shows for a turn the store knows about:

```tsx
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { chipToResource } from '@/lib/assistant/chip-resources';
import { rememberModel } from '@/lib/assistant/model-memory';
import {
  appendPromptHistory,
  loadPromptHistory,
  savePromptHistory,
} from '@/lib/assistant/prompt-history';
import { sendAssistantMessage } from '@/lib/assistant/assistant-session';
import { filterSlashCommands } from '@/lib/assistant/slash-commands';
import type { ReferenceItem } from '@/lib/assistant/types';
import {
  type ConfigOption,
  cancelAgentPrompt,
  setAgentConfigOption,
  setAssistantMode,
} from '@/lib/tauri-api';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { addChip, chipKey, type ComposerChip, MAX_CHIPS, removeChip } from './chips';
import { ComposerChips } from './ComposerChips';
import { type AssistantModeValue, ComposerToolbar, MODEL_OPTION_ID } from './ComposerToolbar';
import { PromptEditor } from './PromptEditor';
import { filterReferences, focusReference, lastResponseReference } from './reference-source';
import { useReferenceItems } from './useReferenceItems';

const NO_OPTIONS: ConfigOption[] = [];

/**
 * The AI Assistant prompt area: chips, the prompt editor and the toolbar. Chips become
 * masked text resources, then Plan 05's `sendAssistantMessage` runs the turn.
 */
export function Composer() {
  const sessionId = useAssistantStore((s) => s.session?.sessionId);
  const sessionActive = useAssistantStore((s) => s.session?.status === 'active');
  const agentConfigId = useAssistantStore((s) => s.session?.agentConfigId);
  const mode = useAssistantStore((s) => s.session?.mode ?? 'ask');
  const configOptions = useAssistantStore((s) => s.session?.configOptions) ?? NO_OPTIONS;
  const usage = useAssistantStore((s) => s.usage);
  const focus = useAssistantStore((s) => s.focus);
  const turnRunning = useAssistantStore(selectTurnRunning);
  const setConfigOptions = useAssistantStore((s) => s.setConfigOptions);
  const setMode = useAssistantStore((s) => s.setMode);
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);

  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  // A turn runs while this composer's send is in flight or the store's reply still streams.
  const running = sending || turnRunning;
  const [extraChips, setExtraChips] = useState<ComposerChip[]>([]);
  const [dismissedFocusKey, setDismissedFocusKey] = useState<string | null>(null);
  const [history, setHistory] = useState<string[]>(() => loadPromptHistory(workspaceId));
  // Refs answer "what is typed" and "is a turn running" at once, before React re-renders,
  // so two quick Enters cannot start two turns.
  const textRef = useRef('');
  const runningRef = useRef(false);

  // Each workspace keeps its own prompt history.
  useEffect(() => {
    setHistory(loadPromptHistory(workspaceId));
  }, [workspaceId]);

  const referenceItems = useReferenceItems();

  // The pane store is read only to label the focus chip with the tab title.
  const focusItem = useMemo(() => focusReference(focus, usePaneStore.getState().root), [focus]);

  const chips = useMemo<ComposerChip[]>(() => {
    if (!focusItem || chipKey(focusItem) === dismissedFocusKey) return extraChips;
    return [{ key: chipKey(focusItem), item: focusItem, focus: true }, ...extraChips];
  }, [focusItem, dismissedFocusKey, extraChips]);

  const referenceSource = useCallback(
    (query: string) => {
      const lastResponse = lastResponseReference(focus, usePaneStore.getState().root);
      return filterReferences(
        lastResponse ? [lastResponse, ...referenceItems] : referenceItems,
        query,
      );
    },
    [focus, referenceItems],
  );

  const handleChange = (next: string) => {
    textRef.current = next;
    setText(next);
  };

  const handleReferencePicked = (item: ReferenceItem) => {
    const result = addChip(chips, item);
    if (result.outcome === 'limit') {
      toast.warning(`A message can carry at most ${MAX_CHIPS} references.`);
      return;
    }
    if (result.outcome === 'added') setExtraChips(result.chips.filter((chip) => !chip.focus));
  };

  const handleRemoveChip = (key: string) => {
    if (chips.some((chip) => chip.key === key && chip.focus)) {
      setDismissedFocusKey(key);
      return;
    }
    setExtraChips((current) => removeChip(current, key));
  };

  const handleHistoryCommit = (prompt: string) => {
    const next = appendPromptHistory(history, prompt);
    setHistory(next);
    savePromptHistory(workspaceId, next);
  };

  const handleSend = async () => {
    const prompt = textRef.current.trim();
    if (!sessionId || !sessionActive || runningRef.current || running || prompt === '') return;
    runningRef.current = true;
    setSending(true);
    const outgoing = chips;
    textRef.current = '';
    setText('');
    setExtraChips([]);
    // The focus chip comes back for the next message.
    setDismissedFocusKey(null);
    try {
      const resources = await Promise.all(outgoing.map((chip) => chipToResource(chip.item)));
      // Plan 05's flow adds the user message, opens the streaming reply, refuses a
      // second turn and marks the reply failed when the send throws.
      await sendAssistantMessage(prompt, resources.length > 0 ? resources : undefined);
    } catch (err) {
      toast.error(`The message could not be sent: ${String(err)}`);
    } finally {
      runningRef.current = false;
      setSending(false);
    }
  };

  const handleStop = () => {
    if (!sessionId) return;
    cancelAgentPrompt(sessionId).catch((err) => {
      toast.error(`Could not stop the turn: ${String(err)}`);
    });
  };

  const handleModeChange = async (next: AssistantModeValue) => {
    if (!sessionId || next === mode) return;
    try {
      await setAssistantMode(sessionId, next);
      setMode(next);
    } catch (err) {
      toast.error(`Could not switch to ${next} mode: ${String(err)}`);
    }
  };

  const handleConfigChange = async (configId: string, value: string) => {
    if (!sessionId) return;
    try {
      // The reply is the full option list, so Effort appears or disappears with the model.
      const options = await setAgentConfigOption(sessionId, configId, value);
      setConfigOptions(sessionId, options);
      if (configId === MODEL_OPTION_ID && agentConfigId) rememberModel(agentConfigId, value);
    } catch (err) {
      toast.error(`Could not change the ${configId}: ${String(err)}`);
    }
  };

  return (
    <div className='m-2 rounded-md border bg-dropdown-bg shadow-xs focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50 dark:bg-input/30'>
      <ComposerChips chips={chips} onRemove={handleRemoveChip} />
      <PromptEditor
        value={text}
        onChange={handleChange}
        onSubmit={() => void handleSend()}
        onStop={handleStop}
        running={running}
        placeholder='Ask about this workspace. Type # to add context or / for a template.'
        disabled={!sessionActive}
        history={history}
        onHistoryCommit={handleHistoryCommit}
        referenceSource={referenceSource}
        commandSource={filterSlashCommands}
        onReferencePicked={handleReferencePicked}
        aria-label='Message the AI assistant'
      />
      <ComposerToolbar
        mode={mode}
        onModeChange={(next) => void handleModeChange(next)}
        configOptions={configOptions}
        onConfigChange={(configId, value) => void handleConfigChange(configId, value)}
        usage={usage}
        running={running}
        canSend={sessionActive && !running && text.trim() !== ''}
        disabled={!sessionActive}
        onSend={() => void handleSend()}
        onStop={handleStop}
      />
    </div>
  );
}
```

- [ ] **Step 7: Replace the stub in `AssistantPanel` and apply the remembered model**

In `src/components/assistant/AssistantPanel.tsx`:

1. Replace the line `import { AssistantInputStub } from './AssistantInputStub';` with `import { Composer } from './composer/Composer';`.
2. Replace the whole `<AssistantInputStub ... />` element with `<Composer />`.
3. Remove every prop, state value and handler in `AssistantPanel.tsx` that only the stub used. `yarn tsc --noEmit` (unused locals) and `yarn check` (`noUnusedVariables`, `noUnusedImports`) list them.

Then delete the stub:

```bash
rm src/components/assistant/AssistantInputStub.tsx
grep -rn "AssistantInputStub" src
```

Expected: the grep prints nothing. Then update Plan 05's `src/components/assistant/__tests__/AssistantPanel.test.tsx`, which drove the stub:
- In `starts a session with the first agent and shows the message box`, query the composer with `await screen.findByRole('textbox', { name: 'Message the AI assistant' })` (CodeMirror's `.cm-content` has the `textbox` role and the label from `PromptEditor`'s `aria-label`), and expect `startWorkspaceAssistant` to have been called with `('agent-1', 'edit', undefined)` (see the remembered model below).
- Delete `sends the typed message and clears the box` and `sends on Enter and keeps Shift+Enter for a new line`: they typed into the stub's textarea, and `Composer.test.tsx` and `PromptEditor.test.tsx` cover sending, Enter and Shift+Enter.
- `stops a running turn` and `renders replies and one muted line per tool call` keep working, because `Composer` shows Stop while `selectTurnRunning` is true.

Apply the remembered model when a session starts. The only call is in `src/lib/assistant/assistant-session.ts` (`startAssistant`, Plan 05; confirm with `grep -rn "startWorkspaceAssistant(" src --include='*.ts' --include='*.tsx' | grep -v __tests__`). Add `import { loadRememberedModel } from '@/lib/assistant/model-memory';` there and change the call to `startWorkspaceAssistant(agentConfigId, mode, loadRememberedModel(agentConfigId))`. In `src/lib/assistant/__tests__/assistant-session.test.ts`, the test `starts a session and activates it with the reported options` now expects `toHaveBeenCalledWith('agent-1', 'edit', undefined)` (its `localStorage` is empty).

- [ ] **Step 8: Document the composer**

Create `.claude/assistant-composer.md`:

```markdown
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
- `chipToResource` builds an embedded text resource from Rocket's own data: open tabs first (unsaved edits included), otherwise the saved item. Secrets are masked, a final `redactKnownSecrets` pass covers free text, and each chip is capped at 8 KB. It never rejects.
- URIs: `rocket://<kind>/<collection>/<path>`.

## Storage

- Prompt history: `rocket-api:assistant-prompt-history:<workspaceId>`, last 50.
- Remembered model: `rocket-api:assistant-model:<agentConfigId>`, applied when the next session starts.
- Every access is in try/catch. Broken storage only loses the convenience.

## Toolbar

- Mode (Ask, Edit, Agent) calls `setAssistantMode`. Model and Effort come from the session's config options with ids `model` and `effort`, and a picker is hidden when its option is absent. `setAgentConfigOption` returns the full list, which replaces the store's list.
- The context indicator shows `used / size` as a percentage, with tokens and cost on hover. It is hidden when the size is 0.
```

In `CLAUDE.md`, after line 62 (`See \`.claude/folder-settings.md\` for the folder settings tab, ...`), add:

```markdown
See `.claude/assistant-composer.md` for the AI Assistant composer (PromptEditor keys, chips, masking, storage keys).
```

- [ ] **Step 9: Run the tests (for the user to run)**

```bash
yarn test src/components/assistant src/lib/assistant src/stores/__tests__/assistant-store.set-mode.test.ts
```

Expected: PASS for the five new files (23 tests), the Task 1 and Task 2 files, and the Plan 05 panel tests after the stub change.

- [ ] **Step 10: Verify types and lint**

```bash
yarn tsc --noEmit
yarn check
```

Expected: both clean, with no reference to `AssistantInputStub` left.

- [ ] **Step 11: Manual check (for the user, in `yarn tauri dev`)**

- The prompt grows line by line up to about 12 lines, then scrolls. Long lines wrap.
- Enter sends, Shift+Enter adds a line, Ctrl/Cmd+Enter sends the prompt and does not send the open HTTP request.
- Esc during a turn stops it, the button turns back into Send, and the session stays usable.
- Up on an empty prompt recalls the last prompt; history survives an app restart and is separate per workspace.
- `#` lists requests, folders, collections, environments and, with a response present, the last response. Picking one adds a chip; the ninth chip shows a warning.
- `/` at the start lists the five templates; picking one fills the editor.
- The focus chip shows the open request; a script edit made without saving reaches the agent (ask it to quote the pre-request script).
- Model switch mid-session works, Effort appears or disappears with the model, and a new session starts with the last chosen model.
- The context percentage appears after the first turn, with tokens and cost on hover.
- Autocomplete list and chips look right in light and dark themes.

- [ ] **Step 12: Commit**

```bash
git add src/components/assistant/composer/usage-format.ts src/lib/assistant/model-memory.ts src/components/assistant/composer/ComposerToolbar.tsx src/components/assistant/composer/Composer.tsx src/stores/assistant-store.ts src/components/assistant/AssistantPanel.tsx src/components/assistant/AssistantInputStub.tsx .claude/assistant-composer.md CLAUDE.md src/components/assistant/composer/__tests__/usage-format.test.ts src/lib/assistant/__tests__/model-memory.test.ts src/components/assistant/composer/__tests__/ComposerToolbar.test.tsx src/components/assistant/composer/__tests__/Composer.test.tsx src/stores/__tests__/assistant-store.set-mode.test.ts
```

Also add, by explicit path, `src/lib/assistant/assistant-session.ts`, `src/lib/assistant/__tests__/assistant-session.test.ts` and `src/components/assistant/__tests__/AssistantPanel.test.tsx`. `git add` of the deleted stub path stages its removal. Invoke the `dev-workflow-skills:1-git-commit` skill with the message `feat(assistant): add composer toolbar and replace the input stub`.

---

## Interface deviations

- `src/stores/assistant-store.ts` gains `setMode(mode)` (Plan 05 has none). It is additive: no locked name changes. The index lists no action that updates `session.mode` after `set_assistant_mode` succeeds.
- Plan 05's `sendAssistantMessage(text)` becomes `sendAssistantMessage(text, resources?)`, and `startAssistant` passes the remembered model as the third argument of `startWorkspaceAssistant`. `Composer` sends through `sendAssistantMessage`, not through `sendAgentPrompt` directly.
- `ReferenceItem` and `SlashCommandItem` are defined in `src/lib/assistant/types.ts`, so `src/lib/assistant/chip-resources.ts` does not import from a component file. `PromptEditor.tsx` re-exports both, so the index import path still works. Shapes are unchanged.
- `ReferenceItem.path` holds the environment name when `kind` is `environment`. The index left `path` optional without saying what it holds for environments.

## Next Plan

None. Plan 06 is the last plan of the series ([index](00-plan-index.md)). After the review below, the branch is ready for the user's manual checklist (spec section 8) and the merge decision, which the user makes.

## Post-Implementation Review

Before calling the series done, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan added or changed: `src/lib/assistant/*`, `src/components/assistant/composer/*`, the `setMode` change in `src/stores/assistant-store.ts`, the stub replacement in `src/components/assistant/AssistantPanel.tsx`, the remembered-model change at the `startWorkspaceAssistant(` call, `.claude/assistant-composer.md` and the `CLAUDE.md` line, plus their tests. It reviews the diff file `git diff <plan-06 base>..HEAD > /tmp/plan-06.diff`, not the live tree. It checks:

- interface gaps against the index's "Plan 06 — composer" contract and the Plan 01, 03 and 05 names actually consumed (every `Ruling:` line in the ledger is justified);
- that the five Review Focus items are pinned by the tests named above and that the tests would fail without the guarded code (for example, removing `runningRef` must fail "sends once when Enter is pressed twice quickly");
- masking completeness in `chip-resources.ts`: every value that reaches the resource text passes through `maskValue`, `redactKnownSecrets` or a secret check, and no error text is echoed;
- frontend rules: shadcn primitives only outside `PromptEditor`, `lucide-react` icons only, one narrow selector per store value, no CodeMirror outside `PromptEditor.tsx` and its helper modules, no ES2021+ APIs;
- that `PromptEditor` keeps the `SingleLineEditor` controlled-value contract (no `onChange` echo while syncing) and that `promptEditorTheme` wins the cascade over `rocketTheme`.

It may fix what it finds directly, then re-run `yarn tsc --noEmit` and `yarn check`. Review Focus behaviour is settled by the spec (section 5, decisions 6 to 8) and must not be changed by the reviewer.
