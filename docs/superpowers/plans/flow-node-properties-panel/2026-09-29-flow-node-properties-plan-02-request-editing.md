# Flow Node Properties Panel — Plan 02: Request Node Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The properties panel edits Request nodes fully:
- inline method, URL, headers and body;
- repointing a Saved node;
- opening its request tab;
- switching source with Convert to inline and Use a saved request.

**Architecture:** Three focused editors live under `src/components/flow/properties/`:
- `InlineSourceEditor` is pure props-in, change-out.
- `SavedSourceEditor` holds the request path, the picker, and the Open and Convert buttons.
- `RequestNodeEditor` owns the two confirmation flows and the IPC call `getRequest`.

All edits go through the panel's `onChange(kind)`, the same single-update path plan 01 wired. The node keeps its id, so its edges never move.

**Tech Stack:** React 18 + TypeScript, shadcn/ui (`Button`, `Input`, `Select`, `Popover`), `lucide-react`, CodeMirror `SingleLineEditor`, Monaco via `MonacoWrapper`, Vitest + Testing Library + `@testing-library/user-event`.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-node-properties-panel-design.md`, §4 (Request), §5.1–5.3, §6, §8. Plan index: `docs/superpowers/plans/flow-node-properties-panel/00-plan-index.md`.

**Prerequisite:** plan 01 is complete. `flow-node-edits.ts`, `LabelField`, `NodePropertiesPanel` and the FlowPane selection wiring all exist.

## Global Constraints

- **UI primitives:** shadcn/ui only. No raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>`, except inside test mocks.
- **Icons:** `lucide-react` only. Use `Plus` to add a header, `X` to remove one, and `ExternalLink` for "Open request".
- **Single-line fields** (URL, header name and header value) use `SingleLineEditor` from `@/components/editor`. **The body** uses `MonacoWrapper` from `@/components/editor/MonacoWrapper`.
- **Confirmations** are in-panel UI, never `window.confirm`.
- **The flow never writes a collection request file.** `getRequest` is the only request IPC call this plan adds.
- **Inline headers** are `{ name, value }` with no enabled flag. An empty body is stored as `body: null`.
- **Methods:** GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS.
- **Zustand:** narrow selectors only.
- Every portalled `PopoverContent` and `SelectContent` rendered from the panel gets `className` including `nokey`, because portals escape the panel's `nokey` root.
- **Packages:** use `yarn`, and do not run `yarn install`.
- **Biome style:** 2 spaces, single quotes (JSX too), trailing commas, 100-column lines. Comments are short full sentences ending with a period.
- **Checks for every task:** the task's own `yarn test --run <patterns>`, then `yarn tsc --noEmit` and `yarn check`. Do not run the whole Vitest suite or any cargo command.
- **Commits:** use the `dev-workflow-skills:1-git-commit` skill, with conventional-commit subjects of 50 characters or fewer.

## Review Focus

1. **Convert to inline on a request with auth or scripts** must list what gets dropped and apply nothing until the user confirms. Task 3 pins this: "shows what is dropped and waits for confirmation".
2. **Convert to inline when the saved file is missing** (`getRequest` rejects) must show the error in the panel and leave the node unchanged. Task 3 pins this: "shows a load error and changes nothing".
3. **Use a saved request… on an empty inline request** must switch at once without a pointless confirmation. On a non-empty one it must ask first. Task 3 pins both.
4. **Removing a header that a wire targets by index** (`headers[1].value`) must show the run-time warning. Task 1 pins this: "warns about a wire whose header index no longer exists".
5. **The request list failing to load in the picker** must show an error with a working Retry, not an empty list. Task 2 pins this: "shows a load error with Retry".

---

### Task 1: Inline request editor

**Files:**
- Create: `src/components/flow/properties/InlineSourceEditor.tsx`
- Test: `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx` (create)

**Interfaces:**
- Consumes:
  - `InlineRequestData`, `InlineHeader` and `FlowEdge` from `@/lib/tauri-api`.
  - `SingleLineEditor` from `@/components/editor`.
  - `MonacoWrapper` from `@/components/editor/MonacoWrapper`, whose props are `value`, `onChange`, `contentType` and `height`.
- Produces: `InlineSourceEditor({ request: InlineRequestData; onChange: (request: InlineRequestData) => void; outOfRangeWires: FlowEdge[] })`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** This task edits the request data a node stores.

- [ ] **Step 2: Write the failing tests**

Create `src/components/flow/properties/__tests__/InlineSourceEditor.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, InlineRequestData } from '@/lib/tauri-api';
import { InlineSourceEditor } from '../InlineSourceEditor';

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

// Monaco cannot run in jsdom. A textarea with the same value/onChange
// contract stands in for the body editor.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Body'
      value={props.value}
      onChange={(e) => props.onChange?.(e.target.value)}
    />
  ),
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const request: InlineRequestData = {
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [
    { name: 'Content-Type', value: 'application/json' },
    { name: 'X-Trace', value: 'on' },
  ],
  body: '{}',
};

function renderEditor(r = request, outOfRangeWires: FlowEdge[] = []) {
  const onChange = vi.fn();
  render(<InlineSourceEditor request={r} onChange={onChange} outOfRangeWires={outOfRangeWires} />);
  return onChange;
}

describe('InlineSourceEditor', () => {
  it('edits the URL', async () => {
    const onChange = renderEditor();
    await userEvent.type(screen.getByLabelText('URL'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({ ...request, url: '{{baseUrl}}/loginx' });
  });

  it('changes the method', async () => {
    const onChange = renderEditor();
    await userEvent.click(screen.getByRole('combobox', { name: 'Method' }));
    await userEvent.click(await screen.findByRole('option', { name: 'PUT' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, method: 'PUT' });
  });

  it('adds, renames and removes headers', async () => {
    const onChange = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Add header' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...request,
      headers: [...request.headers, { name: '', value: '' }],
    });

    await userEvent.type(screen.getByLabelText('Header 2 value'), '!');
    expect(onChange).toHaveBeenLastCalledWith({
      ...request,
      headers: [request.headers[0], { name: 'X-Trace', value: 'on!' }],
    });

    await userEvent.click(screen.getByRole('button', { name: 'Remove header 1' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, headers: [request.headers[1]] });
  });

  it('stores an emptied body as null', async () => {
    const onChange = renderEditor({ ...request, body: 'x' });
    await userEvent.clear(screen.getByLabelText('Body'));
    expect(onChange).toHaveBeenLastCalledWith({ ...request, body: null });
  });

  it('warns about a wire whose header index no longer exists', () => {
    const wire: FlowEdge = {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'r1',
      targetField: 'headers[2].value',
      expression: 'response.body',
    };
    renderEditor(request, [wire]);
    expect(screen.getByRole('alert')).toHaveTextContent(
      'A wire targets header position 3, which no longer exists. This node will fail when the flow runs.',
    );
  });

  it('shows no warning when every index wire still has its header', () => {
    renderEditor();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test --run InlineSourceEditor`
Expected: FAIL, `Failed to resolve import "../InlineSourceEditor"`.

- [ ] **Step 4: Implement `InlineSourceEditor.tsx`**

```tsx
import { Plus, X } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { FlowEdge, InlineHeader, InlineRequestData } from '@/lib/tauri-api';

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const;

const HEADER_INDEX = /^headers\[(\d+)\]/;

// Turns index wires into the one-based positions a person reads.
function missingPositions(wires: FlowEdge[]): number[] {
  return wires
    .map((w) => HEADER_INDEX.exec(w.targetField))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => Number(m[1]) + 1);
}

export function InlineSourceEditor({
  request,
  onChange,
  outOfRangeWires,
}: {
  request: InlineRequestData;
  onChange: (request: InlineRequestData) => void;
  outOfRangeWires: FlowEdge[];
}) {
  const setHeader = (index: number, patch: Partial<InlineHeader>) =>
    onChange({
      ...request,
      headers: request.headers.map((h, i) => (i === index ? { ...h, ...patch } : h)),
    });
  const contentType = request.headers.find((h) => h.name.toLowerCase() === 'content-type')?.value;
  const positions = missingPositions(outOfRangeWires);

  return (
    <div className='space-y-3'>
      <div className='flex items-center gap-2'>
        <Select value={request.method} onValueChange={(method) => onChange({ ...request, method })}>
          <SelectTrigger aria-label='Method' className='h-8 w-28 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent className='nokey'>
            {METHODS.map((m) => (
              <SelectItem key={m} value={m}>
                {m}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <SingleLineEditor
          aria-label='URL'
          className='flex-1'
          value={request.url}
          onChange={(url) => onChange({ ...request, url })}
          placeholder='https://api.example.com/{{path}}'
        />
      </div>

      <div className='space-y-1'>
        <div className='flex items-center justify-between'>
          <span className='text-xs font-medium'>Headers</span>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            className='h-6 gap-1 text-xs'
            onClick={() =>
              onChange({ ...request, headers: [...request.headers, { name: '', value: '' }] })
            }
          >
            <Plus className='h-3 w-3' aria-hidden='true' />
            Add header
          </Button>
        </div>
        {request.headers.map((header, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: inline headers have no id, and each row is fully controlled by its index.
          <div key={i} className='flex items-center gap-1'>
            <SingleLineEditor
              aria-label={`Header ${i + 1} name`}
              className='flex-1'
              value={header.name}
              onChange={(name) => setHeader(i, { name })}
              placeholder='Name'
            />
            <SingleLineEditor
              aria-label={`Header ${i + 1} value`}
              className='flex-1'
              value={header.value}
              onChange={(value) => setHeader(i, { value })}
              placeholder='Value'
            />
            <Button
              type='button'
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0'
              aria-label={`Remove header ${i + 1}`}
              onClick={() =>
                onChange({ ...request, headers: request.headers.filter((_, j) => j !== i) })
              }
            >
              <X className='h-3 w-3' aria-hidden='true' />
            </Button>
          </div>
        ))}
        {positions.length > 0 && (
          <p role='alert' className='text-xs text-amber-600'>
            {positions.length === 1
              ? `A wire targets header position ${positions[0]}, which no longer exists.`
              : `Wires target header positions ${positions.join(', ')}, which no longer exist.`}{' '}
            This node will fail when the flow runs.
          </p>
        )}
      </div>

      <div className='space-y-1'>
        <span className='text-xs font-medium'>Body</span>
        <div className='h-40 overflow-hidden rounded border'>
          <MonacoWrapper
            value={request.body ?? ''}
            onChange={(body) => onChange({ ...request, body: body === '' ? null : body })}
            contentType={contentType}
            height='100%'
          />
        </div>
      </div>
    </div>
  );
}
```

`MonacoWrapper` picks JSON or XML highlighting from `contentType` and falls back to plain text.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test --run InlineSourceEditor`
Expected: PASS, 6 tests.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 6: Commit**

Stage the editor and its test. Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add inline request editor`. Footer: `Relates to: #26`.

---

### Task 2: Saved request editor, picker and "Open request"

**Files:**
- Create: `src/components/flow/properties/RequestPicker.tsx`
- Create: `src/components/flow/properties/openSavedRequest.ts`
- Create: `src/components/flow/properties/SavedSourceEditor.tsx`
- Test: `src/components/flow/properties/__tests__/SavedSourceEditor.test.tsx` (create)

**Interfaces:**
- Consumes:
  - From plan 01: `requestEntriesOf(folder)` and `SavedRequestEntry` from `@/lib/flow-node-edits`.
  - `getCollection(name): Promise<Collection>` and `getRequest(collection, path): Promise<Request>` from `@/lib/tauri-api`.
  - `mapApiRequestToState(req, fromCollection)` and `findTabInTree(root, id)` from `@/lib/pane-utils`.
  - `RequestTab` from `@/types/pane-types`.
  - `usePaneStore` `openTab`.
- Produces:
  - `RequestPicker({ collection: string; triggerLabel: string; onPick: (entry: SavedRequestEntry) => void })`.
  - `openSavedRequestTab(collection: string, path: string): Promise<void>`.
  - `SavedSourceEditor({ requestPath: string; collection: string; onPick: (entry: SavedRequestEntry) => void; onConvertToInline: () => void; converting: boolean })`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** This task reads collection trees and request files.

- [ ] **Step 2: Write the failing tests**

Create `src/components/flow/properties/__tests__/SavedSourceEditor.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getCollection, getRequest, type Collection, type Request } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { SavedSourceEditor } from '../SavedSourceEditor';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getRequest: vi.fn() };
});

const collection: Collection = {
  name: 'demo',
  settings: { headers: [], variables: [], sandboxMode: 'safe' },
  root: {
    uid: 'root',
    name: 'demo',
    items: [
      { type: 'summary', uid: 's1', name: 'Login', method: 'POST', url: '/l', fileName: 'login.yml' },
      {
        type: 'folder',
        uid: 'f1',
        name: 'Users',
        dirName: 'users',
        items: [
          { type: 'summary', uid: 's2', name: 'List users', method: 'GET', url: '/u', fileName: 'list.yml' },
        ],
      },
    ],
  },
};

const fullRequest: Request = {
  uid: 'req-login',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [],
  auth: { authType: 'inherit' },
};

function renderEditor() {
  const onPick = vi.fn();
  const onConvertToInline = vi.fn();
  render(
    <SavedSourceEditor
      requestPath='login.yml'
      collection='demo'
      onPick={onPick}
      onConvertToInline={onConvertToInline}
      converting={false}
    />,
  );
  return { onPick, onConvertToInline };
}

describe('SavedSourceEditor', () => {
  beforeEach(() => {
    vi.mocked(getCollection).mockReset();
    vi.mocked(getRequest).mockReset();
    usePaneStore.getState().reset();
  });

  it('shows the saved request path', () => {
    renderEditor();
    expect(screen.getByTestId('saved-request-path')).toHaveTextContent('login.yml');
  });

  it('lists and filters the collection requests, and picks one', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const { onPick } = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    expect(await screen.findByRole('button', { name: /List users/ })).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText('Filter requests'), 'list');
    expect(screen.queryByRole('button', { name: /Login/ })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /List users/ }));
    expect(onPick).toHaveBeenCalledWith({ path: 'users/list.yml', name: 'List users', method: 'GET' });
  });

  it('shows a load error with Retry', async () => {
    vi.mocked(getCollection)
      .mockRejectedValueOnce('collection not found')
      .mockResolvedValueOnce(collection);
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('collection not found');
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('button', { name: /Login/ })).toBeInTheDocument();
  });

  it('opens the saved request in a request tab', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Open request' }));
    await waitFor(() => {
      const root = usePaneStore.getState().root;
      if (root.type !== 'leaf') throw new Error('Expected a leaf');
      expect(root.tabs.some((t) => t.id === 'req-login' && t.tabType === 'request')).toBe(true);
    });
    expect(getRequest).toHaveBeenCalledWith('demo', 'login.yml');
  });

  it('reports a request that cannot be opened', async () => {
    vi.mocked(getRequest).mockRejectedValue('file not found');
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Open request' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not open the request: file not found',
    );
  });

  it('asks the parent to convert to inline', async () => {
    const { onConvertToInline } = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(onConvertToInline).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test --run SavedSourceEditor`
Expected: FAIL, `Failed to resolve import "../SavedSourceEditor"`.

- [ ] **Step 4: Create `RequestPicker.tsx`**

```tsx
import { useCallback, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { requestEntriesOf, type SavedRequestEntry } from '@/lib/flow-node-edits';
import { getCollection } from '@/lib/tauri-api';

// A searchable list of this collection's requests. The tree is fetched each
// time the popover opens, so it never shows a stale list.
export function RequestPicker({
  collection,
  triggerLabel,
  onPick,
}: {
  collection: string;
  triggerLabel: string;
  onPick: (entry: SavedRequestEntry) => void;
}) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<SavedRequestEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const load = useCallback(() => {
    setError(null);
    setEntries(null);
    getCollection(collection)
      .then((c) => setEntries(requestEntriesOf(c.root)))
      .catch((err) => setError(String(err)));
  }, [collection]);

  useEffect(() => {
    if (open) load();
  }, [open, load]);

  const query = filter.trim().toLowerCase();
  const shown = (entries ?? []).filter(
    (e) => !query || e.name.toLowerCase().includes(query) || e.path.toLowerCase().includes(query),
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button type='button' variant='outline' size='sm' className='h-7 text-xs'>
          {triggerLabel}
        </Button>
      </PopoverTrigger>
      <PopoverContent align='start' className='nokey w-72 p-2'>
        <Input
          aria-label='Filter requests'
          placeholder='Filter requests'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className='mb-2 h-7 text-xs'
        />
        {error ? (
          <div role='alert' className='flex items-center justify-between gap-2 text-xs text-red-600'>
            <span>Could not load requests: {error}</span>
            <Button type='button' variant='outline' size='sm' className='h-6 text-xs' onClick={load}>
              Retry
            </Button>
          </div>
        ) : entries === null ? (
          <p className='text-xs text-muted-foreground'>Loading…</p>
        ) : shown.length === 0 ? (
          <p className='text-xs text-muted-foreground'>No matching requests.</p>
        ) : (
          <div className='max-h-60 space-y-0.5 overflow-y-auto'>
            {shown.map((entry) => (
              <Button
                key={entry.path}
                type='button'
                variant='ghost'
                size='sm'
                className='h-7 w-full justify-start gap-2 text-xs'
                onClick={() => {
                  onPick(entry);
                  setOpen(false);
                }}
              >
                <span className='font-mono text-[10px] text-muted-foreground'>{entry.method}</span>
                <span className='truncate'>{entry.name}</span>
              </Button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 5: Create `openSavedRequest.ts`**

```ts
import { findTabInTree, mapApiRequestToState } from '@/lib/pane-utils';
import { getRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

/**
 * Opens a saved request in a normal request tab, the same way the collection
 * sidebar does. An already-open tab is only focused. Rejects when the file
 * cannot be read, so the caller can show the error.
 */
export async function openSavedRequestTab(collection: string, path: string): Promise<void> {
  const request = await getRequest(collection, path);
  const store = usePaneStore.getState();
  const existing = findTabInTree(store.root, request.uid);
  if (existing) {
    store.openTab(existing.tab);
    return;
  }
  const tab: RequestTab = {
    id: request.uid,
    title: request.name,
    tabType: 'request',
    request: mapApiRequestToState(request, true),
    response: null,
    isDirty: false,
    source: { collection, path },
  };
  store.openTab(tab);
}
```

If `RequestTab` requires more fields than this literal sets, copy the missing ones from `createTab()` in `src/components/collections/RequestNode.tsx`. That function is the sidebar's version of this code, and `tsc` will name any missing field.

- [ ] **Step 6: Create `SavedSourceEditor.tsx`**

```tsx
import { ExternalLink } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import type { SavedRequestEntry } from '@/lib/flow-node-edits';
import { openSavedRequestTab } from './openSavedRequest';
import { RequestPicker } from './RequestPicker';

// A saved request's content is edited in its own tab, never from the flow.
// Here the node can be repointed, opened, or copied into an inline request.
export function SavedSourceEditor({
  requestPath,
  collection,
  onPick,
  onConvertToInline,
  converting,
}: {
  requestPath: string;
  collection: string;
  onPick: (entry: SavedRequestEntry) => void;
  onConvertToInline: () => void;
  converting: boolean;
}) {
  const [openError, setOpenError] = useState<string | null>(null);

  const open = () => {
    setOpenError(null);
    openSavedRequestTab(collection, requestPath).catch((err) => setOpenError(String(err)));
  };

  return (
    <div className='space-y-2'>
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Saved request</span>
        <p
          data-testid='saved-request-path'
          className='truncate rounded border bg-muted px-2 py-1 font-mono text-xs'
        >
          {requestPath}
        </p>
      </div>
      <div className='flex flex-wrap gap-2'>
        <RequestPicker collection={collection} triggerLabel='Choose request…' onPick={onPick} />
        <Button type='button' variant='outline' size='sm' className='h-7 gap-1 text-xs' onClick={open}>
          <ExternalLink className='h-3 w-3' aria-hidden='true' />
          Open request
        </Button>
        <Button
          type='button'
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          disabled={converting}
          onClick={onConvertToInline}
        >
          Convert to inline
        </Button>
      </div>
      {openError && (
        <p role='alert' className='text-xs text-red-600'>
          Could not open the request: {openError}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test --run SavedSourceEditor`
Expected: PASS, 6 tests.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 8: Commit**

Stage the three new source files and the test. Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add saved request editor and picker`. Footer: `Relates to: #26`.

---

### Task 3: Request node editor with source switching, wired into the panel

**Files:**
- Create: `src/components/flow/properties/RequestNodeEditor.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (add the `edges` and `collection` props; the `'Request'` branch uses `RequestNodeEditor`)
- Modify: `src/components/flow/FlowPane.tsx` (pass `edges` and `collection` to the panel)
- Modify: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (pass the two new props)
- Test: `src/components/flow/properties/__tests__/RequestNodeEditor.test.tsx` (create)

**Interfaces:**
- Consumes:
  - From Task 1: `InlineSourceEditor`.
  - From Task 2: `SavedSourceEditor` and `RequestPicker`.
  - From plan 01: `LabelField`, `savedToInline`, `inlineHasContent`, `indexWiresOutOfRange` and `SavedRequestEntry`.
  - `getRequest` from `@/lib/tauri-api`.
- Produces:
  - `RequestNodeEditor({ nodeId: string; kind: RequestKind; edges: FlowEdge[]; collection: string; onChange: (kind: FlowNodeKind) => void })`.
  - `NodePropertiesPanel({ node, edges, collection, onChange, onClose })`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** This task converts saved requests into inline data.

- [ ] **Step 2: Write the failing tests**

Create `src/components/flow/properties/__tests__/RequestNodeEditor.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type Collection,
  type FlowNodeKind,
  getCollection,
  getRequest,
  type Request,
} from '@/lib/tauri-api';
import { RequestNodeEditor } from '../RequestNodeEditor';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getRequest: vi.fn() };
});

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

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Body'
      value={props.value}
      onChange={(e) => props.onChange?.(e.target.value)}
    />
  ),
}));

type RequestKind = Extract<FlowNodeKind, { kind: 'Request' }>;

const savedKind: RequestKind = {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'login.yml' },
};

const emptyInline: RequestKind = {
  kind: 'Request',
  label: 'Draft',
  source: { type: 'Inline', request: { method: 'GET', url: '', headers: [], body: null } },
};

const filledInline: RequestKind = {
  kind: 'Request',
  label: 'Draft',
  source: {
    type: 'Inline',
    request: { method: 'POST', url: 'https://x/y', headers: [], body: null },
  },
};

const withAuth: Request = {
  uid: 'u1',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [{ key: 'Content-Type', value: 'application/json', enabled: true }],
  body: { mode: 'json', content: '{}' },
  auth: { authType: 'bearer', token: 't' },
  preRequestScript: 'rok.setVar("a", 1);',
};

const collection: Collection = {
  name: 'demo',
  settings: { headers: [], variables: [], sandboxMode: 'safe' },
  root: {
    uid: 'root',
    name: 'demo',
    items: [
      { type: 'summary', uid: 's1', name: 'Me', method: 'GET', url: '/me', fileName: 'me.yml' },
    ],
  },
};

function renderEditor(kind: RequestKind) {
  const onChange = vi.fn();
  render(
    <RequestNodeEditor nodeId='r1' kind={kind} edges={[]} collection='demo' onChange={onChange} />,
  );
  return onChange;
}

describe('RequestNodeEditor', () => {
  beforeEach(() => {
    vi.mocked(getRequest).mockReset();
    vi.mocked(getCollection).mockReset();
  });

  it('shows the saved editor for a Saved source and repoints on pick', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...savedKind,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('shows what is dropped and waits for confirmation', async () => {
    vi.mocked(getRequest).mockResolvedValue(withAuth);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(await screen.findByText(/bearer auth, pre-request script/)).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole('button', { name: 'Convert' }));
    expect(onChange).toHaveBeenCalledWith({
      ...savedKind,
      source: {
        type: 'Inline',
        request: {
          method: 'POST',
          url: '{{baseUrl}}/login',
          headers: [{ name: 'Content-Type', value: 'application/json' }],
          body: '{}',
        },
      },
    });
    expect(getRequest).toHaveBeenCalledWith('demo', 'login.yml');
  });

  it('cancelling a conversion changes nothing', async () => {
    vi.mocked(getRequest).mockResolvedValue(withAuth);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Convert' })).not.toBeInTheDocument();
  });

  it('shows a load error and changes nothing', async () => {
    vi.mocked(getRequest).mockRejectedValue('file not found');
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not load "login.yml": file not found',
    );
    expect(onChange).not.toHaveBeenCalled();
  });

  it('switches an empty inline request to a saved one at once', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(emptyInline);
    await userEvent.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...emptyInline,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('asks before discarding a non-empty inline request', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(filledInline);
    await userEvent.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).not.toHaveBeenCalled();
    expect(
      screen.getByText(/The inline method, URL, headers and body will be discarded/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Use saved request' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...filledInline,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('edits the inline request through the inline editor', async () => {
    const onChange = renderEditor(filledInline);
    await userEvent.type(screen.getByLabelText('URL'), 'z');
    expect(onChange).toHaveBeenLastCalledWith({
      ...filledInline,
      source: {
        type: 'Inline',
        request: { method: 'POST', url: 'https://x/yz', headers: [], body: null },
      },
    });
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test --run RequestNodeEditor`
Expected: FAIL, `Failed to resolve import "../RequestNodeEditor"`.

- [ ] **Step 4: Implement `RequestNodeEditor.tsx`**

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  indexWiresOutOfRange,
  inlineHasContent,
  type SavedRequestEntry,
  savedToInline,
} from '@/lib/flow-node-edits';
import {
  type FlowEdge,
  type FlowNodeKind,
  getRequest,
  type InlineRequestData,
} from '@/lib/tauri-api';
import { InlineSourceEditor } from './InlineSourceEditor';
import { LabelField } from './LabelField';
import { RequestPicker } from './RequestPicker';
import { SavedSourceEditor } from './SavedSourceEditor';

type RequestKind = Extract<FlowNodeKind, { kind: 'Request' }>;

// A source switch waiting for the user's confirmation.
type Pending =
  | { type: 'convert'; inline: InlineRequestData; dropped: string[] }
  | { type: 'use-saved'; entry: SavedRequestEntry };

export function RequestNodeEditor({
  nodeId,
  kind,
  edges,
  collection,
  onChange,
}: {
  nodeId: string;
  kind: RequestKind;
  edges: FlowEdge[];
  collection: string;
  onChange: (kind: FlowNodeKind) => void;
}) {
  const [pending, setPending] = useState<Pending | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [converting, setConverting] = useState(false);
  const source = kind.source;

  const useSaved = (entry: SavedRequestEntry) =>
    onChange({ ...kind, source: { type: 'Saved', requestPath: entry.path } });

  // Loads the saved file and shows what the copy keeps and drops. Nothing
  // changes until the user confirms.
  const startConvert = async () => {
    if (source.type !== 'Saved') return;
    setLoadError(null);
    setConverting(true);
    try {
      const request = await getRequest(collection, source.requestPath);
      const { inline, dropped } = savedToInline(request);
      setPending({ type: 'convert', inline, dropped });
    } catch (err) {
      setLoadError(`Could not load "${source.requestPath}": ${String(err)}`);
    } finally {
      setConverting(false);
    }
  };

  // Switching an inline request back to a saved one only asks when it would
  // throw away something the user typed.
  const pickSavedForInline = (entry: SavedRequestEntry) => {
    if (source.type === 'Inline' && inlineHasContent(source.request)) {
      setPending({ type: 'use-saved', entry });
      return;
    }
    useSaved(entry);
  };

  const confirm = () => {
    if (!pending) return;
    if (pending.type === 'convert') {
      onChange({ ...kind, source: { type: 'Inline', request: pending.inline } });
    } else {
      useSaved(pending.entry);
    }
    setPending(null);
  };

  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <p className='text-xs text-muted-foreground'>
        Source: {source.type === 'Saved' ? 'saved request' : 'inline request'}
      </p>

      {source.type === 'Saved' ? (
        <SavedSourceEditor
          requestPath={source.requestPath}
          collection={collection}
          onPick={useSaved}
          onConvertToInline={() => void startConvert()}
          converting={converting}
        />
      ) : (
        <>
          <InlineSourceEditor
            request={source.request}
            onChange={(request) => onChange({ ...kind, source: { type: 'Inline', request } })}
            outOfRangeWires={indexWiresOutOfRange(edges, nodeId, source.request.headers.length)}
          />
          <RequestPicker
            collection={collection}
            triggerLabel='Use a saved request…'
            onPick={pickSavedForInline}
          />
        </>
      )}

      {loadError && (
        <p role='alert' className='text-xs text-red-600'>
          {loadError}
        </p>
      )}

      {pending && (
        <div role='group' aria-label='Confirm source change' className='space-y-2 rounded border p-2'>
          <p className='text-xs'>
            {pending.type === 'convert'
              ? pending.dropped.length > 0
                ? `Convert to inline? This copies the request into this flow. These parts are not carried over: ${pending.dropped.join(', ')}.`
                : 'Convert to inline? This copies the request into this flow. The saved file is not changed.'
              : `Use "${pending.entry.name}"? The inline method, URL, headers and body will be discarded.`}
          </p>
          <div className='flex justify-end gap-2'>
            <Button type='button' variant='outline' size='sm' className='h-7 text-xs' onClick={() => setPending(null)}>
              Cancel
            </Button>
            <Button type='button' size='sm' className='h-7 text-xs' onClick={confirm}>
              {pending.type === 'convert' ? 'Convert' : 'Use saved request'}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Route Request nodes to the new editor**

In `src/components/flow/properties/NodePropertiesPanel.tsx`:

1. Add `import type { FlowEdge } from '@/lib/tauri-api';`, extending the existing type import, and `import { RequestNodeEditor } from './RequestNodeEditor';`.
2. Change `editorFor` to take the node, edges and collection:

```tsx
function editorFor(
  node: FlowNode,
  edges: FlowEdge[],
  collection: string,
  onChange: (kind: FlowNodeKind) => void,
) {
  const kind = node.kind;
  switch (kind.kind) {
    case 'Request':
      return (
        <RequestNodeEditor
          // Keyed by node, so a pending confirmation never carries over to another node.
          key={node.id}
          nodeId={node.id}
          kind={kind}
          edges={edges}
          collection={collection}
          onChange={onChange}
        />
      );
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
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
  }
}
```

3. Add `edges: FlowEdge[]` and `collection: string` to `NodePropertiesPanel`'s props. Destructure them, and change the call to `{editorFor(node, edges, collection, onChange)}`.

In `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`, change the render call in `renderPanel` to:

```tsx
render(
  <NodePropertiesPanel node={n} edges={[]} collection='demo' onChange={onChange} onClose={onClose} />,
);
```

In `src/components/flow/FlowPane.tsx`, add the two props to the `<NodePropertiesPanel …>` element:

```tsx
              edges={tab.edges}
              collection={collectionName}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn test --run RequestNodeEditor NodePropertiesPanel`
Expected: PASS, 7 new tests plus the 5 panel tests.

Run: `yarn test --run flow pane-store`
Expected: PASS, including `FlowPane.properties.test.tsx` from plan 01.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 7: Commit**

Stage every file listed under **Files**. Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): edit request nodes in properties panel`. Footer: `Relates to: #26`.

## Manual check (after this plan)

In `yarn tauri dev`, open a flow and check the following:
- Select an inline Request node. Type in the URL and header fields (real CodeMirror) and the body (real Monaco), then press Backspace in each. The text changes and the node stays.
- Drag the panel's resize handle.
- Convert a saved request that has auth. The confirmation lists "… auth".
- Save the flow. It re-opens with the edits.
