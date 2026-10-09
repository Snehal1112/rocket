# Variable Preview in Flow Editors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `{{variable}}` fields in the flow properties panel highlight, autocomplete and show the resolved value on hover, without letting the click popover edit the wrong environment.

**Architecture:** The flow editors get the same scope-aware variable map the request editors use, through a small React context filled by `FlowVariableScope` (which calls `useCollectionVariableContext` from plan P3). `SingleLineEditor` gains two opt-in props: `readOnlyVariables` (the click popover cannot save) and `hoverPreview` (a new CodeMirror hover tooltip, built in `extensions/variable-hover.ts`). Both are off by default, so no other editor in the app changes. Frontend only.

**Tech Stack:** React, TypeScript, CodeMirror 6 (`@codemirror/view` `hoverTooltip`), Vitest and Testing Library.

**Spec:** Roadmap item F-43 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P13).

**Depends on:** plan P3 merged (`useCollectionVariableContext`, the `secret` flag on environment entries). Without the P3 flag the hover would show secret environment values.

## What was verified before writing

- Flow editors have no variable context today: `InlineSourceEditor.tsx:70,98,105` and `InputNodeEditor.tsx:20` render `SingleLineEditor` without `variableContext`.
- Correction to the design notes: `WireScriptDialog.tsx:127` is a JavaScript expression (`response.body.token`), not a `{{variable}}` template. It is out of scope. So are the `SingleLineEditor`s in `IfNode`, `SwitchNode`, `RepeatUntilSection` and `WaitForCallbackEditor` (also JavaScript conditions).
- The Monaco body editor in `InlineSourceEditor.tsx:127-133` accepts a `variableContext` prop (`MonacoWrapper.tsx:23`) and already masks secrets in its hover (`MonacoWrapper.tsx:249-255`), so it gets the context too.
- The click popover saves through `useVariableCommit` (`src/hooks/useVariableCommit.ts:23-56`), which reads `useEnvStore.activeCollection`. A flow can belong to a different collection, so a popover edit in a flow editor would write to the wrong environment file. That is why `readOnlyVariables` is needed.
- Environment `extends` is stored (`tauri-api.ts:433`, `conversions/environment.rs:73,112`) but nothing in the frontend or the backend resolves it (searched `crates` and `src`). So the preview needs no flattening, and it matches what a run sees.
- `findVarTokenAt` (`variable-popover.ts:97-111`) is not exported yet. `hoverTooltip` is not used anywhere in the app yet. `tooltips({ parent: document.body })` is set in `SingleLineEditor.tsx:123`, and `rocketTooltipBase` (`extensions/theme.ts:194`) makes `.cm-tooltip` borderless and transparent, so the hover DOM draws its own card.
- Existing tests that would break if the panel called the hook without a `QueryClientProvider`: `NodePropertiesPanel.test.tsx` and six `FlowPane.*.test.tsx` files that open Input or Request nodes. Task 1 mocks the hook in each.

Scope limits the preview cannot remove (documented in Task 3): an inline request does not inherit folder or request variables (the backend gives it none, `flow_execution_service.rs:494-507`); a saved request gets per-node folder and request variables a static editor cannot know; runtime and wire values do not exist at edit time. The preview shows dynamic, process, global, collection, vault (masked) and environment scopes.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Single-line variable-aware fields are `SingleLineEditor` (CodeMirror 6); multi-line editors are Monaco.
- Zustand: never fully destructure store state at component top level.
- No Rust changes.
- A secret value must never be placed in the DOM, a `title` attribute, a `data-*` attribute or a log line. The hover DOM is built with `textContent` only.
- Before starting Task 1, read `docs/superpowers/specs/opencollection-spec-reference.md` (variable resolution scope).
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `SingleLineEditor.tsx`, `NodePropertiesPanel.tsx` and `InlineSourceEditor.tsx`.
- Not in scope: hover in the request editors or the URL bar, the `AuthEditor` fields inside `AuthNodeEditor` (they already receive a context and keep their old popover behavior), per-node folder and request scope for saved requests.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A secret value reaches the hover DOM, a `title` or a `data-*` attribute. Pinned in Task 2 with a canary value, a whole-tooltip text check and an attribute scan.
2. The click popover in a flow editor saves to the active collection's environment, not the flow's collection. Pinned in Task 1 (`readOnlyVariables` blocks the commit, with an editable control case).
3. The hover shows while the click popover is open, on a token edge the pointer is not over, or off a token. Pinned in Task 2.
4. A variable value containing HTML renders as markup. Pinned in Task 2 (`renders the value as text`).
5. The hover appears in every editor in the app, not only the flow editors. Pinned in Task 3 (the extension is installed only when `hoverPreview` is set).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/components/flow/properties/flowVariableContext.ts` (new) | React context, `FlowVariableContextProvider`, `useFlowVariableContext()`. |
| `src/components/flow/properties/FlowVariableScope.tsx` (new) | Calls `useCollectionVariableContext(collection)` and provides the map. |
| `src/components/editor/VariablePopover.tsx` (modify) | New `readOnly` prop. |
| `src/components/editor/SingleLineEditor.tsx` (modify) | New `readOnlyVariables` and `hoverPreview` props. |
| `src/components/editor/extensions/variable-popover.ts` (modify) | Export `findVarTokenAt`. |
| `src/components/editor/extensions/variable-hover.ts` (new) | `variableHover()`, `variableHoverSource`, `buildVariableHoverDom`. |
| `src/components/editor/extensions/index.ts` (modify) | Export `variableHover`. |
| `src/components/flow/properties/NodePropertiesPanel.tsx` (modify) | Wraps the Input and Request editors in `FlowVariableScope`. |
| `src/components/flow/properties/InlineSourceEditor.tsx`, `InputNodeEditor.tsx` (modify) | Pass the context, `readOnlyVariables` and `hoverPreview`. |
| `.claude/flow-variable-preview.md` (new) | Short note on what the preview can and cannot show. |

Interfaces:

```ts
// flowVariableContext.ts
export const FlowVariableContextProvider: React.Provider<Map<string, VariableScopeEntry> | undefined>;
export function useFlowVariableContext(): Map<string, VariableScopeEntry> | undefined;

// SingleLineEditor props added
readOnlyVariables?: boolean; // the click popover shows the value but cannot save
hoverPreview?: boolean;      // show the value on hover (needs variableContext)

// variable-hover.ts
export function variableHover(): Extension;
export function variableHoverSource(view: EditorView, pos: number, side: -1 | 1): Tooltip | null;
export function buildVariableHoverDom(varName: string, entry: VariableScopeEntry | undefined): HTMLElement;
export function truncateForHover(value: string): string;
```

---

### Task 1: Pass the variable context into the flow editors, with a read-only click popover

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/components/flow/properties/flowVariableContext.ts`
- Create: `src/components/flow/properties/FlowVariableScope.tsx`
- Modify: `src/components/editor/VariablePopover.tsx` (props near line 11-25, `readOnly` at line 90)
- Modify: `src/components/editor/SingleLineEditor.tsx` (props, `handlePopoverCommit` at line 288, popover render at line 322)
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (imports, `editorFor` cases `Request` and `Input`)
- Modify: `src/components/flow/properties/InlineSourceEditor.tsx` (three `SingleLineEditor`s and the `MonacoWrapper`)
- Modify: `src/components/flow/properties/InputNodeEditor.tsx`
- Create: `src/components/editor/__tests__/SingleLineEditor.readOnlyVariables.test.tsx`
- Modify: `src/components/editor/__tests__/VariablePopover.test.tsx` (created in plan P3)
- Modify: `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx`
- Modify: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
- Modify (hook mock only): `src/components/flow/__tests__/FlowPane.dblclick.test.tsx`, `FlowPane.delete.test.tsx`, `FlowPane.dragdrop.test.tsx`, `FlowPane.properties.test.tsx`, `FlowPane.requestFocus.test.tsx`, `FlowPane.wireEdit.test.tsx`

**Interfaces:**
- Produces: `FlowVariableContextProvider`, `useFlowVariableContext`, `FlowVariableScope`, and the `readOnlyVariables` prop.
- Consumes: `useCollectionVariableContext` (P3).

- [ ] **Step 1: Write the failing popover test**

Append to `src/components/editor/__tests__/VariablePopover.test.tsx` (created by plan P3; reuse its `entry` helper and `SECRET`) a new `describe`, and add `userEvent` to the imports:

```tsx
import userEvent from '@testing-library/user-event';
```

```tsx
describe('VariablePopover readOnly prop', () => {
  it('is editable for an environment variable by default', () => {
    renderPopover(entry());
    expect(screen.getByRole('textbox')).not.toHaveAttribute('readonly');
  });

  it('is read-only when the editor cannot save the change', async () => {
    const onCommit = vi.fn(async () => undefined);
    render(
      <VariablePopover
        varName='apiKey'
        entry={entry()}
        tokenType='variable'
        readOnly
        onCommit={onCommit}
        onClose={vi.fn()}
      />,
    );
    const input = screen.getByRole('textbox');
    expect(input).toHaveAttribute('readonly');
    await userEvent.type(input, 'x{Enter}');
    expect(onCommit).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Write the failing `SingleLineEditor` test**

Create `src/components/editor/__tests__/SingleLineEditor.readOnlyVariables.test.tsx`:

```tsx
import { EditorView } from '@codemirror/view';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { openPopoverEffect } from '../extensions';
import { SingleLineEditor } from '../SingleLineEditor';

const commit = vi.hoisted(() => vi.fn(async () => undefined));
vi.mock('@/hooks/useVariableCommit', () => ({ useVariableCommit: () => commit }));

const entry: VariableScopeEntry = {
  value: 'https://a.test',
  source: 'environment',
  label: 'dev',
  secret: false,
};
const context = new Map([['baseUrl', entry]]);

// Opens the click popover the way a click on the token does.
function openPopover(container: HTMLElement) {
  const dom = container.querySelector<HTMLElement>('.cm-editor');
  const view = dom ? EditorView.findFromDOM(dom) : null;
  if (!view) throw new Error('The editor view was not found.');
  act(() => {
    view.dispatch({
      effects: openPopoverEffect.of({
        varName: 'baseUrl',
        from: 0,
        to: 11,
        tokenType: 'variable',
        entry,
      }),
    });
  });
}

async function popoverInput() {
  const dialog = await screen.findByRole('dialog', { hidden: true });
  return within(dialog).getByRole('textbox', { hidden: true });
}

describe('SingleLineEditor readOnlyVariables', () => {
  beforeEach(() => commit.mockClear());

  it('saves an edit made in the click popover by default', async () => {
    const { container } = render(
      <SingleLineEditor value='{{baseUrl}}' onChange={vi.fn()} variableContext={context} />,
    );
    openPopover(container);
    const input = await popoverInput();
    expect(input).not.toHaveAttribute('readonly');
    await userEvent.type(input, 'x{Enter}');
    expect(commit).toHaveBeenCalledWith('baseUrl', 'https://a.testx', 'environment');
  });

  it('shows the value but cannot save it when readOnlyVariables is set', async () => {
    const { container } = render(
      <SingleLineEditor
        value='{{baseUrl}}'
        onChange={vi.fn()}
        variableContext={context}
        readOnlyVariables
      />,
    );
    openPopover(container);
    const input = await popoverInput();
    expect(input).toHaveAttribute('readonly');
    expect(input).toHaveValue('https://a.test');
    await userEvent.type(input, 'x{Enter}');
    expect(commit).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Write the failing context tests**

In `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx`:

1. Replace the two `vi.mock` blocks for `@/components/editor` and `@/components/editor/MonacoWrapper` (lines 7-32) with versions that record their props:

```tsx
import type { VariableScopeEntry } from '@/lib/url-variables';
import { FlowVariableContextProvider } from '../flowVariableContext';

const seen = vi.hoisted(() => ({
  single: [] as {
    value: string;
    'aria-label'?: string;
    variableContext?: Map<string, unknown>;
    readOnlyVariables?: boolean;
    hoverPreview?: boolean;
  }[],
  monaco: [] as { variableContext?: Map<string, unknown> }[],
}));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
    variableContext?: Map<string, unknown>;
    readOnlyVariables?: boolean;
    hoverPreview?: boolean;
  }) => {
    seen.single.push(props);
    return (
      <input
        aria-label={props['aria-label']}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    );
  },
}));

// Monaco cannot run in jsdom. A textarea with the same value/onChange
// contract stands in for the body editor.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: {
    value: string;
    onChange?: (v: string) => void;
    variableContext?: Map<string, unknown>;
  }) => {
    seen.monaco.push(props);
    return (
      <textarea
        aria-label='Body'
        value={props.value}
        onChange={(e) => props.onChange?.(e.target.value)}
      />
    );
  },
}));
```

(Keep the existing top import line `import { render, screen } from '@testing-library/react';` etc. and move the two new imports with the others.)

2. Append at the end of the file:

```tsx
describe('InlineSourceEditor variable context', () => {
  const ctx = new Map<string, VariableScopeEntry>([
    ['baseUrl', { value: 'https://a.test', source: 'environment', label: 'dev', secret: false }],
  ]);
  const latest = (label: string) => [...seen.single].reverse().find((p) => p['aria-label'] === label);

  beforeEach(() => {
    seen.single.length = 0;
    seen.monaco.length = 0;
  });

  it('gives the URL and header editors the flow context, read-only for saving', () => {
    render(
      <FlowVariableContextProvider value={ctx}>
        <InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />
      </FlowVariableContextProvider>,
    );
    for (const label of ['URL', 'Header 1 name', 'Header 1 value']) {
      expect(latest(label)?.variableContext).toBe(ctx);
      expect(latest(label)?.readOnlyVariables).toBe(true);
    }
  });

  it('gives the body editor the flow context', () => {
    render(
      <FlowVariableContextProvider value={ctx}>
        <InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />
      </FlowVariableContextProvider>,
    );
    expect(seen.monaco.at(-1)?.variableContext).toBe(ctx);
  });

  it('passes no context outside a provider', () => {
    render(<InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />);
    expect(latest('URL')?.variableContext).toBeUndefined();
  });
});
```

Add `beforeEach` to the vitest import on line 3: `import { beforeEach, describe, expect, it, vi } from 'vitest';`.

In `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`:

1. Replace the `vi.mock('@/components/editor', ...)` block (lines 7-20) with:

```tsx
const scope = vi.hoisted(() => ({ variableContext: new Map<string, unknown>() }));
const editorProps = vi.hoisted(() => ({ last: null as null | Record<string, unknown> }));

vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => scope,
}));

// The real CodeMirror editor needs Tauri and react-query. A plain input with
// the same value/onChange contract is enough here.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => {
    editorProps.last = props as unknown as Record<string, unknown>;
    return (
      <input
        aria-label={props['aria-label']}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    );
  },
}));
```

2. Append inside the top-level `describe('NodePropertiesPanel', ...)`:

```tsx
  it('gives an Input node value editor the collection variables, read-only for saving', () => {
    scope.variableContext.set('user', {
      value: 'alice',
      source: 'environment',
      label: 'dev',
      secret: false,
    });
    renderPanel(node('in1', { kind: 'Input', label: 'User', value: '{{user}}' }));
    expect(editorProps.last?.variableContext).toBe(scope.variableContext);
    expect(editorProps.last?.readOnlyVariables).toBe(true);
  });
```

For each of the six files `FlowPane.dblclick.test.tsx`, `FlowPane.delete.test.tsx`, `FlowPane.dragdrop.test.tsx`, `FlowPane.properties.test.tsx`, `FlowPane.requestFocus.test.tsx` and `FlowPane.wireEdit.test.tsx` (all in `src/components/flow/__tests__/`), add this top-level statement directly below the import block. It keeps the panel from needing a `QueryClientProvider`:

```tsx
vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => ({ variableContext: new Map() }),
}));
```

(Each file already imports `vi` from `vitest`.)

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test src/components/editor/__tests__/VariablePopover.test.tsx src/components/editor/__tests__/SingleLineEditor.readOnlyVariables.test.tsx src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
Expected: FAIL. Cannot resolve `../flowVariableContext`; the popover `readOnly` prop and the editor prop do not exist yet.

- [ ] **Step 5: Create the context and the scope component**

Create `src/components/flow/properties/flowVariableContext.ts`:

```ts
import { createContext, useContext } from 'react';
import type { VariableScopeEntry } from '@/lib/url-variables';

// The variables a flow's collection offers, for the editors in the properties panel.
// Undefined outside a provider, which leaves an editor without highlighting.
const FlowVariableContext = createContext<Map<string, VariableScopeEntry> | undefined>(undefined);

export const FlowVariableContextProvider = FlowVariableContext.Provider;

export function useFlowVariableContext(): Map<string, VariableScopeEntry> | undefined {
  return useContext(FlowVariableContext);
}
```

Create `src/components/flow/properties/FlowVariableScope.tsx`:

```tsx
import type { ReactNode } from 'react';
import { useCollectionVariableContext } from '@/hooks/useCollectionVariableContext';
import { FlowVariableContextProvider } from './flowVariableContext';

// Provides the flow collection's variable scope to the editors below it.
export function FlowVariableScope({
  collection,
  children,
}: {
  collection: string;
  children: ReactNode;
}) {
  const { variableContext } = useCollectionVariableContext(collection);
  return <FlowVariableContextProvider value={variableContext}>{children}</FlowVariableContextProvider>;
}
```

- [ ] **Step 6: Add the `readOnly` prop to `VariablePopover`**

In `src/components/editor/VariablePopover.tsx`:

1. In `VariablePopoverProps`, after `tokenType`:

```tsx
  /** Forces the value read-only, for an editor whose scope the popover cannot save to. */
  readOnly?: boolean;
```

2. In the destructured parameters of `VariablePopover`, add `readOnly: forceReadOnly,` after `tokenType,`.

3. Replace the `readOnly` constant:

```tsx
  const readOnly = forceReadOnly || resolvedEntry?.secret || !isEditable(resolvedEntry);
```

- [ ] **Step 7: Add `readOnlyVariables` to `SingleLineEditor`**

In `src/components/editor/SingleLineEditor.tsx`:

1. In `SingleLineEditorProps`, after the `onNavigateToSource` prop:

```tsx
  /**
   * When true, the click popover shows the value but cannot save it. Use it where
   * the variable scope is not the active collection's, such as the flow editors.
   */
  readOnlyVariables?: boolean;
```

2. Add `readOnlyVariables,` to the destructured parameters (after `onNavigateToSource,`).

3. In `handlePopoverCommit`, add as the first line of the callback body and add the prop to the dependency array:

```tsx
      if (readOnlyVariables) return;
```
```tsx
    [commitVariable, onPathParamChange, readOnlyVariables],
```

4. In the `<VariablePopover` element, add `readOnly={readOnlyVariables}` after `tokenType`.

- [ ] **Step 8: Wire the flow editors and the panel**

In `src/components/flow/properties/InlineSourceEditor.tsx`:

1. Add the import `import { useFlowVariableContext } from './flowVariableContext';` next to the other relative imports.
2. At the top of the component, after `const refocusPanel = usePanelRefocus();`:

```tsx
  const variableContext = useFlowVariableContext();
```
3. On each of the three `SingleLineEditor` elements (URL, header name, header value) add:

```tsx
          variableContext={variableContext}
          readOnlyVariables
```
(use the indentation of the neighbouring props), and on the `MonacoWrapper` add `variableContext={variableContext}`.

In `src/components/flow/properties/InputNodeEditor.tsx`: add `import { useFlowVariableContext } from './flowVariableContext';`, call `const variableContext = useFlowVariableContext();` at the top of the component, and add `variableContext={variableContext}` and `readOnlyVariables` to the `SingleLineEditor`.

In `src/components/flow/properties/NodePropertiesPanel.tsx`: add `import { FlowVariableScope } from './FlowVariableScope';`, then wrap the two cases in `editorFor`:

```tsx
    case 'Request':
      return (
        <FlowVariableScope collection={collection}>
          <RequestNodeEditor
            // Keyed by node, so a pending confirmation never carries over to another node.
            key={node.id}
            nodeId={node.id}
            kind={kind}
            edges={edges}
            collection={collection}
            onChange={onChange}
          />
        </FlowVariableScope>
      );
    case 'Input':
      return (
        <FlowVariableScope collection={collection}>
          <InputNodeEditor kind={kind} onChange={onChange} />
        </FlowVariableScope>
      );
```

- [ ] **Step 9: Run to verify the tests pass**

Run: `yarn test src/components/editor src/components/flow`
Expected: PASS. The six `FlowPane.*` files and `NodePropertiesPanel.auth.test.tsx` must still pass (the auth test renders Auth nodes only).

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/properties/flowVariableContext.ts src/components/flow/properties/FlowVariableScope.tsx src/components/editor/VariablePopover.tsx src/components/editor/SingleLineEditor.tsx src/components/flow/properties/NodePropertiesPanel.tsx src/components/flow/properties/InlineSourceEditor.tsx src/components/flow/properties/InputNodeEditor.tsx src/components/editor/__tests__/SingleLineEditor.readOnlyVariables.test.tsx src/components/editor/__tests__/VariablePopover.test.tsx src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx src/components/flow/__tests__/FlowPane.dblclick.test.tsx src/components/flow/__tests__/FlowPane.delete.test.tsx src/components/flow/__tests__/FlowPane.dragdrop.test.tsx src/components/flow/__tests__/FlowPane.properties.test.tsx src/components/flow/__tests__/FlowPane.requestFocus.test.tsx src/components/flow/__tests__/FlowPane.wireEdit.test.tsx`
Suggested subject: `feat(flow): give flow editors the collection variable context`.

---

### Task 2: The hover tooltip extension

**Files:**
- Modify: `src/components/editor/extensions/variable-popover.ts` (export `findVarTokenAt` at line 97)
- Create: `src/components/editor/extensions/variable-hover.ts`
- Modify: `src/components/editor/extensions/index.ts`
- Create: `src/components/editor/extensions/__tests__/variable-hover.test.ts`

**Interfaces:**
- Produces: `variableHover`, `variableHoverSource`, `buildVariableHoverDom`, `truncateForHover` as in the File Structure.
- Consumes: `variableContextField`, `getActivePopover`, `findVarTokenAt`.

Behavior, fixed here:

- The tooltip appears 300 ms after the pointer rests on a `{{name}}` token. It shows a badge with the scope label (`entry.label`, coloured by `sourceBadgeClass`) and the value in a monospace line.
- A secret entry shows `●●●●`. An empty value shows `(empty)`. A value longer than 200 characters is cut and ends with `…`.
- An unknown name shows `Unresolved`. A known dynamic name (`{{$guid}}`) shows `Dynamic ($guid), generated at send`; an unknown `$name` is unresolved.
- It never shows while the click popover is open.
- At the very edge of a token the pointer must be over the token: at `from` only with `side` `1`, at `to` only with `side` `-1`.
- The DOM is built with `textContent` only. No `title`, no `data-*`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/editor/extensions/__tests__/variable-hover.test.ts`:

```ts
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { setVariableContextEffect, variableContextField } from '../variable-context-facet';
import {
  buildVariableHoverDom,
  truncateForHover,
  variableHover,
  variableHoverSource,
} from '../variable-hover';
import { findVarTokenAt, openPopoverEffect, variablePopoverExtension } from '../variable-popover';

const CANARY = 'sk-live-canary-do-not-show-9f3a';

const entry = (over: Partial<VariableScopeEntry> = {}): VariableScopeEntry => ({
  value: 'https://api.test',
  source: 'environment',
  label: 'dev',
  secret: false,
  ...over,
});

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(doc: string, context: Map<string, VariableScopeEntry>) {
  container = document.createElement('div');
  document.body.appendChild(container);
  const state = EditorState.create({
    doc,
    extensions: [variableContextField, variablePopoverExtension(), variableHover()],
  });
  view = new EditorView({ state, parent: container });
  view.dispatch({ effects: setVariableContextEffect.of(context) });
  return view;
}

// The tooltip DOM for the token at `pos`, or null when there is none.
function hoverDom(v: EditorView, pos: number, side: -1 | 1 = 1): HTMLElement | null {
  const tip = variableHoverSource(v, pos, side);
  return tip ? tip.create(v).dom : null;
}

describe('findVarTokenAt', () => {
  it('finds the token under a position, inclusive of both edges', () => {
    expect(findVarTokenAt('a {{host}} b', 5)).toEqual({ varName: 'host', from: 2, to: 10 });
    expect(findVarTokenAt('a {{host}} b', 2)?.varName).toBe('host');
    expect(findVarTokenAt('a {{host}} b', 10)?.varName).toBe('host');
    expect(findVarTokenAt('a {{host}} b', 11)).toBeNull();
  });
});

describe('truncateForHover', () => {
  it('keeps a short value and cuts a long one with an ellipsis', () => {
    expect(truncateForHover('short')).toBe('short');
    const cut = truncateForHover('a'.repeat(500));
    expect(cut.length).toBe(201);
    expect(cut.endsWith('…')).toBe(true);
  });
});

describe('buildVariableHoverDom', () => {
  it('shows the scope label and the value of a resolved variable', () => {
    const dom = buildVariableHoverDom('host', entry());
    expect(dom.textContent).toContain('dev');
    expect(dom.textContent).toContain('https://api.test');
  });

  it('masks a secret and keeps the value out of the whole DOM, including attributes', () => {
    const dom = buildVariableHoverDom('apiKey', entry({ value: CANARY, secret: true }));
    expect(dom.textContent).toContain('●●●●');
    expect(dom.textContent).not.toContain(CANARY);
    expect(dom.outerHTML).not.toContain(CANARY);
  });

  it('sets no title and no data attributes on any element', () => {
    const dom = buildVariableHoverDom('host', entry({ value: CANARY }));
    for (const el of [dom, ...Array.from(dom.querySelectorAll('*'))]) {
      for (const name of el.getAttributeNames()) {
        expect(name === 'title' || name.startsWith('data-')).toBe(false);
      }
    }
  });

  it('renders the value as text, never as markup', () => {
    const dom = buildVariableHoverDom('x', entry({ value: '<img src=x onerror=alert(1)>' }));
    expect(dom.querySelector('img')).toBeNull();
    expect(dom.textContent).toContain('<img src=x onerror=alert(1)>');
  });

  it('shows an empty value as (empty)', () => {
    expect(buildVariableHoverDom('x', entry({ value: '' })).textContent).toContain('(empty)');
  });

  it('shows Unresolved for a missing variable', () => {
    expect(buildVariableHoverDom('nope', undefined).textContent).toContain('Unresolved');
  });

  it('describes a known dynamic variable and treats an unknown $name as unresolved', () => {
    expect(buildVariableHoverDom('$guid', undefined).textContent).toContain(
      'Dynamic ($guid), generated at send',
    );
    expect(buildVariableHoverDom('$notAThing', undefined).textContent).toContain('Unresolved');
  });

  it('truncates a long value', () => {
    const dom = buildVariableHoverDom('x', entry({ value: 'a'.repeat(500) }));
    expect(dom.textContent).not.toContain('a'.repeat(250));
    expect(dom.textContent).toContain('…');
  });
});

describe('variableHoverSource', () => {
  const ctx = new Map([['host', entry()]]);

  it('returns a tooltip anchored to the token, for a position inside it', () => {
    const v = createView('go {{host}} now', ctx);
    const tip = variableHoverSource(v, 6, 1);
    expect(tip).not.toBeNull();
    expect(tip?.pos).toBe(3);
    expect(tip?.end).toBe(11);
    expect(hoverDom(v, 6)?.textContent).toContain('https://api.test');
  });

  it('returns nothing off a token', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 1, 1)).toBeNull();
    expect(variableHoverSource(v, 13, 1)).toBeNull();
  });

  it('needs the pointer on the token at its edges', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 3, -1)).toBeNull();
    expect(variableHoverSource(v, 3, 1)).not.toBeNull();
    expect(variableHoverSource(v, 11, 1)).toBeNull();
    expect(variableHoverSource(v, 11, -1)).not.toBeNull();
  });

  it('shows nothing while the click popover is open', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 6, 1)).not.toBeNull();
    v.dispatch({
      effects: openPopoverEffect.of({
        varName: 'host',
        from: 3,
        to: 11,
        tokenType: 'variable',
        entry: ctx.get('host'),
      }),
    });
    expect(variableHoverSource(v, 6, 1)).toBeNull();
  });

  it('shows Unresolved for a token the context does not know', () => {
    const v = createView('{{ghost}}', new Map());
    expect(hoverDom(v, 4)?.textContent).toContain('Unresolved');
  });

  it('keeps a secret value out of the tooltip DOM', () => {
    const v = createView(
      '{{apiKey}}',
      new Map([['apiKey', entry({ value: CANARY, secret: true })]]),
    );
    const dom = hoverDom(v, 4);
    expect(dom?.textContent).toContain('●●●●');
    expect(dom?.outerHTML).not.toContain(CANARY);
  });
});

describe('variableHover extension', () => {
  it('installs in an editor without throwing', () => {
    const v = createView('{{host}}', new Map([['host', entry()]]));
    expect(v.dom.querySelector('.cm-content')).not.toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/components/editor/extensions/__tests__/variable-hover.test.ts`
Expected: FAIL, cannot resolve `../variable-hover` (and `findVarTokenAt` is not exported).

- [ ] **Step 3: Export `findVarTokenAt`**

In `src/components/editor/extensions/variable-popover.ts`, change `function findVarTokenAt(` to `export function findVarTokenAt(`. Nothing else changes.

- [ ] **Step 4: Write the extension**

Create `src/components/editor/extensions/variable-hover.ts`:

```ts
import type { Extension } from '@codemirror/state';
import { type EditorView, hoverTooltip, type Tooltip } from '@codemirror/view';
import { isDynamicVar } from '@/lib/dynamic-vars';
import { sourceBadgeClass, type VariableScopeEntry } from '@/lib/url-variables';
import { variableContextField } from './variable-context-facet';
import { findVarTokenAt, getActivePopover } from './variable-popover';

/** How long the pointer rests on a token before the value shows. */
const HOVER_DELAY_MS = 300;
/** Longest value shown. A longer one is cut and ends with an ellipsis. */
const HOVER_VALUE_LIMIT = 200;
const MASK = '●●●●';

export function truncateForHover(value: string): string {
  return value.length > HOVER_VALUE_LIMIT ? `${value.slice(0, HOVER_VALUE_LIMIT)}…` : value;
}

// Every node is built with textContent, so a value can never become markup,
// and no attribute carries the value.
function node(tag: 'div' | 'span', className: string, text?: string): HTMLElement {
  const el = document.createElement(tag);
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

/** The tooltip body for one variable token. `entry` is undefined when it does not resolve. */
export function buildVariableHoverDom(
  varName: string,
  entry: VariableScopeEntry | undefined,
): HTMLElement {
  const card = node(
    'div',
    'cm-variable-hover max-w-sm rounded-sm border border-border bg-card px-2 py-1.5 text-xs text-popover-foreground shadow-md',
  );

  if (varName.startsWith('$') && isDynamicVar(varName.slice(1))) {
    card.append(node('div', 'font-mono', `Dynamic (${varName}), generated at send`));
    return card;
  }
  if (!entry) {
    card.append(node('div', 'text-muted-foreground', 'Unresolved'));
    return card;
  }

  const header = node('div', 'mb-1 flex items-center gap-1.5');
  header.append(
    node(
      'span',
      `rounded-full px-1.5 py-0.5 text-2xs font-medium ${sourceBadgeClass(entry.source)}`,
      entry.label,
    ),
  );
  const shown = entry.secret ? MASK : entry.value === '' ? '(empty)' : truncateForHover(entry.value);
  card.append(header, node('div', 'font-mono [overflow-wrap:anywhere]', shown));
  return card;
}

/**
 * The hover source: a tooltip for the `{{name}}` token under the pointer, or null.
 * Exported for tests; the extension passes it to `hoverTooltip`.
 */
export function variableHoverSource(view: EditorView, pos: number, side: -1 | 1): Tooltip | null {
  // The click popover already shows the value, so the hover stays out of its way.
  if (getActivePopover(view)) return null;
  const token = findVarTokenAt(view.state.doc.toString(), pos);
  if (!token) return null;
  // At a token's edge the pointer must be on the token side of the edge.
  if ((pos === token.from && side < 0) || (pos === token.to && side > 0)) return null;
  const entry = view.state.field(variableContextField, false)?.get(token.varName);
  return {
    pos: token.from,
    end: token.to,
    above: true,
    create: () => ({ dom: buildVariableHoverDom(token.varName, entry) }),
  };
}

/** Shows a variable's scope and value when the pointer rests on its `{{name}}` token. */
export function variableHover(): Extension {
  return hoverTooltip(variableHoverSource, { hoverTime: HOVER_DELAY_MS });
}
```

- [ ] **Step 5: Export it**

In `src/components/editor/extensions/index.ts`, add after the `variable-highlight` export:

```ts
export { variableHover } from './variable-hover';
```

- [ ] **Step 6: Run to verify the tests pass**

Run: `yarn test src/components/editor`
Expected: PASS.

- [ ] **Step 7: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors. If `text-2xs` is not a defined utility in this project, use the same size class `VariablePopover.tsx` uses for its badge (`text-2xs` is used there, so it exists).

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/editor/extensions/variable-popover.ts src/components/editor/extensions/variable-hover.ts src/components/editor/extensions/index.ts src/components/editor/extensions/__tests__/variable-hover.test.ts`
Suggested subject: `feat(editor): add a variable hover tooltip extension`.

---

### Task 3: Turn the hover on for the flow editors, and document the limits

**Files:**
- Modify: `src/components/editor/SingleLineEditor.tsx` (props, extension list at line 160-166, memo dependencies at line 195)
- Modify: `src/components/flow/properties/InlineSourceEditor.tsx`, `src/components/flow/properties/InputNodeEditor.tsx`
- Create: `src/components/editor/__tests__/SingleLineEditor.hoverPreview.test.tsx`
- Modify: `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx`, `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
- Create: `.claude/flow-variable-preview.md`

**Interfaces:**
- Produces: the `hoverPreview` prop.
- Consumes: `variableHover` (Task 2).

- [ ] **Step 1: Write the failing tests**

Create `src/components/editor/__tests__/SingleLineEditor.hoverPreview.test.tsx`:

```tsx
import { render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';

const hover = vi.hoisted(() => ({ variableHover: vi.fn(() => []) }));
vi.mock('../extensions/variable-hover', () => hover);
vi.mock('@/hooks/useVariableCommit', () => ({ useVariableCommit: () => vi.fn() }));

import { SingleLineEditor } from '../SingleLineEditor';

const context = new Map<string, VariableScopeEntry>([
  ['host', { value: 'a.test', source: 'environment', label: 'dev', secret: false }],
]);

describe('SingleLineEditor hoverPreview', () => {
  beforeEach(() => hover.variableHover.mockClear());

  it('does not install the hover by default, so other editors are unchanged', () => {
    render(<SingleLineEditor value='{{host}}' onChange={vi.fn()} variableContext={context} />);
    expect(hover.variableHover).not.toHaveBeenCalled();
  });

  it('installs the hover when hoverPreview is set with a variable context', () => {
    render(
      <SingleLineEditor value='{{host}}' onChange={vi.fn()} variableContext={context} hoverPreview />,
    );
    expect(hover.variableHover).toHaveBeenCalled();
  });

  it('does not install the hover without a variable context', () => {
    render(<SingleLineEditor value='{{host}}' onChange={vi.fn()} hoverPreview />);
    expect(hover.variableHover).not.toHaveBeenCalled();
  });
});
```

In `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx`, inside the `describe('InlineSourceEditor variable context', ...)` added in Task 1, add:

```tsx
  it('turns the hover preview on for the URL and header editors', () => {
    render(
      <FlowVariableContextProvider value={ctx}>
        <InlineSourceEditor request={request} onChange={vi.fn()} outOfRangeWires={[]} />
      </FlowVariableContextProvider>,
    );
    for (const label of ['URL', 'Header 1 name', 'Header 1 value']) {
      expect(latest(label)?.hoverPreview).toBe(true);
    }
  });
```

In `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`, extend the Task 1 test `gives an Input node value editor ...` with one more assertion at its end:

```tsx
    expect(editorProps.last?.hoverPreview).toBe(true);
```

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test src/components/editor/__tests__/SingleLineEditor.hoverPreview.test.tsx src/components/flow/properties/__tests__`
Expected: FAIL (`hoverPreview` does not exist; the first test passes trivially but the second fails).

- [ ] **Step 3: Add the prop**

In `src/components/editor/SingleLineEditor.tsx`:

1. Import `variableHover` in the existing `./extensions` import list (sorted position after `variableHighlight`).
2. In `SingleLineEditorProps`, after `readOnlyVariables`:

```tsx
  /**
   * When true, hovering a {{variable}} shows its scope and value. Needs variableContext.
   * Off by default, so only editors that opt in change.
   */
  hoverPreview?: boolean;
```
3. Add `hoverPreview,` to the destructured parameters.
4. In the `if (variableContext) { exts.push(...) }` block, after the push, add:

```tsx
      if (hoverPreview) exts.push(variableHover());
```
5. Add `!!hoverPreview,` to the `useMemo` dependency array, after `!!variableContext,`.

- [ ] **Step 4: Turn it on in the flow editors**

In `InlineSourceEditor.tsx`, add `hoverPreview` after `readOnlyVariables` on the three `SingleLineEditor` elements. In `InputNodeEditor.tsx`, add `hoverPreview` after `readOnlyVariables`.

- [ ] **Step 5: Write the scope note**

Create `.claude/flow-variable-preview.md`:

```markdown
# Variable preview in flow editors

The flow properties panel resolves `{{variable}}` for display with `useCollectionVariableContext(collection)`.
`FlowVariableScope` provides the map to the Input and Request editors; `SingleLineEditor` shows it with
`variableContext`, `readOnlyVariables` (the click popover cannot save) and `hoverPreview` (the hover tooltip).

## What the preview shows

Scopes: dynamic, process, global, collection, vault (always masked) and the active environment of the
flow's collection. A secret entry shows `●●●●`; its value is never put in the DOM.

## What it cannot show

- Inline requests inherit no folder or request variables (the backend gives them none).
- A saved request gets its own folder and request variables at run time. The editor cannot know them.
- Runtime values and wire values exist only during a run.
- Environment `extends` is stored but not resolved anywhere, so inherited variables are unresolved here
  and at run time alike.

## Why the click popover is read-only here

`useVariableCommit` saves to `useEnvStore.activeCollection`, which can differ from the flow's collection.
Do not remove `readOnlyVariables` from a flow editor without changing that hook.
```

- [ ] **Step 6: Run to verify the tests pass**

Run: `yarn test src/components/editor src/components/flow`
Expected: PASS.

- [ ] **Step 7: Gates, manual check and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Manual check once in the real app (`yarn tauri dev`): open a flow, select an Input node, type `{{` followed by a known variable name. Highlighting and autocomplete show. Rest the pointer on the token for about 300 ms: a card shows the scope and value. Click the token: the click popover opens, the hover card is gone, and the value field is read-only. Mark the variable secret in its environment: the hover shows `●●●●`. Record the result in the review note.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/editor/SingleLineEditor.tsx src/components/flow/properties/InlineSourceEditor.tsx src/components/flow/properties/InputNodeEditor.tsx src/components/editor/__tests__/SingleLineEditor.hoverPreview.test.tsx src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx .claude/flow-variable-preview.md`
Suggested subject: `feat(flow): show variable values on hover in flow editors`.

---

## Self-Review

- **Spec coverage:** F-43 context in the flow editors (Task 1), click popover safety (Task 1, `readOnlyVariables`), hover with badge, source label, masking, "Unresolved", dynamic text, truncation and click-popover suppression (Task 2), opt-in wiring and scope documentation (Task 3). `WireScriptDialog` is deliberately excluded (JavaScript, not a template).
- **Placeholders:** none. Every step shows code or the exact edit. The six `FlowPane.*` test files get the same literal statement, shown once in Step 3 of Task 1.
- **Type consistency:** `FlowVariableContextProvider` and `useFlowVariableContext` have the same names in Task 1's tests, components and `FlowVariableScope`. `readOnlyVariables` and `hoverPreview` are the prop names in `SingleLineEditor`, the editors and every test. `variableHoverSource(view, pos, side)` is the signature in the tests and the extension.
- **Review Focus coverage:** item 1 is `masks a secret...`, `sets no title...` and the secret case in `variableHoverSource`; item 2 is the two `SingleLineEditor.readOnlyVariables` tests; item 3 is `shows nothing while the click popover is open`, `needs the pointer on the token at its edges` and `returns nothing off a token`; item 4 is `renders the value as text`; item 5 is the first `hoverPreview` test.
- **Known gaps:** the hover delay (300 ms) and real pointer behavior are not testable in jsdom (no layout), so Task 3 has a manual check. The `AuthEditor` fields inside `AuthNodeEditor` keep the old click-popover save path (wrong collection when the flow's collection is not the active one); a follow-up can pass `readOnlyVariables` through `AuthEditor`.
