# Flow Request Debug Mode — Plan 02: Frontend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users turn Debug mode on from a Request node's ⋮ menu (or its properties panel), see a bug badge on the card, and get one Console HTTP row per debug node after a run.

**Architecture:** TS types mirror Plan 01's `debug` flag and `FlowDebugRequest`. `FlowToolbar` hands each summary step's `debugRequest` to `FlowPane`, which calls `addHttpEntry` with a `requestName`. `NodeMenuButton` becomes a shadcn `DropdownMenu` for Request nodes.

**Tech Stack:** React + TS, shadcn/ui (`dropdown-menu`, `switch`), lucide-react, Zustand, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-request-debug-mode-design.md`

**Depends on:** Plan 01. JSON shapes:
- Node kind: `{ kind: 'Request', label, source, debug?: boolean }` (key `debug`, omitted when false).
- Summary step (camelCase): `debugRequest?: FlowDebugRequest`. Step event (snake_case): `debug_request?: FlowDebugRequest`.
- `FlowDebugRequest` = `{ method: string; url: string; headers: { key: string; value: string }[]; body?: string; response?: { status: number; statusText: string; durationMs: number; sizeBytes: number; headers: { key: string; value: string }[]; body: string }; error?: string }` (already masked by the backend).

## Global Constraints
- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`); lucide-react icons only.
- Zustand: narrow selectors; never destructure a whole store (`useConsoleStore.getState()` inside callbacks is fine).
- Anything portalled from the canvas carries `nokey`; clickable controls on a node carry `nodrag nokey`.
- Debug logs are pushed from the final run summary only, never from streamed events.
- Code comments: short full sentences ending with a period. Biome: 2 spaces, single quotes, trailing commas, 100 columns.
- Never run `yarn install`. Never run the whole Vitest suite; use `yarn test --run <pattern>` (and `yarn test --run flow` once per task).
- Checks per task: `yarn test --run <patterns>`, `yarn tsc --noEmit`, `yarn check`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill (skip its subagent steps if you cannot dispatch subagents); conventional commits.

## Review Focus
1. Backspace while the ⋮ menu is open or just after toggling Debug mode — must not delete the node.
2. Normal (non-Flow) Console HTTP rows — must look exactly as before (no empty name prefix).
3. A debug step that failed to send (`error`, no `response`) — must still produce a row, with status 0 and status text `Error`, and the error in the response body area.
4. Toggling Debug mode — marks the tab unsaved and survives Save and reload (key `debug` in the saved kind).
5. If, Switch, Input and Output nodes — their ⋮ keeps today's single-click "open properties" behaviour.

---

### Task 1: Debug rows in the Console

**Files:**
- Modify: `src/lib/tauri-api.ts` (Request kind ~1761, `FlowStepResult` ~1826, `FlowStepCompletedEvent` ~1895)
- Modify: `src/stores/console-store.ts` (`HttpConsoleEntry` ~5-19)
- Modify: `src/components/layout/ConsolePanel.tsx` (HTTP row rendering)
- Modify: `src/components/flow/FlowToolbar.tsx` (props, summary loop)
- Modify: `src/components/flow/FlowPane.tsx` (`<FlowToolbar>` props, next to `onStepLogs`)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx`, the ConsolePanel test file if one exists (`src/components/layout/__tests__/`)

**Interfaces:**
- Produces:
  - `export interface FlowDebugHeader { key: string; value: string }`, `export interface FlowDebugResponse {...}`, `export interface FlowDebugRequest {...}` in `tauri-api.ts` (shapes above); Request kind gains `debug?: boolean`; `FlowStepResult.debugRequest?: FlowDebugRequest`; `FlowStepCompletedEvent.debug_request?: FlowDebugRequest`.
  - `HttpConsoleEntry.requestName?: string`.
  - `FlowToolbar` prop `onStepDebug?: (nodeId: string, debug: FlowDebugRequest) => void`, called once per summary step that has `debugRequest`, in step order.

- [ ] **Step 1: Write the failing tests**

`FlowToolbar.test.tsx` (use the file's existing render helper and `runFlow` mock):
```tsx
  it('hands each summary step with a debug request to onStepDebug, once, in order', async () => {
    const debugA = { method: 'POST', url: 'https://x.test/a', headers: [], body: '{}' };
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        { nodeId: 'a', status: 'success', statusCode: 200, durationMs: 5, error: null, value: null, debugRequest: debugA },
        { nodeId: 'b', status: 'success', statusCode: 200, durationMs: 5, error: null, value: null },
      ],
    });
    const onStepDebug = vi.fn();
    renderToolbar({ onStepDebug });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onStepDebug).toHaveBeenCalledTimes(1));
    expect(onStepDebug).toHaveBeenCalledWith('a', debugA);
  });
```
`FlowPane.test.tsx` (copy the setup of the existing step-logs test): mock `runFlow` to resolve one step for node `n1` (label `Login`, flow `login-flow`) with
`debugRequest: { method: 'POST', url: 'https://x.test/login', headers: [{ key: 'Authorization', value: 'Bearer ••••••' }], body: '{"u":"a"}', response: { status: 400, statusText: 'Bad Request', durationMs: 12, sizeBytes: 20, headers: [], body: '{"error":"bad"}' } }`.
Click Run; assert `useConsoleStore.getState().entries` has one entry with `kind: 'http'`, `method: 'POST'`, `url: 'https://x.test/login'`, `status: 400`, `requestHeaders` containing the Authorization line, `requestBody: '{"u":"a"}'`, `responseBody: '{"error":"bad"}'`, `requestName: 'login-flow › Login'`.
Add a second FlowPane test: `debugRequest` with no `response` and `error: 'connection refused'` → one entry with `status: 0`, `statusText: 'Error'`, `durationMs: 0`, `sizeBytes: 0`, `responseHeaders: []`, `responseBody: 'connection refused'`.
ConsolePanel: if a test file exists, add one test that an HTTP entry with `requestName` shows that text in its row, and one that an entry without it shows no name element. If no ConsolePanel test file exists, create `src/components/layout/__tests__/ConsolePanel.requestName.test.tsx` with those two tests, seeding the store with `useConsoleStore.getState().addHttpEntry(...)`.

- [ ] **Step 2: Run and watch them fail**

Run: `yarn test --run FlowToolbar FlowPane ConsolePanel`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `tauri-api.ts`: add the types and optional fields, each with a one-line doc comment.
- `console-store.ts`: add `/** Names the source of the entry, such as a Flow node. */ requestName?: string;` to `HttpConsoleEntry`.
- `ConsolePanel.tsx`: in the HTTP row, when `entry.requestName` is set, render it before the method in a muted, truncated `<span>` (`text-muted-foreground`). Rows without it render exactly as before.
- `FlowToolbar.tsx`: add `onStepDebug` (comment: `// Receives each debug node's sent request once the run ends.`) and, in the summary loop after `onStepLogs`, `if (step.debugRequest) onStepDebug?.(step.nodeId, step.debugRequest);`.
- `FlowPane.tsx`: next to `onStepLogs`, pass
```tsx
              onStepDebug={(nodeId, debug) => {
                const node = latestFlowTab()?.nodes.find((n) => n.id === nodeId);
                const label = node?.kind.label || nodeId;
                const response = debug.response;
                useConsoleStore.getState().addHttpEntry({
                  requestName: `${flowName} › ${label}`,
                  method: debug.method,
                  url: debug.url,
                  status: response?.status ?? 0,
                  statusText: response?.statusText ?? 'Error',
                  durationMs: response?.durationMs ?? 0,
                  sizeBytes: response?.sizeBytes ?? 0,
                  requestHeaders: debug.headers,
                  requestBody: debug.body ?? '',
                  responseHeaders: response?.headers ?? [],
                  responseBody: response?.body ?? debug.error ?? '',
                });
              }}
```
  using the same flow-name variable `onStepLogs` uses.

- [ ] **Step 4: Run and watch them pass**

Run: `yarn test --run FlowToolbar FlowPane ConsolePanel`, then `yarn test --run flow`
Expected: PASS.

- [ ] **Step 5: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`.
Commit subject: `feat(flow): show debug requests in the Console`.

---

### Task 2: Debug mode toggle, badge and panel switch

**Files:**
- Modify: `src/components/flow/nodes/NodeMenuButton.tsx`
- Modify: `src/components/flow/nodes/RequestNode.tsx` (title row ~47-56)
- Modify: `src/components/flow/properties/RequestNodeEditor.tsx` (below the Label field ~116)
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, `src/components/flow/properties/__tests__/RequestNodeEditor.test.tsx` (or the existing properties test file), `src/components/flow/__tests__/FlowPane.properties.test.tsx` (or `FlowPane.requestFocus.test.tsx`, whichever already tests ⋮)

**Interfaces:**
- Consumes: `debug?: boolean` on the Request kind from Task 1; `useFlowNodeActions()` (`openProperties`, `updateNodeKind`) from `./FlowNodeActionsContext`.
- Produces: `NodeMenuButton` props `{ nodeId: string; label: string; debug?: { enabled: boolean; onToggle: (enabled: boolean) => void } }`. Without `debug`, it behaves exactly as today.

- [ ] **Step 1: Write the failing tests**

`RequestNode.test.tsx` (existing render helper):
1. With `kind.debug` true, `getByTestId('request-node-debug-badge')` is present and has accessible name `Debug mode on`; with it false or absent, the badge is absent.
2. Clicking `Edit <label>` opens a menu with items **Edit properties** and **Debug mode** (checkbox item, `aria-checked="false"`); clicking **Debug mode** calls the node-actions `updateNodeKind` mock with `{ ...kind, debug: true }`; clicking **Edit properties** calls `openProperties(nodeId)`.
3. The open menu's content element has class `nokey` (`screen.getByRole('menu')` has class `nokey`).
Other node kinds (e.g. `InputOutputNodes.test.tsx` or `IfNode.test.tsx`): clicking `Edit <label>` calls `openProperties` directly and opens no menu.
FlowPane (existing ⋮ test file): open the ⋮ menu of a selected Request node, press Backspace — the node is still in the store; choose **Debug mode** — the store's node kind has `debug: true` and the tab is dirty.
`RequestNodeEditor` test: a `Debug mode` switch (`getByRole('switch', { name: 'Debug mode' })`) reflects `kind.debug`, and toggling it calls `onChange` with `{ ...kind, debug: true }`.

- [ ] **Step 2: Run and watch them fail**

Run: `yarn test --run RequestNode NodeMenuButton RequestNodeEditor FlowPane`
Expected: FAIL.

- [ ] **Step 3: Implement**

`NodeMenuButton.tsx`:
- Without the `debug` prop: unchanged (the single `Button` calling `openProperties(nodeId)`).
- With it: a shadcn `DropdownMenu`. The trigger is the same ghost `Button` (`aria-label={\`Edit ${label}\`}`, `className='nodrag nokey …'`) wrapped in `DropdownMenuTrigger asChild`. The content is `<DropdownMenuContent className='nokey' align='end'>` with `<DropdownMenuItem onSelect={() => openProperties(nodeId)}>Edit properties</DropdownMenuItem>` and `<DropdownMenuCheckboxItem checked={debug.enabled} onCheckedChange={(v) => debug.onToggle(v === true)}>Debug mode</DropdownMenuCheckboxItem>`. Comment the `nokey` on the content: it is portalled, so without it Backspace in the menu would delete the node.

`RequestNode.tsx`: pass `debug={{ enabled: kind.debug === true, onToggle: (enabled) => updateNodeKind(id, { ...kind, debug: enabled }) }}` to `NodeMenuButton` (get `updateNodeKind` from `useFlowNodeActions()`). In the title row, before the menu button:
```tsx
        {kind.debug && (
          <Bug
            data-testid='request-node-debug-badge'
            aria-label='Debug mode on'
            role='img'
            className='h-3.5 w-3.5 shrink-0 text-amber-500'
          />
        )}
```
with `Bug` from `lucide-react`.

`RequestNodeEditor.tsx`: under the Label field, a row with shadcn `Label htmlFor='request-debug-mode'` "Debug mode" and `<Switch id='request-debug-mode' checked={kind.debug === true} onCheckedChange={(v) => onChange({ ...kind, debug: v })} />`, plus a one-line muted hint: "Logs the request as sent and its response to the Console on each run. Secrets are masked."

- [ ] **Step 4: Run and watch them pass**

Run: `yarn test --run RequestNode NodeMenuButton RequestNodeEditor FlowPane`, then `yarn test --run flow`
Expected: PASS.

- [ ] **Step 5: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`.
Commit subject: `feat(flow): add a debug mode toggle to request nodes`.

## Manual check (after this plan)
In `yarn tauri dev`: on the Login node open ⋮ → Debug mode; the bug badge appears; Save; Run; the Console shows one `login-flow › Login` row with the resolved URL, headers (auth as `Bearer ••••••`), body with secrets masked, and the response. Press Backspace with the ⋮ menu open: the node stays.
