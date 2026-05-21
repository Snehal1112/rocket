# Tests Tab Authoring Assistance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a resizable snippets sidebar and Monaco IntelliSense to the Tests sub-tab in ScriptsTab so users can discover and insert `rok`/`res`/`test()`/`expect` API calls without reading docs.

**Architecture:** Pure frontend change — three new/modified files. `rok-types.ts` holds all static data (snippet definitions + TypeScript `.d.ts` string). `ScriptSnippetSidebar` renders the resizable sidebar. `MonacoWrapper` gains a `phase` prop that controls which IntelliSense definitions are active. `ScriptsTab` wires them together for the Tests sub-tab only.

**Tech Stack:** React 18, TypeScript, Monaco Editor (`@monaco-editor/react`), shadcn/ui (`Collapsible`, `ScrollArea`), lucide-react icons.

---

## File Map

| Action | File | Responsibility |
|---|---|---|
| Create | `src/components/editor/rok-types.ts` | All snippet data + `.d.ts` strings for all three phases |
| Create | `src/components/request/ScriptSnippetSidebar.tsx` | Resizable sidebar UI with two collapsible sections |
| Modify | `src/components/editor/MonacoWrapper.tsx` | Add `phase` + `onEditorReady` props; register/dispose IntelliSense |
| Modify | `src/components/request/ScriptsTab.tsx` | Tests sub-tab: two-column layout wiring sidebar + editor |
| Create | `src/components/request/__tests__/ScriptSnippetSidebar.test.tsx` | Unit tests for sidebar |
| Create | `src/components/editor/__tests__/rok-types.test.ts` | Unit tests for snippet data shape |

---

## Task 1: Create `rok-types.ts` — snippet data and type definitions

**Files:**
- Create: `src/components/editor/rok-types.ts`
- Test: `src/components/editor/__tests__/rok-types.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/components/editor/__tests__/rok-types.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { ROK_SNIPPETS, ROK_TYPE_DEFS_FOR_PHASE } from '../rok-types';

describe('ROK_SNIPPETS', () => {
  it('has a common-tests group with at least 7 items', () => {
    const group = ROK_SNIPPETS.find((g) => g.id === 'common-tests');
    expect(group).toBeDefined();
    expect(group!.items.length).toBeGreaterThanOrEqual(7);
  });

  it('has an api-reference group with res, rok, expect sub-groups', () => {
    const group = ROK_SNIPPETS.find((g) => g.id === 'api-reference');
    expect(group).toBeDefined();
    const ids = group!.subGroups!.map((s) => s.id);
    expect(ids).toContain('res');
    expect(ids).toContain('rok');
    expect(ids).toContain('expect');
  });

  it('every snippet item has label, code, and kind', () => {
    for (const group of ROK_SNIPPETS) {
      for (const item of group.items ?? []) {
        expect(item.label).toBeTruthy();
        expect(item.code).toBeTruthy();
        expect(['template', 'expression']).toContain(item.kind);
      }
      for (const sub of group.subGroups ?? []) {
        for (const item of sub.items) {
          expect(item.label).toBeTruthy();
          expect(item.code).toBeTruthy();
          expect(['template', 'expression']).toContain(item.kind);
        }
      }
    }
  });
});

describe('ROK_TYPE_DEFS_FOR_PHASE', () => {
  it('returns a non-empty string for each phase', () => {
    expect(ROK_TYPE_DEFS_FOR_PHASE('pre-request').length).toBeGreaterThan(0);
    expect(ROK_TYPE_DEFS_FOR_PHASE('post-response').length).toBeGreaterThan(0);
    expect(ROK_TYPE_DEFS_FOR_PHASE('tests').length).toBeGreaterThan(0);
  });

  it('tests phase includes test() and expect declarations', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('tests');
    expect(defs).toContain('declare function test(');
    expect(defs).toContain('declare const expect');
  });

  it('pre-request phase includes req but not res or test', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('pre-request');
    expect(defs).toContain('declare const req');
    expect(defs).not.toContain('declare const res');
    expect(defs).not.toContain('declare function test(');
  });

  it('post-response phase includes res but not req or test', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('post-response');
    expect(defs).toContain('declare const res');
    expect(defs).not.toContain('declare const req');
    expect(defs).not.toContain('declare function test(');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
yarn test rok-types
```

Expected: FAIL — `Cannot find module '../rok-types'`

- [ ] **Step 3: Create `src/components/editor/rok-types.ts`**

```typescript
export type ScriptPhase = 'pre-request' | 'post-response' | 'tests';

export interface ScriptSnippetItem {
  label: string;
  code: string;
  kind: 'template' | 'expression';
}

export interface ScriptSnippetSubGroup {
  id: string;
  label: string;
  items: ScriptSnippetItem[];
}

export interface ScriptSnippetGroup {
  id: string;
  label: string;
  items?: ScriptSnippetItem[];
  subGroups?: ScriptSnippetSubGroup[];
}

export const ROK_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-tests',
    label: 'Common Tests',
    items: [
      {
        label: 'Status is 200',
        kind: 'template',
        code: `test("Status is 200", () => {\n  expect(res.getStatus()).to.equal(200);\n});`,
      },
      {
        label: 'Status is 2xx',
        kind: 'template',
        code: `test("Status is 2xx", () => {\n  expect(res.getStatus()).to.be.within(200, 299);\n});`,
      },
      {
        label: 'Response time < 200ms',
        kind: 'template',
        code: `test("Response time < 200ms", () => {\n  expect(res.getResponseTime()).to.be.below(200);\n});`,
      },
      {
        label: 'Body has property',
        kind: 'template',
        code: `test("Body has property", () => {\n  const body = res.getBody();\n  expect(body).to.have.property("key");\n});`,
      },
      {
        label: 'Body equals value',
        kind: 'template',
        code: `test("Body equals value", () => {\n  const body = res.getBody();\n  expect(body.key).to.equal("value");\n});`,
      },
      {
        label: 'Header exists',
        kind: 'template',
        code: `test("Header exists", () => {\n  expect(res.getHeader("content-type")).to.exist;\n});`,
      },
      {
        label: 'Status is 404',
        kind: 'template',
        code: `test("Status is 404", () => {\n  expect(res.getStatus()).to.equal(404);\n});`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'res',
        label: 'res.*',
        items: [
          { label: 'res.getStatus()', kind: 'expression', code: 'res.getStatus()' },
          { label: 'res.getStatusText()', kind: 'expression', code: 'res.getStatusText()' },
          { label: 'res.getHeader("name")', kind: 'expression', code: 'res.getHeader("name")' },
          { label: 'res.getHeaders()', kind: 'expression', code: 'res.getHeaders()' },
          { label: 'res.getBody()', kind: 'expression', code: 'res.getBody()' },
          { label: 'res.getBody({ raw: true })', kind: 'expression', code: 'res.getBody({ raw: true })' },
          { label: 'res.getResponseTime()', kind: 'expression', code: 'res.getResponseTime()' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          { label: 'rok.setVar("key", value)', kind: 'expression', code: 'rok.setVar("key", value)' },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          { label: 'rok.setEnvVar("key", value)', kind: 'expression', code: 'rok.setEnvVar("key", value)' },
          { label: 'rok.getCollectionVar("key")', kind: 'expression', code: 'rok.getCollectionVar("key")' },
          { label: 'rok.getEnvName()', kind: 'expression', code: 'rok.getEnvName()' },
          { label: 'rok.interpolate("{{template}}")', kind: 'expression', code: 'rok.interpolate("{{template}}")' },
        ],
      },
      {
        id: 'expect',
        label: 'expect',
        items: [
          { label: '.to.equal(value)', kind: 'expression', code: '.to.equal(value)' },
          { label: '.to.exist', kind: 'expression', code: '.to.exist' },
          { label: '.to.have.property("key")', kind: 'expression', code: '.to.have.property("key")' },
          { label: '.to.be.within(min, max)', kind: 'expression', code: '.to.be.within(min, max)' },
          { label: '.to.be.below(n)', kind: 'expression', code: '.to.be.below(n)' },
          { label: '.to.include("str")', kind: 'expression', code: '.to.include("str")' },
          { label: '.to.be.an("type")', kind: 'expression', code: '.to.be.an("type")' },
        ],
      },
    ],
  },
];

// Shared rok.* definitions available in all phases.
const ROK_DEFS = `
declare const rok: {
  /** Read a runtime variable set in a previous script. */
  getVar(key: string): unknown;
  /** Set a runtime variable (in-memory, cleared after request). */
  setVar(key: string, value: unknown): void;
  /** Read an environment variable. */
  getEnvVar(key: string): unknown;
  /** Write an environment variable. Pass { persist: true } to save to disk. */
  setEnvVar(key: string, value: unknown, opts?: { persist?: boolean }): void;
  /** Returns true if the environment variable exists. */
  hasEnvVar(key: string): boolean;
  /** Delete an environment variable. */
  deleteEnvVar(key: string): void;
  /** Returns the active environment name. */
  getEnvName(): string | undefined;
  /** Read a collection variable. */
  getCollectionVar(key: string): unknown;
  /** Write a collection variable (persisted to opencollection.yml). */
  setCollectionVar(key: string, value: unknown): void;
  /** Read a global environment variable. */
  getGlobalEnvVar(key: string): unknown;
  /** Write a global environment variable. */
  setGlobalEnvVar(key: string, value: unknown, opts?: { persist?: boolean }): void;
  /** Resolve {{var}} tokens using the current variable context. */
  interpolate(template: string): string;
  runner: {
    /** Jump to the named request in the runner, or pass null to stop. */
    setNextRequest(name: string | null): void;
    /** Skip this request in the runner. */
    skipRequest(): void;
  };
};
`;

// res.* definitions available in after-response and tests phases.
const RES_DEFS = `
declare const res: {
  /** Returns the HTTP status code (e.g. 200). */
  getStatus(): number;
  /** Returns the HTTP status text (e.g. "OK"). */
  getStatusText(): string;
  /** Returns the value of a response header (case-insensitive). */
  getHeader(name: string): string | undefined;
  /** Returns all response headers as a key-value record. */
  getHeaders(): Record<string, string>;
  /** Returns the parsed response body. Pass { raw: true } for the raw string. */
  getBody(opts?: { raw?: boolean }): unknown;
  /** Returns the total response time in milliseconds. */
  getResponseTime(): number;
};
`;

// req.* definitions available in the before-request phase only.
const REQ_DEFS = `
declare const req: {
  getUrl(): string;
  setUrl(url: string): void;
  getHost(): string;
  getPath(): string;
  getQueryString(): string;
  getMethod(): string;
  setMethod(method: string): void;
  getName(): string;
  getAuthMode(): string;
  getHeader(name: string): string | undefined;
  getHeaders(): Record<string, string>;
  setHeader(name: string, value: string): void;
  setHeaders(headers: Record<string, string>): void;
  deleteHeader(name: string): void;
  deleteHeaders(names: string[]): void;
  getBody(opts?: { raw?: boolean }): unknown;
  setBody(body: unknown): void;
  getTimeout(): number;
  setTimeout(ms: number): void;
  setMaxRedirects(n: number): void;
  getExecutionMode(): "runner" | "standalone";
  getExecutionPlatform(): "app";
};
`;

// test() and expect definitions for the tests phase only.
const TEST_DEFS = `
/** Register a named assertion block. Each block runs independently. */
declare function test(name: string, fn: () => void): void;

interface ChaiAssertion {
  to: ChaiAssertion;
  be: ChaiAssertion & {
    an(type: string): ChaiAssertion;
    within(min: number, max: number): ChaiAssertion;
    below(n: number): ChaiAssertion;
  };
  not: ChaiAssertion;
  exist: void;
  equal(value: unknown): void;
  include(str: string): void;
  have: {
    property(key: string, value?: unknown): ChaiAssertion;
  };
}

/** Chai expect — chain assertions with .to.equal(), .to.have.property(), etc. */
declare function expect(value: unknown): ChaiAssertion;
`;

/** Returns the Monaco extra-lib `.d.ts` string for the given script phase. */
export function ROK_TYPE_DEFS_FOR_PHASE(phase: ScriptPhase): string {
  switch (phase) {
    case 'pre-request':
      return ROK_DEFS + REQ_DEFS;
    case 'post-response':
      return ROK_DEFS + RES_DEFS;
    case 'tests':
      return ROK_DEFS + RES_DEFS + TEST_DEFS;
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test rok-types
```

Expected: PASS — 4 test suites, all green.

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/rok-types.ts src/components/editor/__tests__/rok-types.test.ts
git commit -m "feat(scripts): add rok-types snippet data and phase-scoped type definitions"
```

---

## Task 2: Create `ScriptSnippetSidebar` component

**Files:**
- Create: `src/components/request/ScriptSnippetSidebar.tsx`
- Test: `src/components/request/__tests__/ScriptSnippetSidebar.test.tsx`

- [ ] **Step 1: Write the failing tests**

Create `src/components/request/__tests__/ScriptSnippetSidebar.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ScriptSnippetSidebar } from '../ScriptSnippetSidebar';

describe('ScriptSnippetSidebar', () => {
  it('renders Common Tests and API Reference section headings', () => {
    render(<ScriptSnippetSidebar onInsert={vi.fn()} />);
    expect(screen.getByText('Common Tests')).toBeTruthy();
    expect(screen.getByText('API Reference')).toBeTruthy();
  });

  it('calls onInsert with snippet code when a common-tests item is clicked', () => {
    const onInsert = vi.fn();
    render(<ScriptSnippetSidebar onInsert={onInsert} />);
    fireEvent.click(screen.getByText('Status is 200'));
    expect(onInsert).toHaveBeenCalledWith(
      expect.stringContaining('expect(res.getStatus()).to.equal(200)'),
    );
  });

  it('renders res, rok, expect sub-group labels inside API Reference', () => {
    render(<ScriptSnippetSidebar onInsert={vi.fn()} />);
    expect(screen.getByText('res.*')).toBeTruthy();
    expect(screen.getByText('rok.*')).toBeTruthy();
    expect(screen.getByText('expect')).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
yarn test ScriptSnippetSidebar
```

Expected: FAIL — `Cannot find module '../ScriptSnippetSidebar'`

- [ ] **Step 3: Create `src/components/request/ScriptSnippetSidebar.tsx`**

```tsx
import { useState, useCallback, useRef } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { ROK_SNIPPETS } from '@/components/editor/rok-types';
import type { ScriptSnippetGroup, ScriptSnippetSubGroup, ScriptSnippetItem } from '@/components/editor/rok-types';

interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
}

const MIN_WIDTH = 120;
const MAX_WIDTH_FRACTION = 0.5;
const DEFAULT_WIDTH = 220;

function SnippetItem({ item, onInsert }: { item: ScriptSnippetItem; onInsert: (code: string) => void }) {
  return (
    <button
      type='button'
      onClick={() => onInsert(item.code)}
      className='w-full text-left px-3 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded-sm truncate'
      title={item.code}
    >
      {item.label}
    </button>
  );
}

function SubGroupSection({ sub, onInsert }: { sub: ScriptSnippetSubGroup; onInsert: (code: string) => void }) {
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex items-center gap-1 w-full px-3 py-1 text-xs font-medium text-foreground hover:bg-accent rounded-sm'>
        {open ? <ChevronDown className='h-3 w-3 shrink-0' /> : <ChevronRight className='h-3 w-3 shrink-0' />}
        {sub.label}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className='pl-2'>
          {sub.items.map((item) => (
            <SnippetItem key={item.label} item={item} onInsert={onInsert} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function GroupSection({ group, onInsert }: { group: ScriptSnippetGroup; onInsert: (code: string) => void }) {
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex items-center gap-1 w-full px-3 py-1.5 text-xs font-semibold text-foreground hover:bg-accent rounded-sm'>
        {open ? <ChevronDown className='h-3 w-3 shrink-0' /> : <ChevronRight className='h-3 w-3 shrink-0' />}
        {group.label}
      </CollapsibleTrigger>
      <CollapsibleContent>
        {group.items?.map((item) => (
          <SnippetItem key={item.label} item={item} onInsert={onInsert} />
        ))}
        {group.subGroups?.map((sub) => (
          <SubGroupSection key={sub.id} sub={sub} onInsert={onInsert} />
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function ScriptSnippetSidebar({ onInsert }: ScriptSnippetSidebarProps) {
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(DEFAULT_WIDTH);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true;
    startX.current = e.clientX;
    startWidth.current = width;

    const onMouseMove = (ev: MouseEvent) => {
      if (!dragging.current) return;
      const delta = startX.current - ev.clientX;
      const containerWidth = document.body.clientWidth;
      const maxWidth = containerWidth * MAX_WIDTH_FRACTION;
      const next = Math.min(maxWidth, Math.max(MIN_WIDTH, startWidth.current + delta));
      setWidth(next);
    };

    const onMouseUp = () => {
      dragging.current = false;
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  }, [width]);

  return (
    <div className='flex h-full shrink-0' style={{ width }}>
      {/* Drag handle. */}
      <div
        onMouseDown={onMouseDown}
        className='w-1 shrink-0 cursor-col-resize hover:bg-primary/40 transition-colors bg-border'
        role='separator'
        aria-orientation='vertical'
        aria-label='Resize sidebar'
      />
      <div className='flex flex-col flex-1 min-w-0 border-l'>
        <div className='px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide border-b shrink-0'>
          Snippets
        </div>
        <ScrollArea className='flex-1'>
          <div className='py-1'>
            {ROK_SNIPPETS.map((group) => (
              <GroupSection key={group.id} group={group} onInsert={onInsert} />
            ))}
          </div>
        </ScrollArea>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test ScriptSnippetSidebar
```

Expected: PASS — 3 tests green.

- [ ] **Step 5: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/ScriptSnippetSidebar.tsx src/components/request/__tests__/ScriptSnippetSidebar.test.tsx
git commit -m "feat(scripts): add ScriptSnippetSidebar with resizable drag handle"
```

---

## Task 3: Add `phase` and `onEditorReady` props to `MonacoWrapper`

**Files:**
- Modify: `src/components/editor/MonacoWrapper.tsx`

This task adds two new optional props:
- `phase?: ScriptPhase` — when set, registers the phase-appropriate IntelliSense extra-lib on mount and re-registers when it changes.
- `onEditorReady?: (editor: monacoNs.editor.IStandaloneCodeEditor) => void` — fires once the editor instance is ready so callers can call `executeEdits`.

- [ ] **Step 1: Open `src/components/editor/MonacoWrapper.tsx` and update the import block**

Add two imports at the top of the existing import section:

```typescript
import type { ScriptPhase } from './rok-types';
import { ROK_TYPE_DEFS_FOR_PHASE } from './rok-types';
```

- [ ] **Step 2: Extend `MonacoWrapperProps`**

Change the interface from:

```typescript
interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
  variableContext?: Map<string, VariableScopeEntry>;
}
```

to:

```typescript
interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
  variableContext?: Map<string, VariableScopeEntry>;
  phase?: ScriptPhase;
  onEditorReady?: (editor: monacoNs.editor.IStandaloneCodeEditor) => void;
}
```

- [ ] **Step 3: Destructure the new props in the function signature**

Change:

```typescript
export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  variableContext,
}: MonacoWrapperProps) {
```

to:

```typescript
export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  variableContext,
  phase,
  onEditorReady,
}: MonacoWrapperProps) {
```

- [ ] **Step 4: Add a ref for the extra-lib disposable, just below the existing `hoverDisposablesRef`**

After the line:

```typescript
const hoverDisposablesRef = useRef<monacoNs.IDisposable[]>([]);
```

Add:

```typescript
const extraLibDisposableRef = useRef<monacoNs.IDisposable | null>(null);
```

- [ ] **Step 5: Add a `useEffect` to register IntelliSense when `phase` changes**

Add this effect after the existing `useEffect(() => { ensureDecorationStyles(); }, []);` block:

```typescript
useEffect(() => {
  if (!phase) return;
  // Dynamically import monaco so this effect only runs client-side.
  import('monaco-editor').then((monaco) => {
    extraLibDisposableRef.current?.dispose();
    extraLibDisposableRef.current =
      monaco.languages.typescript.javascriptDefaults.addExtraLib(
        ROK_TYPE_DEFS_FOR_PHASE(phase),
        'ts:rok-global.d.ts',
      );
  });
  return () => {
    extraLibDisposableRef.current?.dispose();
    extraLibDisposableRef.current = null;
  };
}, [phase]);
```

- [ ] **Step 6: Call `onEditorReady` inside `handleMount`**

At the very end of the `handleMount` function, just before its closing brace, add:

```typescript
onEditorReady?.(editor);
```

- [ ] **Step 7: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/components/editor/MonacoWrapper.tsx
git commit -m "feat(scripts): add phase and onEditorReady props to MonacoWrapper"
```

---

## Task 4: Wire sidebar and IntelliSense into `ScriptsTab`

**Files:**
- Modify: `src/components/request/ScriptsTab.tsx`

The Tests sub-tab changes from a full-width editor to a two-column flex layout: editor on the left, `ScriptSnippetSidebar` on the right. An `editorRef` captures the Monaco instance via `onEditorReady` and is passed to the sidebar's `onInsert` handler.

- [ ] **Step 1: Update imports in `src/components/request/ScriptsTab.tsx`**

Change the existing import block from:

```typescript
import { lazy, Suspense } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
```

to:

```typescript
import { lazy, Suspense, useRef, useCallback } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScriptSnippetSidebar } from './ScriptSnippetSidebar';
import type * as monacoNs from 'monaco-editor';
```

- [ ] **Step 2: Add `editorRef` and `handleInsert` inside the component function**

Add these two declarations at the top of the `ScriptsTab` function body, before the `return`:

```typescript
const editorRef = useRef<monacoNs.editor.IStandaloneCodeEditor | null>(null);

const handleInsert = useCallback((code: string) => {
  const editor = editorRef.current;
  if (!editor) return;
  const position = editor.getPosition();
  const model = editor.getModel();
  if (!model || !position) {
    // No cursor — append at end with a leading newline.
    const lastLine = model ? model.getLineCount() : 1;
    const lastCol = model ? model.getLineMaxColumn(lastLine) : 1;
    editor.executeEdits('snippet-insert', [
      {
        range: { startLineNumber: lastLine, startColumn: lastCol, endLineNumber: lastLine, endColumn: lastCol },
        text: '\n' + code,
        forceMoveMarkers: true,
      },
    ]);
    return;
  }
  editor.executeEdits('snippet-insert', [
    {
      range: {
        startLineNumber: position.lineNumber,
        startColumn: position.column,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      },
      text: '\n' + code + '\n',
      forceMoveMarkers: true,
    },
  ]);
  editor.focus();
}, []);
```

- [ ] **Step 3: Replace the Tests `TabsContent` with the two-column layout**

Change:

```tsx
<TabsContent value='tests' className='flex-1 m-0 p-0'>
  <Suspense fallback={null}>
    <MonacoWrapper
      language='javascript'
      value={testsScript}
      onChange={readOnly ? undefined : onChangeTests}
      readOnly={readOnly}
      height='100%'
    />
  </Suspense>
</TabsContent>
```

to:

```tsx
<TabsContent value='tests' className='flex-1 m-0 p-0 flex overflow-hidden'>
  <div className='flex-1 min-w-0'>
    <Suspense fallback={null}>
      <MonacoWrapper
        language='javascript'
        value={testsScript}
        onChange={readOnly ? undefined : onChangeTests}
        readOnly={readOnly}
        height='100%'
        phase='tests'
        onEditorReady={(editor) => { editorRef.current = editor; }}
      />
    </Suspense>
  </div>
  {!readOnly && <ScriptSnippetSidebar onInsert={handleInsert} />}
</TabsContent>
```

- [ ] **Step 4: Add `phase` prop to Pre Request and Post Response editors**

Change the Pre Request `MonacoWrapper`:

```tsx
<MonacoWrapper
  language='javascript'
  value={preRequestScript}
  onChange={readOnly ? undefined : onChangePreRequest}
  readOnly={readOnly}
  height='100%'
  phase='pre-request'
/>
```

Change the Post Response `MonacoWrapper`:

```tsx
<MonacoWrapper
  language='javascript'
  value={postResponseScript}
  onChange={readOnly ? undefined : onChangePostResponse}
  readOnly={readOnly}
  height='100%'
  phase='post-response'
/>
```

- [ ] **Step 5: Type-check and lint**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/ScriptsTab.tsx
git commit -m "feat(scripts): wire ScriptSnippetSidebar and IntelliSense into ScriptsTab Tests sub-tab"
```

---

## Task 5: Manual verification

- [ ] **Step 1: Start the dev server**

```bash
yarn dev
```

- [ ] **Step 2: Open any request → Scripts tab → Tests sub-tab**

Verify:
- Two-column layout renders: Monaco editor on the left, snippets sidebar on the right.
- The drag handle between editor and sidebar is visible; dragging resizes the sidebar (min 120px, max 50% of panel).
- "Common Tests" and "API Reference" sections are visible and collapsible.

- [ ] **Step 3: Click "Status is 200" in Common Tests**

Verify:
- The test block is inserted at the cursor position in the Monaco editor.

- [ ] **Step 4: Type `res.` in the editor**

Verify:
- Monaco autocomplete dropdown appears listing `getStatus`, `getStatusText`, `getHeader`, `getHeaders`, `getBody`, `getResponseTime`.
- Hovering a completion shows the JSDoc description.

- [ ] **Step 5: Switch to Pre Request sub-tab, type `req.`**

Verify:
- Monaco autocomplete shows `req.*` methods.
- `res` and `test` are not in the completion list.

- [ ] **Step 6: Switch to Post Response sub-tab, type `rok.`**

Verify:
- Monaco autocomplete shows `rok.*` methods.
- `req` is not in the completion list.

- [ ] **Step 7: Verify Pre Request and Post Response are still full-width (no sidebar)**

Expected: sidebar only appears on Tests sub-tab.

- [ ] **Step 8: Run full verification**

```bash
yarn tsc --noEmit && yarn check && yarn test
```

Expected: all pass.

- [ ] **Step 9: Final commit if any lint fixes were needed**

```bash
git add -p
git commit -m "chore(scripts): lint fixes from verification pass"
```
