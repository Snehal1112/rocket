# Flow Layout, Search and Minimap Implementation Plan

> **Execute this plan:** P6. Before starting it, make sure these are merged to main: P4 (merged first, Tidy adds an undo step) and decision D2 (dagre) answered. After it is merged, the next plan to execute is P7. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a large flow easy to navigate: a minimap, a node search (Ctrl+F) that selects and zooms to each match, and a Tidy button that lays the graph out left to right in one undoable step.

**Architecture:** Three additions to `FlowCanvas`. The minimap is React Flow's `MiniMap` with plain colours. Search is a pure function (`flow-search.ts`) and a small `FlowSearchBar` component. Tidy is a pure function (`flow-layout.ts`, built on dagre) that returns nodes with new positions; `FlowCanvas` writes them with one `onNodesChange(next)` call, which is one undo step through the P4 store. Tidy only moves nodes, so one nodes write is enough. All frontend, no Rust.

**Tech Stack:** React, TypeScript, `@xyflow/react` 12 (`MiniMap`, `Panel`, `useReactFlow().fitView`), `@dagrejs/dagre` (decision D2, needs the human's OK), Vitest and Testing Library, shadcn `Button` and `Input`, lucide-react, `sonner`.

**Spec:** Roadmap item F-42 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P6) and `00-plan-index.md` (plan P6, decision D2).

**Depends on P4 for Task 3 only.** Used verbatim from P4: `FlowCanvas` prop `onNodesChange(nodes, options?)`, where a call without options is its own undo step; the `handleKeyDown` shape (`const key = e.key.toLowerCase()` then one `if (key === ...) { ...; return; }` branch per key); the `CANVAS_HINTS` array. Tasks 1 and 2 do not need P4. If P5 is merged first, its `if (e.shiftKey) return;` line sits after the P4 `y` branch; the new `f` branch goes before it.

## Decision D2 (needs the human's OK before Task 3 Step 1)

Recommended and assumed in this plan: add `@dagrejs/dagre` (about 30 to 40 KB). It changes `package.json` and `yarn.lock`. Task 3 Step 1 is a stop: ask the human, then run `yarn add @dagrejs/dagre`.

Zero-dependency fallback: if the human says no, replace the body of `layoutFlow` in `flow-layout.ts` with a hand-rolled layered layout of about 80 lines. Saved graphs are acyclic, so compute each node's rank as the longest path from a source (a memoised depth-first walk that ignores back edges, so a cycle in an unsaved graph cannot loop), group nodes by rank in input order, give each rank the x of the previous rank's x plus its widest node plus `RANK_SEP`, and stack the nodes of a rank downwards with `NODE_SEP` between them, ordered by the average row of their predecessors. The `layoutFlow` signature, the tests and the canvas wiring in this plan stay the same.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No `unwrap()` and no Rust changes in this plan.
- No `backdrop-filter`, no Tailwind `backdrop-*` classes and no `color-mix` anywhere in the new styling (WebKitGTK on this machine hangs while painting them). Use plain colours or `hsl(var(--token))`. A test in Task 1 checks this.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task.
- If `yarn check` reports only formatting problems in files this plan touched, fix them with `yarn biome format --write <those paths>` (not the repo-wide `yarn format`).
- Coordination with other plans: this plan edits only `FlowCanvas.tsx` plus new files, and does not edit `FlowPane.tsx` or `FlowToolbar.tsx`. P1 and P2 are untouched. P4 and P5 edit `FlowCanvas.tsx` too, so run the plans one at a time, in order.
- The new top-centre control panel sits between the Add node button (top-left, `absolute left-3 top-3`) and the run toolbar (top-right). On a very narrow canvas they can overlap; accepted.
- Not in scope: persisting the minimap on or off, search over wire expressions or node values other than those listed, replace, auto-tidy on every edit, vertical layout, a layout direction setting.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. Tidy must be exactly one undo step, and must write nothing (no empty step, no dirty flag) when the graph is already tidy. Tests pinned in Task 3 (`layoutFlow` returns the same array when nothing moves; canvas writes once; pane integration undoes in one Ctrl+Z).
2. Tidy must not crash on a graph with a cycle, on nodes that were never measured, or on an empty graph, and a failure must show a toast instead of leaving a half-laid-out graph. Tests pinned in Task 3.
3. Tidy of a selection must leave unselected nodes untouched and must not teleport the selection to the origin. Tests pinned in Task 3.
4. Search must be case-insensitive, cover every field in the spec, show "0 of 0" for no match without selecting anything, wrap around with Enter and Shift+Enter, and Ctrl+F inside an inline field must stay with the field. Tests pinned in Task 2.
5. The minimap must not use `backdrop-filter` or `color-mix`, and must colour nodes by run status first. Tests pinned in Task 1.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/components/flow/minimap.ts` (new) | `minimapNodeColor(node)`: plain hex colour by run status, then by node kind. |
| `src/components/flow/__tests__/minimap.test.ts` (new) | Colour and static style checks. |
| `src/lib/flow-search.ts` (new) | `searchFlowNodes(nodes, query)`: ids of matching nodes, in node order. |
| `src/lib/__tests__/flow-search.test.ts` (new) | Search tests. |
| `src/components/flow/FlowSearchBar.tsx` (new) | The search input, match counter and previous, next and close buttons. |
| `src/components/flow/__tests__/FlowSearchBar.test.tsx` (new) | Component tests. |
| `src/lib/flow-layout.ts` (new) | `layoutFlow(nodes, edges, sizes, only?)`: dagre left to right layout. |
| `src/lib/__tests__/flow-layout.test.ts` (new) | Layout tests. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `MiniMap`, the top-centre `Panel` with Tidy, Search and the search bar, `Ctrl+F` branch, hints. |
| `src/components/flow/__tests__/FlowCanvas.minimap.test.tsx` (new) | Minimap renders. |
| `src/components/flow/__tests__/FlowCanvas.search.test.tsx` (new) | Search on the canvas with a mocked `fitView`. |
| `src/components/flow/__tests__/FlowCanvas.tidy.test.tsx` (new) | Tidy on the canvas. |
| `src/components/flow/__tests__/FlowPane.tidy.test.tsx` (new) | Tidy through the store: one undo step. |
| `package.json`, `yarn.lock` (modify, Task 3, with the human's OK) | The dagre dependency. |

Existing tests to know: `src/components/flow/__tests__/FlowCanvas.test.tsx` (real React Flow in jsdom, `Harness`, `SelectHarness`, `trio`; no React Flow mocks, `ResizeObserver` is polyfilled in `src/test-setup.ts`), `src/components/flow/__tests__/FlowPane.delete.test.tsx` (real canvas inside `FlowPane`, needs the `DOMMatrixReadOnly` stub and the resize handle rect patch).

---

### Task 1: Minimap

**Files:**
- Create: `src/components/flow/minimap.ts`
- Create: `src/components/flow/__tests__/minimap.test.ts`
- Create: `src/components/flow/__tests__/FlowCanvas.minimap.test.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx` (import block, the `<Controls />` line, the hint `Panel`)

**Interfaces:**
- Produces: `minimapNodeColor(node: Node): string` (React Flow `Node`, reads `node.type` and `node.data.status`).

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/__tests__/minimap.test.ts`:

```ts
import type { Node } from '@xyflow/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { minimapNodeColor } from '../minimap';

const node = (type: string, status?: string): Node => ({
  id: 'n',
  type,
  position: { x: 0, y: 0 },
  data: status ? { status } : {},
});

describe('minimapNodeColor', () => {
  it('colours a node by run status first', () => {
    expect(minimapNodeColor(node('Request', 'success'))).toBe('#22c55e');
    expect(minimapNodeColor(node('Request', 'failed'))).toBe('#ef4444');
    expect(minimapNodeColor(node('Request', 'running'))).toBe('#3b82f6');
    expect(minimapNodeColor(node('Request', 'skipped'))).toBe('#9ca3af');
  });

  it('falls back to the node kind when idle or unknown', () => {
    const request = minimapNodeColor(node('Request', 'idle'));
    expect(request).toBe(minimapNodeColor(node('Request')));
    expect(request).not.toBe(minimapNodeColor(node('Output')));
  });

  it('gives every kind a plain hex colour', () => {
    for (const kind of [
      'Request',
      'Input',
      'Output',
      'If',
      'Switch',
      'Transform',
      'WaitForCallback',
      'Auth',
      'SomethingNew',
    ]) {
      expect(minimapNodeColor(node(kind))).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});

describe('minimap styling', () => {
  // WebKitGTK hangs while painting these on this machine, so they must never appear.
  it.each(['minimap.ts', 'FlowCanvas.tsx'])('%s avoids backdrop-filter and color-mix', (file) => {
    const source = readFileSync(resolve(__dirname, '..', file), 'utf8');
    expect(source).not.toMatch(/backdrop-filter|backdrop-blur|backdrop-|color-mix/);
  });
});
```

Create `src/components/flow/__tests__/FlowCanvas.minimap.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
];

describe('FlowCanvas minimap', () => {
  it('renders a labelled minimap beside the controls', () => {
    render(
      <FlowCanvas
        nodes={nodes}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    const minimap = document.querySelector('.react-flow__minimap');
    expect(minimap).toBeInTheDocument();
    expect(screen.getByLabelText('Flow minimap')).toBeInTheDocument();
    expect(document.querySelector('.react-flow__controls')).toBeInTheDocument();
    const style = minimap?.getAttribute('style') ?? '';
    expect(style).not.toMatch(/backdrop|color-mix/);
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/minimap.test.ts src/components/flow/__tests__/FlowCanvas.minimap.test.tsx`
Expected: FAIL (cannot resolve `../minimap`; no minimap element).

- [ ] **Step 3: Write the colour function**

Create `src/components/flow/minimap.ts`:

```ts
import type { Node } from '@xyflow/react';

// Plain hex colours only. Keep CSS mixing and blur effects out of the minimap.
const STATUS_COLOURS: Record<string, string> = {
  running: '#3b82f6',
  success: '#22c55e',
  failed: '#ef4444',
  skipped: '#9ca3af',
};

const KIND_COLOURS: Record<string, string> = {
  Request: '#6366f1',
  Input: '#14b8a6',
  Output: '#f59e0b',
  If: '#a855f7',
  Switch: '#a855f7',
  Transform: '#0ea5e9',
  WaitForCallback: '#ec4899',
  Auth: '#64748b',
};

const FALLBACK_COLOUR = '#94a3b8';

// A node shows its run status when it has one, and its kind otherwise.
export function minimapNodeColor(node: Node): string {
  const status = (node.data as { status?: string } | undefined)?.status;
  if (status && STATUS_COLOURS[status]) return STATUS_COLOURS[status];
  return KIND_COLOURS[node.type ?? ''] ?? FALLBACK_COLOUR;
}
```

- [ ] **Step 4: Add the minimap to `FlowCanvas.tsx`**

4a. In the `@xyflow/react` import list add `MiniMap,` (keep the list alphabetical: after `type EdgeChange,` and before `type Node,` it sorts as `MiniMap`; Biome will tell you the exact place). Add the import:

```tsx
import { minimapNodeColor } from './minimap';
```

next to the other `./` imports (after `./flowExits`, before `./nodes/AuthNode`; let Biome sort).

4b. Directly after `<Controls />` add:

```tsx
          {/* Plain colours only. WebKitGTK hangs on some CSS paint effects. */}
          <MiniMap
            pannable
            zoomable
            ariaLabel='Flow minimap'
            position='bottom-right'
            nodeColor={minimapNodeColor}
            nodeStrokeWidth={2}
            bgColor='hsl(var(--card))'
            maskColor='hsl(var(--muted-foreground) / 0.25)'
            // Lifts the minimap clear of the React Flow attribution link.
            style={{ marginBottom: 28 }}
          />
```

4c. Give the bottom-left hint a width limit so it never runs under the minimap. Change its `Panel` to:

```tsx
          <Panel position='bottom-left' className='pointer-events-none ml-14 mb-3 max-w-[50%]'>
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `yarn test src/components/flow/__tests__/minimap.test.ts src/components/flow/__tests__/FlowCanvas.minimap.test.tsx src/components/flow/__tests__/FlowCanvas.test.tsx`
Expected: PASS. If React Flow's minimap fails to render in jsdom because the viewport has zero size, keep the `.react-flow__minimap` assertion and drop only the `getByLabelText` line, and say so in the commit body.

- [ ] **Step 6: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/minimap.ts src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/minimap.test.ts src/components/flow/__tests__/FlowCanvas.minimap.test.tsx`
Suggested subject: `feat(flow): add a minimap to the flow canvas`.

---

### Task 2: Node search

**Files:**
- Create: `src/lib/flow-search.ts`
- Create: `src/lib/__tests__/flow-search.test.ts`
- Create: `src/components/flow/FlowSearchBar.tsx`
- Create: `src/components/flow/__tests__/FlowSearchBar.test.tsx`
- Create: `src/components/flow/__tests__/FlowCanvas.search.test.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx` (imports, `useReactFlow` line, state, handlers, `handleKeyDown`, the new top-centre `Panel`, `CANVAS_HINTS`)

**Interfaces:**
- Produces:

```ts
// src/lib/flow-search.ts
export function searchFlowNodes(nodes: FlowNode[], query: string): string[];

// src/components/flow/FlowSearchBar.tsx
interface FlowSearchBarProps {
  nodes: FlowNode[];
  // Changes whenever the bar is asked to take focus again.
  focusToken: number;
  onShowMatch: (nodeId: string) => void;
  onClose: () => void;
}
```

- [ ] **Step 1: Write the failing search tests**

Create `src/lib/__tests__/flow-search.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { searchFlowNodes } from '../flow-search';

const at = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const nodes: FlowNode[] = [
  at('n1', {
    kind: 'Request',
    label: 'Fetch Users',
    source: { type: 'Saved', requestPath: 'users/list.yml' },
  }),
  at('n2', {
    kind: 'Request',
    label: 'Ping',
    source: {
      type: 'Inline',
      request: { method: 'GET', url: 'https://api.example.test/health', headers: [] },
    },
  }),
  at('n3', {
    kind: 'Switch',
    label: 'Route',
    value: '{{status}}',
    cases: [{ id: 'c1', label: 'Ok', matches: '200' }],
  }),
  at('n4', { kind: 'Output', label: 'Result' }),
  at('n5', { kind: 'Input', label: 'Token', value: 'secret-value' }),
  at('n6', { kind: 'WaitForCallback', label: 'Wait', name: 'pay', timeoutMs: 1000 }),
];

describe('searchFlowNodes', () => {
  it('returns nothing for an empty or blank query', () => {
    expect(searchFlowNodes(nodes, '')).toEqual([]);
    expect(searchFlowNodes(nodes, '   ')).toEqual([]);
  });

  it('matches the label without regard to case', () => {
    expect(searchFlowNodes(nodes, 'fetch')).toEqual(['n1']);
    expect(searchFlowNodes(nodes, 'FETCH USERS')).toEqual(['n1']);
  });

  it('matches the kind name', () => {
    expect(searchFlowNodes(nodes, 'switch')).toEqual(['n3']);
    expect(searchFlowNodes(nodes, 'waitforcallback')).toEqual(['n6']);
  });

  it('matches a saved request path', () => {
    expect(searchFlowNodes(nodes, 'users/list')).toEqual(['n1']);
  });

  it('matches an inline request url', () => {
    expect(searchFlowNodes(nodes, 'example.test/health')).toEqual(['n2']);
  });

  it('matches a Switch value', () => {
    expect(searchFlowNodes(nodes, '{{status}}')).toEqual(['n3']);
  });

  it('lists a node once, in node order, even when several fields match', () => {
    // "request" is the kind of n1 and n2. "users" is in the label and the path of n1.
    expect(searchFlowNodes(nodes, 'request')).toEqual(['n1', 'n2']);
    expect(searchFlowNodes(nodes, 'users')).toEqual(['n1']);
  });

  it('does not search an Input value', () => {
    expect(searchFlowNodes(nodes, 'secret-value')).toEqual([]);
  });

  it('returns nothing when no node matches', () => {
    expect(searchFlowNodes(nodes, 'zzz')).toEqual([]);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-search.test.ts`
Expected: FAIL, cannot resolve `../flow-search`.

- [ ] **Step 3: Write the search function**

Create `src/lib/flow-search.ts`:

```ts
import type { FlowNode } from '@/lib/tauri-api';

// The text a user can search in one node: label, kind, request target and Switch value.
// An Input value is left out on purpose, since it can be a secret.
function searchableText(node: FlowNode): string[] {
  const kind = node.kind;
  const fields = [kind.label, kind.kind];
  if (kind.kind === 'Request') {
    fields.push(
      kind.source.type === 'Saved' ? kind.source.requestPath : kind.source.request.url,
    );
  } else if (kind.kind === 'Switch') {
    fields.push(kind.value);
  }
  return fields;
}

/** Ids of the nodes that match the query, in node order. A blank query matches nothing. */
export function searchFlowNodes(nodes: FlowNode[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  return nodes
    .filter((node) => searchableText(node).some((text) => text.toLowerCase().includes(needle)))
    .map((node) => node.id);
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `yarn test src/lib/__tests__/flow-search.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing search bar tests**

Create `src/components/flow/__tests__/FlowSearchBar.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowSearchBar } from '../FlowSearchBar';

const out = (id: string, label: string): FlowNode => ({
  id,
  kind: { kind: 'Output', label },
  position: { x: 0, y: 0 },
});

const nodes = [out('a', 'Alpha'), out('b', 'Beta'), out('c', 'Gamma')];

function setup(extra: Partial<React.ComponentProps<typeof FlowSearchBar>> = {}) {
  const onShowMatch = vi.fn();
  const onClose = vi.fn();
  const user = userEvent.setup();
  const view = render(
    <FlowSearchBar
      nodes={nodes}
      focusToken={0}
      onShowMatch={onShowMatch}
      onClose={onClose}
      {...extra}
    />,
  );
  const input = () => screen.getByRole('textbox', { name: 'Search nodes' });
  return { user, onShowMatch, onClose, input, view };
}

describe('FlowSearchBar', () => {
  it('takes focus when it opens', () => {
    const { input } = setup();
    expect(input()).toHaveFocus();
  });

  it('shows the first match as you type and counts the matches', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
    expect(screen.getByRole('status')).toHaveTextContent('1 of 3');
  });

  it('moves to the next match on Enter and wraps around', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Enter}');
    expect(onShowMatch).toHaveBeenLastCalledWith('b');
    expect(screen.getByRole('status')).toHaveTextContent('2 of 3');
    await user.keyboard('{Enter}{Enter}');
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
    expect(screen.getByRole('status')).toHaveTextContent('1 of 3');
  });

  it('moves to the previous match on Shift+Enter and wraps to the last', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Shift>}{Enter}{/Shift}');
    expect(onShowMatch).toHaveBeenLastCalledWith('c');
    expect(screen.getByRole('status')).toHaveTextContent('3 of 3');
  });

  it('shows 0 of 0 and selects nothing when no node matches', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'zzz');
    expect(screen.getByRole('status')).toHaveTextContent('0 of 0');
    await user.keyboard('{Enter}');
    expect(onShowMatch).not.toHaveBeenCalled();
  });

  it('shows no counter for an empty query', () => {
    setup();
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
  });

  it('closes on Escape and with the close button', async () => {
    const { user, input, onClose } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('steps with the previous and next buttons', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.click(screen.getByRole('button', { name: 'Next match' }));
    expect(onShowMatch).toHaveBeenLastCalledWith('b');
    await user.click(screen.getByRole('button', { name: 'Previous match' }));
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
  });

  it('takes focus again when the focus token changes', async () => {
    const { user, input, view, onShowMatch, onClose } = setup();
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(input()).not.toHaveFocus();
    view.rerender(
      <FlowSearchBar nodes={nodes} focusToken={1} onShowMatch={onShowMatch} onClose={onClose} />,
    );
    expect(input()).toHaveFocus();
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowSearchBar.test.tsx`
Expected: FAIL, cannot resolve `../FlowSearchBar`.

- [ ] **Step 7: Write the search bar**

Create `src/components/flow/FlowSearchBar.tsx`:

```tsx
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { searchFlowNodes } from '@/lib/flow-search';
import type { FlowNode } from '@/lib/tauri-api';

interface FlowSearchBarProps {
  nodes: FlowNode[];
  // Changes whenever the bar is asked to take focus again, for example by a second Ctrl+F.
  focusToken: number;
  // Called with the node to select and show.
  onShowMatch: (nodeId: string) => void;
  onClose: () => void;
}

// The search field of a flow canvas. The owner selects and zooms to a match.
// The classes keep a click or key press here from panning, dragging or deleting on the canvas.
export function FlowSearchBar({ nodes, focusToken, onShowMatch, onClose }: FlowSearchBarProps) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const matches = useMemo(() => searchFlowNodes(nodes, query), [nodes, query]);
  // The graph can change while the bar is open, so keep the index inside the matches.
  const current = Math.min(index, Math.max(matches.length - 1, 0));

  // biome-ignore lint/correctness/useExhaustiveDependencies: the token is the trigger.
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusToken]);

  const show = (next: number) => {
    if (matches.length === 0) return;
    const wrapped = (next + matches.length) % matches.length;
    setIndex(wrapped);
    onShowMatch(matches[wrapped]);
  };

  const handleChange = (value: string) => {
    setQuery(value);
    setIndex(0);
    const first = searchFlowNodes(nodes, value)[0];
    if (first) onShowMatch(first);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      show(current + (e.shiftKey ? -1 : 1));
    }
  };

  const position = matches.length === 0 ? 0 : current + 1;
  const counter = query.trim() === '' ? '' : `${position} of ${matches.length}`;

  return (
    <div className='nokey nodrag nopan nowheel flex items-center gap-1 rounded-md border bg-card p-1 shadow-sm'>
      <Input
        ref={inputRef}
        aria-label='Search nodes'
        placeholder='Search nodes'
        className='h-7 w-44 text-xs'
        value={query}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
      />
      <span
        role='status'
        aria-live='polite'
        className='min-w-10 text-center text-[11px] text-muted-foreground'
      >
        {counter}
      </span>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Previous match'
        disabled={matches.length === 0}
        onClick={() => show(current - 1)}
      >
        <ChevronUp className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Next match'
        disabled={matches.length === 0}
        onClick={() => show(current + 1)}
      >
        <ChevronDown className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Close search'
        onClick={onClose}
      >
        <X className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
    </div>
  );
}
```

Check that `src/components/ui/input.tsx` forwards `ref` (React 19 passes `ref` as a prop to function components, and shadcn's `Input` spreads `...props`). If the test "takes focus when it opens" fails with `ref` not attached, wrap with `React.forwardRef` in `input.tsx` only if the file does not already pass props through; do not add a raw `<input>`.

- [ ] **Step 8: Run to verify the bar tests pass**

Run: `yarn test src/components/flow/__tests__/FlowSearchBar.test.tsx`
Expected: PASS.

- [ ] **Step 9: Write the failing canvas search tests**

Create `src/components/flow/__tests__/FlowCanvas.search.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const { fitView } = vi.hoisted(() => ({ fitView: vi.fn() }));

// Keep the real React Flow, but record the calls to fitView.
vi.mock('@xyflow/react', async () => {
  const actual = await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react');
  return { ...actual, useReactFlow: () => ({ ...actual.useReactFlow(), fitView }) };
});

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
  { id: 'c', kind: { kind: 'Output', label: 'Gamma' }, position: { x: 600, y: 0 } },
];

function Harness({ onSelect }: { onSelect: (ids: ReadonlySet<string>) => void }) {
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
  return (
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      selectedNodeIds={selected}
      onSelectedNodeIdsChange={(ids) => {
        onSelect(ids);
        setSelected(ids);
      }}
    />
  );
}

const searchBox = () => screen.queryByRole('textbox', { name: 'Search nodes' });

describe('FlowCanvas search', () => {
  beforeEach(() => {
    fitView.mockClear();
  });

  it('opens on Ctrl+F with the field focused and the browser default stopped', () => {
    render(<Harness onSelect={vi.fn()} />);
    expect(searchBox()).not.toBeInTheDocument();
    const notPrevented = fireEvent.keyDown(screen.getByTestId('flow-canvas'), {
      key: 'f',
      ctrlKey: true,
    });
    expect(notPrevented).toBe(false);
    expect(searchBox()).toHaveFocus();
  });

  it('opens on Cmd+F and from the Search button', async () => {
    const user = userEvent.setup();
    render(<Harness onSelect={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', metaKey: true });
    expect(searchBox()).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(searchBox()).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Search nodes' }));
    expect(searchBox()).toHaveFocus();
  });

  it('selects the match and zooms to it, then cycles with Enter', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'a');
    expect(onSelect).toHaveBeenLastCalledWith(new Set(['a']));
    expect(fitView).toHaveBeenLastCalledWith({
      nodes: [{ id: 'a' }],
      duration: 300,
      maxZoom: 1.2,
    });
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenLastCalledWith(new Set(['b']));
    expect(fitView).toHaveBeenLastCalledWith({
      nodes: [{ id: 'b' }],
      duration: 300,
      maxZoom: 1.2,
    });
  });

  it('selects nothing and does not zoom when there is no match', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'zzz');
    expect(screen.getByRole('status')).toHaveTextContent('0 of 0');
    expect(onSelect).not.toHaveBeenCalled();
    expect(fitView).not.toHaveBeenCalled();
  });

  it('closes on Escape and returns focus to the canvas', async () => {
    const user = userEvent.setup();
    render(<Harness onSelect={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.keyboard('{Escape}');
    expect(searchBox()).not.toBeInTheDocument();
    expect(screen.getByTestId('flow-canvas')).toHaveFocus();
  });

  it('leaves Ctrl+F to a field inside a node', () => {
    render(<Harness onSelect={vi.fn()} />);
    const input = document.createElement('input');
    screen.getByTestId('flow-canvas').appendChild(input);
    fireEvent.keyDown(input, { key: 'f', ctrlKey: true });
    expect(searchBox()).not.toBeInTheDocument();
  });

  it('does not delete a node when Backspace is pressed in the search field', async () => {
    const user = userEvent.setup();
    const onNodes = vi.fn();
    render(
      <FlowCanvas
        nodes={nodes}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={onNodes}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
        selectedNodeIds={new Set(['a'])}
        onSelectedNodeIdsChange={vi.fn()}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'ab{Backspace}');
    expect(onNodes).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 10: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowCanvas.search.test.tsx`
Expected: FAIL (no search box).

- [ ] **Step 11: Wire the search into `FlowCanvas.tsx`**

11a. Imports. Add `Search` to lucide: `import { Search } from 'lucide-react';` (new import line), the `Button`: `import { Button } from '@/components/ui/button';`, and `import { FlowSearchBar } from './FlowSearchBar';` (let Biome sort).

11b. Change `const { screenToFlowPosition } = useReactFlow();` to:

```tsx
  const { screenToFlowPosition, fitView } = useReactFlow();
```

11c. After the `selectNodesRef` declaration add state and handlers:

```tsx
  const [searchOpen, setSearchOpen] = useState(false);
  // Bumped by Ctrl+F so an open search bar takes focus again.
  const [searchFocusToken, setSearchFocusToken] = useState(0);
  const openSearch = () => {
    setSearchOpen(true);
    setSearchFocusToken((t) => t + 1);
  };
  const closeSearch = () => {
    setSearchOpen(false);
    focusPane();
  };
  // Selects the match and brings it into view. The properties panel stays as it is.
  const showSearchMatch = (nodeId: string) => {
    selectNodes(new Set([nodeId]));
    void fitView({ nodes: [{ id: nodeId }], duration: 300, maxZoom: 1.2 });
  };
```

`focusPane` is declared further down in the component body as a `const`; both `closeSearch` and `showSearchMatch` only run after render, so the order is fine. If the linter reports use-before-define, move these four statements below the `focusPane` declaration.

11d. In `handleKeyDown`, add this branch after the P4 `key === 'y'` branch and before the P5 `if (e.shiftKey) return;` line (if P5 is not merged yet, put it at the end of the function):

```tsx
    if (key === 'f') {
      // Replaces the webview's own find, which cannot see the canvas.
      e.preventDefault();
      openSearch();
      return;
    }
```

11e. Inside `<ReactFlow>`, directly after the `<MiniMap ... />` from Task 1, add:

```tsx
          <Panel position='top-center' className='nokey flex items-center gap-2'>
            <Button
              type='button'
              size='sm'
              variant='outline'
              className='h-8 gap-1.5'
              aria-label='Search nodes'
              title='Search nodes (Ctrl+F)'
              onClick={openSearch}
            >
              <Search className='h-3.5 w-3.5' aria-hidden='true' />
            </Button>
            {searchOpen && (
              <FlowSearchBar
                nodes={nodes}
                focusToken={searchFocusToken}
                onShowMatch={showSearchMatch}
                onClose={closeSearch}
              />
            )}
          </Panel>
```

11f. Append `'Ctrl+F search'` to `CANVAS_HINTS` (after the last entry, so the list ends `..., 'Ctrl+F search'`).

- [ ] **Step 12: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-search.test.ts`
Expected: PASS. Two tests share the accessible name "Search nodes" (the toolbar button and the text field); the tests above use `getByRole('button', ...)` and `getByRole('textbox', ...)` so they do not clash. If the existing tests that count buttons by name break, adjust only those tests.

- [ ] **Step 13: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib/__tests__/flow-search.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-search.ts src/lib/__tests__/flow-search.test.ts src/components/flow/FlowSearchBar.tsx src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/FlowSearchBar.test.tsx src/components/flow/__tests__/FlowCanvas.search.test.tsx`
Suggested subject: `feat(flow): search nodes with Ctrl+F`.

---

### Task 3: Tidy layout

**Files:**
- Modify: `package.json`, `yarn.lock` (dagre, with the human's OK)
- Create: `src/lib/flow-layout.ts`
- Create: `src/lib/__tests__/flow-layout.test.ts`
- Create: `src/components/flow/__tests__/FlowCanvas.tidy.test.tsx`
- Create: `src/components/flow/__tests__/FlowPane.tidy.test.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx` (imports, one handler, one button in the top-centre `Panel`)

**Interfaces:**
- Consumes: P4 `onNodesChange(nodes, options?)` semantics (a call without options is its own undo step); the top-centre `Panel` from Task 2.
- Produces:

```ts
// src/lib/flow-layout.ts
export const DEFAULT_NODE_SIZE: NodeSize; // 260 x 120
export const RANK_SEP = 80;
export const NODE_SEP = 40;
export interface NodeSize { width: number; height: number }
export function layoutFlow(
  nodes: FlowNode[],
  edges: FlowEdge[],
  sizes: ReadonlyMap<string, NodeSize>,
  only?: ReadonlySet<string>,
): FlowNode[];
```

`layoutFlow` returns the same `nodes` array when no position changes, and a new array with new node objects only for the nodes that moved otherwise.

- [ ] **Step 1: Ask the human, then add dagre**

Stop and ask the human: "Task 3 adds `@dagrejs/dagre` (about 30 to 40 KB), which changes `package.json` and `yarn.lock` (decision D2). OK to add it, or use the hand-rolled fallback described at the top of this plan?" Only after a yes, run:

```bash
yarn add @dagrejs/dagre
```

Then read `node_modules/@dagrejs/dagre/index.d.ts` and confirm it exports `graphlib` and `layout`. The code below uses `import * as dagre from '@dagrejs/dagre'`. If the package only has a default export, change the import to `import dagre from '@dagrejs/dagre'` and nothing else.

- [ ] **Step 2: Write the failing layout tests**

Create `src/lib/__tests__/flow-layout.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { DEFAULT_NODE_SIZE, layoutFlow, NODE_SEP, type NodeSize, RANK_SEP } from '../flow-layout';

const out = (id: string, x = 0, y = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y },
});

const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'trigger',
  expression: 'response.body',
  ...over,
});

const noSizes = new Map<string, NodeSize>();

const rect = (n: FlowNode, sizes: ReadonlyMap<string, NodeSize>) => {
  const s = sizes.get(n.id) ?? DEFAULT_NODE_SIZE;
  return { x: n.position.x, y: n.position.y, w: s.width, h: s.height };
};

function overlaps(nodes: FlowNode[], sizes: ReadonlyMap<string, NodeSize>) {
  for (let i = 0; i < nodes.length; i += 1) {
    for (let j = i + 1; j < nodes.length; j += 1) {
      const a = rect(nodes[i], sizes);
      const b = rect(nodes[j], sizes);
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) return true;
    }
  }
  return false;
}

describe('layoutFlow', () => {
  it('lays a chain out left to right with the rank gap between nodes', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'b', 'c')];
    const laid = layoutFlow(nodes, edges, noSizes);
    const x = (id: string) => laid.find((n) => n.id === id)?.position.x ?? Number.NaN;
    expect(x('b') - x('a')).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
    expect(x('c') - x('b')).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
  });

  it('separates nodes that start on top of each other', () => {
    const nodes = [out('a'), out('b'), out('c'), out('d')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c'), wire('e3', 'a', 'd')];
    const laid = layoutFlow(nodes, edges, noSizes);
    expect(overlaps(laid, noSizes)).toBe(false);
    const column = laid.filter((n) => n.id !== 'a').map((n) => n.position.y);
    const sorted = [...column].sort((p, q) => p - q);
    expect(sorted[1] - sorted[0]).toBeGreaterThanOrEqual(DEFAULT_NODE_SIZE.height + NODE_SEP);
  });

  it('uses the measured sizes', () => {
    const sizes = new Map<string, NodeSize>([
      ['a', { width: 400, height: 300 }],
      ['b', { width: 100, height: 50 }],
    ]);
    const laid = layoutFlow([out('a'), out('b')], [wire('e1', 'a', 'b')], sizes);
    expect(laid[1].position.x - laid[0].position.x).toBe(400 + RANK_SEP);
    expect(overlaps(laid, sizes)).toBe(false);
  });

  it('keeps the top-left corner of the graph where it was', () => {
    const nodes = [out('a', 500, 700), out('b', 900, 700)];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'b')], noSizes);
    expect(Math.min(...laid.map((n) => n.position.x))).toBe(500);
    expect(Math.min(...laid.map((n) => n.position.y))).toBe(700);
  });

  it('is deterministic', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    expect(layoutFlow(nodes, edges, noSizes)).toEqual(layoutFlow(nodes, edges, noSizes));
  });

  it('returns the same array when the graph is already tidy', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    const once = layoutFlow(nodes, edges, noSizes);
    expect(once).not.toBe(nodes);
    expect(layoutFlow(once, edges, noSizes)).toBe(once);
  });

  it('returns the input for an empty graph', () => {
    const nodes: FlowNode[] = [];
    expect(layoutFlow(nodes, [], noSizes)).toBe(nodes);
  });

  it('does not throw on a cycle or a self loop, and returns finite positions', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'b', 'a'), wire('e3', 'c', 'c')];
    const laid = layoutFlow(nodes, edges, noSizes);
    for (const n of laid) {
      expect(Number.isFinite(n.position.x)).toBe(true);
      expect(Number.isFinite(n.position.y)).toBe(true);
    }
  });

  it('ignores wires that point at missing nodes', () => {
    const nodes = [out('a'), out('b')];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'ghost'), wire('e2', 'a', 'b')], noSizes);
    expect(laid).toHaveLength(2);
    expect(overlaps(laid, noSizes)).toBe(false);
  });

  it('lays out only the selection and leaves the others as they are', () => {
    const a = out('a', 100, 100);
    const b = out('b', 100, 100);
    const far = out('far', 5000, 5000);
    const laid = layoutFlow([a, b, far], [wire('e1', 'a', 'b')], noSizes, new Set(['a', 'b']));
    expect(laid.find((n) => n.id === 'far')).toBe(far);
    const [la, lb] = laid;
    expect(la.position).toEqual({ x: 100, y: 100 });
    expect(lb.position.x - la.position.x).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
  });

  it('treats an empty selection as the whole graph', () => {
    const nodes = [out('a'), out('b')];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'b')], noSizes, new Set());
    expect(laid[1].position.x).toBeGreaterThan(laid[0].position.x);
  });

  it('keeps the exits of a Switch in case order', () => {
    const sw: FlowNode = {
      id: 'sw',
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [
          { id: 'c1', label: 'One', matches: '1' },
          { id: 'c2', label: 'Two', matches: '2' },
          { id: 'c3', label: 'Three', matches: '3' },
        ],
      },
      position: { x: 0, y: 0 },
    };
    const nodes = [sw, out('o1'), out('o2'), out('o3')];
    // The wires are listed in a different order from the cases on purpose.
    const edges = [
      wire('e3', 'sw', 'o3', { sourceHandle: caseHandle('c3') }),
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1') }),
      wire('e2', 'sw', 'o2', { sourceHandle: caseHandle('c2') }),
    ];
    const laid = layoutFlow(nodes, edges, noSizes);
    const y = (id: string) => laid.find((n) => n.id === id)?.position.y ?? Number.NaN;
    expect(y('o1')).toBeLessThan(y('o2'));
    expect(y('o2')).toBeLessThan(y('o3'));
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-layout.test.ts`
Expected: FAIL, cannot resolve `../flow-layout`.

- [ ] **Step 4: Write the layout**

Create `src/lib/flow-layout.ts`:

```ts
import * as dagre from '@dagrejs/dagre';
import { caseIdFromHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

export interface NodeSize {
  width: number;
  height: number;
}

// Used for a node React Flow has not measured yet.
export const DEFAULT_NODE_SIZE: NodeSize = { width: 260, height: 120 };
// Gap between columns and between nodes in a column.
export const RANK_SEP = 80;
export const NODE_SEP = 40;

// Position of a wire's exit on its source: a Switch case index, true before false, else 0.
// Sorting wires by it asks dagre to keep the exits in the order the node shows them.
function exitIndex(edge: FlowEdge, source: FlowNode | undefined): number {
  const handle = edge.sourceHandle;
  if (!handle || !source) return 0;
  const kind = source.kind;
  if (kind.kind === 'Switch') {
    const caseId = caseIdFromHandle(handle);
    if (caseId === null) return kind.cases.length;
    const i = kind.cases.findIndex((c) => c.id === caseId);
    return i === -1 ? kind.cases.length : i;
  }
  if (kind.kind === 'If') return handle === 'true' ? 0 : 1;
  return 0;
}

/**
 * Lays the nodes out left to right and returns them with new positions. With a
 * non-empty `only`, just those nodes move. The top-left corner of what is laid
 * out stays where it was, so nothing jumps across the canvas. The same `nodes`
 * array comes back when no position changes.
 */
export function layoutFlow(
  nodes: FlowNode[],
  edges: FlowEdge[],
  sizes: ReadonlyMap<string, NodeSize>,
  only?: ReadonlySet<string>,
): FlowNode[] {
  const target = nodes.filter((n) => !only || only.size === 0 || only.has(n.id));
  if (target.length === 0) return nodes;
  const ids = new Set(target.map((n) => n.id));
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const order = new Map(target.map((n, i) => [n.id, i]));
  const sizeOf = (id: string) => sizes.get(id) ?? DEFAULT_NODE_SIZE;

  const graph = new dagre.graphlib.Graph();
  graph.setGraph({ rankdir: 'LR', ranksep: RANK_SEP, nodesep: NODE_SEP });
  graph.setDefaultEdgeLabel(() => ({}));
  for (const n of target) {
    const { width, height } = sizeOf(n.id);
    graph.setNode(n.id, { width, height });
  }
  const usable = edges
    .filter(
      (e) => ids.has(e.sourceNodeId) && ids.has(e.targetNodeId) && e.sourceNodeId !== e.targetNodeId,
    )
    .sort(
      (a, b) =>
        (order.get(a.sourceNodeId) ?? 0) - (order.get(b.sourceNodeId) ?? 0) ||
        exitIndex(a, byId.get(a.sourceNodeId)) - exitIndex(b, byId.get(b.sourceNodeId)),
    );
  for (const e of usable) graph.setEdge(e.sourceNodeId, e.targetNodeId);
  dagre.layout(graph);

  // Dagre reports node centres. Convert to top-left corners.
  const corners = new Map<string, { x: number; y: number }>();
  for (const n of target) {
    const placed = graph.node(n.id);
    const { width, height } = sizeOf(n.id);
    corners.set(n.id, { x: placed.x - width / 2, y: placed.y - height / 2 });
  }
  const laidMinX = Math.min(...[...corners.values()].map((c) => c.x));
  const laidMinY = Math.min(...[...corners.values()].map((c) => c.y));
  const oldMinX = Math.min(...target.map((n) => n.position.x));
  const oldMinY = Math.min(...target.map((n) => n.position.y));

  let changed = false;
  const next = nodes.map((n) => {
    const corner = corners.get(n.id);
    if (!corner) return n;
    const x = Math.round(corner.x - laidMinX + oldMinX);
    const y = Math.round(corner.y - laidMinY + oldMinY);
    if (x === n.position.x && y === n.position.y) return n;
    changed = true;
    return { ...n, position: { x, y } };
  });
  return changed ? next : nodes;
}
```

- [ ] **Step 5: Run to verify the layout tests pass**

Run: `yarn test src/lib/__tests__/flow-layout.test.ts`
Expected: PASS. If the Switch order test fails, dagre's heuristics did not keep the insertion order for that graph; keep the sort in `layoutFlow` and relax the test to the first and last exit (`o1` above `o3`), and say so in the commit body.

- [ ] **Step 6: Write the failing canvas tests**

Create `src/components/flow/__tests__/FlowCanvas.tidy.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const { fitView } = vi.hoisted(() => ({ fitView: vi.fn() }));

// Keep the real React Flow, but record the calls to fitView.
vi.mock('@xyflow/react', async () => {
  const actual = await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react');
  return { ...actual, useReactFlow: () => ({ ...actual.useReactFlow(), fitView }) };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const out = (id: string, x = 0, y = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y },
});

const edge = (id: string, from: string, to: string): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'trigger',
  expression: 'response.body',
});

function renderCanvas(
  nodes: FlowNode[],
  edges: FlowEdge[],
  selected: string[] = [],
  onNodesChange = vi.fn(),
) {
  render(
    <FlowCanvas
      nodes={nodes}
      edges={edges}
      nodeStatus={{}}
      onNodesChange={onNodesChange}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      selectedNodeIds={new Set(selected)}
      onSelectedNodeIdsChange={vi.fn()}
    />,
  );
  return onNodesChange;
}

const tidyButton = () => screen.getByRole('button', { name: 'Tidy layout' });

describe('FlowCanvas tidy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('writes the new positions with exactly one nodes call and no options', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    expect(onNodes).toHaveBeenCalledTimes(1);
    const [next, options] = onNodes.mock.calls[0];
    expect(options).toBeUndefined();
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('then fits the view', async () => {
    const user = userEvent.setup();
    renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    await vi.waitFor(() => expect(fitView).toHaveBeenCalled());
    expect(fitView.mock.calls[0][0]).toMatchObject({ duration: 300 });
  });

  it('writes nothing and says so when the graph is already tidy', async () => {
    const user = userEvent.setup();
    const tidy = [out('a', 0, 0), out('b', 340, 0)];
    const onNodes = renderCanvas(tidy, [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    expect(onNodes).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith('The layout is already tidy.');
  });

  it('lays out only a selection of two or more nodes', async () => {
    const user = userEvent.setup();
    const far = out('far', 5000, 5000);
    const onNodes = renderCanvas(
      [out('a', 100, 100), out('b', 100, 100), far],
      [edge('e1', 'a', 'b')],
      ['a', 'b'],
    );
    await user.click(tidyButton());
    const [next] = onNodes.mock.calls[0];
    expect(next[2]).toBe(far);
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('lays out the whole graph when only one node is selected', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')], ['a']);
    await user.click(tidyButton());
    const [next] = onNodes.mock.calls[0];
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('is disabled for an empty flow', () => {
    renderCanvas([], []);
    expect(tidyButton()).toBeDisabled();
  });

  it('does not throw on a cycle', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b'), edge('e2', 'b', 'a')]);
    await user.click(tidyButton());
    expect(toast.error).not.toHaveBeenCalled();
    expect(onNodes).toHaveBeenCalledTimes(1);
  });
});
```

Create `src/components/flow/__tests__/FlowPane.tidy.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

// Park the resize handle away from the pointer, as the delete test does.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-tidy-1',
  tabType: 'flow',
  title: 'Flow: tidy',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'tidy',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'out2', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Other' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
    {
      id: 'e2',
      sourceNodeId: 'in1',
      targetNodeId: 'out2',
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

describe('FlowPane tidy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('moves the nodes apart in one undo step and keeps the graph otherwise', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    await waitFor(() => expect(getFlowTab().isDirty).toBe(true));
    const tidy = getFlowTab();
    expect(new Set(tidy.nodes.map((n) => `${n.position.x},${n.position.y}`)).size).toBe(3);
    expect(tidy.nodes.map((n) => n.id)).toEqual(['in1', 'out1', 'out2']);
    expect(tidy.edges).toBe(baseTab.edges);
    expect(tidy.history?.past).toHaveLength(1);

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(getFlowTab().nodes).toBe(baseTab.nodes);
    expect(getFlowTab().isDirty).toBe(false);
  });

  it('adds no undo step when pressed again on a tidy graph', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    await waitFor(() => expect(getFlowTab().history?.past).toHaveLength(1));
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    expect(getFlowTab().history?.past).toHaveLength(1);
  });
});
```

- [ ] **Step 7: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowCanvas.tidy.test.tsx src/components/flow/__tests__/FlowPane.tidy.test.tsx`
Expected: FAIL (no "Tidy layout" button).

- [ ] **Step 8: Wire Tidy into `FlowCanvas.tsx`**

8a. Imports. Add `LayoutGrid` to the lucide import from Task 2 (`import { LayoutGrid, Search } from 'lucide-react';`), and add:

```tsx
import { layoutFlow } from '@/lib/flow-layout';
```

(`toast` is already imported in this file.)

8b. Add the handler next to the search handlers from Task 2:

```tsx
  // Lays the graph out left to right in one write, which is one undo step. With
  // two or more nodes selected it moves only those. A failure leaves the graph alone.
  const handleTidy = () => {
    try {
      const only = selectedNodeIds.size >= 2 ? selectedNodeIds : undefined;
      const next = layoutFlow(nodes, edges, measuredRef.current, only);
      if (next === nodes) {
        toast.info('The layout is already tidy.');
        return;
      }
      onNodesChange(next);
      // Wait one tick, so React Flow has the new positions before it fits the view.
      setTimeout(() => {
        void fitView({
          ...(only ? { nodes: [...only].map((id) => ({ id })) } : {}),
          duration: 300,
          padding: 0.2,
        });
      }, 0);
    } catch (err) {
      toast.error(`Could not tidy the layout: ${String(err)}`);
    }
  };
```

8c. In the top-centre `Panel` from Task 2, add this button before the Search button:

```tsx
            <Button
              type='button'
              size='sm'
              variant='outline'
              className='h-8 gap-1.5'
              aria-label='Tidy layout'
              title='Tidy layout'
              disabled={nodes.length === 0}
              onClick={handleTidy}
            >
              <LayoutGrid className='h-3.5 w-3.5' aria-hidden='true' />
              Tidy
            </Button>
```

- [ ] **Step 9: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-layout.test.ts`
Expected: PASS. In the pane test jsdom measures no sizes, so `layoutFlow` uses the 260 by 120 fallback; that is the case the tests cover on purpose.

- [ ] **Step 10: Manual check for the human**

Run `yarn tauri dev`, open a flow with ten or more nodes, drag a few out of place, press Tidy. Expected: nodes line up in columns, left to right in wire order, nothing overlaps, the view zooms to fit. Press Ctrl+Z once: the old layout returns. Select three nodes, press Tidy: only those three move and stay near where they were. Open the minimap, click and drag in it: the view pans; scroll in it: the view zooms. Press Ctrl+F, type part of a request name: the node is selected and centred; Enter goes to the next match.

- [ ] **Step 11: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib/__tests__`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`package.json yarn.lock src/lib/flow-layout.ts src/lib/__tests__/flow-layout.test.ts src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/FlowCanvas.tidy.test.tsx src/components/flow/__tests__/FlowPane.tidy.test.tsx`
Suggested subject: `feat(flow): tidy the layout in one undo step`.

---

## Self-Review

- **Spec coverage:** F-42 minimap with pan, zoom, plain colours by status and kind, bottom-right clear of the properties panel (Task 1); search over label, kind, saved path, inline URL and Switch value, case-insensitive, Ctrl+F with `preventDefault`, Enter and Shift+Enter cycling, select and `fitView({ nodes, duration: 300, maxZoom: 1.2 })`, Esc closes and refocuses the canvas, "0 of 0" (Task 2); Tidy with dagre LR, `ranksep` 80, `nodesep` 40, sizes from `measuredRef` with 260 by 120 fallback, selection only when any, one write then `fitView`, try/catch with toast, Switch exit order, button in a canvas `Panel` and not in `FlowToolbar.tsx` (Task 3). Decision D2 is a stop step with the fallback paragraph.
- **Placeholders:** none. Every code step shows code. The two "if this fails" notes in Steps 5 and 7 give the exact fallback edit.
- **Type consistency:** `NodeSize` matches React Flow's `{ width, height }` entries in `measuredRef` (a `Map<string, Measured>` is assignable to `ReadonlyMap<string, NodeSize>`). `layoutFlow` has the same signature in the tests, the canvas and the file list. `FlowSearchBarProps` match the canvas usage. `fitView` options in the search tests equal the call in `showSearchMatch`.
- **Review Focus coverage:** item 1 Task 3 (`returns the same array when the graph is already tidy`, canvas "writes nothing", pane "adds no undo step", pane undo); item 2 Task 3 (cycle, empty graph disabled, empty array, missing nodes; the try/catch toast is the safety net); item 3 Task 3 (selection tests, corner anchor test); item 4 Task 2 (search, bar and canvas tests including the nested field); item 5 Task 1.
- **Differences from the design notes:** Tidy changes positions only, so it uses `onNodesChange(next)` once instead of `updateFlowGraph`; it is still exactly one store write and one undo step. The search bar and Tidy share a top-centre panel because the notes' top-left position is taken by the Add node button (`NodePalette.tsx`, `absolute left-3 top-3`). A selection of one node tidies the whole graph, since moving one node alone does nothing. The minimap uses `bgColor` and `maskColor` props with `hsl(var(--token))`, no `color-mix`.

Known follow-ups outside this plan: the minimap has no show or hide toggle; search does not look inside wire expressions or Input values (an Input value can be a secret); Tidy always runs left to right.
