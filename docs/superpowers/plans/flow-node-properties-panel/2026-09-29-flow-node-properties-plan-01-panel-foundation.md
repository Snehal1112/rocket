# Flow Node Properties Panel — Plan 01: Panel Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Selecting one Flow node opens a docked properties panel beside the canvas. The panel edits every node's label and an Input node's value. The pure helpers that plan 02 needs are also added here.

**Architecture:**
- Node selection moves from `FlowCanvas`-local state into `FlowPane`, so the pane can show `NodePropertiesPanel` whenever exactly one node is selected. `FlowCanvas` keeps working uncontrolled when no selection props are passed.
- The panel applies every edit through the existing `handleNodeKindChange` path (`updateNodeKind`, one store update).
- A new `openProperties` node action powers a ⋮ button on every node kind.
- Pure data helpers live in `src/lib/flow-node-edits.ts`.

**Tech Stack:** React 18 + TypeScript, `@xyflow/react` 12, Zustand (`pane-store`), shadcn/ui (`Button`, `Input`, `Label`, resizable panels via `react-resizable-panels` 4), `lucide-react`, CodeMirror `SingleLineEditor`, Vitest + Testing Library + `@testing-library/user-event`.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-node-properties-panel-design.md` (§3, §4 "All kinds" / Input / Output / If / Switch, §5.4, §6, §8). Plan index: `docs/superpowers/plans/flow-node-properties-panel/00-plan-index.md`.

## Global Constraints

- Components use shadcn/ui primitives only. Do not add a raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>`. Raw elements are allowed only inside test mocks.
- Icons come from `lucide-react` only: `MoreVertical` for the node menu button and `X` for close.
- Single-line variable-aware fields (the Input value) use `SingleLineEditor` from `@/components/editor`, never Monaco.
- Zustand: use narrow selectors (`usePaneStore((s) => s.x)`). Never destructure the whole store at the top of a component.
- Edits apply live. There is no Save/Cancel in the panel. Each edit goes through the existing `handleNodeKindChange`, which reads the latest tab from the store.
- Node edits never add, remove or rewrite edges, and they always keep the node id.
- Interactive elements inside a node card carry the `nodrag nokey` classes.
- Package manager is `yarn`. Do not run `yarn install`, because `node_modules` in the worktree is a working symlink.
- Biome style: 2 spaces, single quotes (JSX too), trailing commas, 100-column lines. Comments are short full sentences ending with a period.
- Checks for every task: the task's own `yarn test --run <patterns>`, then `yarn tsc --noEmit` and `yarn check`. Do not run the whole Vitest suite or any cargo command.
- Commit through the `dev-workflow-skills:1-git-commit` skill, with conventional-commit subjects of 50 characters or fewer.

## Review Focus

1. **Backspace in a panel field must edit the text, not delete the node.** Task 3's `FlowPane.properties.test.tsx` pins this ("Backspace in the label field edits text and keeps the node").
2. **Deleting the node while its panel is open must close the panel,** not show a panel for a node that no longer exists. This is pinned in Task 3 ("closes when the node disappears").
3. **A panel edit must keep the node's edges.** A label edit on a wired node must leave `tab.edges` identical. This is pinned in Task 3 ("edits the label live and keeps the edges").
4. **⋮ on a second node while the first is open must switch the panel to the second node.** It must not stack or keep the old one. This is pinned in Task 3 ("switches to the node whose ⋮ was clicked").
5. **An Input value that is not a plain string must stay read-only.** An older file can hold a structured value, and editing it as text would corrupt it. This is pinned in Task 2 ("shows a structured value read-only").

---

### Task 1: Pure node-edit helpers

**Files:**
- Create: `src/lib/flow-node-edits.ts`
- Test: `src/lib/__tests__/flow-node-edits.test.ts` (create)

**Interfaces:**
- Consumes: the `Request`, `Folder`, `FlowEdge`, `InlineRequestData` and `Auth` types from `@/lib/tauri-api`.
- Produces (all exported):
  - `interface InlineConversion { inline: InlineRequestData; dropped: string[] }`
  - `savedToInline(request: Request): InlineConversion`
  - `inlineHasContent(request: InlineRequestData): boolean`
  - `indexWiresOutOfRange(edges: FlowEdge[], nodeId: string, headerCount: number): FlowEdge[]`
  - `interface SavedRequestEntry { path: string; name: string; method: string }`
  - `requestEntriesOf(folder: Folder): SavedRequestEntry[]`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** This task reads the collection and request data models.

- [ ] **Step 2: Write the failing tests**

Create `src/lib/__tests__/flow-node-edits.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  indexWiresOutOfRange,
  inlineHasContent,
  requestEntriesOf,
  savedToInline,
} from '@/lib/flow-node-edits';
import type { FlowEdge, Folder, Request } from '@/lib/tauri-api';

const saved: Request = {
  uid: 'u1',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [
    { key: 'Content-Type', value: 'application/json', enabled: true },
    { key: 'X-Debug', value: '1', enabled: false },
  ],
  body: { mode: 'json', content: '{"user":"{{user}}"}' },
  auth: { authType: 'inherit' },
};

describe('savedToInline', () => {
  it('copies method, url, enabled headers and a raw body', () => {
    const { inline, dropped } = savedToInline(saved);
    expect(inline).toEqual({
      method: 'POST',
      url: '{{baseUrl}}/login',
      headers: [{ name: 'Content-Type', value: 'application/json' }],
      body: '{"user":"{{user}}"}',
    });
    expect(dropped).toEqual(['1 disabled header']);
  });

  it('drops a form body and names it', () => {
    const { inline, dropped } = savedToInline({
      ...saved,
      headers: [],
      body: { mode: 'formdata', formData: [{ key: 'a', value: 'b', entryType: 'text', enabled: true }] },
    });
    expect(inline.body).toBeNull();
    expect(dropped).toEqual(['formdata body']);
  });

  it('treats a none or empty raw body as no body without dropping anything', () => {
    expect(savedToInline({ ...saved, headers: [], body: { mode: 'none' } })).toEqual({
      inline: { method: 'POST', url: '{{baseUrl}}/login', headers: [], body: null },
      dropped: [],
    });
    expect(savedToInline({ ...saved, headers: [], body: { mode: 'text', content: '' } }).inline.body).toBeNull();
    expect(savedToInline({ ...saved, headers: [], body: undefined }).inline.body).toBeNull();
  });

  it('names auth, scripts, tests and assertions it cannot carry', () => {
    const { dropped } = savedToInline({
      ...saved,
      headers: [],
      body: undefined,
      auth: { authType: 'bearer', token: 't' },
      preRequestScript: 'rok.setVar("a", 1);',
      postResponseScript: '  ',
      tests: 'test("ok", () => {});',
      assertions: [{ expression: 'res.status', operator: 'eq', value: '200' }],
    });
    expect(dropped).toEqual(['bearer auth', 'pre-request script', 'tests', 'assertions']);
  });

  it('keeps nothing for none or inherit auth', () => {
    expect(savedToInline({ ...saved, headers: [], auth: { authType: 'none' } }).dropped).toEqual([]);
  });
});

describe('inlineHasContent', () => {
  const empty = { method: 'GET', url: '', headers: [], body: null };

  it('is false for an empty inline request', () => {
    expect(inlineHasContent(empty)).toBe(false);
    expect(inlineHasContent({ ...empty, url: '   ', body: '  ' })).toBe(false);
  });

  it('is true when a url, a header or a body is set', () => {
    expect(inlineHasContent({ ...empty, url: 'https://x' })).toBe(true);
    expect(inlineHasContent({ ...empty, headers: [{ name: '', value: '' }] })).toBe(true);
    expect(inlineHasContent({ ...empty, body: '{}' })).toBe(true);
  });
});

describe('indexWiresOutOfRange', () => {
  const wire = (id: string, targetNodeId: string, targetField: string): FlowEdge => ({
    id,
    sourceNodeId: 'src',
    targetNodeId,
    targetField,
    expression: 'response.body',
  });
  const edges = [
    wire('e0', 'n1', 'headers[0].value'),
    wire('e2', 'n1', 'headers[2].value'),
    wire('eName', 'n1', 'headers[Authorization].value'),
    wire('eUrl', 'n1', 'url'),
    wire('eOther', 'n2', 'headers[5].value'),
  ];

  it('returns only index wires into the node at or past the header count', () => {
    expect(indexWiresOutOfRange(edges, 'n1', 2).map((e) => e.id)).toEqual(['e2']);
    expect(indexWiresOutOfRange(edges, 'n1', 3)).toEqual([]);
    expect(indexWiresOutOfRange(edges, 'n1', 0).map((e) => e.id)).toEqual(['e0', 'e2']);
  });
});

describe('requestEntriesOf', () => {
  const root: Folder = {
    uid: 'root',
    name: 'demo',
    items: [
      { type: 'request', ...saved, fileName: 'login.yml' },
      {
        type: 'folder',
        uid: 'f1',
        name: 'Auth',
        dirName: 'auth',
        items: [
          { type: 'summary', uid: 's1', name: 'Refresh', method: 'POST', url: '/r', fileName: 'refresh.yml' },
          {
            type: 'folder',
            uid: 'f2',
            name: 'Admin',
            items: [{ type: 'summary', uid: 's2', name: 'users', method: 'GET', url: '/u' }],
          },
          { type: 'opaque', protocol: 'graphql', name: 'gql', raw: {} },
        ],
      },
    ],
  };

  it('lists every request with the same path the sidebar uses', () => {
    expect(requestEntriesOf(root)).toEqual([
      { path: 'login.yml', name: 'Login', method: 'POST' },
      { path: 'auth/refresh.yml', name: 'Refresh', method: 'POST' },
      { path: 'auth/Admin/users', name: 'users', method: 'GET' },
    ]);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test --run flow-node-edits`
Expected: FAIL, `Failed to resolve import "@/lib/flow-node-edits"`.

- [ ] **Step 4: Implement `src/lib/flow-node-edits.ts`**

```ts
import type {
  Auth,
  FlowEdge,
  Folder,
  InlineRequestData,
  Request,
} from '@/lib/tauri-api';

// Body modes whose content is plain text, so an inline request can carry it.
const RAW_BODY_MODES = new Set(['json', 'xml', 'text']);

export interface InlineConversion {
  inline: InlineRequestData;
  /** Human-readable names of the parts the inline model cannot carry. */
  dropped: string[];
}

const authCarriesNothing = (auth: Auth) =>
  auth.authType === 'none' || auth.authType === 'inherit';

const hasText = (value: string | null | undefined) => (value ?? '').trim() !== '';

/**
 * Copies a saved request into the inline model a Flow node stores. The inline
 * model holds only method, url, headers and a text body, so everything else
 * is listed in `dropped` for the confirmation the user sees first.
 */
export function savedToInline(request: Request): InlineConversion {
  const dropped: string[] = [];

  const headers = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ name: h.key, value: h.value }));
  const disabled = request.headers.length - headers.length;
  if (disabled > 0) dropped.push(`${disabled} disabled header${disabled === 1 ? '' : 's'}`);

  let body: string | null = null;
  const source = request.body;
  if (source && source.mode !== 'none') {
    if (RAW_BODY_MODES.has(source.mode)) {
      body = hasText(source.content) ? (source.content ?? null) : null;
    } else {
      dropped.push(`${source.mode} body`);
    }
  }

  if (!authCarriesNothing(request.auth)) dropped.push(`${request.auth.authType} auth`);
  if (hasText(request.preRequestScript)) dropped.push('pre-request script');
  if (hasText(request.postResponseScript)) dropped.push('post-response script');
  if (hasText(request.tests)) dropped.push('tests');
  if ((request.assertions?.length ?? 0) > 0) dropped.push('assertions');

  return {
    inline: { method: request.method, url: request.url, headers, body },
    dropped,
  };
}

/** True when discarding this inline request would lose something the user typed. */
export function inlineHasContent(request: InlineRequestData): boolean {
  return hasText(request.url) || request.headers.length > 0 || hasText(request.body);
}

const HEADER_INDEX_TARGET = /^headers\[(\d+)\]/;

/**
 * Wires into `nodeId` that target a header by position, where that position
 * no longer exists. Such a wire fails at run time with "header index out of
 * range". Wires that name a header are safe, because the run adds it.
 */
export function indexWiresOutOfRange(
  edges: FlowEdge[],
  nodeId: string,
  headerCount: number,
): FlowEdge[] {
  return edges.filter((e) => {
    if (e.targetNodeId !== nodeId) return false;
    const match = HEADER_INDEX_TARGET.exec(e.targetField);
    return match !== null && Number(match[1]) >= headerCount;
  });
}

export interface SavedRequestEntry {
  /** Path relative to the collection root, as a Saved source stores it. */
  path: string;
  name: string;
  method: string;
}

/**
 * Flattens a collection tree into its requests. Paths are built exactly like
 * the sidebar builds them: folder directory names joined by "/", then the
 * request's file name (or its name when there is no file name).
 */
export function requestEntriesOf(folder: Folder, basePath = ''): SavedRequestEntry[] {
  const entries: SavedRequestEntry[] = [];
  for (const item of folder.items) {
    if (item.type === 'folder') {
      const dir = item.dirName ?? item.name;
      entries.push(...requestEntriesOf(item, basePath ? `${basePath}/${dir}` : dir));
    } else if (item.type === 'request' || item.type === 'summary') {
      const file = item.fileName ?? item.name;
      entries.push({
        path: basePath ? `${basePath}/${file}` : file,
        name: item.name,
        method: item.method,
      });
    }
  }
  return entries;
}
```

`requestEntriesOf`'s second parameter is internal to the recursion. Callers pass only the folder.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test --run flow-node-edits`
Expected: PASS, all tests.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0. If Biome reports formatting only, run `yarn format`, then revert any formatting changes to files this task does not name.

- [ ] **Step 6: Commit**

Stage `src/lib/flow-node-edits.ts` and `src/lib/__tests__/flow-node-edits.test.ts`, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add node edit helpers`. Footer: `Relates to: #26`.

---

### Task 2: Panel and editors for label and Input value

**Files:**
- Create: `src/components/flow/properties/LabelField.tsx`
- Create: `src/components/flow/properties/InputNodeEditor.tsx`
- Create: `src/components/flow/properties/LabelOnlyEditor.tsx`
- Create: `src/components/flow/properties/NodePropertiesPanel.tsx`
- Test: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (create)

**Interfaces:**
- Consumes: `FlowNode` and `FlowNodeKind` from `@/lib/tauri-api`, and `SingleLineEditor` from `@/components/editor`.
- Produces:
  - `LabelField({ value: string; onChange: (value: string) => void })`, which renders a shadcn `Input` labelled "Label".
  - `InputNodeEditor({ kind: InputKind; onChange: (kind: FlowNodeKind) => void })`.
  - `LabelOnlyEditor({ kind: FlowNodeKind; onChange: (kind: FlowNodeKind) => void; note?: string })`.
  - `NodePropertiesPanel({ node: FlowNode; onChange: (kind: FlowNodeKind) => void; onClose: () => void })`, which renders `data-testid="node-properties-panel"`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { NodePropertiesPanel } from '../NodePropertiesPanel';

// The real CodeMirror editor needs Tauri and react-query. A plain input with
// the same value/onChange contract is enough here.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const node = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });

function renderPanel(n: FlowNode) {
  const onChange = vi.fn();
  const onClose = vi.fn();
  render(<NodePropertiesPanel node={n} onChange={onChange} onClose={onClose} />);
  return { onChange, onClose };
}

describe('NodePropertiesPanel', () => {
  it('edits the label of any node kind with one change per keystroke', async () => {
    const { onChange } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Output', label: 'Outx' });
  });

  it('edits an Input value through the variable-aware editor', async () => {
    const { onChange } = renderPanel(node('i1', { kind: 'Input', label: 'User', value: 'al' }));
    await userEvent.type(screen.getByLabelText('Input value'), 'i');
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Input', label: 'User', value: 'ali' });
  });

  it('shows a structured value read-only', () => {
    renderPanel(node('i1', { kind: 'Input', label: 'User', value: { secret: true } }));
    expect(screen.queryByLabelText('Input value')).not.toBeInTheDocument();
    expect(screen.getByTestId('input-value-readonly')).toHaveTextContent('{"secret":true}');
  });

  it('tells the user where If and Switch details are edited', () => {
    renderPanel(node('if1', { kind: 'If', label: 'Ok?', condition: 'true' }));
    expect(screen.getByText('The condition is edited on the node itself.')).toBeInTheDocument();
  });

  it('shows the kind and label in its header and closes on ✕', async () => {
    const { onClose } = renderPanel(node('o1', { kind: 'Output', label: 'Result' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · Result');
    await userEvent.click(screen.getByRole('button', { name: 'Close properties' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test --run NodePropertiesPanel`
Expected: FAIL, `Failed to resolve import "../NodePropertiesPanel"`.

- [ ] **Step 3: Create `LabelField.tsx`**

```tsx
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

// Every node kind has a label, so every editor starts with this field.
export function LabelField({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <div className='space-y-1'>
      <Label htmlFor='flow-node-label' className='text-xs'>
        Label
      </Label>
      <Input
        id='flow-node-label'
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className='h-8 text-xs'
      />
    </div>
  );
}
```

- [ ] **Step 4: Create `LabelOnlyEditor.tsx`**

```tsx
import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

// Output nodes have only a label. If and Switch nodes edit their condition,
// value and cases on the card, so the panel offers the label and a pointer.
export function LabelOnlyEditor({
  kind,
  onChange,
  note,
}: {
  kind: FlowNodeKind;
  onChange: (kind: FlowNodeKind) => void;
  note?: string;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      {note && <p className='text-xs text-muted-foreground'>{note}</p>}
    </div>
  );
}
```

- [ ] **Step 5: Create `InputNodeEditor.tsx`**

```tsx
import { SingleLineEditor } from '@/components/editor';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

type InputKind = Extract<FlowNodeKind, { kind: 'Input' }>;

export function InputNodeEditor({
  kind,
  onChange,
}: {
  kind: InputKind;
  onChange: (kind: FlowNodeKind) => void;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Value</span>
        {typeof kind.value === 'string' ? (
          <SingleLineEditor
            aria-label='Input value'
            value={kind.value}
            onChange={(value) => onChange({ ...kind, value })}
            placeholder='Text or {{variable}}'
          />
        ) : (
          // A structured value from an older file. Editing it as text would
          // replace its shape, so it is shown but not editable.
          <p data-testid='input-value-readonly' className='text-xs text-muted-foreground'>
            This value has a structured form and can't be edited here:{' '}
            <code className='font-mono'>{JSON.stringify(kind.value)}</code>
          </p>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Create `NodePropertiesPanel.tsx`**

```tsx
import { X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { InputNodeEditor } from './InputNodeEditor';
import { LabelOnlyEditor } from './LabelOnlyEditor';

// Picks the editor for the node's kind. Each editor reports a whole new kind,
// and the caller applies it with one store update.
function editorFor(kind: FlowNodeKind, onChange: (kind: FlowNodeKind) => void) {
  switch (kind.kind) {
    case 'Input':
      return <InputNodeEditor kind={kind} onChange={onChange} />;
    case 'If':
      return (
        <LabelOnlyEditor
          kind={kind}
          onChange={onChange}
          note='The condition is edited on the node itself.'
        />
      );
    case 'Switch':
      return (
        <LabelOnlyEditor
          kind={kind}
          onChange={onChange}
          note='The value and cases are edited on the node itself.'
        />
      );
    case 'Output':
    case 'Request':
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
  }
}

export function NodePropertiesPanel({
  node,
  onChange,
  onClose,
}: {
  node: FlowNode;
  onChange: (kind: FlowNodeKind) => void;
  onClose: () => void;
}) {
  return (
    <aside
      aria-label='Node properties'
      data-testid='node-properties-panel'
      className='flex h-full flex-col bg-background'
    >
      <div className='flex items-center justify-between gap-2 border-b px-3 py-2'>
        <span className='truncate text-xs font-medium'>
          {node.kind.kind} · {node.kind.label}
        </span>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-6 w-6'
          aria-label='Close properties'
          onClick={onClose}
        >
          <X className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      <div className='flex-1 overflow-y-auto p-3'>{editorFor(node.kind, onChange)}</div>
    </aside>
  );
}
```

Plan 02 Task 3 replaces the `'Request'` branch with the full Request editor.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test --run NodePropertiesPanel`
Expected: PASS, 5 tests.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 8: Commit**

Stage the four new component files and the test, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add node properties panel`. Footer: `Relates to: #26`.

---

### Task 3: Selection wiring, ⋮ button on every node, panel layout

**Files:**
- Create: `src/components/flow/nodes/NodeMenuButton.tsx`
- Modify: `src/components/flow/nodes/FlowNodeActionsContext.tsx` (add `openProperties`)
- Modify: `src/components/flow/nodes/RequestNode.tsx` (replace the decorative `MoreVertical` icon)
- Modify: `src/components/flow/nodes/InputNode.tsx`, `OutputNode.tsx`, `IfNode.tsx`, `SwitchNode.tsx` (add the button to each header)
- Modify: `src/components/flow/FlowCanvas.tsx` (controlled selection props, `openProperties` action)
- Modify: `src/components/flow/FlowPane.tsx` (selection state, resizable layout, panel)
- Modify: `src/components/flow/nodes/__tests__/IfNode.test.tsx` and `SwitchNode.test.tsx` (their action literals gain `openProperties: vi.fn()`)
- Test: `src/components/flow/__tests__/FlowPane.properties.test.tsx` (create)

**Interfaces:**
- Consumes: `NodePropertiesPanel` (Task 2); `handleNodeKindChange` in `FlowPane` (Phase 2); `ResizablePanelGroup`, `ResizablePanel` and `ResizableHandle` from `@/components/ui/resizable`.
- Produces:
  - `FlowNodeActions.openProperties(nodeId: string): void`
  - `NodeMenuButton({ nodeId: string; label: string })`, with accessible name `Edit <label>`.
  - `FlowCanvas` props `selectedNodeIds?: ReadonlySet<string>` and `onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void`. Without them the canvas keeps its own selection, as before.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/__tests__/FlowPane.properties.test.tsx`:

```tsx
import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const baseTab: FlowTab = {
  id: 'flow-props-1',
  tabType: 'flow',
  title: 'Flow: props',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'props',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
  ],
  nodeStatus: {},
  runState: 'idle',
};

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab');
  return tab;
}

// FlowPane receives the tab as a prop. Re-render it from the store after each
// store change, the way PaneRenderer does in the app.
function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

describe('FlowPane node properties panel', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('is closed until a node is chosen', () => {
    render(<Harness />);
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('opens from a node ⋮ button and closes on ✕', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit User'));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Input · User');
    await userEvent.click(screen.getByRole('button', { name: 'Close properties' }));
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('switches to the node whose ⋮ was clicked', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit User'));
    await userEvent.click(screen.getByLabelText('Edit Result'));
    expect(screen.getAllByTestId('node-properties-panel')).toHaveLength(1);
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · Result');
  });

  it('edits the label live and keeps the edges', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.type(screen.getByLabelText('Label'), '!');
    const tab = getFlowTab();
    expect(tab.nodes.find((n) => n.id === 'out1')?.kind.label).toBe('Result!');
    expect(tab.edges).toEqual(baseTab.edges);
    expect(tab.isDirty).toBe(true);
    expect(screen.getByTestId('output-node-card')).toHaveTextContent('Result!');
  });

  it('Backspace in the label field edits text and keeps the node', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const field = screen.getByLabelText('Label');
    await userEvent.click(field);
    await userEvent.keyboard('{Backspace}');
    expect(document.activeElement).toBe(field);
    const tab = getFlowTab();
    expect(tab.nodes.map((n) => n.id)).toEqual(['in1', 'out1']);
    expect(tab.nodes.find((n) => n.id === 'out1')?.kind.label).toBe('Resul');
  });

  it('closes when the node disappears', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    act(() => {
      usePaneStore
        .getState()
        .updateFlowNodes(baseTab.id, getFlowTab().nodes.filter((n) => n.id !== 'out1'));
    });
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('opens on a node added from the palette', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Output' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · New Output');
  });

  it('closes when the empty canvas is clicked', async () => {
    const { container } = render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const pane = container.querySelector('.react-flow__pane');
    if (!pane) throw new Error('Expected the React Flow pane');
    fireEvent.click(pane);
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test --run FlowPane.properties`
Expected: FAIL. `getByLabelText('Edit User')` finds nothing, because no node has a menu button yet.

- [ ] **Step 3: Add `openProperties` to the node actions**

Replace the contents of `src/components/flow/nodes/FlowNodeActionsContext.tsx` with:

```tsx
import { createContext, useContext } from 'react';
import type { FlowNodeKind } from '@/lib/tauri-api';

export interface FlowNodeActions {
  updateNodeKind: (nodeId: string, kind: FlowNodeKind) => void;
  removeSwitchCase: (nodeId: string, caseId: string) => void;
  /** Selects exactly this node, which opens its properties panel. */
  openProperties: (nodeId: string) => void;
}

const noop = () => {
  // Intentionally empty.
};

// The default is a no-op, so a node rendered outside a canvas (for example in
// a unit test) stays inert instead of throwing.
export const FlowNodeActionsContext = createContext<FlowNodeActions>({
  updateNodeKind: noop,
  removeSwitchCase: noop,
  openProperties: noop,
});

export function useFlowNodeActions(): FlowNodeActions {
  return useContext(FlowNodeActionsContext);
}
```

In `src/components/flow/nodes/__tests__/IfNode.test.tsx` and `SwitchNode.test.tsx`, add `openProperties: vi.fn()` to each `actions` object literal, next to `updateNodeKind: vi.fn()` and `removeSwitchCase: vi.fn()`. `tsc` reports every literal that needs it.

- [ ] **Step 4: Create `NodeMenuButton.tsx`**

```tsx
import { MoreVertical } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useFlowNodeActions } from './FlowNodeActionsContext';

// Opens the node's properties panel. `nodrag nokey` keeps a click from
// dragging the node and keeps key presses on the button away from the canvas.
export function NodeMenuButton({ nodeId, label }: { nodeId: string; label: string }) {
  const { openProperties } = useFlowNodeActions();
  return (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      aria-label={`Edit ${label}`}
      className='nodrag nokey ml-auto h-5 w-5 shrink-0 text-muted-foreground'
      onClick={() => openProperties(nodeId)}
    >
      <MoreVertical className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );
}
```

- [ ] **Step 5: Put the button in every node header**

1. **`RequestNode.tsx`:**
   - Change the signature to `export function RequestNode({ id, data, isConnectable }: NodeProps & { data: RequestNodeData })`.
   - Remove the `MoreVertical` import.
   - Replace the line `<MoreVertical className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />` with `<NodeMenuButton nodeId={id} label={kind.label} />`.
   - Add `import { NodeMenuButton } from './NodeMenuButton';`.
2. **`IfNode.tsx` and `SwitchNode.tsx`:** each header row already ends with `<span className='truncate font-medium'>{kind.label}</span>`. Add `<NodeMenuButton nodeId={id} label={kind.label} />` directly after that span, plus the import. Both components already receive `id`.
3. **`InputNode.tsx`:**
   - Change the signature to `({ id, data, isConnectable }: NodeProps & { data: InputNodeData })`.
   - Replace `<div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>` with:

   ```tsx
   <div className='flex items-center gap-1.5 border-b px-2 py-1.5 font-medium'>
     <span className='truncate'>{data.kind.label}</span>
     <NodeMenuButton nodeId={id} label={data.kind.label} />
   </div>
   ```

   - Add the import.
4. **`OutputNode.tsx`:** make the same two changes as InputNode: the `id` in the signature and the same header replacement.

- [ ] **Step 6: Make `FlowCanvas` selection controllable and wire `openProperties`**

In `src/components/flow/FlowCanvas.tsx`:

1. Add to `FlowCanvasProps`, after `onRemoveSwitchCase`:

```ts
  // Controlled node selection. Without these props the canvas keeps its own
  // selection, as before. FlowPane passes them to drive the properties panel.
  selectedNodeIds?: ReadonlySet<string>;
  onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void;
```

2. Destructure `selectedNodeIds: selectedNodeIdsProp` and `onSelectedNodeIdsChange` in `FlowCanvasInner`'s parameter list.

3. Replace the line `const [selectedNodeIds, setSelectedNodeIds] = useState<ReadonlySet<string>>(() => new Set());` with:

```ts
  const [localSelection, setLocalSelection] = useState<ReadonlySet<string>>(() => new Set());
  const selectedNodeIds = selectedNodeIdsProp ?? localSelection;
  // Reports a new selection to the owner, or keeps it locally when uncontrolled.
  const selectNodes = (next: ReadonlySet<string>) => {
    if (next === selectedNodeIds) return;
    if (onSelectedNodeIdsChange) onSelectedNodeIdsChange(next);
    else setLocalSelection(next);
  };
  // Node actions are memoised, so they call the latest selectNodes through a ref.
  const selectNodesRef = useRef(selectNodes);
  selectNodesRef.current = selectNodes;
```

4. In `handleNodesChange`, replace `setSelectedNodeIds((prev) => nextSelection(prev, changes));` with `selectNodes(nextSelection(selectedNodeIds, changes));`.

5. Replace the `nodeActions` `useMemo` with:

```ts
  const nodeActions = useMemo<FlowNodeActions>(
    () => ({
      updateNodeKind: (nodeId, kind) => onNodeKindChange?.(nodeId, kind),
      removeSwitchCase: (nodeId, caseId) => onRemoveSwitchCase?.(nodeId, caseId),
      openProperties: (nodeId) => selectNodesRef.current(new Set([nodeId])),
    }),
    [onNodeKindChange, onRemoveSwitchCase],
  );
```

- [ ] **Step 7: Own the selection in `FlowPane` and render the panel**

In `src/components/flow/FlowPane.tsx`:

1. Add the imports:

```ts
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import { NodePropertiesPanel } from './properties/NodePropertiesPanel';
```

2. Next to the other `useState` calls, above every early return, add:

```ts
  // UI state only. The panel shows while exactly one node is selected.
  const [selectedNodeIds, setSelectedNodeIds] = useState<ReadonlySet<string>>(() => new Set());
```

3. Replace `handleAddNode` with:

```ts
  // A new node becomes the only selection, so its properties panel opens.
  const handleAddNode = (node: FlowNode) => {
    updateFlowNodes(tab.id, [...tab.nodes, node]);
    setSelectedNodeIds(new Set([node.id]));
  };
```

4. Directly before the final `return (`, add:

```ts
  // The panel follows the selection. A node that no longer exists shows nothing.
  const panelNodeId = selectedNodeIds.size === 1 ? [...selectedNodeIds][0] : null;
  const panelNode = panelNodeId ? tab.nodes.find((n) => n.id === panelNodeId) : undefined;
```

5. Replace the whole returned JSX with the version below. The toolbar, palette, canvas and popover move inside the canvas panel unchanged, apart from the two new `FlowCanvas` props.

```tsx
  return (
    <ResizablePanelGroup className='h-full'>
      <ResizablePanel id='flow-canvas' minSize='40%'>
        <div className='relative h-full'>
          <div className='absolute top-2 right-2 z-10 flex items-center gap-2'>
            <FlowToolbar
              collection={collectionName}
              flowName={flowName}
              environmentName={activeEnvironmentName}
              onPatchStatus={(nodeId, status, detail) =>
                patchFlowNodeStatus(tab.id, nodeId, status as FlowNodeStatus, detail)
              }
              onRunStateChange={(state, runId) => setFlowRunState(tab.id, state, runId)}
              tabRunState={tab.runState}
              tabRunId={tab.runId}
              onBeforeRun={handleBeforeRun}
            />
            <Button size='sm' variant='outline' onClick={() => void handleSave()}>
              Save
            </Button>
          </div>
          <NodePalette onAddNode={handleAddNode} />
          <FlowCanvas
            nodes={tab.nodes}
            edges={tab.edges}
            nodeStatus={tab.nodeStatus}
            nodeDetail={tab.nodeDetail}
            cycleNodeIds={cycleNodeIds}
            cycleEdgeIds={cycleEdgeIds}
            onNodesChange={(nodes) => updateFlowNodes(tab.id, nodes)}
            onEdgesChange={(edges) => updateFlowEdges(tab.id, edges)}
            onConnect={handleConnect}
            onAddNode={handleAddNode}
            flowCollectionName={tab.collectionName}
            onNodeKindChange={handleNodeKindChange}
            onRemoveSwitchCase={handleRemoveSwitchCase}
            selectedNodeIds={selectedNodeIds}
            onSelectedNodeIdsChange={setSelectedNodeIds}
          />
          {pendingEdge && pendingTargetNode && (
            <WireExpressionPopover
              // Keyed by edge id so a second connection made before the first
              // popover is committed/dismissed remounts this component instead
              // of reusing it — otherwise its internal `expression`/`headerName`
              // state (initialized once via useState) would leak from the
              // previous edge onto the new one.
              key={pendingEdge.id}
              edge={pendingEdge}
              targetNode={pendingTargetNode}
              open={pendingEdge !== null}
              onOpenChange={(open) => {
                if (open) return;
                if (isUncommittedHeadersEdge(pendingEdge)) {
                  updateFlowEdges(
                    tab.id,
                    tab.edges.filter((e) => e.id !== pendingEdge.id),
                  );
                }
                setPendingEdge(null);
              }}
              onCommit={(updated) => {
                committedEdgeIdRef.current = updated.id;
                updateFlowEdges(
                  tab.id,
                  tab.edges.map((e) => (e.id === updated.id ? updated : e)),
                );
              }}
            >
              {/* Plan 09's edge/handle DOM node the popover anchors to. */}
              <span />
            </WireExpressionPopover>
          )}
        </div>
      </ResizablePanel>
      {panelNode && (
        <>
          <ResizableHandle />
          {/* minSize/maxSize take percentage strings. A plain number there
              means pixels in this version of react-resizable-panels. */}
          <ResizablePanel id='flow-node-properties' defaultSize={30} minSize='20%' maxSize='50%'>
            <NodePropertiesPanel
              node={panelNode}
              onChange={(kind) => handleNodeKindChange(panelNode.id, kind)}
              onClose={() => setSelectedNodeIds(new Set())}
            />
          </ResizablePanel>
        </>
      )}
    </ResizablePanelGroup>
  );
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `yarn test --run FlowPane.properties`
Expected: PASS, 8 tests.

The last test depends on React Flow emitting deselect changes when `.react-flow__pane` is clicked in jsdom. If it fails only because no change is emitted there, do not weaken the test. Report DONE_WITH_CONCERNS with the output, and the controller will decide.

Run: `yarn test --run flow pane-store`
Expected: PASS, every existing flow test included. The `FlowPane` save tests still expect `'Out a—'` and `'Out b—'` as card text, and the icon-only menu button adds no text.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 9: Commit**

Stage every file listed under **Files**, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): open node properties from canvas`. Footer: `Relates to: #26`.
