# Load Test Tab — Sidebar Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix PhaseBuilder overflow in the 208px sidebar and make the sidebar freely resizable via a drag handle.

**Architecture:** Two isolated changes in two files. `PhaseBuilder.tsx` gets a new card-per-phase layout with HTML5 drag-to-reorder replacing the up/down buttons. `LoadTestTab.tsx` gets a drag handle divider and ref-based width management (no React state, direct DOM writes for zero-lag resize), with width persisted to `localStorage`.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, shadcn/ui (Button, Input, Select, Card, CardContent, Label), Lucide React

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-sidebar-redesign.md`

---

## File Map

| File | Change |
|---|---|
| `src/components/request/load-test/PhaseBuilder.tsx` | Rewrite — grid card layout per phase, HTML5 drag-to-reorder, remove up/down buttons |
| `src/components/request/load-test/LoadTestTab.tsx` | Add `asideRef`, sidebar width init from localStorage, drag handle element, `handleResizeStart` handler |

---

## Task 1: Redesign PhaseBuilder — grid card layout with drag-to-reorder

**Files:**
- Modify: `src/components/request/load-test/PhaseBuilder.tsx`

- [ ] **Step 1: Read the current PhaseBuilder**

```bash
cat src/components/request/load-test/PhaseBuilder.tsx
```

Confirm: rows use inline flex with ArrowUp/ArrowDown/Trash2 icons — these are the overflow culprits.

- [ ] **Step 2: Replace the full file content**

```tsx
import { GripVertical, Plus, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { LoadTestPhase, PhaseKind } from '@/lib/tauri-api';

interface Props {
  phases: LoadTestPhase[];
  onChange: (phases: LoadTestPhase[]) => void;
  disabled?: boolean;
}

const KIND_COLORS: Record<PhaseKind, string> = {
  RampUp: 'hsl(var(--chart-4))',
  Hold: 'hsl(var(--chart-2))',
  RampDown: 'hsl(var(--destructive))',
};

export function PhaseBuilder({ phases, onChange, disabled }: Props) {
  const dragIndex = useRef<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);

  const update = (index: number, patch: Partial<LoadTestPhase>) => {
    onChange(phases.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };

  const remove = (index: number) => {
    onChange(phases.filter((_, i) => i !== index));
  };

  const addPhase = () => {
    onChange([...phases, { kind: 'Hold', durationSecs: 30, targetConcurrency: 10 }]);
  };

  const handleDragStart = (index: number) => {
    dragIndex.current = index;
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    setDragOverIndex(index);
  };

  const handleDrop = (dropIndex: number) => {
    const from = dragIndex.current;
    if (from === null || from === dropIndex) {
      dragIndex.current = null;
      setDragOverIndex(null);
      return;
    }
    const next = phases.slice();
    const [moved] = next.splice(from, 1);
    next.splice(dropIndex, 0, moved);
    onChange(next);
    dragIndex.current = null;
    setDragOverIndex(null);
  };

  const handleDragEnd = () => {
    dragIndex.current = null;
    setDragOverIndex(null);
  };

  return (
    <div className='flex flex-col gap-2'>
      {phases.map((phase, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: phases are ordered and edited in place.
        <div
          key={i}
          draggable={!disabled}
          onDragStart={() => handleDragStart(i)}
          onDragOver={(e) => handleDragOver(e, i)}
          onDrop={() => handleDrop(i)}
          onDragEnd={handleDragEnd}
          className={`rounded-md border bg-muted/30 p-2 transition-all ${
            dragOverIndex === i ? 'ring-1 ring-primary' : 'border-border/60'
          }`}
        >
          {/* Top row: grip + kind selector + delete */}
          <div className='mb-2 flex items-center gap-1.5'>
            <GripVertical className='h-3 w-3 shrink-0 cursor-grab text-muted-foreground' />
            <span
              className='h-2 w-2 shrink-0 rounded-full'
              style={{ backgroundColor: KIND_COLORS[phase.kind] }}
            />
            <Select
              value={phase.kind}
              onValueChange={(v) => update(i, { kind: v as PhaseKind })}
              disabled={disabled}
            >
              <SelectTrigger className='h-6 flex-1 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='RampUp'>Ramp up</SelectItem>
                <SelectItem value='Hold'>Hold</SelectItem>
                <SelectItem value='RampDown'>Ramp down</SelectItem>
              </SelectContent>
            </Select>
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 text-destructive hover:text-destructive'
              onClick={() => remove(i)}
              disabled={disabled}
              aria-label='Remove phase'
            >
              <Trash2 className='h-3 w-3' />
            </Button>
          </div>

          {/* Bottom row: duration + concurrency inputs */}
          <div className='grid grid-cols-2 gap-2'>
            <div className='flex flex-col gap-1'>
              <Label className='text-[10px] uppercase tracking-wider text-muted-foreground'>
                Duration (s)
              </Label>
              <Input
                type='number'
                min={1}
                value={phase.durationSecs}
                onChange={(e) => update(i, { durationSecs: Number(e.target.value) })}
                disabled={disabled}
                className='h-6 text-xs'
                aria-label='Duration in seconds'
              />
            </div>
            <div className='flex flex-col gap-1'>
              <Label className='text-[10px] uppercase tracking-wider text-muted-foreground'>
                Concurrency
              </Label>
              <Input
                type='number'
                min={0}
                value={phase.targetConcurrency}
                onChange={(e) => update(i, { targetConcurrency: Number(e.target.value) })}
                disabled={disabled}
                className='h-6 text-xs'
                aria-label='Target concurrency'
              />
            </div>
          </div>
        </div>
      ))}

      <Button
        variant='ghost'
        size='sm'
        className='h-7 justify-start text-xs'
        onClick={addPhase}
        disabled={disabled}
      >
        <Plus className='mr-1 h-3.5 w-3.5' />
        Add phase
      </Button>
    </div>
  );
}
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors (or only errors in `LoadTestTab.tsx` where `disabled` prop is not yet passed — fixed in Task 2).

- [ ] **Step 4: Lint check**

```bash
yarn check 2>&1 | head -20
```

Expected: zero new warnings.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/load-test/PhaseBuilder.tsx
git commit -m "feat(load-test): redesign PhaseBuilder — grid card layout with drag-to-reorder"
```

---

## Task 2: Add resizable sidebar to LoadTestTab

**Files:**
- Modify: `src/components/request/load-test/LoadTestTab.tsx`

- [ ] **Step 1: Read the current LoadTestTab**

```bash
cat src/components/request/load-test/LoadTestTab.tsx
```

Confirm: `<aside>` has `className='flex w-52 shrink-0 ...'` — the `w-52` (208px) is what we're replacing.

- [ ] **Step 2: Replace the full file content**

```tsx
import { Activity, GripVertical, Play, ShieldCheck, Square } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useLoadTestStore } from '@/stores/load-test-store';
import type { RequestState } from '@/types/pane-types';
import { ExportMenu } from './ExportMenu';
import { LiveDashboard } from './LiveDashboard';
import { PhaseBuilder } from './PhaseBuilder';

const SIDEBAR_WIDTH_KEY = 'load-test-sidebar-width';
const DEFAULT_WIDTH = 260;
const MIN_WIDTH = 180;
const MAX_WIDTH = 480;

interface Props {
  request: RequestState;
  tabId: string;
}

function SectionHeader({ icon: Icon, label }: { icon: React.ElementType; label: string }) {
  return (
    <div className='flex items-center gap-2 mb-1'>
      <Icon className='h-3.5 w-3.5 text-muted-foreground' />
      <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
        {label}
      </span>
    </div>
  );
}

export function LoadTestTab({ request, tabId }: Props) {
  const phases = useLoadTestStore((s) => s.phases);
  const successStatusBelow = useLoadTestStore((s) => s.successStatusBelow);
  const status = useLoadTestStore((s) => s.status);
  const error = useLoadTestStore((s) => s.error);
  const setPhases = useLoadTestStore((s) => s.setPhases);
  const setSuccessStatusBelow = useLoadTestStore((s) => s.setSuccessStatusBelow);
  const startTest = useLoadTestStore((s) => s.startTest);
  const stopTest = useLoadTestStore((s) => s.stopTest);

  const isRunning = status === 'running';

  const asideRef = useRef<HTMLDivElement>(null);
  const sidebarWidth = useRef<number>(DEFAULT_WIDTH);
  const isDragging = useRef(false);

  // Apply persisted width on mount.
  useEffect(() => {
    const saved = localStorage.getItem(SIDEBAR_WIDTH_KEY);
    if (saved) {
      const w = Number(saved);
      if (w >= MIN_WIDTH && w <= MAX_WIDTH) {
        sidebarWidth.current = w;
        if (asideRef.current) asideRef.current.style.width = `${w}px`;
      }
    }
  }, []);

  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    isDragging.current = true;
    const startX = e.clientX;
    const startW = sidebarWidth.current;

    const onMove = (ev: MouseEvent) => {
      if (!isDragging.current) return;
      const next = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startW + ev.clientX - startX));
      sidebarWidth.current = next;
      if (asideRef.current) asideRef.current.style.width = `${next}px`;
    };

    const onUp = () => {
      isDragging.current = false;
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth.current));
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  return (
    <div className='flex h-full min-h-0 overflow-hidden'>
      <aside
        ref={asideRef}
        style={{ width: DEFAULT_WIDTH }}
        className='flex shrink-0 flex-col gap-3 overflow-y-auto bg-background p-3'
      >
        <Card>
          <CardContent className='p-3 space-y-2'>
            <SectionHeader icon={ShieldCheck} label='Success rule' />
            <div className='flex flex-col gap-1.5'>
              <Label htmlFor='success-status-below' className='text-xs'>
                Success if status &lt;
              </Label>
              <Input
                id='success-status-below'
                type='number'
                min={100}
                max={600}
                value={successStatusBelow}
                onChange={(e) => setSuccessStatusBelow(Number(e.target.value))}
                disabled={isRunning}
                className='h-7 text-xs'
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className='p-3 space-y-2'>
            <SectionHeader icon={Activity} label='Ramp-up phases' />
            <PhaseBuilder phases={phases} onChange={setPhases} disabled={isRunning} />
          </CardContent>
        </Card>

        {error && <p className='text-[11px] text-destructive px-1'>{error}</p>}

        <div className='mt-auto flex flex-col gap-2'>
          {isRunning ? (
            <Button variant='outline' className='w-full' onClick={() => stopTest()}>
              <Square className='mr-2 h-3.5 w-3.5' />
              Stop
            </Button>
          ) : (
            <Button
              className='w-full bg-chart-4 text-primary-foreground hover:bg-chart-4/80'
              onClick={() => startTest(request, tabId)}
            >
              <Play className='mr-2 h-3.5 w-3.5' />
              Run load test
            </Button>
          )}
          <ExportMenu />
        </div>
      </aside>

      {/* Drag handle */}
      <div
        className='w-[5px] shrink-0 cursor-col-resize border-l border-r border-border/40 bg-transparent hover:bg-border/40 transition-colors flex items-center justify-center'
        onMouseDown={handleResizeStart}
        aria-hidden='true'
      >
        <GripVertical className='h-4 w-4 text-border' />
      </div>

      <div className='flex min-w-0 flex-1 flex-col overflow-hidden'>
        <LiveDashboard />
      </div>
    </div>
  );
}
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors.

- [ ] **Step 4: Lint check**

```bash
yarn check 2>&1 | head -20
```

Expected: zero new warnings.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/load-test/LoadTestTab.tsx
git commit -m "feat(load-test): resizable sidebar with drag handle, default 260px, persisted to localStorage"
```

---

## Smoke Test Checklist

After both tasks complete, manually verify:

- [ ] Open a request — navigate to the Load Test tab — no horizontal overflow in the sidebar
- [ ] Phases show as grid cards: kind selector + dot + × on top row, Duration/Concurrency inputs below with labels
- [ ] Add a phase — new card appears, all inputs functional
- [ ] Remove a phase — card disappears
- [ ] Drag a phase card up/down — order changes correctly
- [ ] Dragging is disabled while a test is running (button shows Stop, inputs grayed out)
- [ ] Drag the handle between sidebar and dashboard — sidebar resizes smoothly
- [ ] Resize to minimum (~180px) and maximum (~480px) — clamps correctly
- [ ] Reload the app — sidebar width is restored from localStorage

---

## Milestone Checklist

- [ ] PhaseBuilder: no ArrowUp/ArrowDown buttons
- [ ] PhaseBuilder: each phase is a grid card with Duration (s) and Concurrency labels
- [ ] PhaseBuilder: drag-to-reorder with GripVertical indicator and ring-1 drop highlight
- [ ] PhaseBuilder: `disabled` prop disables all inputs, select, delete, drag, and add button
- [ ] LoadTestTab: `<aside>` has no `w-52` class, width controlled via `style`
- [ ] LoadTestTab: drag handle renders between sidebar and main panel
- [ ] LoadTestTab: width persisted to `localStorage` key `load-test-sidebar-width`
- [ ] LoadTestTab: width restored from localStorage on mount
- [ ] `yarn tsc --noEmit` passes clean
- [ ] `yarn check` passes clean
