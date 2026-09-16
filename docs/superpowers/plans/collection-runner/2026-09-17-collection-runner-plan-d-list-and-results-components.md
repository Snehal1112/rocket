# Collection Runner Plan D: List and Results Components — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the three presentational pieces of the Runner tab: the
pre-run checklist, the results list with expandable per-request detail,
and the aggregate summary/controls bar.

**Architecture:** Each component takes the whole `RunnerTab` as a prop
(the same pattern `RequestPanel({ tab, groupId })` already uses), reads
what it needs from `tab.requests`/`tab.runState`, and calls pane-store
actions (from Plan C) via `usePaneStore`. `RunnerResultsList` reuses the
existing `TestsPanel` component for each row's expanded test detail
instead of reimplementing test-result rendering.

**Tech Stack:** React, TypeScript, Zustand, Vitest, Testing Library,
shadcn/ui (`Checkbox`, `Badge`, `Button`), `lucide-react`.

**Spec:** `docs/superpowers/specs/2026-09-17-collection-runner-frontend-design.md`

## Global Constraints

- No backend/Rust changes.
- Method/status coloring reuses `METHOD_TEXT_COLOR` and
  `statusTextColor` from `src/lib/colors.ts` (already used by
  `HistoryPanel.tsx`) — do not invent new color constants.
- Checkboxes use the existing `Checkbox` component
  (`src/components/ui/checkbox.tsx`), following the exact
  `checked`/`onCheckedChange`/`aria-label` pattern already used in
  `src/components/request/KeyValueEditor.tsx:62-66`.
- No shadcn `Progress` component exists in this codebase
  (`src/components/ui/` has no `progress.tsx`) — the "progress
  indicator while running" from the spec is rendered as plain text
  counts (`"3 / 8"`), not a progress bar. Do not add a new UI primitive
  for this.
- Component tests use `fireEvent` from `@testing-library/react` for
  clicks, matching `src/components/contracts/ContractsFilterBar.test.tsx`.

---

### Task 1: `RunnerRequestList`

**Files:**
- Create: `src/components/request/runner/RunnerRequestList.tsx`
- Test: `src/components/request/runner/RunnerRequestList.test.tsx`

**Interfaces:**
- Consumes: `RunnerTab`/`RunnerRequestEntry` (Plan A), `toggleRunnerEntry`
  (Plan C Task 2, via `usePaneStore`).
- Produces: `RunnerRequestList({ tab }: { tab: RunnerTab })` — consumed
  by Plan E Task 1 (`RunnerPane`).

- [ ] **Step 1: Write the failing test**

```tsx
// src/components/request/runner/RunnerRequestList.test.tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerRequestList } from './RunnerRequestList';

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'a.yml',
    request: { uid: 'a', name: 'A', method: 'GET', url: 'https://example.com/a', headers: [], auth: { authType: 'none' } },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

function tab(requests: RunnerRequestEntry[], runState: RunnerTab['runState'] = 'idle'): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState,
    requests,
  };
}

describe('RunnerRequestList', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('shows an empty state when there are no requests', () => {
    render(<RunnerRequestList tab={tab([])} />);
    expect(screen.getByText(/no requests/i)).toBeInTheDocument();
  });

  it('renders one row per request with method and name', () => {
    render(<RunnerRequestList tab={tab([entry({ requestPath: 'a.yml' }), entry({ requestPath: 'b.yml', request: { uid: 'b', name: 'B', method: 'POST', url: '', headers: [], auth: { authType: 'none' } } })])} />);
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
    expect(screen.getByText('GET')).toBeInTheDocument();
    expect(screen.getByText('POST')).toBeInTheDocument();
  });

  it('toggling a checkbox calls toggleRunnerEntry with the tab id and request path', () => {
    const runnerTab = tab([entry({ requestPath: 'a.yml' })]);
    // Seed the store so toggleRunnerEntry has a real tab to patch.
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [runnerTab], activeTabId: runnerTab.id } as typeof s.root,
    }));

    render(<RunnerRequestList tab={runnerTab} />);
    fireEvent.click(screen.getByRole('checkbox'));

    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.requests[0].included).toBe(false);
  });

  it('disables checkboxes while a run is in progress', () => {
    render(<RunnerRequestList tab={tab([entry({})], 'running')} />);
    expect(screen.getByRole('checkbox')).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/request/runner/RunnerRequestList.test.tsx`
Expected: FAIL — the component file does not exist.

- [ ] **Step 3: Implement the component**

```tsx
// src/components/request/runner/RunnerRequestList.tsx
import { Checkbox } from '@/components/ui/checkbox';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';

export function RunnerRequestList({ tab }: { tab: RunnerTab }) {
  const toggleRunnerEntry = usePaneStore((s) => s.toggleRunnerEntry);

  if (tab.requests.length === 0) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No requests found in this collection/folder.
      </div>
    );
  }

  return (
    <div className='flex-1 overflow-auto'>
      {tab.requests.map((entry) => (
        <div
          key={entry.requestPath}
          className='flex items-center gap-3 px-3 py-2 border-b last:border-b-0 text-sm'
        >
          <Checkbox
            checked={entry.included}
            disabled={tab.runState === 'running'}
            onCheckedChange={() => toggleRunnerEntry(tab.id, entry.requestPath)}
            aria-label={`${entry.included ? 'Exclude' : 'Include'} ${entry.request.name}`}
          />
          <span
            className={cn(
              'w-14 shrink-0 font-mono text-xs font-medium',
              METHOD_TEXT_COLOR[entry.request.method] ?? 'text-muted-foreground',
            )}
          >
            {entry.request.method}
          </span>
          <span className='truncate text-foreground'>{entry.request.name}</span>
          <span className='ml-auto shrink-0 truncate text-xs text-muted-foreground'>
            {entry.requestPath}
          </span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/components/request/runner/RunnerRequestList.test.tsx`
Expected: PASS (4 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/runner/RunnerRequestList.tsx src/components/request/runner/RunnerRequestList.test.tsx
git commit -m "feat: add RunnerRequestList pre-run checklist component"
```

---

### Task 2: `RunnerResultsList`

**Files:**
- Create: `src/components/request/runner/RunnerResultsList.tsx`
- Test: `src/components/request/runner/RunnerResultsList.test.tsx`

**Interfaces:**
- Consumes: `RunnerTab`/`RunnerRequestEntry` (Plan A), `TestsPanel`
  (`src/components/response/TestsPanel.tsx`, existing, unmodified).
- Produces: `RunnerResultsList({ tab }: { tab: RunnerTab })` — consumed
  by Plan E Task 1 (`RunnerPane`).

- [ ] **Step 1: Write the failing test**

```tsx
// src/components/request/runner/RunnerResultsList.test.tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerResultsList } from './RunnerResultsList';

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'a.yml',
    request: { uid: 'a', name: 'A', method: 'GET', url: 'https://example.com/a', headers: [], auth: { authType: 'none' } },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

function tab(requests: RunnerRequestEntry[]): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState: 'done',
    requests,
  };
}

describe('RunnerResultsList', () => {
  it('renders a row per request with its status', () => {
    render(
      <RunnerResultsList
        tab={tab([
          entry({ requestPath: 'a.yml', status: 'passed' }),
          entry({
            requestPath: 'b.yml',
            status: 'failed',
            request: { uid: 'b', name: 'B', method: 'POST', url: '', headers: [], auth: { authType: 'none' } },
          }),
        ])}
      />,
    );
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
  });

  it('shows a skipped row distinctly', () => {
    render(<RunnerResultsList tab={tab([entry({ status: 'skipped' })])} />);
    expect(screen.getByText(/skipped/i)).toBeInTheDocument();
  });

  it('shows the error message for a row that threw before executing', () => {
    render(<RunnerResultsList tab={tab([entry({ status: 'failed', error: 'network down' })])} />);
    expect(screen.getByText('network down')).toBeInTheDocument();
  });

  it('expands a row to show its test results via TestsPanel', () => {
    render(
      <RunnerResultsList
        tab={tab([
          entry({
            status: 'passed',
            result: {
              status: 200,
              statusText: 'OK',
              headers: [],
              body: '',
              durationMs: 12,
              ttfbMs: 5,
              sizeBytes: 0,
              testResults: [{ name: 'status is 200', status: 'passed', error: null }],
              consoleEntries: [],
              scriptError: null,
            },
          }),
        ])}
      />,
    );

    // Detail is collapsed until the row is clicked.
    expect(screen.queryByText('status is 200')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('A'));
    expect(screen.getByText('status is 200')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/request/runner/RunnerResultsList.test.tsx`
Expected: FAIL — the component file does not exist.

- [ ] **Step 3: Implement the component**

```tsx
// src/components/request/runner/RunnerResultsList.tsx
import { CheckCircle2, ChevronDown, ChevronRight, CircleDashed, SkipForward, XCircle } from 'lucide-react';
import { useState } from 'react';
import { TestsPanel } from '@/components/response/TestsPanel';
import { METHOD_TEXT_COLOR, statusTextColor } from '@/lib/colors';
import { cn } from '@/lib/utils';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

function StatusIcon({ status }: { status: RunnerRequestEntry['status'] }) {
  switch (status) {
    case 'passed':
      return <CheckCircle2 className='h-4 w-4 text-green-500 shrink-0' />;
    case 'failed':
      return <XCircle className='h-4 w-4 text-red-500 shrink-0' />;
    case 'skipped':
      return <SkipForward className='h-4 w-4 text-muted-foreground shrink-0' />;
    default:
      return <CircleDashed className='h-4 w-4 text-muted-foreground shrink-0 animate-pulse' />;
  }
}

function ResultRow({ entry }: { entry: RunnerRequestEntry }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetail = Boolean(entry.result) || Boolean(entry.error);

  return (
    <div className='border-b last:border-b-0'>
      <button
        type='button'
        className='flex w-full items-center gap-3 px-3 py-2 text-sm text-left'
        onClick={() => hasDetail && setExpanded((v) => !v)}
      >
        {hasDetail ? (
          expanded ? (
            <ChevronDown className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          ) : (
            <ChevronRight className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )
        ) : (
          <span className='w-3.5 shrink-0' />
        )}
        <StatusIcon status={entry.status} />
        <span
          className={cn(
            'w-14 shrink-0 font-mono text-xs font-medium',
            METHOD_TEXT_COLOR[entry.request.method] ?? 'text-muted-foreground',
          )}
        >
          {entry.request.method}
        </span>
        <span className='truncate text-foreground'>{entry.request.name}</span>
        {entry.status === 'skipped' && (
          <span className='ml-2 text-xs text-muted-foreground'>skipped</span>
        )}
        {entry.result && (
          <span className={cn('ml-auto shrink-0 text-xs font-mono', statusTextColor(entry.result.status))}>
            {entry.result.status} · {entry.result.durationMs}ms
          </span>
        )}
      </button>
      {expanded && (
        <div className='px-3 pb-3'>
          {entry.error ? (
            <div className='text-xs text-red-500 font-mono break-all'>{entry.error}</div>
          ) : (
            <div className='h-48 border rounded'>
              <TestsPanel results={entry.result?.testResults ?? []} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function RunnerResultsList({ tab }: { tab: RunnerTab }) {
  if (tab.requests.length === 0) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No requests found in this collection/folder.
      </div>
    );
  }

  return (
    <div className='flex-1 overflow-auto'>
      {tab.requests.map((entry) => (
        <ResultRow key={entry.requestPath} entry={entry} />
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/components/request/runner/RunnerResultsList.test.tsx`
Expected: PASS (4 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/runner/RunnerResultsList.tsx src/components/request/runner/RunnerResultsList.test.tsx
git commit -m "feat: add RunnerResultsList component"
```

---

### Task 3: `RunnerSummaryHeader`

**Files:**
- Create: `src/components/request/runner/RunnerSummaryHeader.tsx`
- Test: `src/components/request/runner/RunnerSummaryHeader.test.tsx`

**Interfaces:**
- Consumes: `RunnerTab` (Plan A), `getRunnerSummary` (Plan C Task 3),
  `startRun`/`stopRun`/`rerunAll` (Plan C Task 2, via `usePaneStore`).
- Produces: `RunnerSummaryHeader({ tab }: { tab: RunnerTab })` —
  consumed by Plan E Task 1 (`RunnerPane`).

- [ ] **Step 1: Write the failing test**

```tsx
// src/components/request/runner/RunnerSummaryHeader.test.tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerSummaryHeader } from './RunnerSummaryHeader';

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'a.yml',
    request: { uid: 'a', name: 'A', method: 'GET', url: '', headers: [], auth: { authType: 'none' } },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

function tab(requests: RunnerRequestEntry[], runState: RunnerTab['runState'] = 'idle'): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState,
    requests,
  };
}

function seedStore(runnerTab: RunnerTab) {
  usePaneStore.setState((s) => ({
    root: { ...s.root, type: 'leaf', tabs: [runnerTab], activeTabId: runnerTab.id } as typeof s.root,
  }));
}

describe('RunnerSummaryHeader', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('shows pass/fail counts derived from the tab', () => {
    render(<RunnerSummaryHeader tab={tab([entry({ status: 'passed' }), entry({ requestPath: 'b.yml', status: 'failed' })], 'done')} />);
    expect(screen.getByText(/1.*passed/i)).toBeInTheDocument();
    expect(screen.getByText(/1.*failed/i)).toBeInTheDocument();
  });

  it('shows a Start button when idle, disabled if nothing is included', () => {
    render(<RunnerSummaryHeader tab={tab([entry({ included: false })])} />);
    expect(screen.getByRole('button', { name: /start/i })).toBeDisabled();
  });

  it('clicking Start calls startRun with the tab id', () => {
    const runnerTab = tab([entry({})]);
    seedStore(runnerTab);
    render(<RunnerSummaryHeader tab={runnerTab} />);
    fireEvent.click(screen.getByRole('button', { name: /start/i }));
    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.runState).not.toBe('idle');
  });

  it('shows a Stop button while running, and clicking it calls stopRun', () => {
    const runnerTab = tab([entry({ status: 'running' })], 'running');
    seedStore(runnerTab);
    render(<RunnerSummaryHeader tab={runnerTab} />);
    fireEvent.click(screen.getByRole('button', { name: /stop/i }));
    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.runState).toBe('stopped');
  });

  it('shows a Re-run button once done', () => {
    render(<RunnerSummaryHeader tab={tab([entry({ status: 'passed' })], 'done')} />);
    expect(screen.getByRole('button', { name: /re-run/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/request/runner/RunnerSummaryHeader.test.tsx`
Expected: FAIL — the component file does not exist.

- [ ] **Step 3: Implement the component**

```tsx
// src/components/request/runner/RunnerSummaryHeader.tsx
import { Play, RotateCcw, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { getRunnerSummary } from '@/lib/runner-summary';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';

export function RunnerSummaryHeader({ tab }: { tab: RunnerTab }) {
  const startRun = usePaneStore((s) => s.startRun);
  const stopRun = usePaneStore((s) => s.stopRun);
  const rerunAll = usePaneStore((s) => s.rerunAll);

  const summary = getRunnerSummary(tab);
  const isRunning = tab.runState === 'running';
  const isFinished = tab.runState === 'done' || tab.runState === 'stopped';

  return (
    <div className='flex items-center gap-3 px-3 py-2 border-b shrink-0 text-sm'>
      <span className='text-muted-foreground'>
        {summary.total === 0 ? 'No requests' : `${summary.passed + summary.failed + summary.skipped} / ${summary.included} complete`}
      </span>
      {summary.passed > 0 && <span className='text-green-600 dark:text-green-400'>{summary.passed} passed</span>}
      {summary.failed > 0 && <span className='text-red-600 dark:text-red-400'>{summary.failed} failed</span>}
      {summary.skipped > 0 && <span className='text-muted-foreground'>{summary.skipped} skipped</span>}

      <div className='ml-auto flex items-center gap-2'>
        {isRunning ? (
          <Button size='sm' variant='outline' onClick={() => stopRun(tab.id)}>
            <Square className='h-3.5 w-3.5 mr-1.5' /> Stop
          </Button>
        ) : isFinished ? (
          <Button size='sm' onClick={() => void rerunAll(tab.id)}>
            <RotateCcw className='h-3.5 w-3.5 mr-1.5' /> Re-run
          </Button>
        ) : (
          <Button size='sm' disabled={summary.included === 0} onClick={() => void startRun(tab.id)}>
            <Play className='h-3.5 w-3.5 mr-1.5' /> Start
          </Button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/components/request/runner/RunnerSummaryHeader.test.tsx`
Expected: PASS (5 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/runner/RunnerSummaryHeader.tsx src/components/request/runner/RunnerSummaryHeader.test.tsx
git commit -m "feat: add RunnerSummaryHeader component"
```
