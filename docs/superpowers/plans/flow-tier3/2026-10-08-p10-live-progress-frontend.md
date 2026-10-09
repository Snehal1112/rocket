# Live Progress Frontend Implementation Plan

> **Execute this plan:** P10. Before starting it, make sure these are merged to main: P8 and P9 (both merged first). After it is merged, the next plan to execute is P11. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** While a flow runs, the Last run tab says why a node is still waiting: "Last status 202 · condition false · 12s left" for a poll, and the ignored count, a countdown and the last turned-down call for a Wait for callback node. The Wait node shows its live callback URL with a copy button. After the run the same details come from the step trace.

**Architecture:** P9 adds a structured `live` payload to `flow-step-progress`, `callbacks` to `flow-run-started`, and `poll`/`wait` to the step trace. The toolbar forwards `live` as an optional third argument of `onPatchProgress` and hands the callback URLs to a new `onCallbackUrls` prop. The store merges `live` into the node detail (keeping the last turned-down call across ticks) and keeps the URLs on the tab as `callbackUrls`, not in `nodeDetail`, because a run start wipes `nodeDetail`. Both are dropped when the run ends. The Last run tab counts down locally from `remainingMs`.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), shadcn `Button`, `Collapsible`, `Table`, lucide `Copy`, `Check`, `ChevronRight`, `copyTextAsync` (`src/lib/clipboard.ts`), Vitest with fake timers and Testing Library.

**Spec:** Roadmap item F-40 (frontend) in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P10 Live progress frontend". Backend contract: `docs/superpowers/plans/flow-tier3/2026-10-08-p9-live-progress-backend.md` (Interfaces of each task). Builds on `docs/superpowers/plans/flow-tier3/2026-10-08-p8-step-trace-frontend.md`.

## Global Constraints

- Requires P8 and P9 merged. The TS types below mirror P9's Rust types exactly (camelCase inside snake_case events).
- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: narrow selectors only (`usePaneStore((s) => s.setFlowCallbackUrls)`), never full destructuring.
- The callback URL holds a bearer token. It lives only in the tab's `callbackUrls` while `runState === 'running'`, is never written to `nodeDetail`, history, the console or a toast, and is cleared when the run ends. Turned-down calls are shown exactly as the backend masked them.
- Clipboard writes go through `copyTextAsync(Promise.resolve(text))`, started synchronously in the click handler (WebKit drops user activation after an await). This plan switches `LastRunTab`'s `CopyButton` to it too.
- Code comments are short full sentences that end with a punctuation mark.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <path>` listed in the task.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with explicit staged paths. Never `git add -A`, `--all` or `.`.
- Only one implementer at a time touches `FlowToolbar.tsx`, `FlowPane.tsx`, `FlowCanvas.tsx` and `LastRunTab.tsx` (P1, P2, P4, P5, P6, P8, P12 and P15 edit them too). Line numbers are from HEAD b047bbc6; find each edit by the quoted code.
- Not in scope: run history (P11), export (P15), a cancellable pre-run phase (roadmap F-05), masking a callback URL that a Request sends.

## Decisions assumed

- No open decision from the index (D1 to D6) affects this plan.
- A progress event that arrives after the tab's run is `done` is dropped by the store. The toolbar already unsubscribes when `runFlow` resolves; the store guard covers events already queued.
- A live event without `lastRejected` (a ticker event) keeps the previous `lastRejected`, so the turned-down call stays on screen between ticks.
- The callback URL is shown whenever the tab holds one, which is from run start to run end, also before the Wait node itself runs (a Request may send the URL earlier).
- If P15 already switched `CopyButton` to `copyTextAsync`, skip Step 7 of Task 3.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. An event arrives after the run ended and puts "waiting" or a stale countdown back on a finished node. Tests pinned in Task 1 (`patchFlowNodeProgress ignores progress after the run finished`) and Task 2 (`hides the live line once the node finished and stops its timer`).
2. An old payload without `live` or `callbacks` changes today's behaviour (the existing two-argument `onPatchProgress` assertions, no URLs). Tests pinned in Task 1 (`passes live progress as a third argument only when present`, `does not call onCallbackUrls for a run without callbacks`) and the existing toolbar progress tests stay green.
3. A large turned-down body (2 KB live, 256 KB in the trace) renders expanded or unbounded. Test pinned in Task 2 (`keeps the last rejected call collapsed until asked`).
4. The callback URL outlives its run, or reaches `nodeDetail`. Tests pinned in Task 1 (`callback URLs are kept only while a run is active`) and Task 3 (`shows no callback URL when the tab holds none`).
5. The countdown keeps a timer running after the node finishes or the panel unmounts. Test pinned in Task 2 (`hides the live line once the node finished and stops its timer`).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/tauri-api.ts` (modify) | `FlowRejectedCall`, `FlowPollDetail`, `FlowWaitDetail`, `FlowLiveProgress`, `FlowCallbackInfo`; `poll`/`wait` on `FlowStepTrace`; `callbacks?` and `live?` on the events. |
| `src/types/pane-types.ts` (modify) | `FlowNodeDetail.live`, `FlowTab.callbackUrls`. |
| `src/stores/pane-store.ts` (modify) | `patchFlowNodeProgress(..., live?)` with the done guard and `lastRejected` merge; `setFlowCallbackUrls`; `setFlowRunState` clears the URLs. |
| `src/components/flow/FlowToolbar.tsx` (modify) | Forwards `live`; `onCallbackUrls` from `flow-run-started`. |
| `src/components/flow/FlowPane.tsx` (modify) | Wires the two callbacks; passes `callbackUrls` to the canvas and the panel. |
| `src/components/flow/properties/LastRunTab.tsx` (modify) | Poll and Wait panels, countdown, last rejected call, callback URL, `CopyButton` via `copyTextAsync`. |
| `src/components/flow/CallbackUrlField.tsx` (new) | URL, copy button, "Valid while the run is active." |
| `src/components/flow/nodes/WaitForCallbackNode.tsx` (modify) | Shows `data.callbackUrl`. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `callbackUrls` prop spread into Wait node data. |
| `src/components/flow/properties/NodePropertiesPanel.tsx` (modify) | `callbackUrl` prop to the Wait editor and the Last run tab. |
| `src/components/flow/properties/WaitForCallbackEditor.tsx` (modify) | Shows the live URL. |

Existing tests to know: `src/stores/__tests__/pane-store.test.ts` (`flowTabWithNode`, `findFirstFlowTab`), `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `startedHandler`, `progressHandler`), `src/components/flow/__tests__/FlowPane.test.tsx` (describe `FlowPane run logs`, `logTab`), `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (`node()`, MonacoWrapper textarea mock), `src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx` (`renderNode`), `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (`renderPanel`, describe `Wait for callback editor`).

---

### Task 1: Types, store and toolbar wiring

**Files:**
- Modify: `src/lib/tauri-api.ts` (after P8's `FlowStepTrace`; `FlowRunStartedEvent` :2283-2289; `FlowStepProgressEvent` :2333-2342)
- Modify: `src/types/pane-types.ts` (`FlowNodeDetail` :168-185, `FlowTab` :187-199)
- Modify: `src/stores/pane-store.ts` (action types :290-291, `patchFlowNodeProgress` :953-967, `setFlowRunState` :969-980)
- Modify: `src/components/flow/FlowToolbar.tsx` (imports :6-19, props :21-44, resume effect :157-163, `handleRun` started handler :238-244 and progress handler :253-256)
- Modify: `src/components/flow/FlowPane.tsx` (selectors near :54-55, `<FlowToolbar` props near :386-387)
- Test: `src/stores/__tests__/pane-store.test.ts`, `src/components/flow/__tests__/FlowToolbar.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx` (extend)

**Interfaces:**
- Consumes: P9 JSON `live: { lastStatusCode?, conditionMet?, elapsedMs?, remainingMs?, ignored?, lastRejected? }`, `callbacks?: [{ nodeId, name, url }]`, `trace.poll`, `trace.wait`.
- Produces: TS types above; `FlowNodeDetail.live?: FlowLiveProgress`; `FlowTab.callbackUrls?: Record<string, string>` (node id to URL); store `patchFlowNodeProgress(tabId, nodeId, message, live?)`, `setFlowCallbackUrls(tabId, urls | undefined)`; toolbar props `onPatchProgress?: (nodeId, message, live?) => void`, `onCallbackUrls?: (urls: Record<string, string>) => void`.

- [ ] **Step 1: Write the failing store tests**

In `src/stores/__tests__/pane-store.test.ts`, after the test `'patchFlowNodeProgress for an unknown node id is a safe no-op'`, add:

```ts
  it('patchFlowNodeProgress stores live detail next to the text', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r1');
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'n1', 'attempt 2/5 · condition false', {
      lastStatusCode: 202,
      conditionMet: false,
      remainingMs: 12000,
    });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({
      progress: 'attempt 2/5 · condition false',
      live: { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 },
    });
  });

  it('patchFlowNodeProgress keeps the last rejected call across ticks', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r1');
    const lastRejected = {
      method: 'POST',
      url: '/cb/…',
      headers: [],
      body: '{}',
      reason: 'Accept when returned false.',
    };
    usePaneStore
      .getState()
      .patchFlowNodeProgress(tabId, 'n1', 'waiting… 9s left · 1 ignored call(s)', {
        ignored: 1,
        remainingMs: 9000,
        lastRejected,
      });
    usePaneStore
      .getState()
      .patchFlowNodeProgress(tabId, 'n1', 'waiting… 8s left · 1 ignored call(s)', {
        ignored: 1,
        remainingMs: 8000,
      });
    expect(findFirstFlowTab()?.nodeDetail?.n1?.live).toEqual({
      ignored: 1,
      remainingMs: 8000,
      lastRejected,
    });
  });

  it('patchFlowNodeProgress ignores progress after the run finished', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r1');
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', { durationMs: 5 });
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'r1');
    usePaneStore
      .getState()
      .patchFlowNodeProgress(tabId, 'n1', 'waiting… 3s left', { remainingMs: 3000 });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ durationMs: 5 });
  });

  it('callback URLs are kept only while a run is active', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().setFlowCallbackUrls(tabId, { w: 'http://h:1/cb/t' });
    expect(findFirstFlowTab()?.callbackUrls).toBeUndefined();

    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r1');
    usePaneStore.getState().setFlowCallbackUrls(tabId, { w: 'http://h:1/cb/t' });
    expect(findFirstFlowTab()?.callbackUrls).toEqual({ w: 'http://h:1/cb/t' });
    expect(findFirstFlowTab()?.nodeDetail).toEqual({});

    usePaneStore.getState().setFlowRunState(tabId, 'done', 'r1');
    expect(findFirstFlowTab()?.callbackUrls).toBeUndefined();
  });

  it('a new run drops the previous run callback URLs', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r1');
    usePaneStore.getState().setFlowCallbackUrls(tabId, { w: 'http://h:1/cb/old' });
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'r2');
    expect(findFirstFlowTab()?.callbackUrls).toBeUndefined();
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store.test.ts`
Expected: FAIL (`setFlowCallbackUrls` is not a function; `live` is not stored; late progress is applied).

- [ ] **Step 3: Add the types**

In `src/lib/tauri-api.ts`, after P8's `FlowStepTrace` interface, insert:

```ts
/** A call a Wait for callback node turned down. Masked by the backend. */
export interface FlowRejectedCall {
  method: string;
  /** Path and query. The token path is shown as `/cb/…`. */
  url: string;
  headers: FlowDebugHeader[];
  body: string;
  /** True when the body was cut: at 2 KB in a live event, at 256 KB in the trace. */
  bodyTruncated?: boolean;
  reason: string;
}

/** How a repeat-until poll went. */
export interface FlowPollDetail {
  attempts: number;
  maxAttempts: number;
  lastStatusCode?: number;
  /** Absent when no verdict was reached, as after a condition script error. */
  conditionMet?: boolean;
  elapsedMs: number;
  timeoutMs: number;
}

/** How a callback wait went. */
export interface FlowWaitDetail {
  ignored: number;
  timeoutMs: number;
  lastRejected?: FlowRejectedCall;
}

/** Structured progress of a node that is still running. Every key is optional. */
export interface FlowLiveProgress {
  lastStatusCode?: number;
  conditionMet?: boolean;
  elapsedMs?: number;
  /** Time left before the node gives up. The UI counts down from it. */
  remainingMs?: number;
  ignored?: number;
  lastRejected?: FlowRejectedCall;
}

/** The callback URL of one Wait for callback node. Valid only while its run is active. */
export interface FlowCallbackInfo {
  nodeId: string;
  name: string;
  url: string;
}
```

In P8's `FlowStepTrace`, add after `route?: FlowRouteEval;`:

```ts
  poll?: FlowPollDetail;
  wait?: FlowWaitDetail;
```

In `FlowRunStartedEvent`, add after `total_nodes: number;`:

```ts
  /** Every Wait for callback node's URL. Omitted when the flow has none. Keys are camelCase. */
  callbacks?: FlowCallbackInfo[];
```

In `FlowStepProgressEvent`, add after `message: string;`:

```ts
  /** Structured progress. Omitted by older backends. Keys are camelCase. */
  live?: FlowLiveProgress;
```

In `src/types/pane-types.ts`, inside `FlowNodeDetail`, add after the `progress` field:

```ts
  /** Structured progress of a running node, such as a poll verdict or a countdown. */
  live?: import('@/lib/tauri-api').FlowLiveProgress;
```

and inside `FlowTab`, add after `runId?: string;`:

```ts
  /**
   * Callback URL per Wait for callback node id, set from `flow-run-started`.
   * Each URL holds a token, so it is kept only while the run is active.
   */
  callbackUrls?: Record<string, string>;
```

- [ ] **Step 4: Implement the store changes**

In `src/stores/pane-store.ts`, change the action types (lines 290-291) to:

```ts
  patchFlowNodeProgress: (
    tabId: string,
    nodeId: string,
    message: string,
    live?: FlowLiveProgress,
  ) => void;
  setFlowRunState: (tabId: string, runState: 'idle' | 'running' | 'done', runId?: string) => void;
  /** Stores the running flow's callback URLs. Ignored when no run is active. */
  setFlowCallbackUrls: (tabId: string, urls: Record<string, string> | undefined) => void;
```

and add `type FlowLiveProgress,` to the `@/lib/tauri-api` import at the top of the file (lines 20-30, after `type FlowEdge,`).

Replace `patchFlowNodeProgress` (lines 953-967):

```ts
  // Merges progress into the node's detail and leaves its status alone. The
  // next status patch with a detail replaces the detail, which clears it.
  patchFlowNodeProgress(tabId, nodeId, message) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
        const previous = tab.nodeDetail?.[nodeId];
        return {
          ...tab,
          nodeDetail: { ...tab.nodeDetail, [nodeId]: { ...previous, progress: message } },
        };
      }),
    });
  },
```

with:

```ts
  // Merges progress into the node's detail and leaves its status alone. The
  // next status patch with a detail replaces the detail, which clears it.
  // A finished run ignores late progress.
  patchFlowNodeProgress(tabId, nodeId, message, live) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (tab.runState === 'done') return tab;
        if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
        const previous = tab.nodeDetail?.[nodeId];
        // A ticker event has no call, so the last turned-down call stays shown.
        const kept = previous?.live?.lastRejected;
        const nextLive =
          live && !live.lastRejected && kept ? { ...live, lastRejected: kept } : live;
        return {
          ...tab,
          nodeDetail: {
            ...tab.nodeDetail,
            [nodeId]: { ...previous, progress: message, ...(nextLive ? { live: nextLive } : {}) },
          },
        };
      }),
    });
  },
```

Replace `setFlowRunState` (lines 969-980):

```ts
  setFlowRunState(tabId, runState, runId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        // A new run starts from a clean canvas. Otherwise the last run's
        // results stay on nodes this run skips or never reaches.
        if (runState === 'running') {
          return { ...tab, runState, runId, nodeStatus: {}, nodeDetail: {} };
        }
        return { ...tab, runState, runId };
      }),
    });
  },
```

with (when P2 already added `lastRun: undefined` to the running branch, keep it):

```ts
  setFlowRunState(tabId, runState, runId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        // A new run starts from a clean canvas. Otherwise the last run's
        // results stay on nodes this run skips or never reaches. Callback URLs
        // work only while their run is active, so every change drops them.
        if (runState === 'running') {
          return {
            ...tab,
            runState,
            runId,
            nodeStatus: {},
            nodeDetail: {},
            callbackUrls: undefined,
          };
        }
        return { ...tab, runState, runId, callbackUrls: undefined };
      }),
    });
  },

  setFlowCallbackUrls(tabId, urls) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) && tab.runState === 'running' ? { ...tab, callbackUrls: urls } : tab,
      ),
    });
  },
```

- [ ] **Step 5: Run the store tests**

Run: `yarn test src/stores/__tests__/pane-store.test.ts`
Expected: PASS, including the existing progress tests (their tabs are `idle`, not `done`).

- [ ] **Step 6: Write the failing toolbar and pane tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, after `'a remounted toolbar forwards progress for the resumed run'`, add:

```tsx
  it('passes live progress as a third argument only when present', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    const live = { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 };
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 2,
      max_attempts: 5,
      message: 'attempt 2/5 · condition false',
      live,
    });
    expect(onPatchProgress).toHaveBeenLastCalledWith(
      'node-a',
      'attempt 2/5 · condition false',
      live,
    );
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 3,
      max_attempts: 5,
      message: 'attempt 3/5',
    });
    expect(onPatchProgress.mock.calls.at(-1)).toEqual(['node-a', 'attempt 3/5']);
  });

  it('a remounted toolbar forwards live progress for the resumed run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress, tabRunState: 'running', tabRunId: 'run-9' });
    await waitFor(() => expect(progressHandler).toBeDefined());
    const live = { ignored: 2, remainingMs: 5000 };
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-9',
      node_id: 'w',
      attempt: null,
      max_attempts: null,
      message: 'waiting… 5s left · 2 ignored call(s)',
      live,
    });
    expect(onPatchProgress).toHaveBeenCalledWith('w', 'waiting… 5s left · 2 ignored call(s)', live);
  });

  it('hands the callback URLs from flow-run-started to onCallbackUrls', async () => {
    const onCallbackUrls = vi.fn();
    renderToolbar({ onCallbackUrls });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: 'run-1',
      flow_name: 'my-flow',
      collection: 'my-collection',
      total_nodes: 2,
      callbacks: [{ nodeId: 'w', name: 'payment', url: 'http://10.0.0.5:4000/cb/tok' }],
    });
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-1');
    expect(onCallbackUrls).toHaveBeenCalledWith({ w: 'http://10.0.0.5:4000/cb/tok' });
    // The run state is set first, because a new run drops older URLs.
    expect(onRunStateChange.mock.invocationCallOrder[0]).toBeLessThan(
      onCallbackUrls.mock.invocationCallOrder[0],
    );
  });

  it('does not call onCallbackUrls for a run without callbacks', async () => {
    const onCallbackUrls = vi.fn();
    renderToolbar({ onCallbackUrls });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-1');
    expect(onCallbackUrls).not.toHaveBeenCalled();
  });
```

In `src/components/flow/__tests__/FlowPane.test.tsx`, inside `describe('FlowPane run logs', ...)`, after `'stores flow-step-progress text on the node detail'`, add:

```tsx
  it('stores live progress and the callback URLs on the tab', async () => {
    let startedHandler: Parameters<typeof onFlowRunStarted>[0] | undefined;
    let progress: Parameters<typeof onFlowStepProgress>[0] | undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => undefined;
    });
    vi.mocked(onFlowStepProgress).mockImplementation(async (h) => {
      progress = h;
      return () => undefined;
    });
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
    render(<FlowPane tab={logTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progress).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: 'r1',
      flow_name: 'login-flow',
      collection: 'demo',
      total_nodes: 1,
      callbacks: [{ nodeId: 'n1', name: 'hook', url: 'http://h:1/cb/tok' }],
    });
    progress?.({
      type: 'flowStepProgress',
      run_id: 'r1',
      node_id: 'n1',
      attempt: null,
      max_attempts: null,
      message: 'waiting… 9s left · 0 ignored call(s)',
      live: { ignored: 0, remainingMs: 9000 },
    });
    const { root } = usePaneStore.getState();
    const stored = root.type === 'leaf' ? root.tabs.find((t) => t.id === logTab.id) : undefined;
    expect(stored && 'callbackUrls' in stored ? stored.callbackUrls : undefined).toEqual({
      n1: 'http://h:1/cb/tok',
    });
    expect(stored && 'nodeDetail' in stored ? stored.nodeDetail?.n1?.live : undefined).toEqual({
      ignored: 0,
      remainingMs: 9000,
    });
  });
```

- [ ] **Step 7: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Expected: the new tests FAIL (no third argument, no `onCallbackUrls`, nothing stored).

- [ ] **Step 8: Forward `live` and the URLs in the toolbar**

In `src/components/flow/FlowToolbar.tsx`:

1. Add `type FlowLiveProgress,`, `type FlowRunStartedEvent,` and `type FlowStepProgressEvent,` to the `@/lib/tauri-api` import list (keep it sorted).

2. In `FlowToolbarProps`, replace

```tsx
  // Receives progress text for a running node, such as "attempt 3/30".
  onPatchProgress?: (nodeId: string, message: string) => void;
```

with:

```tsx
  // Receives progress text for a running node, such as "attempt 3/30", and
  // its structured progress when the backend sent one.
  onPatchProgress?: (nodeId: string, message: string, live?: FlowLiveProgress) => void;
  // Receives the callback URL of each Wait node when this toolbar's run starts.
  onCallbackUrls?: (urls: Record<string, string>) => void;
```

and add `onCallbackUrls,` to the destructured parameters after `onPatchProgress,`.

3. After `detailFromStep`, add:

```tsx
// Forwards one progress event. The structured part is passed only when the
// backend sent it, so older payloads call the handler as before.
function forwardProgress(
  handler: FlowToolbarProps['onPatchProgress'],
  event: FlowStepProgressEvent,
) {
  if (!handler) return;
  if (event.live) handler(event.node_id, event.message, event.live);
  else handler(event.node_id, event.message);
}

// Maps the run-started callbacks to node id and URL.
function callbackUrlsFrom(event: FlowRunStartedEvent): Record<string, string> {
  return Object.fromEntries((event.callbacks ?? []).map((c) => [c.nodeId, c.url]));
}
```

4. In the resume effect, replace

```tsx
      onPatchProgressRef.current?.(event.node_id, event.message);
```

(inside `void onFlowStepProgress((event) => { if (event.run_id !== resumedRunId) return; ... })`) with:

```tsx
      forwardProgress(onPatchProgressRef.current, event);
```

5. In `handleRun`, replace

```tsx
      runId = event.run_id;
      setActiveRunId(event.run_id);
      onRunStateChange('running', event.run_id);
    });
```

with:

```tsx
      runId = event.run_id;
      setActiveRunId(event.run_id);
      onRunStateChange('running', event.run_id);
      // After the run state, because a new run drops older URLs.
      const urls = callbackUrlsFrom(event);
      if (Object.keys(urls).length > 0) onCallbackUrls?.(urls);
    });
```

and replace

```tsx
      if (runId === null || event.run_id !== runId) return;
      onPatchProgressRef.current?.(event.node_id, event.message);
```

with:

```tsx
      if (runId === null || event.run_id !== runId) return;
      forwardProgress(onPatchProgressRef.current, event);
```

In `src/components/flow/FlowPane.tsx`:

1. After `const setFlowRunState = usePaneStore((s) => s.setFlowRunState);` (line 55), add:

```tsx
  const setFlowCallbackUrls = usePaneStore((s) => s.setFlowCallbackUrls);
```

2. In the `<FlowToolbar` element, replace

```tsx
              onPatchProgress={(nodeId, message) => patchFlowNodeProgress(tab.id, nodeId, message)}
```

with:

```tsx
              onPatchProgress={(nodeId, message, live) =>
                patchFlowNodeProgress(tab.id, nodeId, message, live)
              }
              onCallbackUrls={(urls) => setFlowCallbackUrls(tab.id, urls)}
```

- [ ] **Step 9: Run the tests**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx src/components/flow/__tests__/FlowPane.test.tsx src/stores/__tests__/pane-store.test.ts`
Expected: PASS, including the existing two-argument progress tests.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/tauri-api.ts src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Suggested subject: `feat(flow): keep live progress and callback URLs for a running flow`.

---

### Task 2: Poll and Wait panels in the Last run tab

**Files:**
- Modify: `src/components/flow/properties/LastRunTab.tsx` (imports, new helpers before `export function LastRunTab`, body of `LastRunTab`)
- Test: `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowNodeDetail.live`, `FlowNodeDetail.trace.poll`, `FlowNodeDetail.trace.wait` (Task 1), `HeadersTable` and `BodyViewer` (already in `LastRunTab.tsx`).
- Produces: test ids `last-run-poll-live`, `last-run-poll`, `last-run-wait-live`, `last-run-wait`, `last-run-rejected`; internal hook `useSecondsLeft(live?: FlowLiveProgress): number | undefined`.

- [ ] **Step 1: Write the failing tests**

In `src/components/flow/properties/__tests__/LastRunTab.test.tsx`, change the first import line to:

```tsx
import { act, render, screen } from '@testing-library/react';
```

add `afterEach` to the vitest import, add `FlowRejectedCall` to the type import from `@/lib/tauri-api`, and append:

```tsx
describe('LastRunTab live progress', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  const poller = node({
    kind: 'Request',
    label: 'Job',
    source: { type: 'Saved', requestPath: 'jobs/status.yml' },
  });
  const wait = node({ kind: 'WaitForCallback', label: 'Hook', name: 'hook', timeoutMs: 60000 });
  const rejected: FlowRejectedCall = {
    method: 'POST',
    url: '/cb/…?event=pending',
    headers: [{ key: 'Authorization', value: '••••••' }],
    body: '{"event":"pending"}',
    bodyTruncated: true,
    reason: 'Accept when returned false.',
  };

  it('shows live poll progress with a countdown', () => {
    vi.useFakeTimers();
    render(
      <LastRunTab
        node={poller}
        status='running'
        detail={{
          progress: 'attempt 2/5 · condition false',
          live: { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll-live')).toHaveTextContent(
      'Last status 202 · condition false · 12s left',
    );
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByTestId('last-run-poll-live')).toHaveTextContent('9s left');
  });

  it('hides the live line once the node finished and stops its timer', () => {
    vi.useFakeTimers();
    const live = { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 };
    const { rerender } = render(
      <LastRunTab node={poller} status='running' detail={{ live }} />,
    );
    expect(vi.getTimerCount()).toBe(1);
    rerender(<LastRunTab node={poller} status='success' detail={{ statusCode: 200 }} />);
    expect(screen.queryByTestId('last-run-poll-live')).not.toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('shows the poll result after the run', () => {
    render(
      <LastRunTab
        node={poller}
        status='failed'
        detail={{
          error: 'condition not met after 30 attempts (60.0s)',
          trace: {
            poll: {
              attempts: 30,
              maxAttempts: 30,
              lastStatusCode: 202,
              conditionMet: false,
              elapsedMs: 60000,
              timeoutMs: 60000,
            },
          },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll')).toHaveTextContent(
      'Last status 202 · condition false · 30 of 30 attempts',
    );
  });

  it('says when a poll reached no verdict', () => {
    render(
      <LastRunTab
        node={poller}
        status='failed'
        detail={{
          trace: {
            poll: {
              attempts: 1,
              maxAttempts: 5,
              lastStatusCode: 200,
              elapsedMs: 10,
              timeoutMs: 60000,
            },
          },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll')).toHaveTextContent('no verdict');
  });

  it('shows ignored calls and a countdown while waiting', () => {
    vi.useFakeTimers();
    render(
      <LastRunTab
        node={wait}
        status='running'
        detail={{ live: { ignored: 2, remainingMs: 42000, lastRejected: rejected } }}
      />,
    );
    expect(screen.getByTestId('last-run-wait-live')).toHaveTextContent('2 ignored · 42s left');
    expect(screen.getByTestId('last-run-rejected')).toBeInTheDocument();
  });

  it('keeps the last rejected call collapsed until asked', async () => {
    render(
      <LastRunTab
        node={wait}
        status='running'
        detail={{ live: { ignored: 1, remainingMs: 42000, lastRejected: rejected } }}
      />,
    );
    expect(screen.queryByLabelText('Body viewer')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Last rejected call/ }));
    const section = screen.getByTestId('last-run-rejected');
    expect(section).toHaveTextContent('POST /cb/…?event=pending');
    expect(section).toHaveTextContent('Accept when returned false.');
    expect(section).toHaveTextContent('Authorization');
    expect(section).toHaveTextContent('••••••');
    expect(section).toHaveTextContent('Body truncated.');
    expect(await screen.findByLabelText('Body viewer')).toBeInTheDocument();
  });

  it('shows the wait result after a timeout', () => {
    render(
      <LastRunTab
        node={wait}
        status='failed'
        detail={{
          error: 'no matching callback within 60s (3 ignored)',
          trace: { wait: { ignored: 3, timeoutMs: 60000, lastRejected: rejected } },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-wait')).toHaveTextContent('3 ignored');
    expect(screen.getByTestId('last-run-rejected')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-wait-live')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: the new tests FAIL (no panels).

- [ ] **Step 3: Add the countdown hook and the panels**

In `src/components/flow/properties/LastRunTab.tsx`:

1. Add `FlowLiveProgress` and `FlowRejectedCall` to the type import from `@/lib/tauri-api`.

2. Before `export function LastRunTab`, add:

```tsx
// Whole seconds left before a running node gives up. Each live event restarts
// the count from the backend's `remainingMs`, and the hook ticks once a second.
function useSecondsLeft(live?: FlowLiveProgress): number | undefined {
  const [deadline, setDeadline] = useState<number | undefined>(undefined);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const remaining = live?.remainingMs;
    if (remaining === undefined) {
      setDeadline(undefined);
      return;
    }
    const start = Date.now();
    setNow(start);
    setDeadline(start + remaining);
  }, [live]);
  useEffect(() => {
    if (deadline === undefined) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [deadline]);
  return deadline === undefined ? undefined : Math.max(0, Math.ceil((deadline - now) / 1000));
}

function verdictText(conditionMet?: boolean): string {
  if (conditionMet === undefined) return 'no verdict';
  return conditionMet ? 'condition true' : 'condition false';
}

// While a poll runs: its last status, verdict and time left.
function PollLiveLine({ live }: { live: FlowLiveProgress }) {
  const seconds = useSecondsLeft(live);
  const parts: string[] = [];
  if (live.lastStatusCode !== undefined) parts.push(`Last status ${live.lastStatusCode}`);
  if (live.conditionMet !== undefined) parts.push(verdictText(live.conditionMet));
  if (seconds !== undefined) parts.push(`${seconds}s left`);
  if (parts.length === 0) return null;
  return (
    <p data-testid='last-run-poll-live' className='text-muted-foreground'>
      {parts.join(' · ')}
    </p>
  );
}

// While a callback wait runs: ignored calls and time left.
function WaitLiveLine({ live }: { live: FlowLiveProgress }) {
  const seconds = useSecondsLeft(live);
  const parts: string[] = [];
  if (live.ignored !== undefined) parts.push(`${live.ignored} ignored`);
  if (seconds !== undefined) parts.push(`${seconds}s left`);
  if (parts.length === 0) return null;
  return (
    <p data-testid='last-run-wait-live' className='text-muted-foreground'>
      {parts.join(' · ')}
    </p>
  );
}

// A call the wait turned down, collapsed because its body can be large.
function RejectedCallSection({ call }: { call: FlowRejectedCall }) {
  const [open, setOpen] = useState(false);
  return (
    <section data-testid='last-run-rejected'>
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger asChild>
          <Button type='button' variant='ghost' size='sm' className='h-6 px-1 text-xs'>
            <ChevronRight
              className={
                open ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'
              }
            />
            Last rejected call
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className='mt-1.5 space-y-1.5'>
            <p className='select-text font-mono text-[11px] [overflow-wrap:anywhere]'>
              {call.method} {call.url}
            </p>
            <p className='text-muted-foreground'>{call.reason}</p>
            <HeadersTable headers={call.headers} />
            <BodyViewer body={call.body} />
            {call.bodyTruncated && <p className='text-muted-foreground'>Body truncated.</p>}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </section>
  );
}
```

3. In `LastRunTab`, after the `{status === 'failed' && detail?.error && ( ... )}` block (the `last-run-error` box), add:

```tsx
      {node.kind.kind === 'Request' && status === 'running' && detail?.live && (
        <PollLiveLine live={detail.live} />
      )}
      {node.kind.kind === 'Request' && status !== 'running' && detail?.trace?.poll && (
        <p data-testid='last-run-poll' className='text-muted-foreground'>
          {[
            detail.trace.poll.lastStatusCode !== undefined
              ? `Last status ${detail.trace.poll.lastStatusCode}`
              : null,
            verdictText(detail.trace.poll.conditionMet),
            `${detail.trace.poll.attempts} of ${detail.trace.poll.maxAttempts} attempts`,
          ]
            .filter((part) => part !== null)
            .join(' · ')}
        </p>
      )}
      {node.kind.kind === 'WaitForCallback' && status === 'running' && detail?.live && (
        <>
          <WaitLiveLine live={detail.live} />
          {detail.live.lastRejected && <RejectedCallSection call={detail.live.lastRejected} />}
        </>
      )}
      {node.kind.kind === 'WaitForCallback' && status !== 'running' && detail?.trace?.wait && (
        <>
          <p data-testid='last-run-wait' className='text-muted-foreground'>
            {detail.trace.wait.ignored} ignored
          </p>
          {detail.trace.wait.lastRejected && (
            <RejectedCallSection call={detail.trace.wait.lastRejected} />
          )}
        </>
      )}
```

If `yarn check` reports `useExhaustiveDependencies` on the `[live]` dependency of `useSecondsLeft`, keep `[live]` (each live event must restart the count, even with an equal `remainingMs`) and add the same `// biome-ignore lint/correctness/useExhaustiveDependencies: each live event restarts the count.` comment form used in `NodePropertiesPanel.tsx:141`.

- [ ] **Step 4: Run the tests**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/properties/LastRunTab.tsx src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Suggested subject: `feat(flow): show poll verdicts and turned-down callbacks in Last run`.

---

### Task 3: Callback URL display and copy

**Files:**
- Create: `src/components/flow/CallbackUrlField.tsx`
- Create: `src/components/flow/__tests__/CallbackUrlField.test.tsx`
- Modify: `src/components/flow/nodes/WaitForCallbackNode.tsx` (data type :13-25, body after `NodeStatusCaption`)
- Modify: `src/components/flow/FlowCanvas.tsx` (`FlowCanvasProps` :48-80, `toRfNodes` :92-135, `FlowCanvasInner` params :225-242, `rfNodes` memo :301-314)
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (`editorFor` :23-88, props :90-136, Last run tab :236)
- Modify: `src/components/flow/properties/WaitForCallbackEditor.tsx` (props and body)
- Modify: `src/components/flow/properties/LastRunTab.tsx` (`CopyButton` :82-109, props, Wait section)
- Modify: `src/components/flow/FlowPane.tsx` (`<FlowCanvas` and `<NodePropertiesPanel` props)
- Test: `src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx`, `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`, `src/components/flow/properties/__tests__/LastRunTab.test.tsx`, `src/components/flow/__tests__/FlowPane.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowTab.callbackUrls` (Task 1).
- Produces: `<CallbackUrlField url={string} />`; `FlowCanvas` prop `callbackUrls?: Record<string, string>`; `WaitForCallbackNodeData.callbackUrl?: string`; `NodePropertiesPanel` prop `callbackUrl?: string`; `WaitForCallbackEditor` prop `callbackUrl?: string`; `LastRunTab` prop `callbackUrl?: string`.

- [ ] **Step 1: Write the failing field test**

Create `src/components/flow/__tests__/CallbackUrlField.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { copyTextAsync } from '@/lib/clipboard';
import { CallbackUrlField } from '../CallbackUrlField';

vi.mock('@/lib/clipboard', () => ({ copyTextAsync: vi.fn(async () => undefined) }));

describe('CallbackUrlField', () => {
  it('shows the URL and says how long it works', () => {
    render(<CallbackUrlField url='http://10.0.0.5:4000/cb/tok' />);
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
    expect(screen.getByText('Valid while the run is active.')).toBeInTheDocument();
  });

  it('copies the URL through the native-first clipboard helper', async () => {
    render(<CallbackUrlField url='http://10.0.0.5:4000/cb/tok' />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy callback URL' }));
    expect(copyTextAsync).toHaveBeenCalledTimes(1);
    await expect(vi.mocked(copyTextAsync).mock.calls[0][0]).resolves.toBe(
      'http://10.0.0.5:4000/cb/tok',
    );
  });
});
```

Run: `yarn test src/components/flow/__tests__/CallbackUrlField.test.tsx`
Expected: FAIL (cannot resolve `../CallbackUrlField`).

- [ ] **Step 2: Create the field**

Create `src/components/flow/CallbackUrlField.tsx`:

```tsx
import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { copyTextAsync } from '@/lib/clipboard';

const COPIED_MS = 1500;

// The live callback URL of a running flow's Wait node, with a copy button. The
// URL holds a token and works only while the run is active.
export function CallbackUrlField({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = () => {
    // Started in the click handler, so WebKit keeps the user activation.
    copyTextAsync(Promise.resolve(url)).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };

  return (
    <div data-testid='callback-url' className='nodrag nokey space-y-0.5'>
      <div className='flex items-center gap-1'>
        <code className='min-w-0 truncate font-mono text-[11px]'>{url}</code>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-5 w-5 shrink-0'
          aria-label='Copy callback URL'
          onClick={copy}
        >
          {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
        </Button>
      </div>
      <p className='text-[10px] text-muted-foreground'>Valid while the run is active.</p>
    </div>
  );
}
```

Run: `yarn test src/components/flow/__tests__/CallbackUrlField.test.tsx`
Expected: PASS.

- [ ] **Step 3: Write the failing display tests**

In `src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx`, add inside `describe('WaitForCallbackNode', ...)`:

```tsx
  it('shows the live callback URL while the run holds one', () => {
    renderNode({ status: 'running', callbackUrl: 'http://10.0.0.5:4000/cb/tok' });
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
  });

  it('shows no callback URL when the tab holds none', () => {
    renderNode({ status: 'success' });
    expect(screen.queryByTestId('callback-url')).not.toBeInTheDocument();
  });
```

In `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`, add inside `describe('Wait for callback editor', ...)`:

```tsx
  it('shows the live callback URL in Settings', () => {
    const url = 'http://10.0.0.5:4000/cb/tok';
    renderPanel(node('w', waitKind), { callbackUrl: url });
    expect(screen.getByTestId('callback-url')).toHaveTextContent(url);
  });

  it('shows the live callback URL on the Last run tab of a running wait', () => {
    const url = 'http://10.0.0.5:4000/cb/tok';
    renderPanel(node('w', waitKind), {
      callbackUrl: url,
      activeTab: 'last-run',
      status: 'running',
    });
    expect(screen.getByTestId('callback-url')).toHaveTextContent(url);
  });
```

In `src/components/flow/__tests__/FlowPane.test.tsx`, inside `describe('FlowPane run logs', ...)` (its `beforeEach` gives every run listener a fake unlisten, which the toolbar's resume effect needs for a `running` tab), add:

```tsx
  it('shows the running flow callback URL on its Wait node', () => {
    const running: FlowTab = {
      ...logTab,
      id: 'flow-wait-live',
      runState: 'running',
      runId: 'r1',
      callbackUrls: { w: 'http://10.0.0.5:4000/cb/tok' },
      nodes: [
        ...logTab.nodes,
        {
          id: 'w',
          kind: { kind: 'WaitForCallback', label: 'Hook', name: 'payment', timeoutMs: 60000 },
          position: { x: 0, y: 0 },
        },
      ],
    };
    usePaneStore.getState().openTab(running);
    render(<FlowPane tab={running} groupId={usePaneStore.getState().activeGroupId} />);
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
  });
```

Run: `yarn test src/components/flow`
Expected: the new display tests FAIL.

- [ ] **Step 4: Show the URL on the Wait node card**

In `src/components/flow/nodes/WaitForCallbackNode.tsx`:

1. Add `import { CallbackUrlField } from '../CallbackUrlField';` after the `@/lib/utils` import.

2. In `WaitForCallbackNodeData`, add after the `progress` field:

```tsx
  /** This run's callback URL. Set only while the run is active. */
  callbackUrl?: string;
```

3. Directly after the closing `/>` of `<NodeStatusCaption ... />`, add:

```tsx
      {data.callbackUrl && (
        <div className='px-2 pt-1'>
          <CallbackUrlField url={data.callbackUrl} />
        </div>
      )}
```

- [ ] **Step 5: Pass the URLs through the canvas**

In `src/components/flow/FlowCanvas.tsx`:

1. In `FlowCanvasProps`, add after `onOpenProperties?: (nodeId: string) => void;`:

```tsx
  // The running flow's callback URL per Wait node id. Absent when no run is active.
  callbackUrls?: Record<string, string>;
```

2. Add a last parameter to `toRfNodes`, after `savedPreviews: Record<string, SavedRequestPreview> = {},`:

```tsx
  callbackUrls?: Record<string, string>,
```

and in its `data` object, after the `...(preview && { ... }),` spread, add:

```tsx
        // A running flow's callback URL, for its Wait node.
        ...(n.kind.kind === 'WaitForCallback' &&
          callbackUrls?.[n.id] !== undefined && { callbackUrl: callbackUrls[n.id] }),
```

3. Add `callbackUrls,` to the destructured parameters of `FlowCanvasInner` after `onOpenProperties,`.

4. In the `rfNodes` memo, pass `callbackUrls` as the last argument of `toRfNodes(...)` (after `savedPreviews,`) and add `callbackUrls` to its dependency array.

- [ ] **Step 6: Pass the URL to the properties panel and the editor**

In `src/components/flow/properties/NodePropertiesPanel.tsx`:

1. Add a parameter to `editorFor` after `onChange: (kind: FlowNodeKind) => void,`:

```tsx
  callbackUrl?: string,
```

and change its WaitForCallback case to:

```tsx
    case 'WaitForCallback':
      return <WaitForCallbackEditor kind={kind} onChange={onChange} callbackUrl={callbackUrl} />;
```

2. Add `callbackUrl,` to the destructured props after `focusRequest = null,` and to the props type after `focusRequest?: { nodeId: string } | null;`:

```tsx
  // This run's callback URL for a Wait node. Absent when no run is active.
  callbackUrl?: string;
```

3. Change the editor call to `{editorFor(node, edges, nodes, collection, flowName, onChange, callbackUrl)}` and the Last run tab to:

```tsx
            <LastRunTab
              node={node}
              status={status}
              detail={detail}
              nodes={nodes}
              callbackUrl={callbackUrl}
            />
```

In `src/components/flow/properties/WaitForCallbackEditor.tsx`:

1. Add `import { CallbackUrlField } from '../CallbackUrlField';` after the `LabelField` import.

2. Change the component signature to:

```tsx
export function WaitForCallbackEditor({
  kind,
  onChange,
  callbackUrl,
}: {
  kind: WaitKind;
  onChange: (kind: FlowNodeKind) => void;
  // This run's URL for the node. Absent when no run is active.
  callbackUrl?: string;
}) {
```

3. After the paragraph that ends with `{callbackVariable(kind.name)}</code>\n        </p>` (inside the Name block), add:

```tsx
        {callbackUrl && <CallbackUrlField url={callbackUrl} />}
```

In `src/components/flow/FlowPane.tsx`, add to the `<FlowCanvas` element after `onEdgeEdit={openWireEditor}`:

```tsx
            callbackUrls={tab.callbackUrls}
```

and to the `<NodePropertiesPanel` element after `autoFocusLabel={panelNode.id === labelFocusNodeId}`:

```tsx
              callbackUrl={tab.callbackUrls?.[panelNode.id]}
```

- [ ] **Step 7: Show the URL in Last run and switch `CopyButton` to `copyTextAsync`**

In `src/components/flow/properties/LastRunTab.tsx`:

1. Add imports:

```tsx
import { copyTextAsync } from '@/lib/clipboard';
import { CallbackUrlField } from '../CallbackUrlField';
```

2. In `CopyButton`, replace

```tsx
  const copy = () => {
    navigator.clipboard?.writeText(text).then(
```

with:

```tsx
  const copy = () => {
    // Started in the click handler, so WebKit keeps the user activation.
    copyTextAsync(Promise.resolve(text)).then(
```

3. Add to `LastRunTabProps`:

```tsx
  /** This run's callback URL, for a Wait node. Absent when no run is active. */
  callbackUrl?: string;
```

add `callbackUrl` to the destructured props, and right after the WaitForCallback running block added in Task 2, add:

```tsx
      {node.kind.kind === 'WaitForCallback' && callbackUrl && (
        <section className='space-y-1'>
          <h4 className='font-medium'>Callback URL</h4>
          <CallbackUrlField url={callbackUrl} />
        </section>
      )}
```

In `src/components/flow/properties/__tests__/LastRunTab.test.tsx`, add next to the MonacoWrapper mock:

```tsx
vi.mock('@/lib/clipboard', () => ({ copyTextAsync: vi.fn(async () => undefined) }));
```

add `import { copyTextAsync } from '@/lib/clipboard';`, and replace the test `'copies the raw response body'` with:

```tsx
  it('copies the raw response body', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy response body' }));
    expect(copyTextAsync).toHaveBeenCalledTimes(1);
    await expect(vi.mocked(copyTextAsync).mock.calls[0][0]).resolves.toBe('{"token":"abc"}');
  });
```

and add inside `describe('LastRunTab live progress', ...)`:

```tsx
  it('shows the callback URL of a waiting node', () => {
    render(
      <LastRunTab
        node={wait}
        status='running'
        callbackUrl='http://10.0.0.5:4000/cb/tok'
        detail={{ live: { ignored: 0, remainingMs: 1000 } }}
      />,
    );
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
  });
```

(`vi.clearAllMocks` is not used in this file; if a later copy test is added, reset `copyTextAsync` in a `beforeEach`.)

- [ ] **Step 8: Run the tests**

Run: `yarn test src/components/flow src/stores/__tests__/pane-store.test.ts`
Expected: PASS.

- [ ] **Step 9: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/stores`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/CallbackUrlField.tsx src/components/flow/__tests__/CallbackUrlField.test.tsx src/components/flow/nodes/WaitForCallbackNode.tsx src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx src/components/flow/FlowCanvas.tsx src/components/flow/properties/NodePropertiesPanel.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx src/components/flow/properties/WaitForCallbackEditor.tsx src/components/flow/properties/LastRunTab.tsx src/components/flow/properties/__tests__/LastRunTab.test.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Suggested subject: `feat(flow): show and copy the live callback URL of a running flow`.

---

## Self-Review

- **Spec coverage:** TS types for `callbacks`, `live`, `poll` and `wait`; `onPatchProgress` third argument at both subscription sites; callback URLs stored on the tab as `callbackUrls` (not `nodeDetail`) and cleared on run end; `patchFlowNodeProgress` merges `live` (Task 1). Poll line "Last status 202 · condition false · 12s left" with a local countdown, Wait "N ignored" with countdown and a collapsible "Last rejected call" reusing `HeadersTable` and `BodyViewer`, and the same after the run from `trace` (Task 2). URL with copy via `copyTextAsync` in Last run, on the Wait node and in the Wait editor, with "valid while the run is active"; `CopyButton` switched to `copyTextAsync` (Task 3).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowLiveProgress`, `FlowPollDetail`, `FlowWaitDetail`, `FlowRejectedCall` and `FlowCallbackInfo` match P9's Rust fields in camelCase. `callbackUrls` is `Record<string, string>` in the tab, the store action, the toolbar callback, the canvas prop and the tests. The single-URL props are all named `callbackUrl`.
- **Review Focus coverage:** item 1 in Task 1 and Task 2, item 2 in Task 1, item 3 in Task 2, item 4 in Task 1 and Task 3, item 5 in Task 2.

Known follow-ups outside this plan: `WaitForCallbackNode`'s own "Copy variable" button still uses `navigator.clipboard` directly; switch it with the same helper when P15 touches clipboard code.
