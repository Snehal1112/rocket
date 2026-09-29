# Flow Async P1 — Repeat Until Frontend — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users turn on, edit and see Repeat until on Request nodes: a row on the node card, an attempt count after a run, a "Repeat until" section in the properties panel, and a "Poll request" palette entry.

**Architecture:** Pure frontend work over the types from plan 03 and the `attempts` field from plan 04. `FlowToolbar` copies `attempts` into `FlowNodeDetail`, which `FlowCanvas.toRfNodes` already spreads into node data. `RequestNode` renders a read-only repeat row and a richer success line. A new `RepeatUntilSection` component, mounted in `RequestNodeEditor`, edits the setting. `NodePalette` gets one more entry.

**Tech Stack:** React + TypeScript, shadcn/ui (`Switch`, `Input`, `Label`, `DropdownMenuItem`), lucide-react (`Repeat`), `SingleLineEditor` (CodeMirror), Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` (§6.5). Index and locked contract: `docs/superpowers/plans/flow-async/00-index.md`.

**Depends on:** plan 03 (`RepeatUntil` TS type, `DEFAULT_REPEAT_UNTIL`, `msToSecondsLabel` in `src/lib/flow-repeat.ts`), plan 04 (`attempts` on step results and events), plan 02 (running-node progress text; this plan does not touch it).

## Global Constraints

- shadcn/ui primitives only: no raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`.
- Icons from `lucide-react` only (`Repeat`). No inline SVG.
- The condition is edited with `SingleLineEditor`, like the If node's condition. Never Monaco for this single-line field.
- Zustand: no full destructuring of store state at component top level.
- Interval and timeout are shown and edited in seconds and stored in milliseconds.
- Card row text: `until <condition> · <interval> · max <N>` after a `Repeat` icon. Success line with attempts: `✓ <status> · <n> attempts · <seconds>`.
- Checks: `yarn test src/components/flow src/lib`, `yarn tsc --noEmit`, `yarn check`.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill.

## Review Focus

1. **A poll that met its condition on attempt 1.** Expected: the success line says `1 attempt`, not `1 attempts`. Pinned in Task 1 (`says 1 attempt in the singular`).
2. **A long condition on the card.** Expected: the row truncates and shows the full condition as a tooltip (`title`), and the card keeps its width. Pinned in Task 1 (`truncates a long condition and keeps it in the title`).
3. **The user clears the interval field while typing.** Expected: an empty or non-numeric value is ignored, so the stored value never becomes `NaN` or `0`. Pinned in Task 2 (`ignores an empty interval while typing`).
4. **Turning Repeat until off and on again.** Expected: turning it back on starts from the defaults and the saved node has `repeatUntil: null` while off. Pinned in Task 2 (`turns off to null and back on to the defaults`).
5. **A Request node without `repeatUntil` (every existing flow).** Expected: no repeat row and the plain `✓ 200 · 184ms` line. Pinned in Task 1 (`shows no repeat row for a plain request`).

---

### Task 1: Attempts in node detail, repeat row and success line on the card

**Files:**
- Modify: `src/lib/tauri-api.ts` (`FlowStepResult` :1822-1836, `FlowStepCompletedEvent` :1910-1925)
- Modify: `src/types/pane-types.ts:156-165` (`FlowNodeDetail`)
- Modify: `src/components/flow/FlowToolbar.tsx:39-60` (`detailFromEvent`, `detailFromStep`)
- Modify: `src/components/flow/nodes/RequestNode.tsx` (`RequestNodeData` :11-24, success line :75-79, field rows after the Body row :128-138)
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: `RepeatUntil` on the Request kind, `msToSecondsLabel` (plan 03); `attempts` on `FlowStepResult` and `flow-step-completed` (plan 04).
- Produces: `FlowStepResult.attempts?: number`, `FlowStepCompletedEvent.attempts?: number`, `FlowNodeDetail.attempts?: number`, `RequestNodeData.attempts?: number`; card row `data-testid='request-node-repeat-row'`.

- [ ] **Step 1: Write the failing card tests**

Append to the `describe('RequestNode', …)` block in `RequestNode.test.tsx`:

```tsx
  const pollingKind = {
    ...baseKind,
    repeatUntil: {
      condition: 'response.body.status === "done"',
      intervalMs: 2000,
      maxAttempts: 30,
      timeoutMs: 60000,
    },
  };

  it('shows the repeat-until row for a polling request', () => {
    renderNode({ kind: pollingKind, status: 'idle' });
    expect(screen.getByTestId('request-node-repeat-row')).toHaveTextContent(
      'until response.body.status === "done" · 2s · max 30',
    );
  });

  it('shows no repeat row for a plain request', () => {
    renderNode({ kind: baseKind, status: 'success', statusCode: 200, durationMs: 184 });
    expect(screen.queryByTestId('request-node-repeat-row')).toBeNull();
    expect(screen.getByText('✓ 200 · 184ms')).toBeInTheDocument();
  });

  it('truncates a long condition and keeps it in the title', () => {
    const condition = `response.body.${'x'.repeat(200)} === "done"`;
    renderNode({ kind: { ...pollingKind, repeatUntil: { ...pollingKind.repeatUntil, condition } }, status: 'idle' });
    const row = screen.getByTestId('request-node-repeat-row');
    expect(row).toHaveAttribute('title', condition);
    expect(row.querySelector('.truncate')).not.toBeNull();
  });

  it('shows the attempt count and total time after a poll', () => {
    renderNode({
      kind: pollingKind,
      status: 'success',
      statusCode: 200,
      durationMs: 14200,
      attempts: 7,
    });
    expect(screen.getByText('✓ 200 · 7 attempts · 14.2s')).toBeInTheDocument();
  });

  it('says 1 attempt in the singular', () => {
    renderNode({
      kind: pollingKind,
      status: 'success',
      statusCode: 200,
      durationMs: 300,
      attempts: 1,
    });
    expect(screen.getByText('✓ 200 · 1 attempt · 0.3s')).toBeInTheDocument();
  });
```

If the existing success line renders its parts in separate text nodes so `getByText` with the full string does not match, match with a function: `screen.getByText((_, el) => el?.textContent === '✓ 200 · 184ms')`.

- [ ] **Step 2: Write the failing toolbar test**

Add to `FlowToolbar.test.tsx`:

```tsx
  it('forwards attempts from the step event and the summary', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'job',
      status: 'success',
      status_code: 200,
      duration_ms: 14200,
      error: null,
      value: null,
      attempts: 7,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'job',
      'success',
      expect.objectContaining({ attempts: 7 }),
    );

    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'job',
          status: 'success',
          statusCode: 200,
          durationMs: 14200,
          error: null,
          value: null,
          attempts: 7,
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'job',
        'success',
        expect.objectContaining({ attempts: 7 }),
      ),
    );
  });
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/RequestNode.test.tsx src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: FAIL. The repeat row is not found, the attempts line is not found, and `attempts` is missing from the patched detail. `yarn tsc --noEmit` also reports `attempts` as an unknown property.

- [ ] **Step 4: Implement the types and detail mapping**

`src/lib/tauri-api.ts`, in `FlowStepResult` after `debugRequest`:

```ts
  /** How many times a repeat-until Request node sent its request. */
  attempts?: number;
```

and in `FlowStepCompletedEvent` after `debug_request`:

```ts
  /** How many times a repeat-until Request node sent its request. */
  attempts?: number;
```

`src/types/pane-types.ts`, in `FlowNodeDetail`:

```ts
  /** Attempts a repeat-until Request node made. */
  attempts?: number;
```

`src/components/flow/FlowToolbar.tsx`: add `attempts: event.attempts ?? undefined,` to `detailFromEvent` and `attempts: step.attempts ?? undefined,` to `detailFromStep`.

- [ ] **Step 5: Implement the card changes**

In `RequestNode.tsx`:
- Import `Repeat` from `lucide-react` (next to `Bug`) and `msToSecondsLabel` from `@/lib/flow-repeat`.
- Add to `RequestNodeData`:

```ts
  /** Attempts a repeat-until run made. Set after a run. */
  attempts?: number;
```

- Replace the success line with:

```tsx
      {status === 'success' && (
        <div className='px-2 pt-1 text-green-600'>
          {data.attempts === undefined
            ? `✓ ${statusCode} · ${durationMs}ms`
            : `✓ ${statusCode} · ${data.attempts} ${data.attempts === 1 ? 'attempt' : 'attempts'} · ${msToSecondsLabel(durationMs ?? 0)}`}
        </div>
      )}
```

- After the Body row, still inside the field-rows container, add:

```tsx
        {/* Repeat until has no handle: it is a setting, not an input. */}
        {kind.repeatUntil && (
          <div
            data-testid='request-node-repeat-row'
            title={kind.repeatUntil.condition}
            className='flex min-w-0 items-center gap-1.5 pl-2 text-muted-foreground'
          >
            <Repeat className='h-3 w-3 shrink-0' aria-hidden='true' />
            <span className='truncate'>
              until {kind.repeatUntil.condition} · {msToSecondsLabel(kind.repeatUntil.intervalMs)} ·
              max {kind.repeatUntil.maxAttempts}
            </span>
          </div>
        )}
```

If Biome reflows the JSX text so the rendered text gains or loses a space, adjust until `toHaveTextContent('until … · 2s · max 30')` passes; the test is the spec.

- [ ] **Step 6: Run the checks to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 7: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): show repeat-until settings and attempts on Request nodes`.

---

### Task 2: Repeat until section in the properties panel

**Files:**
- Create: `src/components/flow/properties/RepeatUntilSection.tsx`
- Modify: `src/components/flow/properties/RequestNodeEditor.tsx` (after the Debug mode block, before the "Source:" line)
- Test: `src/components/flow/properties/__tests__/RepeatUntilSection.test.tsx`, `src/components/flow/properties/__tests__/RequestNodeEditor.test.tsx`

**Interfaces:**
- Consumes: `RepeatUntil`, `DEFAULT_REPEAT_UNTIL` (plan 03).
- Produces: `export function RepeatUntilSection({ value, onChange }: { value: RepeatUntil | null; onChange: (value: RepeatUntil | null) => void })`.

- [ ] **Step 1: Write the failing section tests**

Create `src/components/flow/properties/__tests__/RepeatUntilSection.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { RepeatUntil } from '@/lib/tauri-api';
import { RepeatUntilSection } from '../RepeatUntilSection';

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

const on: RepeatUntil = {
  condition: 'response.body.done',
  intervalMs: 1500,
  maxAttempts: 10,
  timeoutMs: 30000,
};

describe('RepeatUntilSection', () => {
  it('hides the fields while off and turns on with the defaults', async () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={null} onChange={onChange} />);
    expect(screen.queryByLabelText('Repeat condition')).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenCalledWith(DEFAULT_REPEAT_UNTIL);
  });

  it('shows the settings in seconds', () => {
    render(<RepeatUntilSection value={on} onChange={vi.fn()} />);
    expect(screen.getByLabelText('Repeat condition')).toHaveValue('response.body.done');
    expect(screen.getByLabelText('Interval (s)')).toHaveValue(1.5);
    expect(screen.getByLabelText('Max attempts')).toHaveValue(10);
    expect(screen.getByLabelText('Timeout (s)')).toHaveValue(30);
  });

  it('edits the condition', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Repeat condition'), {
      target: { value: 'response.status === 200' },
    });
    expect(onChange).toHaveBeenCalledWith({ ...on, condition: 'response.status === 200' });
  });

  it('stores seconds as milliseconds', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Interval (s)'), { target: { value: '0.5' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, intervalMs: 500 });
    fireEvent.change(screen.getByLabelText('Timeout (s)'), { target: { value: '90' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, timeoutMs: 90000 });
    fireEvent.change(screen.getByLabelText('Max attempts'), { target: { value: '4' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, maxAttempts: 4 });
  });

  it('ignores an empty interval while typing', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Interval (s)'), { target: { value: '' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('turns off to null and back on to the defaults', async () => {
    const onChange = vi.fn();
    const { rerender } = render(<RepeatUntilSection value={on} onChange={onChange} />);
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenLastCalledWith(null);
    rerender(<RepeatUntilSection value={null} onChange={onChange} />);
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenLastCalledWith(DEFAULT_REPEAT_UNTIL);
  });
});
```

Add to `RequestNodeEditor.test.tsx`:

```tsx
  it('turns on repeat until for the node', async () => {
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenCalledWith({
      ...savedKind,
      repeatUntil: {
        condition: 'response.status === 200',
        intervalMs: 2000,
        maxAttempts: 30,
        timeoutMs: 60000,
      },
    });
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties`
Expected: FAIL, "Failed to resolve import '../RepeatUntilSection'" and no "Repeat until" switch in the editor.

- [ ] **Step 3: Implement `RepeatUntilSection`**

Create `src/components/flow/properties/RepeatUntilSection.tsx`:

```tsx
import { SingleLineEditor } from '@/components/editor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { RepeatUntil } from '@/lib/tauri-api';

// Returns the number typed into a field, or null while it is empty or not a number.
function parsed(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isFinite(n) ? n : null;
}

export function RepeatUntilSection({
  value,
  onChange,
}: {
  value: RepeatUntil | null;
  onChange: (value: RepeatUntil | null) => void;
}) {
  const setSeconds = (field: 'intervalMs' | 'timeoutMs', text: string) => {
    const seconds = parsed(text);
    if (seconds === null || value === null) return;
    onChange({ ...value, [field]: Math.round(seconds * 1000) });
  };

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between gap-2'>
        <Label htmlFor='request-repeat-until'>Repeat until</Label>
        <Switch
          id='request-repeat-until'
          checked={value !== null}
          onCheckedChange={(checked) => onChange(checked ? { ...DEFAULT_REPEAT_UNTIL } : null)}
        />
      </div>
      <p className='text-xs text-muted-foreground'>
        Sends the request again until the condition is true. The node fails if the condition is
        still false after the last attempt or the timeout.
      </p>
      {value !== null && (
        <div className='space-y-2'>
          <div className='space-y-1'>
            <span className='text-xs font-medium'>Condition</span>
            <SingleLineEditor
              aria-label='Repeat condition'
              value={value.condition}
              onChange={(condition) => onChange({ ...value, condition })}
              placeholder='response.status === 200'
              className='text-xs'
            />
          </div>
          <div className='grid grid-cols-3 gap-2'>
            <div className='space-y-1'>
              <Label htmlFor='repeat-interval' className='text-xs'>
                Interval (s)
              </Label>
              <Input
                id='repeat-interval'
                type='number'
                min={0.1}
                step={0.1}
                value={value.intervalMs / 1000}
                onChange={(e) => setSeconds('intervalMs', e.target.value)}
                className='h-8 text-xs'
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='repeat-max-attempts' className='text-xs'>
                Max attempts
              </Label>
              <Input
                id='repeat-max-attempts'
                type='number'
                min={1}
                max={1000}
                step={1}
                value={value.maxAttempts}
                onChange={(e) => {
                  const n = parsed(e.target.value);
                  if (n !== null) onChange({ ...value, maxAttempts: Math.round(n) });
                }}
                className='h-8 text-xs'
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='repeat-timeout' className='text-xs'>
                Timeout (s)
              </Label>
              <Input
                id='repeat-timeout'
                type='number'
                min={1}
                max={3600}
                step={1}
                value={value.timeoutMs / 1000}
                onChange={(e) => setSeconds('timeoutMs', e.target.value)}
                className='h-8 text-xs'
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
```

The section does not validate limits. Save-time validation (plan 03, rule V9) rejects out-of-range values with a message naming the node, like every other flow validation error.

- [ ] **Step 4: Mount it in `RequestNodeEditor`**

Import `RepeatUntilSection` and add, directly after the Debug mode `<div className='space-y-1'>…</div>` block:

```tsx
      <RepeatUntilSection
        value={kind.repeatUntil ?? null}
        onChange={(repeatUntil) => onChange({ ...kind, repeatUntil })}
      />
```

The panel root already carries `nokey`, so typing in these fields cannot delete the node. Check with the existing panel focus tests that nothing else is needed.

- [ ] **Step 5: Run the checks to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): edit repeat-until in the properties panel`.

---

### Task 3: "Poll request" palette entry

**Files:**
- Modify: `src/components/flow/NodePalette.tsx` (imports :1, new item after "Inline Request" :55-70)
- Modify: `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` (§6.5 Palette bullet)
- Test: `src/components/flow/__tests__/NodePalette.test.tsx`

**Interfaces:**
- Consumes: `DEFAULT_REPEAT_UNTIL` (plan 03).
- Produces: a palette item named "Poll request" that calls `onAddNode` with an inline Request node, label `New Poll`, id prefix `request-`, `repeatUntil` equal to the defaults.

- [ ] **Step 1: Write the failing test**

Add to `NodePalette.test.tsx`:

```tsx
  it('adds a Poll request node with repeat-until turned on', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Poll request' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^request-/);
    expect(node.kind).toEqual({
      kind: 'Request',
      label: 'New Poll',
      source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      repeatUntil: {
        condition: 'response.status === 200',
        intervalMs: 2000,
        maxAttempts: 30,
        timeoutMs: 60000,
      },
    });
  });
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/NodePalette.test.tsx`
Expected: FAIL, "Unable to find role menuitem with name 'Poll request'".

- [ ] **Step 3: Implement the entry**

In `NodePalette.tsx`, add `Repeat` to the `lucide-react` import and `import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';`. After the "Inline Request" item add:

```tsx
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('request'),
                kind: {
                  kind: 'Request',
                  label: 'New Poll',
                  source: {
                    type: 'Inline',
                    request: { method: 'GET', url: '', headers: [] },
                  },
                  repeatUntil: { ...DEFAULT_REPEAT_UNTIL },
                },
                position: defaultPosition,
              })
            }
          >
            <Repeat className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Poll request
          </DropdownMenuItem>
```

- [ ] **Step 4: Run the checks to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 5: Update the spec's palette wording**

The palette has no request picker: saved requests reach the canvas by drag and drop, and the palette only adds inline requests. In `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §6.5, replace the **Palette** bullet with:

"- **Palette:** a "Poll request" entry that adds an inline Request node labelled "New Poll" with `repeat_until` set to the defaults (condition `response.status === 200`). The user sets its URL in the properties panel or points it at a saved request there with "Use a saved request…"."

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): add a Poll request palette entry`.

- [ ] **Step 7: Manual check (P1 done)**

Run `yarn tauri dev`. Build a flow with a Poll request pointed at a local endpoint that answers `{"status":"pending"}` twice and then `{"status":"done"}`, with the condition `response.body.status === "done"` and a 1 s interval. Run it. Expected: the node shows `attempt 1/30`, `attempt 2/30`, `attempt 3/30` while running (plan 02), then `✓ 200 · 3 attempts · 2.0s` (roughly), and History has one entry for the poll. Press Stop during a run with a long interval: the node fails with "cancelled" within a second.
