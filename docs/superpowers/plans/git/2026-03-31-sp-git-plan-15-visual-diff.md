# Plan 15: Visual Diff Mode for Git Diff Viewer

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Visual" mode toggle to the git diff viewer that renders API request file changes as a structured, field-by-field comparison (method, URL, headers, query params, body, auth, scripts) instead of raw Monaco text diffs.

**Architecture:** Pure frontend — `DiffState.oldContent`/`newContent` already carry the full file strings. A parser utility (`parse-request-diff.ts`) tries to parse both as JSON request objects and returns a `RequestDiff` with per-field change tracking. `VisualDiffView` renders the structured output with color-coded rows. `DiffViewer` holds mode state (persisted to `localStorage`), passes toggle props to `DiffHeader`, and conditionally renders the visual or Monaco view. Visual mode is only offered when the file has a `.json` extension; all other files keep the text diff only.

**Tech Stack:** React 19, TypeScript 5.8, TailwindCSS 4.2, shadcn/ui Tabs, Vitest

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/types/visual-diff-types.ts` | `FieldChange<T>`, `RowChange`, `RequestDiff` |
| Create | `src/lib/parse-request-diff.ts` | `parseRequestDiff(old, new) → RequestDiff \| null` |
| Create | `src/lib/__tests__/parse-request-diff.test.ts` | Unit tests for parser |
| Create | `src/components/git/VisualDiffView.tsx` | Visual diff renderer |
| Modify | `src/components/git/DiffHeader.tsx` | Add Text/Visual mode toggle |
| Modify | `src/components/git/DiffViewer.tsx` | Mode state, conditional render |

---

## Chunk 1: Types and Parser

### Task 1: Visual diff types

**Files:**
- Create: `src/types/visual-diff-types.ts`

- [ ] **Step 1: Create the types file**

Create `src/types/visual-diff-types.ts` with the full content below:

```typescript
// Types for the visual diff view — field-by-field comparison of request changes.

/** A single labeled field that may have changed between old and new versions. */
export interface FieldChange<T> {
  label: string;
  oldValue: T | undefined;
  newValue: T | undefined;
  /** True when old and new differ by JSON.stringify comparison. */
  changed: boolean;
}

/** A single row in a key-value list (headers, query params, path params). */
export interface RowChange {
  key: string;
  oldRow: { value: string; enabled: boolean } | undefined;
  newRow: { value: string; enabled: boolean } | undefined;
  status: 'added' | 'removed' | 'modified' | 'unchanged';
}

/** Structured diff of a single JSON request file. */
export interface RequestDiff {
  method: FieldChange<string>;
  url: FieldChange<string>;
  headers: RowChange[];
  queryParams: RowChange[];
  pathParams: RowChange[];
  body: FieldChange<{ mode: string; content: string | undefined }>;
  auth: FieldChange<string>;
  preRequestScript: FieldChange<string>;
  postResponseScript: FieldChange<string>;
  /** True when at least one field has changed. */
  hasChanges: boolean;
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/types/visual-diff-types.ts
git commit -m "feat(frontend): add visual diff types"
```

---

### Task 2: Request diff parser (TDD)

**Files:**
- Create: `src/lib/parse-request-diff.ts`
- Create: `src/lib/__tests__/parse-request-diff.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/parse-request-diff.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { parseRequestDiff } from '../parse-request-diff';

describe('parseRequestDiff', () => {
  const baseRequest = {
    method: 'GET',
    url: 'https://api.example.com/users',
    headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    queryParams: [],
    pathParams: [],
    body: null,
    auth: { type: 'none' },
    preRequestScript: null,
    postResponseScript: null,
  };

  it('returns null when both contents are undefined', () => {
    expect(parseRequestDiff(undefined, undefined)).toBeNull();
  });

  it('returns null when neither is valid JSON', () => {
    expect(parseRequestDiff('not json', 'also not json')).toBeNull();
  });

  it('returns null when JSON lacks method and url', () => {
    expect(parseRequestDiff('{"foo":"bar"}', '{"foo":"baz"}')).toBeNull();
  });

  it('returns a diff when both sides are valid request objects', () => {
    const content = JSON.stringify(baseRequest);
    expect(parseRequestDiff(content, content)).not.toBeNull();
  });

  it('reports no changes when files are identical', () => {
    const content = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(content, content)!;
    expect(diff.hasChanges).toBe(false);
    expect(diff.method.changed).toBe(false);
    expect(diff.url.changed).toBe(false);
    expect(diff.auth.changed).toBe(false);
  });

  it('detects method change', () => {
    const old = JSON.stringify({ ...baseRequest, method: 'GET' });
    const nw = JSON.stringify({ ...baseRequest, method: 'POST' });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.method.changed).toBe(true);
    expect(diff.method.oldValue).toBe('GET');
    expect(diff.method.newValue).toBe('POST');
    expect(diff.hasChanges).toBe(true);
  });

  it('detects url change', () => {
    const old = JSON.stringify({ ...baseRequest, url: 'https://a.com' });
    const nw = JSON.stringify({ ...baseRequest, url: 'https://b.com' });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.url.changed).toBe(true);
  });

  it('detects added header', () => {
    const old = JSON.stringify({ ...baseRequest, headers: [] });
    const nw = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Authorization', value: 'Bearer token', enabled: true }],
    });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Authorization')!;
    expect(row.status).toBe('added');
    expect(row.oldRow).toBeUndefined();
    expect(row.newRow?.value).toBe('Bearer token');
  });

  it('detects removed header', () => {
    const old = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    });
    const nw = JSON.stringify({ ...baseRequest, headers: [] });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Accept')!;
    expect(row.status).toBe('removed');
    expect(row.newRow).toBeUndefined();
  });

  it('detects modified header value', () => {
    const old = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    });
    const nw = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'text/plain', enabled: true }],
    });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Accept')!;
    expect(row.status).toBe('modified');
    expect(row.oldRow?.value).toBe('application/json');
    expect(row.newRow?.value).toBe('text/plain');
  });

  it('handles new file (oldContent undefined)', () => {
    const nw = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(undefined, nw)!;
    expect(diff).not.toBeNull();
    expect(diff.method.oldValue).toBeUndefined();
    expect(diff.method.newValue).toBe('GET');
    expect(diff.method.changed).toBe(true);
  });

  it('handles deleted file (newContent undefined)', () => {
    const old = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(old, undefined)!;
    expect(diff).not.toBeNull();
    expect(diff.method.oldValue).toBe('GET');
    expect(diff.method.newValue).toBeUndefined();
    expect(diff.method.changed).toBe(true);
  });

  it('treats null and undefined script fields as equivalent (no false positive)', () => {
    const a = JSON.stringify({ ...baseRequest, preRequestScript: null });
    const b = JSON.stringify({ ...baseRequest, preRequestScript: undefined });
    const diff = parseRequestDiff(a, b)!;
    expect(diff.preRequestScript.changed).toBe(false);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn test src/lib/__tests__/parse-request-diff.test.ts --run`
Expected: all 14 tests FAIL with "Cannot find module '../parse-request-diff'"

- [ ] **Step 3: Implement the parser**

Create `src/lib/parse-request-diff.ts`:

```typescript
import type { FieldChange, RowChange, RequestDiff } from '@/types/visual-diff-types';

interface KVRow {
  key: string;
  value: string;
  enabled: boolean;
}

interface RequestJson {
  method?: string;
  url?: string;
  headers?: KVRow[];
  queryParams?: KVRow[];
  pathParams?: KVRow[];
  body?: { mode?: string; content?: string } | null;
  auth?: { type?: string } | null;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
}

/** Tries to parse content as a JSON request object. Returns null if unparseable or not a request. */
function tryParseRequest(content: string | undefined): RequestJson | null {
  if (!content) return null;
  try {
    const obj = JSON.parse(content) as unknown;
    if (typeof obj !== 'object' || obj === null || Array.isArray(obj)) return null;
    const r = obj as Record<string, unknown>;
    // Must have at least one of method or url to qualify as a request file.
    if (!r.method && !r.url) return null;
    return r as RequestJson;
  } catch {
    return null;
  }
}

/** Converts null to undefined so null/undefined fields compare as equal. */
function norm<T>(val: T | null | undefined): T | undefined {
  return val === null ? undefined : val;
}

/** Builds a FieldChange for a single scalar field. */
function field<T>(label: string, oldVal: T | undefined, newVal: T | undefined): FieldChange<T> {
  const changed = JSON.stringify(oldVal) !== JSON.stringify(newVal);
  return { label, oldValue: oldVal, newValue: newVal, changed };
}

/** Diffs two KV-row arrays by key, preserving new order and appending removed keys at end. */
function diffRows(oldRows: KVRow[], newRows: KVRow[]): RowChange[] {
  const result: RowChange[] = [];
  const oldMap = new Map(oldRows.map((r) => [r.key, r]));
  const newMap = new Map(newRows.map((r) => [r.key, r]));

  // Walk new rows first, then append keys only in old.
  const allKeys = [
    ...newRows.map((r) => r.key),
    ...oldRows.filter((r) => !newMap.has(r.key)).map((r) => r.key),
  ];
  const seen = new Set<string>();

  for (const key of allKeys) {
    if (seen.has(key)) continue;
    seen.add(key);
    const o = oldMap.get(key);
    const n = newMap.get(key);
    if (!o) {
      result.push({ key, oldRow: undefined, newRow: { value: n!.value, enabled: n!.enabled }, status: 'added' });
    } else if (!n) {
      result.push({ key, oldRow: { value: o.value, enabled: o.enabled }, newRow: undefined, status: 'removed' });
    } else {
      const changed = o.value !== n.value || o.enabled !== n.enabled;
      result.push({
        key,
        oldRow: { value: o.value, enabled: o.enabled },
        newRow: { value: n.value, enabled: n.enabled },
        status: changed ? 'modified' : 'unchanged',
      });
    }
  }
  return result;
}

/**
 * Parses old and new JSON request file strings into a structured diff.
 * Returns null if neither parses as a valid request object.
 */
export function parseRequestDiff(
  oldContent: string | undefined,
  newContent: string | undefined,
): RequestDiff | null {
  const old = tryParseRequest(oldContent);
  const nw = tryParseRequest(newContent);
  if (!old && !nw) return null;

  const method = field('Method', norm(old?.method), norm(nw?.method));
  const url = field('URL', norm(old?.url), norm(nw?.url));
  const headers = diffRows(old?.headers ?? [], nw?.headers ?? []);
  const queryParams = diffRows(old?.queryParams ?? [], nw?.queryParams ?? []);
  const pathParams = diffRows(old?.pathParams ?? [], nw?.pathParams ?? []);

  const oldBody = old?.body
    ? { mode: old.body.mode ?? 'none', content: norm(old.body.content) }
    : { mode: 'none', content: undefined };
  const newBody = nw?.body
    ? { mode: nw.body.mode ?? 'none', content: norm(nw.body.content) }
    : { mode: 'none', content: undefined };
  const body = field('Body', oldBody, newBody);

  const auth = field('Auth', old?.auth?.type ?? 'none', nw?.auth?.type ?? 'none');
  const preRequestScript = field('Pre-request Script', norm(old?.preRequestScript), norm(nw?.preRequestScript));
  const postResponseScript = field('Post-response Script', norm(old?.postResponseScript), norm(nw?.postResponseScript));

  const rowsChanged = [...headers, ...queryParams, ...pathParams].some(
    (r) => r.status !== 'unchanged',
  );
  const hasChanges =
    method.changed ||
    url.changed ||
    rowsChanged ||
    body.changed ||
    auth.changed ||
    preRequestScript.changed ||
    postResponseScript.changed;

  return {
    method,
    url,
    headers,
    queryParams,
    pathParams,
    body,
    auth,
    preRequestScript,
    postResponseScript,
    hasChanges,
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn test src/lib/__tests__/parse-request-diff.test.ts --run`
Expected: 14 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/parse-request-diff.ts src/lib/__tests__/parse-request-diff.test.ts
git commit -m "feat(frontend): add request diff parser with tests"
```

---

## Chunk 2: Visual Component

### Task 3: VisualDiffView component

**Files:**
- Create: `src/components/git/VisualDiffView.tsx`

- [ ] **Step 1: Create the component**

Create `src/components/git/VisualDiffView.tsx`:

```tsx
import { cn } from '@/lib/utils';
import { parseRequestDiff } from '@/lib/parse-request-diff';
import type { RowChange } from '@/types/visual-diff-types';

interface VisualDiffViewProps {
  oldContent: string | undefined;
  newContent: string | undefined;
}

// Renders a single labeled field change. Shows old/new on separate rows when changed.
function DiffField({
  label,
  oldValue,
  newValue,
  changed,
}: {
  label: string;
  oldValue: string | undefined;
  newValue: string | undefined;
  changed: boolean;
}) {
  if (!changed) {
    return (
      <tr>
        <td className="py-1 pr-4 text-muted-foreground w-32 align-top">{label}</td>
        <td className="py-1 font-mono text-xs" colSpan={2}>
          {newValue ?? oldValue ?? '—'}
        </td>
      </tr>
    );
  }
  return (
    <>
      {oldValue !== undefined && (
        <tr className="bg-red-50 dark:bg-red-950/20">
          <td className="py-1 pr-4 text-muted-foreground w-32 align-top">{label}</td>
          <td className="py-1 pr-2 text-xs text-red-500 dark:text-red-400 w-8 align-top">old</td>
          <td className="py-1 font-mono text-xs text-red-700 dark:text-red-300 line-through break-all">
            {oldValue}
          </td>
        </tr>
      )}
      {newValue !== undefined && (
        <tr className="bg-green-50 dark:bg-green-950/20">
          <td className="py-1 pr-4 text-muted-foreground w-32 align-top">
            {oldValue !== undefined ? '' : label}
          </td>
          <td className="py-1 pr-2 text-xs text-green-500 dark:text-green-400 w-8 align-top">new</td>
          <td className="py-1 font-mono text-xs text-green-700 dark:text-green-300 break-all">
            {newValue}
          </td>
        </tr>
      )}
    </>
  );
}

const ROW_BG: Record<RowChange['status'], string> = {
  added: 'bg-green-50 dark:bg-green-950/20',
  removed: 'bg-red-50 dark:bg-red-950/20',
  modified: 'bg-amber-50 dark:bg-amber-950/20',
  unchanged: '',
};

const ROW_BADGE: Record<RowChange['status'], string> = {
  added: 'text-green-600 dark:text-green-400',
  removed: 'text-red-600 dark:text-red-400',
  modified: 'text-amber-600 dark:text-amber-400',
  unchanged: 'text-muted-foreground',
};

const ROW_LABEL: Record<RowChange['status'], string> = {
  added: 'A',
  removed: 'D',
  modified: 'M',
  unchanged: '',
};

// Renders a key/value list diff (headers, params) as a table with status indicators.
function KVTable({ title, rows }: { title: string; rows: RowChange[] }) {
  if (rows.length === 0) return null;
  return (
    <section>
      <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
        {title}
      </h3>
      <div className="border rounded-md overflow-hidden">
        <table className="w-full text-xs">
          <tbody>
            {rows.map((row, i) => (
              <tr key={i} className={cn('border-b last:border-0', ROW_BG[row.status])}>
                <td className={cn('py-1.5 pl-2 pr-1 font-bold w-4', ROW_BADGE[row.status])}>
                  {ROW_LABEL[row.status]}
                </td>
                <td className="py-1.5 px-2 font-mono text-muted-foreground w-1/3">
                  {row.key}
                </td>
                <td className="py-1.5 px-2 font-mono opacity-60 w-1/3 line-through">
                  {row.status === 'removed' || row.status === 'modified'
                    ? (row.oldRow?.value ?? '')
                    : ''}
                </td>
                <td className="py-1.5 px-2 font-mono">
                  {row.status === 'added' || row.status === 'modified'
                    ? (row.newRow?.value ?? '')
                    : row.status === 'unchanged'
                      ? (row.newRow?.value ?? '')
                      : ''}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// Renders API request file changes as a structured field-by-field comparison.
export function VisualDiffView({ oldContent, newContent }: VisualDiffViewProps) {
  const diff = parseRequestDiff(oldContent, newContent);

  if (!diff) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Visual diff is not available for this file type.
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-5 text-sm">
      {/* Request — method and URL. */}
      <section>
        <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
          Request
        </h3>
        <div className="border rounded-md overflow-hidden px-3 py-1">
          <table className="w-full text-xs">
            <tbody>
              <DiffField
                label="Method"
                oldValue={diff.method.oldValue}
                newValue={diff.method.newValue}
                changed={diff.method.changed}
              />
              <DiffField
                label="URL"
                oldValue={diff.url.oldValue}
                newValue={diff.url.newValue}
                changed={diff.url.changed}
              />
            </tbody>
          </table>
        </div>
      </section>

      <KVTable title="Headers" rows={diff.headers} />
      <KVTable title="Query Params" rows={diff.queryParams} />
      <KVTable title="Path Params" rows={diff.pathParams} />

      {/* Body — show only when body mode changed. */}
      {diff.body.changed && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Body
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                <DiffField
                  label="Mode"
                  oldValue={diff.body.oldValue?.mode}
                  newValue={diff.body.newValue?.mode}
                  changed={diff.body.oldValue?.mode !== diff.body.newValue?.mode}
                />
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Auth — show only when auth type changed. */}
      {diff.auth.changed && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Auth
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                <DiffField
                  label={diff.auth.label}
                  oldValue={diff.auth.oldValue}
                  newValue={diff.auth.newValue}
                  changed={diff.auth.changed}
                />
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Scripts — show only when pre/post scripts changed. */}
      {(diff.preRequestScript.changed || diff.postResponseScript.changed) && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Scripts
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                {diff.preRequestScript.changed && (
                  <DiffField
                    label={diff.preRequestScript.label}
                    oldValue={
                      diff.preRequestScript.oldValue !== undefined
                        ? `${diff.preRequestScript.oldValue.slice(0, 60)}${diff.preRequestScript.oldValue.length > 60 ? '…' : ''}`
                        : undefined
                    }
                    newValue={
                      diff.preRequestScript.newValue !== undefined
                        ? `${diff.preRequestScript.newValue.slice(0, 60)}${diff.preRequestScript.newValue.length > 60 ? '…' : ''}`
                        : undefined
                    }
                    changed={diff.preRequestScript.changed}
                  />
                )}
                {diff.postResponseScript.changed && (
                  <DiffField
                    label={diff.postResponseScript.label}
                    oldValue={
                      diff.postResponseScript.oldValue !== undefined
                        ? `${diff.postResponseScript.oldValue.slice(0, 60)}${diff.postResponseScript.oldValue.length > 60 ? '…' : ''}`
                        : undefined
                    }
                    newValue={
                      diff.postResponseScript.newValue !== undefined
                        ? `${diff.postResponseScript.newValue.slice(0, 60)}${diff.postResponseScript.newValue.length > 60 ? '…' : ''}`
                        : undefined
                    }
                    changed={diff.postResponseScript.changed}
                  />
                )}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {!diff.hasChanges && (
        <p className="text-center text-xs text-muted-foreground py-8">No changes detected.</p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/git/VisualDiffView.tsx
git commit -m "feat(frontend): add VisualDiffView component"
```

---

## Chunk 3: Integration

### Task 4: Wire mode toggle into DiffHeader and DiffViewer

**Files:**
- Modify: `src/components/git/DiffHeader.tsx`
- Modify: `src/components/git/DiffViewer.tsx`

**Context:** `DiffHeader.tsx` currently renders a status badge, file path, and a staged/working Tabs toggle. `DiffViewer.tsx` owns the internal diff state (fetched on staged toggle) and renders `DiffHeader` + a Monaco `DiffEditor`. Both files are small (31 and 76 lines respectively).

- [ ] **Step 1: Update DiffHeader to accept and render the mode toggle**

Replace the full content of `src/components/git/DiffHeader.tsx`:

```tsx
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { GitStatusBadge } from './GitStatusBadge';
import type { DiffState } from '@/types/pane-types';
import type { GitStatusKind } from '@/lib/tauri-api';

interface DiffHeaderProps {
  diffState: DiffState;
  onToggleStaged: (isStaged: boolean) => void;
  mode: 'text' | 'visual';
  onModeChange: (mode: 'text' | 'visual') => void;
  canShowVisual: boolean;
}

// Header bar showing file status badge, path, staged/working toggle, and text/visual mode toggle.
export function DiffHeader({
  diffState,
  onToggleStaged,
  mode,
  onModeChange,
  canShowVisual,
}: DiffHeaderProps) {
  return (
    <div className="flex items-center gap-2 border-b px-3 py-1.5">
      <GitStatusBadge status={diffState.status as GitStatusKind} />
      <span className="font-mono text-xs truncate">{diffState.filePath}</span>
      <div className="ml-auto flex items-center gap-2">
        {canShowVisual && (
          <Tabs
            value={mode}
            onValueChange={(v) => onModeChange(v as 'text' | 'visual')}
          >
            <TabsList className="h-6">
              <TabsTrigger value="text" className="text-xs px-2 py-0.5">Text</TabsTrigger>
              <TabsTrigger value="visual" className="text-xs px-2 py-0.5">Visual</TabsTrigger>
            </TabsList>
          </Tabs>
        )}
        <Tabs
          value={diffState.isStaged ? 'staged' : 'working'}
          onValueChange={(v) => onToggleStaged(v === 'staged')}
        >
          <TabsList className="h-6">
            <TabsTrigger value="working" className="text-xs px-2 py-0.5">Working</TabsTrigger>
            <TabsTrigger value="staged" className="text-xs px-2 py-0.5">Staged</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Update DiffViewer to own mode state and render VisualDiffView conditionally**

Replace the full content of `src/components/git/DiffViewer.tsx`:

```tsx
import { useState, useCallback } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import { DiffHeader } from './DiffHeader';
import { VisualDiffView } from './VisualDiffView';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import { useTheme } from '@/hooks/useTheme';
import type { DiffState } from '@/types/pane-types';

interface DiffViewerProps {
  diffState: DiffState;
}

// Maps file extension to a Monaco language identifier.
function getLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, string> = {
    json: 'json',
    js: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    jsx: 'javascript',
    md: 'markdown',
    yaml: 'yaml',
    yml: 'yaml',
    xml: 'xml',
    html: 'html',
    css: 'css',
    bru: 'plaintext',
  };
  return map[ext] ?? 'plaintext';
}

// Renders a side-by-side Monaco diff or visual structured diff for a single file.
export function DiffViewer({ diffState: initialDiffState }: DiffViewerProps) {
  const [diffState, setDiffState] = useState(initialDiffState);
  const { isDark } = useTheme();

  // Persist mode preference across sessions.
  const [mode, setMode] = useState<'text' | 'visual'>(() => {
    return (localStorage.getItem('git-diff-mode') as 'text' | 'visual') ?? 'text';
  });

  const handleModeChange = useCallback((m: 'text' | 'visual') => {
    setMode(m);
    localStorage.setItem('git-diff-mode', m);
  }, []);

  const handleToggleStaged = useCallback(async (isStaged: boolean) => {
    try {
      const diff = isStaged
        ? await gitDiffStaged(diffState.collectionPath, diffState.filePath)
        : await gitDiff(diffState.collectionPath, diffState.filePath);
      setDiffState({
        ...diffState,
        oldContent: diff.oldContent ?? '',
        newContent: diff.newContent ?? '',
        isStaged,
      });
    } catch {
      // Keep current state on error.
    }
  }, [diffState.collectionPath, diffState.filePath]);

  // Visual mode is only available for JSON request files.
  const canShowVisual = diffState.filePath.endsWith('.json');
  const language = getLanguage(diffState.filePath);

  return (
    <div className="flex flex-col h-full">
      <DiffHeader
        diffState={diffState}
        onToggleStaged={handleToggleStaged}
        mode={mode}
        onModeChange={handleModeChange}
        canShowVisual={canShowVisual}
      />
      {mode === 'visual' && canShowVisual ? (
        <VisualDiffView
          oldContent={diffState.oldContent}
          newContent={diffState.newContent}
        />
      ) : (
        <div className="flex-1">
          <DiffEditor
            original={diffState.oldContent}
            modified={diffState.newContent}
            language={language}
            theme={isDark ? 'vs-dark' : 'vs'}
            options={{
              readOnly: true,
              renderSideBySide: true,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              fontSize: 12,
            }}
          />
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 4: Run all tests**

Run: `yarn test --run`
Expected: all tests pass (142+ total)

- [ ] **Step 5: Verify the app builds**

Run: `yarn build`
Expected: build succeeds with no errors

- [ ] **Step 6: Commit**

```bash
git add src/components/git/DiffHeader.tsx src/components/git/DiffViewer.tsx
git commit -m "feat(frontend): add text/visual mode toggle to git diff viewer"
```
