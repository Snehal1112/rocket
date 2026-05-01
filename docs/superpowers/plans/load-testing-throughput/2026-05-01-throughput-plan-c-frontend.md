# Throughput-Target Phases — Plan C: Frontend (Types, Store, UI)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the frontend to (1) match the new `PhaseTarget` shape, (2) add a run-level "Workload type" toggle (Concurrent users / Throughput) to `LoadTestTab`, and (3) update charts and StatBar to display rate-related info when in RPS mode.

**Architecture:** A new `targetUnit: 'concurrency' | 'rps'` field on `useLoadTestStore` drives both the default phase shape and which phase-row input renders in `PhaseBuilder`. Switching the unit rewrites the phase array to sensible defaults for the new unit (we cannot reinterpret 25 users as 25 rps — they're not the same). The `ThroughputChart` adds a dashed "target" overlay when in RPS mode showing the configured rate over time. `StatBar` swaps "Active concurrent" → "Target req/sec" in RPS mode.

**Tech Stack:** TypeScript, Zustand, React, recharts, shadcn/ui.

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

**Depends on:** Plan A and Plan B complete.

---

## File Map

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Replace `LoadTestPhase.targetConcurrency` with `target: PhaseTarget` discriminated union |
| `src/stores/load-test-store.ts` | Add `targetUnit` field, default-phase rewriting on toggle, validation guard |
| `src/stores/__tests__/load-test-store.test.ts` | New tests for unit-toggle behavior |
| `src/components/request/load-test/LoadTestTab.tsx` | Add "Workload type" toggle in advanced sidebar |
| `src/components/request/load-test/PhaseBuilder.tsx` | Render concurrency input or rps input per row based on unit |
| `src/components/request/load-test/StatBar.tsx` | Swap last stat between "Concurrent" and "Target rps" |
| `src/components/request/load-test/ThroughputChart.tsx` | Add dashed target-rate overlay line in RPS mode |

---

## Chunk 1: Type updates

### Task 1: Update `tauri-api.ts` types to match Rust

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Find the existing types**

```bash
grep -n "PhaseKind\|LoadTestPhase\|targetConcurrency" src/lib/tauri-api.ts
```

Note the line numbers.

- [ ] **Step 2: Replace `LoadTestPhase` and add `PhaseTarget`**

Find the existing `LoadTestPhase` interface (~line 462) and replace it, plus add `PhaseTarget` and `TargetUnit` immediately above:

```typescript
export type TargetUnit = 'concurrency' | 'rps';

export type PhaseTarget =
  | { kind: 'concurrency'; value: number }
  | { kind: 'rps'; value: number };

export interface LoadTestPhase {
  kind: PhaseKind;
  durationSecs: number;
  target: PhaseTarget;
}
```

Note: `PhaseKind` (`'RampUp' | 'Hold' | 'RampDown'`) does not change.

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -25
```

Expected: errors at every site that references `phase.targetConcurrency`. List them — you'll fix each in subsequent tasks. Common sites:
- `src/stores/load-test-store.ts` (DEFAULT_PHASES)
- `src/components/request/load-test/PhaseBuilder.tsx`
- `src/stores/__tests__/load-test-store.test.ts` (if it constructs phases)

Don't commit yet — the codebase doesn't compile. Move to Task 2.

---

## Chunk 2: Store updates

### Task 2: Add `targetUnit` field and unit-toggle logic to `useLoadTestStore`

**Files:**
- Modify: `src/stores/load-test-store.ts`

- [ ] **Step 1: Add the new state and update defaults**

In `src/stores/load-test-store.ts`, find `DEFAULT_PHASES` (~line 30) and replace it:

```typescript
const DEFAULT_PHASES_CONCURRENCY: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, target: { kind: 'concurrency', value: 10 } },
  { kind: 'Hold', durationSecs: 30, target: { kind: 'concurrency', value: 10 } },
  { kind: 'RampDown', durationSecs: 10, target: { kind: 'concurrency', value: 0 } },
];

const DEFAULT_PHASES_RPS: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, target: { kind: 'rps', value: 50 } },
  { kind: 'Hold', durationSecs: 30, target: { kind: 'rps', value: 50 } },
  { kind: 'RampDown', durationSecs: 10, target: { kind: 'rps', value: 0 } },
];
```

Update the import block to include the new types:

```typescript
import {
  type ExportFormat,
  exportLoadTest,
  type LoadTestConfig,
  type LoadTestConfigV2,
  type LoadTestPhase,
  type LoadTestProgressEvent,
  type LoadTestResult,
  type RequestLogEntry,
  runLoadTest,
  runLoadTestV2,
  type TargetUnit,
  type TimeSeriesPoint,
} from '@/lib/tauri-api';
```

- [ ] **Step 2: Extend the `LoadTestState` interface**

Find the `interface LoadTestState` block. Add `targetUnit` and `setTargetUnit`:

```typescript
  // Advanced config
  targetUnit: TargetUnit;
  phases: LoadTestPhase[];
  successStatusBelow: number;
  ringBufferSize: number;

  // ...

  // Actions
  setMode: (mode: LoadTestMode) => void;
  setSimpleConfig: (patch: Partial<LoadTestConfig>) => void;
  setTargetUnit: (unit: TargetUnit) => void;
  setPhases: (phases: LoadTestPhase[]) => void;
```

- [ ] **Step 3: Initialize and implement `setTargetUnit`**

In the `create<LoadTestState>` body, replace the existing `phases:` initializer and add the new fields. The replacements look like:

```typescript
  targetUnit: 'concurrency',
  phases: DEFAULT_PHASES_CONCURRENCY,
```

And add the action:

```typescript
  setTargetUnit: (unit) => {
    // Switching unit replaces the phase array with safe defaults for the new
    // unit because numeric values are not interchangeable: 10 concurrent
    // users is not 10 req/sec.
    const phases = unit === 'rps' ? DEFAULT_PHASES_RPS : DEFAULT_PHASES_CONCURRENCY;
    set({ targetUnit: unit, phases });
  },
```

- [ ] **Step 4: Guard `startTest` against mixed-unit phases**

The user can edit phases freely in the UI. Defensive guard before calling `runLoadTestV2`: confirm all phases match `targetUnit`. Replace the existing `advancedConfig` block in `startTest`:

```typescript
    const targetUnit = get().targetUnit;
    const allMatch = get().phases.every((p) => p.target.kind === targetUnit);
    if (!allMatch) {
      if (safetyTimer) clearTimeout(safetyTimer);
      set({
        status: 'error',
        error: 'All phases must match the selected workload type. Reset phases or switch the toggle.',
      });
      get().stopTest();
      return;
    }

    const advancedConfig: LoadTestConfigV2 = {
      phases: get().phases,
      successRule: { statusBelow: get().successStatusBelow },
      ringBufferSize: get().ringBufferSize,
    };
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -20
```

Expected: errors moved to `PhaseBuilder.tsx` and the store test file. Store itself should now type-check.

---

## Chunk 3: Store tests

### Task 3: Test unit-toggle behavior

**Files:**
- Modify: `src/stores/__tests__/load-test-store.test.ts`

- [ ] **Step 1: Update any existing test that constructs `LoadTestPhase`**

```bash
grep -n "targetConcurrency\|target:" src/stores/__tests__/load-test-store.test.ts
```

For each `targetConcurrency: N`, replace with `target: { kind: 'concurrency', value: N }`.

- [ ] **Step 2: Add new tests for `setTargetUnit`**

Append:

```typescript
  it('setTargetUnit to rps replaces phases with rps defaults', () => {
    act(() => { useLoadTestStore.getState().setTargetUnit('rps'); });
    const state = useLoadTestStore.getState();
    expect(state.targetUnit).toBe('rps');
    expect(state.phases.length).toBeGreaterThan(0);
    expect(state.phases.every((p) => p.target.kind === 'rps')).toBe(true);
  });

  it('setTargetUnit back to concurrency replaces phases again', () => {
    act(() => { useLoadTestStore.getState().setTargetUnit('rps'); });
    act(() => { useLoadTestStore.getState().setTargetUnit('concurrency'); });
    const state = useLoadTestStore.getState();
    expect(state.targetUnit).toBe('concurrency');
    expect(state.phases.every((p) => p.target.kind === 'concurrency')).toBe(true);
  });
```

- [ ] **Step 3: Run tests**

```bash
yarn test src/stores/__tests__/load-test-store.test.ts --run 2>&1 | tail -15
```

Expected: all tests pass (existing 8 + 2 new = 10).

If existing tests reference `targetConcurrency`, fix them per Step 1.

- [ ] **Step 4: Commit (after Task 4, since the codebase still doesn't compile end-to-end)**

Hold the commit until PhaseBuilder is updated — see Task 4.

---

## Chunk 4: PhaseBuilder UI

### Task 4: Render concurrency or rps input per phase row

**Files:**
- Modify: `src/components/request/load-test/PhaseBuilder.tsx`

- [ ] **Step 1: Read the current file**

```bash
cat src/components/request/load-test/PhaseBuilder.tsx
```

Note the existing structure: each phase row has a kind-select, a duration input, and a `targetConcurrency` input.

- [ ] **Step 2: Replace `targetConcurrency` references with the new `target` shape**

In `PhaseBuilder.tsx`:

**2a.** Update the `addPhase` default (currently produces `{ kind: 'Hold', durationSecs: 30, targetConcurrency: 10 }`). Accept a unit prop and produce the right default. First, change the component signature:

```tsx
import type { LoadTestPhase, PhaseKind, TargetUnit } from '@/lib/tauri-api';

interface Props {
  phases: LoadTestPhase[];
  onChange: (phases: LoadTestPhase[]) => void;
  disabled?: boolean;
  unit: TargetUnit;
}
```

**2b.** Update `addPhase`:

```tsx
const addPhase = () => {
  const target = unit === 'rps'
    ? { kind: 'rps' as const, value: 50 }
    : { kind: 'concurrency' as const, value: 10 };
  onChange([...phases, { kind: 'Hold', durationSecs: 30, target }]);
};
```

**2c.** Replace the row's number input. Find the input with `value={phase.targetConcurrency}` and replace its block with:

```tsx
<Input
  type='number'
  min={0}
  value={phase.target.value}
  onChange={(e) => {
    const value = Number(e.target.value);
    if (Number.isNaN(value)) return;
    update(i, {
      target: phase.target.kind === 'rps'
        ? { kind: 'rps', value }
        : { kind: 'concurrency', value },
    });
  }}
  disabled={disabled}
  className='h-7 w-16 text-xs'
/>
```

**2d.** Update the unit label next to the input. Find the text `users` (or the label currently shown next to the count input) and replace with a unit-aware label:

```tsx
<span className='text-[10px] text-muted-foreground whitespace-nowrap'>
  {phase.target.kind === 'rps' ? 'req/sec' : 'users'}
</span>
```

If no such label exists today, add one inside the row immediately after the input.

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -10
```

Expected: clean (the `phases` array now has `target.value` everywhere, all type errors cleared).

- [ ] **Step 4: Commit Tasks 1–4 together**

```bash
git add src/lib/tauri-api.ts src/stores/load-test-store.ts \
        src/stores/__tests__/load-test-store.test.ts \
        src/components/request/load-test/PhaseBuilder.tsx
git commit -m "refactor(load-test): replace targetConcurrency with PhaseTarget discriminated union (concurrency | rps)"
```

---

## Chunk 5: LoadTestTab — workload-type toggle

### Task 5: Add the run-level toggle in the sidebar

**Files:**
- Modify: `src/components/request/load-test/LoadTestTab.tsx`

- [ ] **Step 1: Read the current file to find where the advanced-mode card is rendered**

```bash
grep -n "Ramp-up phases\|PhaseBuilder\|setPhases" src/components/request/load-test/LoadTestTab.tsx
```

Find the card containing `<PhaseBuilder ... />`. The new toggle goes inside this card, above the PhaseBuilder.

- [ ] **Step 2: Pull the new state out of the store**

Find the existing `const phases = useLoadTestStore(...)` block. Add:

```tsx
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const setTargetUnit = useLoadTestStore((s) => s.setTargetUnit);
```

- [ ] **Step 3: Render the toggle inside the "Ramp-up phases" card**

Find the JSX:

```tsx
            <SectionHeader icon={Activity} label='Ramp-up phases' />
            <PhaseBuilder phases={phases} onChange={setPhases} disabled={isRunning} />
```

Replace with:

```tsx
            <SectionHeader icon={Activity} label='Ramp-up phases' />

            {/* Workload type — chooses concurrency or rps for all phases */}
            <div className='flex rounded-md border border-border/60 p-0.5'>
              <button
                type='button'
                className={`flex-1 rounded py-1 text-[11px] font-medium transition-colors ${
                  targetUnit === 'concurrency'
                    ? 'bg-muted text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
                onClick={() => setTargetUnit('concurrency')}
                disabled={isRunning}
              >
                Concurrent users
              </button>
              <button
                type='button'
                className={`flex-1 rounded py-1 text-[11px] font-medium transition-colors ${
                  targetUnit === 'rps'
                    ? 'bg-muted text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
                onClick={() => setTargetUnit('rps')}
                disabled={isRunning}
              >
                Throughput
              </button>
            </div>

            <PhaseBuilder
              phases={phases}
              onChange={setPhases}
              disabled={isRunning}
              unit={targetUnit}
            />
```

- [ ] **Step 4: TypeScript + biome check**

```bash
yarn tsc --noEmit 2>&1 | tail -10
yarn check src/components/request/load-test/LoadTestTab.tsx 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/load-test/LoadTestTab.tsx
git commit -m "feat(load-test): add workload type toggle (concurrent users / throughput) in advanced sidebar"
```

---

## Chunk 6: StatBar — swap last stat in RPS mode

### Task 6: Show "Target rps" instead of (effective) concurrency when in RPS mode

**Files:**
- Modify: `src/components/request/load-test/StatBar.tsx`

- [ ] **Step 1: Read current StatBar**

```bash
cat src/components/request/load-test/StatBar.tsx
```

Identify the six stats currently rendered. The existing layout shows: Completed, Succeeded, Failed, Req/sec, p95 latency, Elapsed. None of these mention concurrency directly, so the existing six are fine for both modes — but in RPS mode it's useful to show **target rps** alongside actual rps so the user can see whether the server is keeping up.

- [ ] **Step 2: Add a "Target rps" stat in RPS mode**

Pull `targetUnit`, `phases`, and `latestSnapshot` from the store, compute the current target rate from the active phase, and conditionally render an extra stat. Easiest: replace the "Req/sec" stat's value with a combined `actual / target` string in RPS mode.

In `StatBar.tsx`, add to the imports:

```tsx
import { useLoadTestStore } from '@/stores/load-test-store';
```

Inside the component body, after `const result = useLoadTestStore(...)`, add:

```tsx
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const phases = useLoadTestStore((s) => s.phases);
  const phaseIndex = latestSnapshot?.currentPhaseIndex ?? 0;
  const targetRps =
    targetUnit === 'rps' && phases[phaseIndex]?.target.kind === 'rps'
      ? phases[phaseIndex].target.value
      : null;
```

Then find the "Req / sec" `<Stat>` line and update its `value` prop:

```tsx
      <Stat
        label='Req / sec'
        value={targetRps !== null ? `${rps.toFixed(1)} / ${targetRps}` : rps.toFixed(1)}
        className='text-chart-4'
      />
```

The label stays "Req / sec"; the value becomes "12.3 / 50" in RPS mode (actual / target) and "12.3" in concurrency mode.

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/load-test/StatBar.tsx
git commit -m "feat(load-test): show actual/target req/sec in StatBar when in throughput mode"
```

---

## Chunk 7: ThroughputChart — target overlay

### Task 7: Render a dashed target-rate overlay in RPS mode

**Files:**
- Modify: `src/components/request/load-test/ThroughputChart.tsx`

- [ ] **Step 1: Read current chart**

```bash
cat src/components/request/load-test/ThroughputChart.tsx
```

Note how the data is shaped (`{ t, rps }`).

- [ ] **Step 2: Compute a target series and add a dashed line**

Replace the entire file body with:

```tsx
import { Area, AreaChart, Line, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

/** Returns the target rate (rps) at a given elapsed_ms timestamp. */
function targetRateAt(
  elapsedMs: number,
  phases: ReturnType<typeof useLoadTestStore.getState>['phases'],
): number | null {
  let boundaryMs = 0;
  let prevValue = 0;
  for (const phase of phases) {
    const phaseEndMs = boundaryMs + phase.durationSecs * 1000;
    if (phase.target.kind !== 'rps') return null;
    if (elapsedMs <= phaseEndMs) {
      // Linear interp for ramp phases; flat for hold.
      if (phase.kind === 'Hold') return phase.target.value;
      const progress = (elapsedMs - boundaryMs) / (phaseEndMs - boundaryMs);
      return Math.round(prevValue + (phase.target.value - prevValue) * progress);
    }
    boundaryMs = phaseEndMs;
    prevValue = phase.target.value;
  }
  return prevValue;
}

export function ThroughputChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const phases = useLoadTestStore((s) => s.phases);
  const showTarget = targetUnit === 'rps' && phases.every((p) => p.target.kind === 'rps');

  const data = timeSeries.map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1),
    rps: +p.rps.toFixed(1),
    target: showTarget ? targetRateAt(p.elapsedMs, phases) : null,
  }));

  return (
    <div className='flex h-full flex-col'>
      <p className='mb-1 text-[11px] font-medium text-muted-foreground'>Throughput (req / sec)</p>
      <ResponsiveContainer width='100%' height='100%'>
        <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey='t' tick={{ fontSize: 10 }} interval='preserveStartEnd' />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Area
            type='monotone'
            dataKey='rps'
            stroke='#7F77DD'
            fill='rgba(127,119,221,0.12)'
            strokeWidth={1.5}
            dot={false}
            name='actual'
          />
          {showTarget && (
            <Line
              type='monotone'
              dataKey='target'
              stroke='#1D9E75'
              strokeDasharray='4 3'
              strokeWidth={1.2}
              dot={false}
              name='target'
              isAnimationActive={false}
            />
          )}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 3: TypeScript check + lint**

```bash
yarn tsc --noEmit 2>&1 | tail -5
yarn check src/components/request/load-test/ThroughputChart.tsx 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/load-test/ThroughputChart.tsx
git commit -m "feat(load-test): overlay target rate on throughput chart in rps mode"
```

---

## Chunk 8: Manual smoke test

### Task 8: Verify end-to-end behavior in the running app

**Files:** none.

- [ ] **Step 1: Start the dev app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Open any saved request and switch to the Load test tab. Switch sidebar to Advanced mode.**

Expected: the "Ramp-up phases" card now shows a "Concurrent users / Throughput" toggle. Default selection: Concurrent users.

- [ ] **Step 3: Verify concurrency mode (regression check)**

Click "Run load test" with the default phases. Verify:
- Status transitions to running
- StatBar shows numeric values
- ThroughputChart shows only the actual (purple) area — no target overlay
- Test completes; result populates

- [ ] **Step 4: Verify rps mode**

Click the "Throughput" toggle. The phase list resets to RPS defaults (50 rps hold). Click Run.

Verify:
- StatBar's "Req / sec" stat shows "actual / target" (e.g. "47.2 / 50")
- ThroughputChart shows the dashed green target line alongside the purple actual area
- After completion, total request count is approximately `target_rps × duration_secs` (50 × 30 = ~1500 ± a few hundred for ramps)

- [ ] **Step 5: Verify the validation guard**

This case is hard to construct from the UI alone (the toggle keeps phases in sync), so just confirm by inspection that `setTargetUnit` rewrites all phases. No code change here.

- [ ] **Step 6: Commit nothing — this task is verification only.**

If you find a bug, return to whichever chunk owns the broken code, fix, and re-verify.

---

## Verification Gate

```bash
yarn tsc --noEmit
yarn check 2>&1 | grep -E "load-test|tauri-api|stores" | head -20
yarn test src/stores/__tests__/load-test-store.test.ts --run 2>&1 | tail -10
```

Expected: tsc clean, biome clean for files we touched (pre-existing repo errors are not our concern), all store tests pass.

Plan C is then complete. The throughput-target feature is end-to-end shipped.
