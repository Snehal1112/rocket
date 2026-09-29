# Flow Wire Script Editor — Plan 02: Frontend dialog, editing and logs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A centred Dialog with a Monaco JavaScript editor replaces the wire popover, existing wires reopen on double-click, and step logs from a run appear in the Console panel.

**Architecture:** `WireScriptDialog` replaces `WireExpressionPopover` and reuses FlowPane's `pendingEdge` state. `FlowCanvas` reports edge double-clicks. `FlowToolbar` hands each summary step's `logs` to FlowPane, which pushes them to `useConsoleStore`.

**Tech Stack:** React + TS, shadcn/ui `Dialog`, Monaco (`MonacoWrapper`), `@xyflow/react` 12, Zustand, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-wire-script-editor-design.md`

**Depends on:** Plan 01 (the `logs` field on `FlowStepResult` / `FlowStepCompleted`, JSON shape `logs: [{ level: 'log' | 'warn' | 'error', message: string }]`, omitted when empty).

## Global Constraints
- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`); lucide-react icons only.
- Multi-line editors are Monaco; single-line fields are `SingleLineEditor` or shadcn `Input`.
- Zustand: narrow selectors; never destructure a whole store.
- Everything portalled inside the canvas UI carries the `nokey` class, so React Flow's Backspace/Delete handler ignores it.
- Code comments: short full sentences ending with a period. Biome: 2 spaces, single quotes, trailing commas, 100 columns.
- Never run `yarn install` (node_modules is a symlink). Never run the whole Vitest suite; use `yarn test --run <pattern>`.
- Checks per task: `yarn test --run <patterns>`, `yarn tsc --noEmit`, `yarn check`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill (skip its subagent steps if you cannot dispatch subagents); conventional commits.

## Review Focus
1. Backspace/Delete while typing in the Monaco editor or with focus on the dialog's buttons — must never delete a node or wire.
2. Cancelling the dialog for an EXISTING `headers[X].value` wire — must keep the wire unchanged (only an uncommitted NEW `headers` wire is removed).
3. Double-clicking a "Run when" (trigger) wire — must not open the dialog.
4. A run whose steps carry no `logs` — must add nothing to the Console.
5. The Monaco `response` typings — must be removed when the dialog closes, so the Scripts tab never sees `response`.

---

### Task 1: Push step logs to the Console panel

**Files:**
- Modify: `src/lib/tauri-api.ts` (`FlowStepResult` ~1809, `FlowStepCompletedEvent` ~1875)
- Modify: `src/components/flow/FlowToolbar.tsx` (props ~15-30, summary loop ~170-175)
- Modify: `src/components/flow/FlowPane.tsx` (the `<FlowToolbar>` element ~325-336)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx`

**Interfaces:**
- Produces:
  - `export interface FlowLogEntry { level: 'log' | 'warn' | 'error'; message: string }` in `tauri-api.ts`.
  - `logs?: FlowLogEntry[]` on `FlowStepResult` and on `FlowStepCompletedEvent`.
  - `FlowToolbar` prop `onStepLogs?: (nodeId: string, logs: FlowLogEntry[]) => void`, called once per summary step that has at least one log, in step order.

- [ ] **Step 1: Write the failing tests**

In `FlowToolbar.test.tsx`, following how existing tests mock `runFlow` to resolve a summary, add:

```tsx
  it('hands each summary step with logs to onStepLogs, once, in order', async () => {
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        { nodeId: 'a', status: 'success', statusCode: null, durationMs: null, error: null, value: null,
          logs: [{ level: 'log', message: 'one' }] },
        { nodeId: 'b', status: 'success', statusCode: null, durationMs: null, error: null, value: null },
        { nodeId: 'c', status: 'failed', statusCode: null, durationMs: null, error: 'x', value: null,
          logs: [{ level: 'error', message: 'two' }] },
      ],
    });
    const onStepLogs = vi.fn();
    renderToolbar({ onStepLogs }); // use this file's existing render helper and default props
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onStepLogs).toHaveBeenCalledTimes(2));
    expect(onStepLogs).toHaveBeenNthCalledWith(1, 'a', [{ level: 'log', message: 'one' }]);
    expect(onStepLogs).toHaveBeenNthCalledWith(2, 'c', [{ level: 'error', message: 'two' }]);
  });
```
Adapt the helper name and default props to the file; do not change existing tests.

In `FlowPane.test.tsx` (copy that file's setup), add a test: seed a tab with a node `{ id: 'n1', kind: { kind: 'Output', label: 'Show token' } }` and flow name `login-flow`; mock `runFlow` to resolve one step `{ nodeId: 'n1', …, logs: [{ level: 'warn', message: 'hi' }] }`; click Run; assert `useConsoleStore.getState().entries` contains an entry with `kind: 'script'`, `level: 'warn'`, `message: 'hi'`, `requestName: 'login-flow › Show token'`. Reset the console store in `beforeEach` with `useConsoleStore.getState().clearEntries()`.

- [ ] **Step 2: Run and watch them fail**

Run: `yarn test --run FlowToolbar FlowPane.test`
Expected: FAIL (`onStepLogs` never called; no console entry).

- [ ] **Step 3: Implement**

- `tauri-api.ts`: add `FlowLogEntry` and the optional `logs` fields, each with a one-line doc comment.
- `FlowToolbar.tsx`: add the `onStepLogs` prop (comment: `// Receives each step's script console output once the run ends.`). In the summary loop, after `onPatchStatus(...)`, add `if (step.logs?.length) onStepLogs?.(step.nodeId, step.logs);`. Do NOT read logs from the streamed events; the summary is the single source, so logs are never duplicated.
- `FlowPane.tsx`: pass
```tsx
              onStepLogs={(nodeId, logs) => {
                const node = latestFlowTab()?.nodes.find((n) => n.id === nodeId);
                const label = node?.kind.label ?? nodeId;
                useConsoleStore.getState().addScriptEntries(
                  logs.map((l) => ({
                    level: l.level,
                    message: l.message,
                    requestName: `${flowName ?? 'Flow'} › ${label}`,
                  })),
                );
              }}
```
  Use FlowPane's existing `latestFlowTab()` helper and flow-name variable (read the file for their exact names; if the node kind's label field differs, use it). Import `useConsoleStore` from `@/stores/console-store`.

- [ ] **Step 4: Run and watch them pass**

Run: `yarn test --run FlowToolbar FlowPane`
Expected: PASS.

- [ ] **Step 5: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`.
Commit subject: `feat(flow): show step script logs in the Console`.

---

### Task 2: Wire script dialog with Monaco

**Files:**
- Create: `src/components/flow/WireScriptDialog.tsx`
- Create: `src/components/flow/wire-script-types.ts` (the `response` typings string)
- Modify: `src/components/editor/MonacoWrapper.tsx` (props ~15-26, extra-lib effect ~109-121)
- Modify: `src/components/flow/FlowPane.tsx` (import ~39, render block ~360-395)
- Delete: `src/components/flow/WireExpressionPopover.tsx` and `src/components/flow/__tests__/WireExpressionPopover.test.tsx`
- Test: `src/components/flow/__tests__/WireScriptDialog.test.tsx`, `src/components/editor/__tests__/` (only if a MonacoWrapper test file already exists there)

**Interfaces:**
- Consumes: FlowPane's `pendingEdge` / `pendingTargetNode` / `onCommit` / `onOpenChange` wiring (unchanged semantics).
- Produces:
  - `MonacoWrapper` prop `extraLib?: { content: string; filePath: string }`, registered with `monacoNs.typescript.javascriptDefaults.addExtraLib(content, filePath)` on mount / change and disposed on unmount or change.
  - `export const WIRE_SCRIPT_TYPES: string` and `export const WIRE_SCRIPT_TYPES_PATH = 'ts:flow-wire-response.d.ts'` in `wire-script-types.ts`.
  - `export function WireScriptDialog(props: { edge: FlowEdge; targetNode: FlowNode; open: boolean; onOpenChange: (open: boolean) => void; onCommit: (edge: FlowEdge) => void })`.
  - `export function headerNameFromTarget(targetField: string): string` (returns `X` for `headers[X].value`, else `''`), exported from `WireScriptDialog.tsx` for Task 3.

- [ ] **Step 1: Write the failing tests** in `WireScriptDialog.test.tsx`. Mock Monaco as a textarea, as other flow tests do:

```tsx
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='Wire script' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));
```
Tests (use `userEvent`):
1. Opens with the edge's expression in the editor and a dialog titled `Value from source`; editing the text and clicking **Save** calls `onCommit` with `{ ...edge, expression: <new text> }` and `onOpenChange(false)`.
2. **Cancel** calls `onOpenChange(false)` and never `onCommit`.
3. For a new `headers` edge (`targetField: 'headers'`): Save is ignored while the header name is empty; with name `Authorization` it commits `targetField: 'headers[Authorization].value'`.
4. For an existing edge with `targetField: 'headers[X-Token].value'`, the header name field starts as `X-Token`, and Save with the unchanged name keeps `targetField` unchanged.
5. The dialog content element has the `nokey` class (`screen.getByRole('dialog')` has class `nokey`).
6. `headerNameFromTarget('headers[A].value') === 'A'` and `headerNameFromTarget('url') === ''`.

- [ ] **Step 2: Run and watch them fail**

Run: `yarn test --run WireScriptDialog`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

`wire-script-types.ts`:
```ts
// Typings for the `response` object a Flow wire script can read.
export const WIRE_SCRIPT_TYPES = `
declare const response: {
  /** HTTP status code of the source node. */
  status: number;
  statusText: string;
  headers: Record<string, string>;
  /** Parsed JSON body, or the raw text when it is not JSON. */
  body: any;
  duration_ms: number;
};
`;
export const WIRE_SCRIPT_TYPES_PATH = 'ts:flow-wire-response.d.ts';
```

`MonacoWrapper.tsx`: add the `extraLib` prop (doc comment: extra typings for this editor only). Add an effect beside the `phase` effect, with its own disposable ref:
```tsx
  useEffect(() => {
    if (!extraLib) return;
    const disposable = monacoNs.typescript.javascriptDefaults.addExtraLib(
      extraLib.content,
      extraLib.filePath,
    );
    return () => disposable.dispose();
  }, [extraLib?.content, extraLib?.filePath]);
```

`WireScriptDialog.tsx` (keep the popover's commit logic, including the case-insensitive reuse of an existing inline header name):
```tsx
import { lazy, Suspense, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { WIRE_SCRIPT_TYPES, WIRE_SCRIPT_TYPES_PATH } from './wire-script-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const EXTRA_LIB = { content: WIRE_SCRIPT_TYPES, filePath: WIRE_SCRIPT_TYPES_PATH };

// Returns the header name of a `headers[Name].value` target, or '' for any other target.
export function headerNameFromTarget(targetField: string): string {
  const match = /^headers\[(.+)\]\.value$/.exec(targetField);
  return match ? match[1] : '';
}
```
Component body:
- State: `expression` (from `edge.expression`), `headerName` (from `headerNameFromTarget(edge.targetField)`).
- `isHeadersTarget = edge.targetField === 'headers' || edge.targetField.startsWith('headers[')`.
- `handleSave`: for a headers target, trim the name and return if empty; build `headers[${existing ?? trimmed}].value` as the popover did (existing inline header names via the popover's `targetHeaderNames` logic, moved here). Call `onCommit({ ...edge, targetField, expression })` then `onOpenChange(false)`.
- Render:
```tsx
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* nokey keeps Backspace and Delete in the dialog from deleting canvas items. */}
      <DialogContent className='nokey max-w-3xl'>
        <DialogHeader>
          <DialogTitle>Value from source</DialogTitle>
          <DialogDescription>
            Write one expression, or several lines that end with return value. The source result
            is available as response. console.log output appears in the Console.
          </DialogDescription>
        </DialogHeader>
        {isHeadersTarget && (
          <div className='space-y-1'>
            <Label htmlFor='wire-header-name'>Header name</Label>
            <Input
              id='wire-header-name'
              value={headerName}
              onChange={(e) => setHeaderName(e.target.value)}
              placeholder='e.g. Authorization'
            />
          </div>
        )}
        <div className='h-80 overflow-hidden rounded border'>
          <Suspense fallback={<div className='h-full animate-pulse bg-muted' />}>
            <MonacoWrapper
              value={expression}
              onChange={setExpression}
              language='javascript'
              height='100%'
              extraLib={EXTRA_LIB}
            />
          </Suspense>
        </div>
        <DialogFooter>
          <Button variant='outline' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
```
`FlowPane.tsx`: replace the `WireExpressionPopover` import and element with `WireScriptDialog`, keeping the same `key`, `edge`, `targetNode`, `open`, `onOpenChange` and `onCommit` props, and dropping the `{/* … */}<span />` child. Delete `WireExpressionPopover.tsx` and its test file (`git rm`). Update any other import of it (`grep -rn WireExpressionPopover src`).

- [ ] **Step 4: Run and watch them pass**

Run: `yarn test --run WireScriptDialog FlowPane MonacoWrapper`
Expected: PASS. Existing FlowPane tests that drove the popover by its `Value from source` label or `Save` button should still pass; if one queried the popover by role, update the query to the dialog, not the behaviour.

- [ ] **Step 5: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`.
Commit subject: `feat(flow): edit wire scripts in a Monaco dialog`.

---

### Task 3: Reopen an existing wire on double-click

**Files:**
- Modify: `src/components/flow/FlowCanvas.tsx` (`FlowCanvasProps` ~38-68, `<ReactFlow>` props ~347)
- Modify: `src/components/flow/FlowPane.tsx` (pass `onEdgeEdit`; dialog close handling)
- Test: `src/components/flow/__tests__/FlowCanvas.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx`

**Interfaces:**
- Consumes: `WireScriptDialog`, `headerNameFromTarget` from Task 2; `shouldPromptForExpression` from `src/lib/flow-wiring.ts`.
- Produces: `FlowCanvas` prop `onEdgeEdit?: (edgeId: string) => void`.

- [ ] **Step 1: Write the failing tests**

`FlowCanvas.test.tsx`: render two nodes with one data edge (`targetField: 'url'`); double-click the rendered edge path (`container.querySelector('.react-flow__edge')`, `fireEvent.doubleClick`) and assert `onEdgeEdit` was called with the edge id.

`FlowPane.test.tsx` (existing setup):
1. Seed a tab with a data edge `{ id: 'e1', targetField: 'url', expression: 'response.body' }`; double-click it; the dialog opens showing `response.body`; change the text to `response.body.url` and Save; the store's edge `e1` now has that expression and the same id.
2. Seed a trigger edge (`targetField: 'trigger'`); double-click it; no dialog opens.
3. Seed an existing `headers[X-Token].value` edge; double-click; Cancel; the edge is still in the store unchanged.
4. After Save or Cancel, `document.activeElement` is not `document.body` (focus returns to the canvas).

- [ ] **Step 2: Run and watch them fail**

Run: `yarn test --run FlowCanvas FlowPane.test`
Expected: FAIL (`onEdgeEdit` never called; dialog never opens).

- [ ] **Step 3: Implement**

- `FlowCanvas.tsx`: add the prop with comment `// Called when a wire is double-clicked, to edit its script.`, and on `<ReactFlow>` add `onEdgeDoubleClick={(_, edge) => onEdgeEdit?.(edge.id)}` next to `onEdgeClick={focusPane}`.
- `FlowPane.tsx`: pass
```tsx
            onEdgeEdit={(edgeId) => {
              const edge = tab.edges.find((e) => e.id === edgeId);
              // Run when wires carry no value, so there is nothing to edit.
              if (edge && shouldPromptForExpression(edge)) setPendingEdge(edge);
            }}
```
  An existing headers edge has a `headers[X].value` target, so `isUncommittedHeadersEdge` never removes it on Cancel; keep that rule unchanged.
- Focus: in the dialog's close path in FlowPane (`onOpenChange(false)`), return focus to the canvas the same way `handleDeleteNode` does (read it and reuse its helper or selector). If Radix's own focus return lands on `document.body`, handle `onCloseAutoFocus` on `DialogContent` in `WireScriptDialog` by calling `event.preventDefault()` and let FlowPane focus the canvas.

- [ ] **Step 4: Run and watch them pass**

Run: `yarn test --run FlowCanvas FlowPane WireScriptDialog`
Expected: PASS.

- [ ] **Step 5: Check and commit**

Run: `yarn test --run flow`, `yarn tsc --noEmit`, `yarn check`.
Commit subject: `feat(flow): reopen a wire's script on double-click`.

## Manual check (after this plan)
In `yarn tauri dev`: draw a wire and double-click an existing one. The dialog is centred, Monaco fills its box and shows `response.` completions, Backspace in the editor deletes nothing on the canvas, and a run with `console.log` in a wire shows the lines in the Console panel.
