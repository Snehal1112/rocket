# Console Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Postman-style Console panel that logs every request sent during the session with expandable request/response detail, toggled from the StatusBar.

**Architecture:** Task 1 creates the Zustand console store. Task 2 wires it into execute-request.ts. Task 3 builds the ConsolePanel UI component. Task 4 adds the toggle button to StatusBar. Task 5 wires everything in App.tsx.

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-console-panel-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/stores/console-store.ts` | Create — console entry store with 200-entry cap |
| `src/lib/execute-request.ts` | Modify — add console entry after request completion |
| `src/components/layout/ConsolePanel.tsx` | Create — resizable console panel with entry list |
| `src/components/layout/StatusBar.tsx` | Modify — add Console toggle button |
| `src/App.tsx` | Modify — wire console state and render panel |

---

### Task 1: Create console store

**Files:**
- Create: `src/stores/console-store.ts`

- [ ] **Step 1: Create the store**

Create `src/stores/console-store.ts` with:

```ts
import { create } from 'zustand';

const MAX_ENTRIES = 200;

export interface ConsoleEntry {
  id: string;
  timestamp: string;
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  requestHeaders: { key: string; value: string }[];
  requestBody: string;
  responseHeaders: { key: string; value: string }[];
  responseBody: string;
}

interface ConsoleState {
  entries: ConsoleEntry[];
  addEntry: (entry: Omit<ConsoleEntry, 'id' | 'timestamp'>) => void;
  clearEntries: () => void;
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],

  addEntry: (entry) => {
    const full: ConsoleEntry = {
      ...entry,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  clearEntries: () => set({ entries: [] }),
}));
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/stores/console-store.ts
git commit -m "feat: add console store with 200-entry cap"
```

---

### Task 2: Wire console store into execute-request.ts

**Files:**
- Modify: `src/lib/execute-request.ts`

**Depends on:** Task 1

**Context:** The `sendRequest` function at `src/lib/execute-request.ts` is where every request completes. After `usePaneStore.getState().setResponse(tabId, responseState)` on line 116, add a call to log the entry in the console store. Also log errors from the catch block.

- [ ] **Step 1: Add import**

Find the imports at the top of `src/lib/execute-request.ts`. Add:

```ts
import { useConsoleStore } from '@/stores/console-store';
```

- [ ] **Step 2: Add console entry after successful response**

Find (~line 116):
```ts
    usePaneStore.getState().setResponse(tabId, responseState);
```

Add AFTER it:
```ts
    useConsoleStore.getState().addEntry({
      method: request.method,
      url: resolvedUrl,
      status: result.status,
      statusText: result.statusText,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      requestHeaders: resolvedHeaders.map((h) => ({ key: h.key, value: h.value })),
      requestBody: resolvedBody?.content ?? '',
      responseHeaders: result.headers.map((h) => ({ key: h.key, value: h.value })),
      responseBody: result.body,
    });
```

- [ ] **Step 3: Add console entry for failed requests**

Find the catch block (~lines 117-128). After `usePaneStore.getState().setResponse(tabId, {...})` in the catch block, add:

```ts
    useConsoleStore.getState().addEntry({
      method: request.method,
      url: resolvedUrl,
      status: 0,
      statusText: 'Error',
      durationMs: 0,
      sizeBytes: msg.length,
      requestHeaders: resolvedHeaders.map((h) => ({ key: h.key, value: h.value })),
      requestBody: resolvedBody?.content ?? '',
      responseHeaders: [],
      responseBody: msg,
    });
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/lib/execute-request.ts
git commit -m "feat: log every request to console store from execute-request"
```

---

### Task 3: Create ConsolePanel component

**Files:**
- Create: `src/components/layout/ConsolePanel.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Create the component**

Create `src/components/layout/ConsolePanel.tsx` with:

```tsx
import { useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { useConsoleStore, type ConsoleEntry } from '@/stores/console-store';

interface ConsolePanelProps {
  isOpen: boolean;
  height: number;
  onHeightChange: (height: number) => void;
}

const MIN_HEIGHT = 120;
const MAX_HEIGHT = 600;

function statusColor(status: number): string {
  if (status >= 500 || status === 0) return 'text-red-500';
  if (status >= 400) return 'text-orange-500';
  if (status >= 300) return 'text-yellow-500';
  if (status >= 200) return 'text-green-500';
  return 'text-muted-foreground';
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
}

function EntryDetail({ entry }: { entry: ConsoleEntry }) {
  const sections = [
    { label: 'Request Headers', content: entry.requestHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)' },
    { label: 'Request Body', content: entry.requestBody || '(empty)' },
    { label: 'Response Headers', content: entry.responseHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)' },
    { label: 'Response Body', content: entry.responseBody || '(empty)' },
  ];

  return (
    <div className="grid grid-cols-2 gap-2 px-4 py-2 bg-muted/30 border-t text-xs">
      {sections.map((s) => (
        <div key={s.label}>
          <div className="font-medium text-muted-foreground mb-1">{s.label}</div>
          <pre className="font-mono whitespace-pre-wrap break-all bg-background/60 rounded p-1.5 max-h-32 overflow-auto text-2xs">
            {s.content}
          </pre>
        </div>
      ))}
    </div>
  );
}

export function ConsolePanel({ isOpen, height, onHeightChange }: ConsolePanelProps) {
  const entries = useConsoleStore((s) => s.entries);
  const clearEntries = useConsoleStore((s) => s.clearEntries);
  const [search, setSearch] = useState('');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const dragRef = useRef<{ y: number; h: number } | null>(null);

  if (!isOpen) return null;

  const filtered = search
    ? entries.filter((e) => e.url.toLowerCase().includes(search.toLowerCase()))
    : entries;

  const handleDragDown = (e: React.PointerEvent) => {
    e.preventDefault();
    dragRef.current = { y: e.clientY, h: height };
    const onMove = (ev: PointerEvent) => {
      if (!dragRef.current) return;
      const delta = dragRef.current.y - ev.clientY;
      onHeightChange(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, dragRef.current.h + delta)));
    };
    const onUp = () => {
      dragRef.current = null;
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  };

  return (
    <div
      className="shrink-0 border-t border-border/70 bg-card/85 backdrop-blur-sm flex flex-col"
      style={{ '--console-h': `${height}px` } as React.CSSProperties}
      className-extra="h-[var(--console-h)]"
    >
      <div
        style={{ height }}
        className="flex flex-col"
      >
        {/* Drag handle. */}
        <div
          className="h-1 cursor-row-resize bg-border/40 hover:bg-primary/40 transition-colors shrink-0"
          onPointerDown={handleDragDown}
        />

        {/* Toolbar. */}
        <div className="flex items-center gap-2 px-2 py-1 border-b border-border/70 shrink-0">
          <span className="text-xs font-medium">Console</span>
          {entries.length > 0 && (
            <span className="text-2xs px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground">
              {entries.length}
            </span>
          )}
          <div className="flex-1" />
          <Input
            placeholder="Filter by URL"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-6 text-xs w-48"
          />
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={clearEntries}
            aria-label="Clear console"
          >
            <Trash2 className="h-3 w-3 mr-1" />
            Clear
          </Button>
        </div>

        {/* Entry list. */}
        <div className="flex-1 overflow-y-auto font-mono text-2xs">
          {filtered.length === 0 ? (
            <div className="flex items-center justify-center h-full text-muted-foreground text-xs">
              No requests sent yet
            </div>
          ) : (
            filtered.map((entry) => (
              <div key={entry.id}>
                <div
                  className="flex items-center gap-1.5 px-2 py-1 hover:bg-accent/40 cursor-pointer border-b border-border/30"
                  onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                >
                  {expandedId === entry.id
                    ? <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
                    : <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground" />
                  }
                  <span className="text-muted-foreground w-16 shrink-0">{formatTime(entry.timestamp)}</span>
                  <span className="font-semibold w-12 shrink-0">{entry.method}</span>
                  <span className="flex-1 truncate text-foreground/80">{entry.url}</span>
                  <span className={cn('w-10 text-right shrink-0 font-semibold', statusColor(entry.status))}>
                    {entry.status || 'ERR'}
                  </span>
                  <span className="text-muted-foreground w-16 text-right shrink-0">
                    {entry.durationMs}ms
                  </span>
                </div>
                {expandedId === entry.id && <EntryDetail entry={entry} />}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
```

**Note:** The component above has a mistake — it uses both `style={{ height }}` and a CSS variable pattern. The implementer should use `style={{ height }}` directly on the outer container div (not a CSS variable, since we need the dynamic height). Remove the `className-extra` and the CSS variable — just use:

```tsx
<div
  className="shrink-0 border-t border-border/70 bg-card/85 backdrop-blur-sm flex flex-col"
  style={{ height }}
>
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/layout/ConsolePanel.tsx
git commit -m "feat: create ConsolePanel component with entry list and expandable detail"
```

---

### Task 4: Add Console toggle to StatusBar

**Files:**
- Modify: `src/components/layout/StatusBar.tsx`

- [ ] **Step 1: Rewrite StatusBar with toggle**

Replace the entire contents of `src/components/layout/StatusBar.tsx` with:

```tsx
import { Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { EnvironmentSwitcher } from '@/components/layout/EnvironmentSwitcher';
import { useConsoleStore } from '@/stores/console-store';

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);

  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <Button
        variant="ghost"
        size="sm"
        className={cn('h-5 px-1.5 text-2xs gap-1', isConsoleOpen && 'bg-accent')}
        onClick={onConsoleToggle}
        aria-label="Toggle Console"
      >
        <Terminal className="h-3 w-3" />
        Console
        {entryCount > 0 && (
          <span className="text-2xs px-1 rounded-full bg-muted text-muted-foreground">
            {entryCount}
          </span>
        )}
      </Button>
      <div className="ml-auto">
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/layout/StatusBar.tsx
git commit -m "feat: add Console toggle button with entry count to StatusBar"
```

---

### Task 5: Wire ConsolePanel in App.tsx

**Files:**
- Modify: `src/App.tsx`

**Depends on:** Tasks 3, 4

- [ ] **Step 1: Add import**

Find the imports at the top of `src/App.tsx`. Add:

```tsx
import { ConsolePanel } from '@/components/layout/ConsolePanel';
```

- [ ] **Step 2: Add console state**

Find (~line 14-15):
```tsx
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed] = useState(false);
```

Add after:
```tsx
  const [isConsoleOpen, setIsConsoleOpen] = useState(false);
  const [consoleHeight, setConsoleHeight] = useState(280);
```

- [ ] **Step 3: Insert ConsolePanel and update StatusBar**

Find (~line 56):
```tsx
      <StatusBar />
```

Replace with:
```tsx
      <ConsolePanel isOpen={isConsoleOpen} height={consoleHeight} onHeightChange={setConsoleHeight} />
      <StatusBar isConsoleOpen={isConsoleOpen} onConsoleToggle={() => setIsConsoleOpen((o) => !o)} />
```

- [ ] **Step 4: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/App.tsx
git commit -m "feat: wire ConsolePanel into App layout between main content and StatusBar"
```

---

## Done

Console Panel complete:
- Every request sent is logged in the console store (success and error)
- Console toggle button in StatusBar with entry count badge
- Resizable panel slides up above StatusBar (120-600px, default 280px)
- Entry rows show timestamp, method, URL, status (color-coded), duration
- Click to expand shows request/response headers and body in a 2-column grid
- URL filter input for searching entries
- Clear button to empty the log
- Capped at 200 entries (oldest dropped)
