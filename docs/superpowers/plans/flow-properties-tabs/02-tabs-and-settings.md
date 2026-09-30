# Plan 02 — Tabs shell and Settings details Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The node properties panel gets a tab bar with a Settings tab that shows real details — a saved request's method, URL, masked headers, auth type and body preview; If, Switch and Output details; and the last save error — and saved Request cards on the canvas stop showing `SAVED · 0 set · —`.

**Architecture:** `FlowPane` passes the panel everything it needs (status, detail, nodes, save error, the selected tab, wire and node callbacks). A small non-React module caches one preview per saved request (`getRequest` once, masked with the header helper from plan 01); two hooks read it — one for the panel, one for all cards on the canvas. This plan renders only the Settings trigger; plans 03 and 04 each add their own trigger and content, so no tab ever shows placeholder text.

**Tech Stack:** React 19 + TypeScript, shadcn/ui (`Tabs`, `Badge`), Radix Tabs, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-properties-panel-tabs-design.md` (§3, §4). Contract: `docs/superpowers/plans/flow-properties-tabs/00-index.md`. Depends on plan 01 (`isSensitiveHeader`, `REDACTED_VALUE`).

## Global Constraints

- shadcn/ui primitives only (`Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` from `@/components/ui/tabs`; `Badge`; `Button`); lucide-react icons only; no raw `<button>`, `<input>`.
- Zustand: narrow selectors; never destructure the whole store.
- The panel root keeps its `nokey` class; every new control sits inside it, so Backspace never deletes a node.
- Header values whose name is sensitive show `REDACTED_VALUE`; auth shows its type only; `{{var}}` text is shown as typed, never resolved.
- A saved request that cannot be loaded shows `Could not load request: <error>` inside its section only (no `role="alert"`, so existing alert queries in `RequestNodeEditor` tests stay unique).
- Body preview: the first 20 lines.
- Checks: `yarn test src/components/flow src/lib src/stores`, `yarn tsc --noEmit`, `yarn check`.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill. Never `git stash`.

## Review Focus

1. **A saved request with an `Authorization` header and bearer auth.** Expected: the header value shows `••••••` and auth shows "Bearer", never the token. Pinned in Task 2 (`masks sensitive headers and shows only the auth type`).
2. **A saved request file that fails to load.** Expected: the Settings section says "Could not load request: …", the card keeps its `SAVED` fallback, and nothing throws. Pinned in Task 2 (`shows a load error and keeps the card fallback`).
3. **The same saved request on three cards.** Expected: `getRequest` is called once. Pinned in Task 2 (`loads each saved request once for many cards`).
4. **Backspace with focus on the tab bar.** Expected: the tab trigger is inside the `nokey` panel root. Pinned in Task 1 (`keeps the tab bar inside the nokey panel`).
5. **A save error that names this node.** Expected: the full message appears at the top of Settings; a node the error does not name shows nothing. Pinned in Task 1 (`shows the save error only for a flagged node`).

---

### Task 1: Panel props, tab shell and the save-error box

**Files:**
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (props `:58-77`, body `:108-145`)
- Modify: `src/components/flow/FlowPane.tsx` (state near `:66-71`, `handleSave` `:286-312`, canvas `onEdgeEdit` `:425-429`, panel mount `:471-483`)
- Test: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (`renderPanel` `:27-43`)

**Interfaces:**
- Produces: `export type PanelTab = 'settings' | 'last-run' | 'wires'` (in `NodePropertiesPanel.tsx`); new panel props `status`, `detail?`, `nodes`, `saveError?`, `activeTab`, `onTabChange`, `onEditWire`, `onSelectNode` exactly as in the index. Plans 03 and 04 add their `TabsTrigger` and `TabsContent` inside the same `Tabs`.

- [ ] **Step 1: Write the failing tests**

In `NodePropertiesPanel.test.tsx`, replace `renderPanel` with a version that passes the new props:

```tsx
function renderPanel(n: FlowNode, extra: Partial<Parameters<typeof NodePropertiesPanel>[0]> = {}) {
  const onChange = vi.fn();
  const onClose = vi.fn();
  const onDelete = vi.fn();
  const onTabChange = vi.fn();
  render(
    <NodePropertiesPanel
      node={n}
      edges={[]}
      nodes={[n]}
      collection='demo'
      status='idle'
      activeTab='settings'
      onTabChange={onTabChange}
      onEditWire={vi.fn()}
      onSelectNode={vi.fn()}
      onChange={onChange}
      onClose={onClose}
      onDelete={onDelete}
      {...extra}
    />,
  );
  return { onChange, onClose, onDelete, onTabChange };
}
```

Add:

```tsx
  it('shows a Settings tab holding the editors', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.getByRole('tab', { name: 'Settings' })).toHaveAttribute('data-state', 'active');
    expect(screen.getByRole('tabpanel')).toContainElement(screen.getByLabelText('Label'));
  });

  it('keeps the tab bar inside the nokey panel', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.getByRole('tab', { name: 'Settings' }).closest('.nokey')).not.toBeNull();
  });

  it('shows the save error only for a flagged node', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }), {
      saveError: 'flow contains a cycle through node(s): o1',
    });
    expect(screen.getByTestId('node-save-error')).toHaveTextContent(
      'flow contains a cycle through node(s): o1',
    );
  });

  it('shows no save error box without an error', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.queryByTestId('node-save-error')).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
Expected: FAIL — `Unable to find role="tab"` and no `node-save-error`; tsc-in-test may also report the unknown props.

- [ ] **Step 3: Implement**

`NodePropertiesPanel.tsx`: import `Tabs, TabsContent, TabsList, TabsTrigger` from `@/components/ui/tabs`, the `FlowNodeStatus` type from `@/lib/tauri-api`, and `FlowNodeDetail` from `@/types/pane-types`. Export the tab type and extend the props:

```tsx
/** The panel's tabs. The selected one is kept by FlowPane across nodes. */
export type PanelTab = 'settings' | 'last-run' | 'wires';
```

```tsx
  node: FlowNode;
  edges: FlowEdge[];
  nodes: FlowNode[];
  collection: string;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
  // The full message of the last failed save, when it named this node.
  saveError?: string;
  activeTab: PanelTab;
  onTabChange: (tab: PanelTab) => void;
  // Opens the script dialog of a wire, as a double-click on the canvas does.
  onEditWire: (edgeId: string) => void;
  // Selects another node and shows it in this panel.
  onSelectNode: (nodeId: string) => void;
  onChange: (kind: FlowNodeKind) => void;
```

Destructure only the props this plan reads (`nodes`, `saveError`, `activeTab`, `onTabChange` plus the existing ones); plans 03 and 04 start reading `status`, `detail`, `onEditWire` and `onSelectNode`. Replace the body `<div key={node.id} …>` (`:140-144`) with:

```tsx
      <Tabs
        value={activeTab}
        onValueChange={(value) => onTabChange(value as PanelTab)}
        className='flex min-h-0 flex-1 flex-col'
      >
        <TabsList className='mx-3 mt-2 self-start'>
          <TabsTrigger value='settings' className='text-xs'>
            Settings
          </TabsTrigger>
        </TabsList>
        <TabsContent value='settings' className='min-h-0 flex-1 overflow-y-auto p-3'>
          <div key={node.id} className='space-y-3'>
            {saveError && (
              <p
                data-testid='node-save-error'
                className='break-words rounded border border-red-500/50 bg-red-500/10 p-2 text-xs text-red-600'
              >
                {saveError}
              </p>
            )}
            <PanelFocusProvider value={refocusPanel}>
              {editorFor(node, edges, nodes, collection, onChange)}
            </PanelFocusProvider>
          </div>
        </TabsContent>
      </Tabs>
```

Add `nodes: FlowNode[]` as the third parameter of `editorFor` (Task 3 reads it) and pass it through.

`FlowPane.tsx`: import `type PanelTab` from `./properties/NodePropertiesPanel`. Add state next to `cycleNodeIds`:

```tsx
  // The full text of the last failed save. The panel shows it for flagged nodes.
  const [saveErrorMessage, setSaveErrorMessage] = useState<string | null>(null);
  // Kept across nodes, so after a run the user can click through Last run.
  const [panelTab, setPanelTab] = useState<PanelTab>('settings');
```

In `handleSave`: after `setCycleEdgeIds([])` on success add `setSaveErrorMessage(null);`; in the catch, inside `if (parsed)`, add `setSaveErrorMessage(message);`.

Extract the canvas wire opener into one function and use it for both callers:

```tsx
  // Opens the script dialog of a wire. Run when wires carry no value.
  const openWireEditor = (edgeId: string) => {
    const edge = tab.edges.find((e) => e.id === edgeId);
    if (edge && shouldPromptForExpression(edge)) setPendingEdge(edge);
  };
```

Replace the inline `onEdgeEdit` body with `onEdgeEdit={openWireEditor}`. Pass the new panel props:

```tsx
            <NodePropertiesPanel
              node={panelNode}
              edges={tab.edges}
              nodes={tab.nodes}
              collection={collectionName}
              status={tab.nodeStatus[panelNode.id] ?? 'idle'}
              detail={tab.nodeDetail?.[panelNode.id]}
              saveError={
                saveErrorMessage && cycleNodeIds.includes(panelNode.id)
                  ? saveErrorMessage
                  : undefined
              }
              activeTab={panelTab}
              onTabChange={setPanelTab}
              onEditWire={openWireEditor}
              onSelectNode={(nodeId) => {
                setSelectedNodeIds(new Set([nodeId]));
                setPanelNodeId(nodeId);
              }}
              onChange={(kind) => handleNodeKindChange(panelNode.id, kind)}
```

(keep `onClose`, `onDelete`, `focusRequest`, `autoFocusLabel` as they are).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS; every existing FlowPane and panel test still green.

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): add a tab bar and save error to the node panel`.

---

### Task 2: Saved request preview, Settings details and the card fix

**Files:**
- Create: `src/lib/saved-request-preview.ts` (cache, loader, `toSavedRequestPreview`)
- Create: `src/components/flow/properties/useSavedRequestPreview.ts` (hooks)
- Create: `src/components/flow/properties/SavedRequestDetails.tsx`
- Modify: `src/components/flow/properties/RequestNodeEditor.tsx:141-148` (render the details under `SavedSourceEditor`)
- Modify: `src/components/flow/FlowCanvas.tsx:86-110` (`toRfNodes`) and `:269-281` (its `useMemo`)
- Test: `src/lib/__tests__/saved-request-preview.test.ts`, `src/components/flow/properties/__tests__/SavedRequestDetails.test.tsx`, `src/components/flow/__tests__/FlowCanvas.savedPreview.test.tsx`
- Modify test: `src/components/flow/properties/__tests__/RequestNodeEditor.test.tsx:97-100` (clear the preview cache in `beforeEach`)

**Interfaces:**
- Consumes: `isSensitiveHeader`, `REDACTED_VALUE` (plan 01), `getRequest`, `onCollectionChanged`, `Request` (tauri-api).
- Produces (index extended — see "Contract extension" below): `SavedRequestPreview`; `toSavedRequestPreview(request: Request): SavedRequestPreview`; `loadSavedRequestPreview(collection, path): void`; `peekSavedRequestPreview(collection, path): PreviewEntry | undefined`; `clearSavedRequestPreviewCache(collection?: string): void`; `subscribeSavedRequestPreviews(listener): () => void`; `getSavedRequestPreviewVersion(): number`; hooks `useSavedRequestPreview(collection, requestPath)` and `useSavedRequestPreviews(collection, paths): Record<string, SavedRequestPreview>`.

**Contract extension:** the index named only `useSavedRequestPreview` and `clearSavedRequestPreviewCache` in `properties/useSavedRequestPreview.ts`. The cache and pure mapping move to `src/lib/saved-request-preview.ts` so `FlowCanvas` can share them without importing panel code, and a second hook serves the cards. `00-index.md` is updated to match.

- [ ] **Step 1: Write the failing tests**

`src/lib/__tests__/saved-request-preview.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => {})),
}));

import {
  clearSavedRequestPreviewCache,
  loadSavedRequestPreview,
  peekSavedRequestPreview,
  toSavedRequestPreview,
} from '../saved-request-preview';

const request = (overrides: Partial<Request> = {}): Request => ({
  uid: 'u',
  name: 'Login',
  method: 'POST',
  url: '{{base}}/login',
  headers: [
    { key: 'Authorization', value: 'Bearer abc', enabled: true },
    { key: 'X-Trace', value: '1', enabled: true },
    { key: 'X-Off', value: 'no', enabled: false },
  ],
  body: { mode: 'json', content: Array.from({ length: 25 }, (_, i) => `line ${i + 1}`).join('\n') },
  auth: { authType: 'bearer', token: 'secret-token' },
  ...overrides,
});

describe('toSavedRequestPreview', () => {
  it('masks sensitive headers and shows only the auth type', () => {
    const p = toSavedRequestPreview(request());
    expect(p.method).toBe('POST');
    expect(p.url).toBe('{{base}}/login');
    expect(p.headers).toEqual([
      { key: 'Authorization', value: '••••••', enabled: true },
      { key: 'X-Trace', value: '1', enabled: true },
      { key: 'X-Off', value: 'no', enabled: false },
    ]);
    expect(p.authType).toBe('bearer');
    expect(JSON.stringify(p)).not.toContain('secret-token');
  });

  it('keeps the first 20 lines of the body', () => {
    const lines = toSavedRequestPreview(request()).bodyPreview?.split('\n') ?? [];
    expect(lines).toHaveLength(20);
    expect(lines[19]).toBe('line 20');
  });

  it('shows form fields as key=value and no body as null', () => {
    const form = toSavedRequestPreview(
      request({
        body: {
          mode: 'formurlencoded',
          formData: [
            { key: 'a', value: '1', entryType: 'text', enabled: true },
            { key: 'b', value: '2', entryType: 'text', enabled: false },
          ],
        },
      }),
    );
    expect(form.bodyPreview).toBe('a=1');
    expect(toSavedRequestPreview(request({ body: { mode: 'none' } })).bodyPreview).toBeNull();
  });
});

describe('the preview cache', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('loads a request once and serves it from the cache', async () => {
    getRequest.mockResolvedValue(request());
    loadSavedRequestPreview('demo', 'auth/login.yml');
    loadSavedRequestPreview('demo', 'auth/login.yml');
    await vi.waitFor(() =>
      expect(peekSavedRequestPreview('demo', 'auth/login.yml')?.status).toBe('ready'),
    );
    expect(getRequest).toHaveBeenCalledTimes(1);
  });

  it('records a load failure, including a missing request', async () => {
    getRequest.mockResolvedValue(undefined);
    loadSavedRequestPreview('demo', 'gone.yml');
    await vi.waitFor(() => expect(peekSavedRequestPreview('demo', 'gone.yml')?.status).toBe('error'));
  });

  it('clears one collection only', async () => {
    getRequest.mockResolvedValue(request());
    loadSavedRequestPreview('demo', 'a.yml');
    loadSavedRequestPreview('other', 'a.yml');
    await vi.waitFor(() => expect(peekSavedRequestPreview('other', 'a.yml')?.status).toBe('ready'));
    clearSavedRequestPreviewCache('demo');
    expect(peekSavedRequestPreview('demo', 'a.yml')).toBeUndefined();
    expect(peekSavedRequestPreview('other', 'a.yml')?.status).toBe('ready');
  });
});
```

`src/components/flow/properties/__tests__/SavedRequestDetails.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => {})),
}));

import { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
import { SavedRequestDetails } from '../SavedRequestDetails';

describe('SavedRequestDetails', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('shows the method, URL, masked headers, auth type and body', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'POST',
      url: '{{base}}/login',
      headers: [{ key: 'Authorization', value: 'Bearer abc', enabled: true }],
      body: { mode: 'json', content: '{"a":1}' },
      auth: { authType: 'bearer', token: 't' },
    });
    render(<SavedRequestDetails collection='demo' requestPath='auth/login.yml' />);
    expect(await screen.findByText('POST')).toBeInTheDocument();
    expect(screen.getByTestId('saved-request-url')).toHaveTextContent('{{base}}/login');
    expect(screen.getByTestId('saved-request-headers')).toHaveTextContent('Authorization');
    expect(screen.getByTestId('saved-request-headers')).toHaveTextContent('••••••');
    expect(screen.getByTestId('saved-request-auth')).toHaveTextContent('Bearer');
    expect(screen.getByTestId('saved-request-body')).toHaveTextContent('{"a":1}');
  });

  it('shows a load error and keeps the card fallback', async () => {
    getRequest.mockRejectedValue('file not found');
    render(<SavedRequestDetails collection='demo' requestPath='gone.yml' />);
    expect(await screen.findByText('Could not load request: file not found')).toBeInTheDocument();
  });
});
```

`src/components/flow/__tests__/FlowCanvas.savedPreview.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => {})),
}));

import { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
import { FlowCanvas } from '../FlowCanvas';

const saved = (id: string): FlowNode => ({
  id,
  kind: {
    kind: 'Request',
    label: `Login ${id}`,
    source: { type: 'Saved', requestPath: 'auth/login.yml' },
  },
  position: { x: 0, y: Number(id.slice(1)) * 200 },
});

function renderCanvas(nodes: FlowNode[]) {
  render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      flowCollectionName='demo'
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
    />,
  );
}

describe('saved Request cards', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('show the saved method, header count and body preview', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'POST',
      url: 'https://x.test/login',
      headers: [
        { key: 'A', value: '1', enabled: true },
        { key: 'B', value: '2', enabled: false },
      ],
      body: { mode: 'json', content: '{"u":1}' },
      auth: { authType: 'none' },
    });
    renderCanvas([saved('n1')]);
    expect(await screen.findByText('POST')).toBeInTheDocument();
    expect(screen.getByTestId('request-node-headers-row')).toHaveTextContent('1 set');
    expect(screen.getByTestId('request-node-card')).toHaveTextContent('{"u":1}');
  });

  it('loads each saved request once for many cards', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'GET',
      url: 'https://x.test',
      headers: [],
      auth: { authType: 'none' },
    });
    renderCanvas([saved('n1'), saved('n2'), saved('n3')]);
    expect(await screen.findAllByText('GET')).toHaveLength(3);
    expect(getRequest).toHaveBeenCalledTimes(1);
  });

  it('keeps the SAVED fallback when the request cannot be loaded', async () => {
    getRequest.mockRejectedValue('file not found');
    renderCanvas([saved('n1')]);
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalled());
    expect(screen.getByText('SAVED')).toBeInTheDocument();
  });
});
```

In `RequestNodeEditor.test.tsx`, in `beforeEach` (`:97`), add `clearSavedRequestPreviewCache();` (import it from `@/lib/saved-request-preview`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/lib/__tests__/saved-request-preview.test.ts src/components/flow/properties/__tests__/SavedRequestDetails.test.tsx src/components/flow/__tests__/FlowCanvas.savedPreview.test.tsx`
Expected: FAIL — `Cannot find module '../saved-request-preview'` / `'../SavedRequestDetails'`, and the card still shows `SAVED` with `0 set`.

- [ ] **Step 3: Implement**

`src/lib/saved-request-preview.ts`:

```ts
import { getRequest, onCollectionChanged, type Request } from '@/lib/tauri-api';
import { isSensitiveHeader, REDACTED_VALUE } from '@/lib/sensitive-headers';

/** What the flow shows of a saved request. Secrets are already masked. */
export interface SavedRequestPreview {
  method: string;
  url: string;
  headers: { key: string; value: string; enabled: boolean }[];
  /** The auth kind, such as 'bearer'. Never a credential. */
  authType: string;
  /** The first 20 lines of the body, or null when there is none. */
  bodyPreview: string | null;
}

export type PreviewEntry =
  | { status: 'loading' }
  | { status: 'ready'; preview: SavedRequestPreview }
  | { status: 'error'; error: string };

const BODY_PREVIEW_LINES = 20;

export function toSavedRequestPreview(request: Request): SavedRequestPreview {
  const body = request.body;
  let text: string | null = null;
  if (body && (body.mode === 'formdata' || body.mode === 'formurlencoded')) {
    const lines = (body.formData ?? []).filter((e) => e.enabled).map((e) => `${e.key}=${e.value}`);
    text = lines.length ? lines.join('\n') : null;
  } else if (body && body.mode !== 'none' && body.content) {
    text = body.content;
  }
  return {
    method: request.method,
    url: request.url,
    headers: request.headers.map((h) => ({
      key: h.key,
      value: isSensitiveHeader(h.key) ? REDACTED_VALUE : h.value,
      enabled: h.enabled,
    })),
    authType: request.auth.authType,
    bodyPreview: text === null ? null : text.split('\n').slice(0, BODY_PREVIEW_LINES).join('\n'),
  };
}

const cache = new Map<string, PreviewEntry>();
const listeners = new Set<() => void>();
let version = 0;
let subscribedToChanges = false;

const keyOf = (collection: string, path: string) => `${collection}\u0000${path}`;

function notify() {
  version += 1;
  for (const listener of listeners) listener();
}

// A saved request can change in its own tab, so a collection change drops
// that collection's previews. Tests that mock tauri-api without this
// listener simply skip it.
function watchCollectionChanges() {
  if (subscribedToChanges) return;
  subscribedToChanges = true;
  try {
    onCollectionChanged((event) => clearSavedRequestPreviewCache(event.collection)).catch(() => {});
  } catch {
    // No event bridge (tests); the cache is cleared explicitly there.
  }
}

export function peekSavedRequestPreview(collection: string, path: string): PreviewEntry | undefined {
  return cache.get(keyOf(collection, path));
}

export function loadSavedRequestPreview(collection: string, path: string): void {
  watchCollectionChanges();
  const key = keyOf(collection, path);
  if (cache.has(key)) return;
  cache.set(key, { status: 'loading' });
  notify();
  Promise.resolve()
    .then(() => getRequest(collection, path))
    .then((request) => {
      if (!request) throw new Error('request not found');
      cache.set(key, { status: 'ready', preview: toSavedRequestPreview(request) });
    })
    .catch((err) => {
      cache.set(key, { status: 'error', error: err instanceof Error ? err.message : String(err) });
    })
    .finally(notify);
}

export function clearSavedRequestPreviewCache(collection?: string): void {
  if (collection === undefined) cache.clear();
  else for (const key of [...cache.keys()]) if (key.startsWith(`${collection}\u0000`)) cache.delete(key);
  notify();
}

export function subscribeSavedRequestPreviews(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getSavedRequestPreviewVersion(): number {
  return version;
}
```

`src/components/flow/properties/useSavedRequestPreview.ts`:

```ts
import { useEffect, useSyncExternalStore } from 'react';
import {
  getSavedRequestPreviewVersion,
  loadSavedRequestPreview,
  peekSavedRequestPreview,
  type SavedRequestPreview,
  subscribeSavedRequestPreviews,
} from '@/lib/saved-request-preview';

export { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';

const useCacheVersion = () =>
  useSyncExternalStore(subscribeSavedRequestPreviews, getSavedRequestPreviewVersion);

/** One saved request's preview, loaded on first use. */
export function useSavedRequestPreview(collection: string, requestPath: string | null) {
  useCacheVersion();
  useEffect(() => {
    if (requestPath) loadSavedRequestPreview(collection, requestPath);
  }, [collection, requestPath]);
  const entry = requestPath ? peekSavedRequestPreview(collection, requestPath) : undefined;
  return {
    preview: entry?.status === 'ready' ? entry.preview : null,
    error: entry?.status === 'error' ? entry.error : null,
    loading: requestPath !== null && (!entry || entry.status === 'loading'),
  };
}

/** The ready previews of many saved requests, keyed by path. */
export function useSavedRequestPreviews(
  collection: string | null,
  paths: string[],
): Record<string, SavedRequestPreview> {
  useCacheVersion();
  const joined = paths.join('\n');
  // biome-ignore lint/correctness/useExhaustiveDependencies: `joined` stands for `paths`.
  useEffect(() => {
    if (!collection) return;
    for (const path of paths) loadSavedRequestPreview(collection, path);
  }, [collection, joined]);
  const out: Record<string, SavedRequestPreview> = {};
  if (!collection) return out;
  for (const path of paths) {
    const entry = peekSavedRequestPreview(collection, path);
    if (entry?.status === 'ready') out[path] = entry.preview;
  }
  return out;
}
```

`src/components/flow/properties/SavedRequestDetails.tsx`:

```tsx
import { Badge } from '@/components/ui/badge';
import { useSavedRequestPreview } from './useSavedRequestPreview';

const AUTH_LABELS: Record<string, string> = {
  none: 'None',
  inherit: 'Inherited',
  basic: 'Basic',
  bearer: 'Bearer',
  'api-key': 'API key',
  'o-auth2': 'OAuth 2.0',
  'aws-sig-v4': 'AWS Signature v4',
};

// Read-only summary of a saved request. Its content is edited in its own tab.
export function SavedRequestDetails({
  collection,
  requestPath,
}: {
  collection: string;
  requestPath: string;
}) {
  const { preview, error, loading } = useSavedRequestPreview(collection, requestPath);
  if (error) {
    return <p className='text-xs text-red-600'>Could not load request: {error}</p>;
  }
  if (loading || !preview) {
    return <p className='text-xs text-muted-foreground'>Loading request…</p>;
  }
  const headers = preview.headers.filter((h) => h.enabled);
  return (
    <div className='space-y-2 text-xs' data-testid='saved-request-details'>
      <div className='flex items-center gap-2'>
        <Badge variant='secondary' className='font-mono text-[10px]'>
          {preview.method}
        </Badge>
        <span data-testid='saved-request-url' className='truncate font-mono' title={preview.url}>
          {preview.url}
        </span>
      </div>
      <div data-testid='saved-request-headers' className='space-y-0.5'>
        <span className='font-medium'>Headers</span>
        {headers.length === 0 ? (
          <p className='text-muted-foreground'>None</p>
        ) : (
          headers.map((h) => (
            <p key={h.key} className='truncate font-mono' title={`${h.key}: ${h.value}`}>
              {h.key}: {h.value}
            </p>
          ))
        )}
      </div>
      <p data-testid='saved-request-auth'>
        <span className='font-medium'>Auth</span> {AUTH_LABELS[preview.authType] ?? preview.authType}
      </p>
      <div className='space-y-0.5'>
        <span className='font-medium'>Body</span>
        {preview.bodyPreview === null ? (
          <p className='text-muted-foreground'>None</p>
        ) : (
          <pre
            data-testid='saved-request-body'
            className='max-h-40 overflow-auto whitespace-pre-wrap rounded border bg-muted p-2 font-mono'
          >
            {preview.bodyPreview}
          </pre>
        )}
      </div>
    </div>
  );
}
```

`RequestNodeEditor.tsx`: import `SavedRequestDetails`, and in the Saved branch render it after `SavedSourceEditor`:

```tsx
      {source.type === 'Saved' ? (
        <>
          <SavedSourceEditor
            requestPath={source.requestPath}
            collection={collection}
            onPick={applySaved}
            onConvertToInline={() => void startConvert()}
            converting={converting}
          />
          <SavedRequestDetails collection={collection} requestPath={source.requestPath} />
        </>
      ) : (
```

`FlowCanvas.tsx`: import `type SavedRequestPreview` from `@/lib/saved-request-preview` and `useSavedRequestPreviews` from `./properties/useSavedRequestPreview`. Add a last parameter to `toRfNodes` and spread the card fields for Saved Request nodes:

```tsx
  cycleNodeIds?: string[],
  savedPreviews: Record<string, SavedRequestPreview> = {},
): Node[] {
  return nodes.map((n) => {
    const preview =
      n.kind.kind === 'Request' && n.kind.source.type === 'Saved'
        ? savedPreviews[n.kind.source.requestPath]
        : undefined;
    return {
      id: n.id,
      type: n.kind.kind,
      position: n.position,
      data: {
        kind: n.kind,
        status: nodeStatus[n.id] ?? 'idle',
        ...nodeDetail?.[n.id],
        hasCycleError: cycleNodeIds?.includes(n.id) ?? false,
        ...(n.kind.kind === 'Output' && {
          hasValueWire: edges.some((e) => e.targetNodeId === n.id && e.targetField === 'value'),
        }),
        // A saved request's own method, headers and body, once loaded.
        ...(preview && {
          method: preview.method,
          headerCount: preview.headers.filter((h) => h.enabled).length,
          bodyPreview: preview.bodyPreview ?? undefined,
        }),
      },
      selected: selectedIds.has(n.id),
      measured: measured.get(n.id),
    };
  });
}
```

In `FlowCanvasInner`, before `rfNodes`:

```tsx
  const savedPaths = useMemo(
    () =>
      [
        ...new Set(
          nodes.flatMap((n) =>
            n.kind.kind === 'Request' && n.kind.source.type === 'Saved'
              ? [n.kind.source.requestPath]
              : [],
          ),
        ),
      ],
    [nodes],
  );
  const savedPreviews = useSavedRequestPreviews(flowCollectionName ?? null, savedPaths);
```

and pass `savedPreviews` as the last `toRfNodes` argument, adding it to the `useMemo` dependency list.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow src/lib src/stores && yarn tsc --noEmit && yarn check`
Expected: PASS; existing FlowCanvas and RequestNodeEditor tests still green (a failed load keeps today's fallback).

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): show saved request details in panel and cards`.

---

### Task 3: If, Switch and Output details

**Files:**
- Create: `src/components/flow/properties/NodeDetails.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (`editorFor` If/Switch/Output cases `:34-50`)
- Test: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (replace `tells the user where If and Switch details are edited`)

**Interfaces:**
- Consumes: `editorFor(node, edges, nodes, collection, onChange)` (Task 1).
- Produces: `IfDetails({ condition })`, `SwitchDetails({ value, cases })`, `OutputDetails({ nodeId, edges, nodes })`.

- [ ] **Step 1: Write the failing tests**

Replace the test `tells the user where If and Switch details are edited` with:

```tsx
  it('shows an If condition read-only with a pointer to the node', () => {
    renderPanel(node('if1', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' }));
    expect(screen.getByTestId('if-details-condition')).toHaveTextContent('response.status === 200');
    expect(screen.getByText('Edit on the node.')).toBeInTheDocument();
  });

  it('shows a Switch value and each case', () => {
    renderPanel(
      node('s1', {
        kind: 'Switch',
        label: 'Route',
        value: 'response.body.type',
        cases: [
          { id: 'c1', label: 'Card', matches: 'card' },
          { id: 'c2', label: 'Cash', matches: 'cash' },
        ],
      }),
    );
    expect(screen.getByTestId('switch-details-value')).toHaveTextContent('response.body.type');
    const cases = screen.getAllByTestId('switch-details-case').map((c) => c.textContent);
    expect(cases).toEqual(['Card = card', 'Cash = cash']);
  });

  it('shows which wire feeds an Output', () => {
    const out = node('o1', { kind: 'Output', label: 'Result' });
    const login = node('r1', {
      kind: 'Request',
      label: 'Login',
      source: { type: 'Saved', requestPath: 'auth/login.yml' },
    });
    renderPanel(out, {
      nodes: [out, login],
      edges: [
        {
          id: 'e1',
          sourceNodeId: 'r1',
          targetNodeId: 'o1',
          targetField: 'value',
          expression: 'response.body.token',
        },
      ],
    });
    expect(screen.getByTestId('output-details')).toHaveTextContent('response.body.token');
    expect(screen.getByTestId('output-details')).toHaveTextContent('Login');
  });

  it('says when an Output has no value wire', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Result' }));
    expect(screen.getByTestId('output-details')).toHaveTextContent('No value wire.');
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
Expected: FAIL — `Unable to find an element by: [data-testid="if-details-condition"]` and the others.

- [ ] **Step 3: Implement**

`src/components/flow/properties/NodeDetails.tsx`:

```tsx
import type { FlowEdge, FlowNode, SwitchCase } from '@/lib/tauri-api';

// If and Switch are edited on the node. The panel shows the same values so
// a long condition is readable.
export function IfDetails({ condition }: { condition: string }) {
  return (
    <div className='space-y-1 text-xs'>
      <span className='font-medium'>Condition</span>
      <pre
        data-testid='if-details-condition'
        className='whitespace-pre-wrap break-words rounded border bg-muted p-2 font-mono'
      >
        {condition || '—'}
      </pre>
      <p className='text-muted-foreground'>Edit on the node.</p>
    </div>
  );
}

export function SwitchDetails({ value, cases }: { value: string; cases: SwitchCase[] }) {
  return (
    <div className='space-y-1 text-xs'>
      <span className='font-medium'>Value</span>
      <pre
        data-testid='switch-details-value'
        className='whitespace-pre-wrap break-words rounded border bg-muted p-2 font-mono'
      >
        {value || '—'}
      </pre>
      <span className='font-medium'>Cases</span>
      {cases.length === 0 ? (
        <p className='text-muted-foreground'>No cases.</p>
      ) : (
        cases.map((c) => (
          <p key={c.id} data-testid='switch-details-case' className='truncate font-mono'>
            {c.label} = {c.matches}
          </p>
        ))
      )}
      <p className='text-muted-foreground'>Edit on the node.</p>
    </div>
  );
}

export function OutputDetails({
  nodeId,
  edges,
  nodes,
}: {
  nodeId: string;
  edges: FlowEdge[];
  nodes: FlowNode[];
}) {
  const wire = edges.find((e) => e.targetNodeId === nodeId && e.targetField === 'value');
  const source = wire ? nodes.find((n) => n.id === wire.sourceNodeId) : undefined;
  return (
    <div data-testid='output-details' className='space-y-1 text-xs'>
      {wire ? (
        <p>
          Shows <code className='rounded bg-muted px-1 font-mono'>{wire.expression || '(no script)'}</code>{' '}
          from <span className='font-medium'>{source?.kind.label ?? '(missing node)'}</span>
        </p>
      ) : (
        <p className='text-muted-foreground'>No value wire.</p>
      )}
    </div>
  );
}
```

`NodePropertiesPanel.tsx` `editorFor`: import the three components and replace the If, Switch and Output cases:

```tsx
    case 'If':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <IfDetails condition={kind.condition} />
        </div>
      );
    case 'Switch':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <SwitchDetails value={kind.value} cases={kind.cases} />
        </div>
      );
    case 'Output':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <OutputDetails nodeId={node.id} edges={edges} nodes={nodes} />
        </div>
      );
```

`LabelOnlyEditor` keeps its optional `note` prop; nothing passes it now. Update its comment to: "Output, If and Switch nodes edit only their label here. Their details show below it."

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): show If, Switch and Output details in the panel`.
