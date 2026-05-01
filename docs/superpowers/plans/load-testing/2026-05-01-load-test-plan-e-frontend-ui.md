# Enhanced Load Testing — Plan E: Frontend UI Components

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the full Load Test tab UI — `LoadTestTab`, `PhaseBuilder`, `LiveDashboard`, all 6 chart components, `RequestLogTable`, `ExportMenu` — and wire into `RequestTabs`.

**Architecture:** All load test UI lives under `src/components/request/load-test/`. Charts use `recharts` (already a dep). The tab is wired into the existing `RequestTabs` / `RequestTabContent` pattern. All components consume `useLoadTestStore` — no local state for run data.

**Tech Stack:** React 18, TypeScript, shadcn/ui, Lucide icons, recharts, Zustand

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

**Depends on:** Plan D complete

---

## File Map

| File | Change |
|---|---|
| `src/components/request/load-test/LoadTestTab.tsx` | New — root layout: sidebar + main area |
| `src/components/request/load-test/PhaseBuilder.tsx` | New — add/edit/remove/reorder phases |
| `src/components/request/load-test/LiveDashboard.tsx` | New — stat bar + progress + 4-chart grid + log |
| `src/components/request/load-test/StatBar.tsx` | New — 6 KPI cards |
| `src/components/request/load-test/LatencyChart.tsx` | New — p50/p95/p99 line chart |
| `src/components/request/load-test/ThroughputChart.tsx` | New — req/sec area chart |
| `src/components/request/load-test/ErrorRateChart.tsx` | New — % failed line chart |
| `src/components/request/load-test/ConcurrencyChart.tsx` | New — active concurrent area chart |
| `src/components/request/load-test/HistogramChart.tsx` | New — response time distribution bar chart |
| `src/components/request/load-test/RequestLogTable.tsx` | New — virtualised log table |
| `src/components/request/load-test/ExportMenu.tsx` | New — dropdown for 4 export formats |
| `src/components/request/RequestTabs.tsx` | Modify — add Load test tab trigger |
| `src/components/request/RequestTabContent.tsx` | Modify — render LoadTestTab |

---

## Chunk 1: LoadTestTab shell + PhaseBuilder

### Task 1: `LoadTestTab` and `PhaseBuilder`

**Files:**
- Create: `src/components/request/load-test/LoadTestTab.tsx`
- Create: `src/components/request/load-test/PhaseBuilder.tsx`

- [ ] **Step 1: Read how existing tabs are structured**

```bash
cat src/components/request/RequestTabs.tsx | head -60
grep -n "tabValue\|TabsContent\|TabsTrigger" src/components/request/RequestTabContent.tsx | head -20
```

Note the exact `value` strings used (e.g. `"params"`, `"headers"`) and how `RequestState` and `tabId` are passed to tab content components.

- [ ] **Step 2: Create `PhaseBuilder.tsx`**

Create `src/components/request/load-test/PhaseBuilder.tsx`:

```tsx
import { Plus, Trash2, ArrowUp, ArrowDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import type { LoadTestPhase, PhaseKind } from '@/lib/tauri-api';

interface PhaseBuilderProps {
  phases: LoadTestPhase[];
  onChange: (phases: LoadTestPhase[]) => void;
}

const PHASE_COLORS: Record<PhaseKind, string> = {
  RampUp: 'bg-[#7F77DD]',
  Hold: 'bg-[#1D9E75]',
  RampDown: 'bg-[#E24B4A]',
};

export function PhaseBuilder({ phases, onChange }: PhaseBuilderProps) {
  const update = (index: number, patch: Partial<LoadTestPhase>) => {
    onChange(phases.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };

  const remove = (index: number) => onChange(phases.filter((_, i) => i !== index));

  const moveUp = (index: number) => {
    if (index === 0) return;
    const next = [...phases];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    onChange(next);
  };

  const moveDown = (index: number) => {
    if (index === phases.length - 1) return;
    const next = [...phases];
    [next[index], next[index + 1]] = [next[index + 1], next[index]];
    onChange(next);
  };

  const addPhase = () =>
    onChange([...phases, { kind: 'Hold', durationSecs: 30, targetConcurrency: 10 }]);

  return (
    <div className="flex flex-col gap-2">
      {phases.map((phase, i) => (
        <div key={i} className="flex items-center gap-2 rounded-md border border-border/40 bg-background/60 p-2">
          <div className={`h-2 w-2 shrink-0 rounded-full ${PHASE_COLORS[phase.kind]}`} />
          <Select
            value={phase.kind}
            onValueChange={(v) => update(i, { kind: v as PhaseKind })}
          >
            <SelectTrigger className="h-7 w-24 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="RampUp">Ramp up</SelectItem>
              <SelectItem value="Hold">Hold</SelectItem>
              <SelectItem value="RampDown">Ramp down</SelectItem>
            </SelectContent>
          </Select>
          <Input
            type="number"
            min={1}
            value={phase.durationSecs}
            onChange={(e) => update(i, { durationSecs: Number(e.target.value) })}
            className="h-7 w-14 text-xs"
          />
          <span className="text-[10px] text-muted-foreground">s @</span>
          <Input
            type="number"
            min={0}
            value={phase.targetConcurrency}
            onChange={(e) => update(i, { targetConcurrency: Number(e.target.value) })}
            className="h-7 w-14 text-xs"
          />
          <div className="ml-auto flex gap-1">
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => moveUp(i)} disabled={i === 0}>
              <ArrowUp className="h-3 w-3" />
            </Button>
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => moveDown(i)} disabled={i === phases.length - 1}>
              <ArrowDown className="h-3 w-3" />
            </Button>
            <Button variant="ghost" size="icon" className="h-6 w-6 text-destructive" onClick={() => remove(i)}>
              <Trash2 className="h-3 w-3" />
            </Button>
          </div>
        </div>
      ))}
      <Button variant="ghost" size="sm" className="h-7 justify-start text-xs text-muted-foreground" onClick={addPhase}>
        <Plus className="mr-1 h-3 w-3" />
        Add phase
      </Button>
    </div>
  );
}
```

- [ ] **Step 3: Create `LoadTestTab.tsx`**

Create `src/components/request/load-test/LoadTestTab.tsx`:

```tsx
import { Play, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Separator } from '@/components/ui/separator';
import { useLoadTestStore } from '@/stores/load-test-store';
import type { RequestState } from '@/types/pane-types';
import { PhaseBuilder } from './PhaseBuilder';
import { LiveDashboard } from './LiveDashboard';
import { ExportMenu } from './ExportMenu';

interface LoadTestTabProps {
  request: RequestState;
  tabId: string;
}

export function LoadTestTab({ request, tabId }: LoadTestTabProps) {
  const {
    phases, setPhases,
    successStatusBelow, setSuccessStatusBelow,
    status, error,
    startTest, stopTest,
  } = useLoadTestStore();

  const isRunning = status === 'running';

  return (
    <div className="flex h-full min-h-0 overflow-hidden">
      {/* Sidebar */}
      <div className="flex w-52 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border/40 bg-background p-3">
        <div>
          <p className="mb-2 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Configuration</p>
          <div className="flex flex-col gap-2">
            <div>
              <Label className="text-[11px] text-muted-foreground">Success if status &lt;</Label>
              <Input
                type="number"
                value={successStatusBelow}
                onChange={(e) => setSuccessStatusBelow(Number(e.target.value))}
                className="mt-1 h-7 text-xs"
                disabled={isRunning}
              />
            </div>
          </div>
        </div>

        <Separator />

        <div>
          <p className="mb-2 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Ramp-up phases</p>
          <PhaseBuilder phases={phases} onChange={setPhases} />
        </div>

        {error && (
          <>
            <Separator />
            <p className="text-[11px] text-destructive">{error}</p>
          </>
        )}

        <div className="mt-auto flex flex-col gap-2">
          {!isRunning ? (
            <Button
              size="sm"
              className="w-full bg-[#533AB7] text-[#EEEDFE] hover:bg-[#7F77DD]"
              onClick={() => startTest(request, tabId)}
            >
              <Play className="mr-1 h-3 w-3" />
              Run load test
            </Button>
          ) : (
            <Button size="sm" variant="outline" className="w-full" onClick={stopTest}>
              <Square className="mr-1 h-3 w-3" />
              Stop
            </Button>
          )}
          <ExportMenu />
        </div>
      </div>

      {/* Main content */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <LiveDashboard />
      </div>
    </div>
  );
}
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -15
```

Fix any import errors — `Separator` is `@/components/ui/separator` (shadcn). If not installed: `yarn dlx shadcn@latest add separator`.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/load-test/
git commit -m "feat(frontend): add LoadTestTab shell and PhaseBuilder"
```

---

## Chunk 2: Charts + LiveDashboard

### Task 2: Chart components and `LiveDashboard`

**Files:**
- Create: `src/components/request/load-test/StatBar.tsx`
- Create: `src/components/request/load-test/LatencyChart.tsx`
- Create: `src/components/request/load-test/ThroughputChart.tsx`
- Create: `src/components/request/load-test/ErrorRateChart.tsx`
- Create: `src/components/request/load-test/ConcurrencyChart.tsx`
- Create: `src/components/request/load-test/HistogramChart.tsx`
- Create: `src/components/request/load-test/RequestLogTable.tsx`
- Create: `src/components/request/load-test/ExportMenu.tsx`
- Create: `src/components/request/load-test/LiveDashboard.tsx`

- [ ] **Step 1: Verify recharts is available**

```bash
grep '"recharts"' package.json
```

If missing: `yarn add recharts`. It is likely already present as it is used elsewhere.

- [ ] **Step 2: Create `StatBar.tsx`**

```tsx
import { useLoadTestStore } from '@/stores/load-test-store';

function Stat({ label, value, className = '' }: { label: string; value: string | number; className?: string }) {
  return (
    <div className="rounded-md bg-muted/50 px-3 py-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</p>
      <p className={`mt-0.5 text-sm font-medium ${className}`}>{value}</p>
    </div>
  );
}

export function StatBar() {
  const { latestSnapshot, status, result } = useLoadTestStore();
  const snap = latestSnapshot;
  const src = status === 'complete' && result ? result : snap;

  if (!src) {
    return (
      <div className="grid grid-cols-6 gap-2 border-b border-border/40 bg-background px-3 py-2">
        {['Completed', 'Succeeded', 'Failed', 'Req / sec', 'p95 latency', 'Elapsed'].map((l) => (
          <Stat key={l} label={l} value="—" />
        ))}
      </div>
    );
  }

  const isSnap = 'elapsedMs' in src;
  return (
    <div className="grid grid-cols-6 gap-2 border-b border-border/40 bg-background px-3 py-2">
      <Stat label="Completed"  value={isSnap ? (src as typeof snap)!.completed : (src as typeof result)!.total_requests} />
      <Stat label="Succeeded"  value={isSnap ? (src as typeof snap)!.succeeded : (src as typeof result)!.succeeded} className="text-[#1D9E75]" />
      <Stat label="Failed"     value={isSnap ? ((src as typeof snap)!.failedStatus + (src as typeof snap)!.failedTransport) : (src as typeof result)!.failed} className={(isSnap ? ((src as typeof snap)!.failedStatus + (src as typeof snap)!.failedTransport) : (src as typeof result)!.failed) > 0 ? 'text-destructive' : ''} />
      <Stat label="Req / sec"  value={isSnap ? (src as typeof snap)!.requestsPerSecond.toFixed(1) : (src as typeof result)!.requests_per_second.toFixed(1)} className="text-[#7F77DD]" />
      <Stat label="p95 latency" value={`${(isSnap ? (src as typeof snap)!.p95Ms : (src as typeof result)!.p95_latency_ms).toFixed(0)}ms`} />
      <Stat label="Elapsed"    value={isSnap ? `${((src as typeof snap)!.elapsedMs / 1000).toFixed(1)}s` : `${((src as typeof result)!.total_duration_ms / 1000).toFixed(1)}s`} />
    </div>
  );
}
```

- [ ] **Step 3: Create chart components**

Create `src/components/request/load-test/LatencyChart.tsx`:

```tsx
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, Legend } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function LatencyChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = timeSeries.map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1),
    p50: +p.p50Ms.toFixed(1),
    p95: +p.p95Ms.toFixed(1),
    p99: +p.p99Ms.toFixed(1),
  }));
  return (
    <div className="flex h-full flex-col">
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">Latency over time (ms)</p>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey="t" tick={{ fontSize: 10 }} interval="preserveStartEnd" />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Legend iconSize={8} wrapperStyle={{ fontSize: 10 }} />
          <Line type="monotone" dataKey="p50" stroke="#7F77DD" dot={false} strokeWidth={1.5} name="p50" />
          <Line type="monotone" dataKey="p95" stroke="#1D9E75" dot={false} strokeWidth={1.5} name="p95" />
          <Line type="monotone" dataKey="p99" stroke="#E24B4A" dot={false} strokeWidth={1.5} name="p99" strokeDasharray="3 2" />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
```

Create `src/components/request/load-test/ThroughputChart.tsx`:

```tsx
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ThroughputChart() {
  const data = useLoadTestStore((s) => s.timeSeries).map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1), rps: +p.rps.toFixed(1),
  }));
  return (
    <div className="flex h-full flex-col">
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">Throughput (req / sec)</p>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey="t" tick={{ fontSize: 10 }} interval="preserveStartEnd" />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Area type="monotone" dataKey="rps" stroke="#7F77DD" fill="rgba(127,119,221,0.12)" strokeWidth={1.5} dot={false} name="req/sec" />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
```

Create `src/components/request/load-test/ErrorRateChart.tsx`:

```tsx
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ErrorRateChart() {
  const data = useLoadTestStore((s) => s.timeSeries).map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1), err: +p.errorRatePct.toFixed(2),
  }));
  return (
    <div className="flex h-full flex-col">
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">Error rate (%)</p>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey="t" tick={{ fontSize: 10 }} interval="preserveStartEnd" />
          <YAxis tick={{ fontSize: 10 }} width={36} domain={[0, 100]} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Line type="monotone" dataKey="err" stroke="#E24B4A" dot={false} strokeWidth={1.5} name="% error" />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
```

Create `src/components/request/load-test/ConcurrencyChart.tsx`:

```tsx
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ConcurrencyChart() {
  const data = useLoadTestStore((s) => s.timeSeries).map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1), conc: p.activeConcurrent,
  }));
  return (
    <div className="flex h-full flex-col">
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">Active concurrent</p>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey="t" tick={{ fontSize: 10 }} interval="preserveStartEnd" />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Area type="monotone" dataKey="conc" stroke="#1D9E75" fill="rgba(29,158,117,0.12)" strokeWidth={1.5} dot={false} name="users" />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
```

Create `src/components/request/load-test/HistogramChart.tsx`:

```tsx
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

const BUCKETS = [0, 50, 100, 200, 500, 1000, 2000, 5000];

export function HistogramChart() {
  const requestLog = useLoadTestStore((s) => s.requestLog);
  const buckets = BUCKETS.map((lo, i) => {
    const hi = BUCKETS[i + 1] ?? Infinity;
    return {
      label: hi === Infinity ? `>${lo}` : `${lo}–${hi}`,
      count: requestLog.filter((e) => e.latencyMs >= lo && e.latencyMs < hi).length,
    };
  });
  return (
    <div className="flex h-full flex-col">
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">Response time distribution (ms)</p>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={buckets} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey="label" tick={{ fontSize: 9 }} />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Bar dataKey="count" fill="#7F77DD" name="requests" radius={[2, 2, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 4: Create `RequestLogTable.tsx`**

```tsx
import { useLoadTestStore } from '@/stores/load-test-store';

export function RequestLogTable() {
  const requestLog = useLoadTestStore((s) => s.requestLog);
  const visible = requestLog.slice(-50); // show last 50 for performance

  return (
    <div className="flex flex-col overflow-hidden">
      <div className="border-b border-border/40 bg-muted/30 px-3 py-1.5">
        <p className="text-[11px] font-medium text-muted-foreground">
          Per-request log
          <span className="ml-2 text-[10px] font-normal">showing last 50 of {requestLog.length}</span>
        </p>
      </div>
      <div className="overflow-y-auto">
        <table className="w-full text-[11px]">
          <thead className="sticky top-0 bg-muted/50">
            <tr>
              {['#', 'Status', 'Latency', 'Size', 'Phase', 'Error'].map((h) => (
                <th key={h} className="px-3 py-1.5 text-left font-medium text-muted-foreground">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.map((entry) => {
              const ok = entry.status !== null && entry.status < 400;
              return (
                <tr key={entry.seq} className="border-b border-border/20">
                  <td className="px-3 py-1 text-muted-foreground">{entry.seq}</td>
                  <td className={`px-3 py-1 font-medium ${ok ? 'text-[#1D9E75]' : 'text-destructive'}`}>
                    {entry.status ?? 'ERR'}
                  </td>
                  <td className="px-3 py-1">{entry.latencyMs.toFixed(1)}ms</td>
                  <td className="px-3 py-1">{entry.responseBytes}b</td>
                  <td className="px-3 py-1">{entry.phaseIndex}</td>
                  <td className="max-w-[200px] truncate px-3 py-1 text-muted-foreground">{entry.error ?? '—'}</td>
                </tr>
              );
            })}
            {visible.length === 0 && (
              <tr>
                <td colSpan={6} className="px-3 py-4 text-center text-muted-foreground">No requests yet</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Create `ExportMenu.tsx`**

```tsx
import { Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useLoadTestStore } from '@/stores/load-test-store';
import type { ExportFormat } from '@/lib/tauri-api';

const FORMATS: { label: string; value: ExportFormat }[] = [
  { label: 'HTML report', value: 'html' },
  { label: 'CSV (request log)', value: 'csv' },
  { label: 'JSON (full snapshot)', value: 'json' },
  { label: 'PDF summary', value: 'pdf' },
];

export function ExportMenu() {
  const { exportResult, status } = useLoadTestStore();
  const disabled = status !== 'complete';

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="w-full text-xs" disabled={disabled}>
          <Download className="mr-1 h-3 w-3" />
          Export
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {FORMATS.map((f) => (
          <DropdownMenuItem key={f.value} onSelect={() => exportResult(f.value)} className="text-xs">
            {f.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
```

- [ ] **Step 6: Create `LiveDashboard.tsx`**

```tsx
import { useLoadTestStore } from '@/stores/load-test-store';
import { StatBar } from './StatBar';
import { LatencyChart } from './LatencyChart';
import { ThroughputChart } from './ThroughputChart';
import { ErrorRateChart } from './ErrorRateChart';
import { ConcurrencyChart } from './ConcurrencyChart';
import { HistogramChart } from './HistogramChart';
import { RequestLogTable } from './RequestLogTable';

export function LiveDashboard() {
  const { status, timeSeries } = useLoadTestStore();
  const idle = status === 'idle' && timeSeries.length === 0;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <StatBar />

      {idle ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          Configure phases and click Run load test to start.
        </div>
      ) : (
        <>
          {/* 4-chart grid */}
          <div className="grid min-h-0 flex-1 grid-cols-2 grid-rows-2 gap-2 p-2 pb-0">
            <div className="rounded-md border border-border/40 bg-background p-2">
              <LatencyChart />
            </div>
            <div className="rounded-md border border-border/40 bg-background p-2">
              <ThroughputChart />
            </div>
            <div className="rounded-md border border-border/40 bg-background p-2">
              <ErrorRateChart />
            </div>
            <div className="rounded-md border border-border/40 bg-background p-2">
              <HistogramChart />
            </div>
          </div>

          {/* Request log — fixed height bottom panel */}
          <div className="h-36 shrink-0 overflow-hidden border-t border-border/40">
            <RequestLogTable />
          </div>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 7: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -15
```

Fix any import issues. If `DropdownMenu` components are missing: `yarn dlx shadcn@latest add dropdown-menu`.

- [ ] **Step 8: Commit**

```bash
git add src/components/request/load-test/
git commit -m "feat(frontend): add LiveDashboard, 5 chart components, RequestLogTable, ExportMenu"
```

---

## Chunk 3: Wire into RequestTabs

### Task 3: Add "Load test" tab to `RequestTabs` and `RequestTabContent`

**Files:**
- Modify: `src/components/request/RequestTabs.tsx`
- Modify: `src/components/request/RequestTabContent.tsx`

- [ ] **Step 1: Read existing tab values**

```bash
grep -n "TabsTrigger\|value=" src/components/request/RequestTabs.tsx | head -20
```

Note the exact string values for all existing tabs (`"params"`, `"headers"`, etc.) and the order they appear.

- [ ] **Step 2: Add "Load test" trigger to `RequestTabs.tsx`**

Find the row of `<TabsTrigger>` elements. Add a new one — position it after `"scripts"` and before `"contract"` (or at the end if "contract" doesn't exist yet):

```tsx
<TabsTrigger value="load-test" className="text-xs">
  Load test
</TabsTrigger>
```

- [ ] **Step 3: Add tab content to `RequestTabContent.tsx`**

Find where `TabsContent` elements are rendered. Add:

```tsx
import { LoadTestTab } from './load-test/LoadTestTab';

// Inside the JSX, add:
<TabsContent value="load-test" className="h-full min-h-0 overflow-hidden p-0 data-[state=inactive]:hidden">
  <LoadTestTab request={request} tabId={tabId} />
</TabsContent>
```

Verify the exact prop names (`request`, `tabId`) match what the parent passes — read the component signature at the top of `RequestTabContent.tsx` before making changes.

- [ ] **Step 4: TypeScript check + lint**

```bash
yarn tsc --noEmit && yarn check 2>&1 | tail -15
```

- [ ] **Step 5: Manual smoke test**

```bash
yarn tauri dev
```

1. Open any saved request.
2. Click the "Load test" tab — the sidebar with phases and the empty dashboard should appear.
3. Add/remove a phase — phase list updates correctly.
4. Click "Run load test" — status transitions to running, stat bar shows live numbers, charts start populating.
5. After completion, "Export" button enables — click HTML, a file should download.

- [ ] **Step 6: Final commit**

```bash
git add src/components/request/RequestTabs.tsx src/components/request/RequestTabContent.tsx
git commit -m "feat(frontend): wire LoadTestTab into RequestTabs — load test feature complete"
```
