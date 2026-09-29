# Flow Async P0 — Progress Event Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a running node report progress, such as `attempt 3/30`, and show that text on its card while it runs.

**Architecture:** A new `DomainEvent::FlowStepProgress` is published by `FlowExecutionService::publish_progress` and emitted to the frontend as `flow-step-progress`. `FlowToolbar` subscribes next to its existing step listeners and hands each message to `FlowPane`, which stores it as `nodeDetail[nodeId].progress` through a new `patchFlowNodeProgress` store action. `NodeStatusCaption` shows it while the node is `running`. A completed step replaces the detail, so the text clears on its own.

**Tech Stack:** Rust (`rocket-shared`, `rocket-app`, `src-tauri`), React + TypeScript, Zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §5.2. Index and locked contract: `docs/superpowers/plans/flow-async/00-index.md`.

## Global Constraints

- Domain event fields stay snake_case (`run_id`, `max_attempts`); only the `type` tag is camelCase (`flowStepProgress`). Do not camelCase event payloads in TS.
- `FlowStepProgress` never changes a node's status. Only `FlowStepStarted` / `FlowStepCompleted` do.
- UI: shadcn/ui primitives and `lucide-react` icons only. Zustand: narrow selectors, never destructure the whole store.
- Cargo commands always pass `-j4` and target one crate. Never run the full workspace test suite.
- Frontend checks: `yarn test src/components/flow src/stores`, `yarn tsc --noEmit`, `yarn check`.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill.

## Review Focus

1. A progress event for a different run (an older run still streaming, or another tab's run) must not touch this tab's nodes. Pinned by Task 2 `ignores flow-step-progress for another run`.
2. A progress event that arrives after the node completed must not bring back stale text on a finished card: the completed step replaces the detail, and the caption shows progress only while `running`. Pinned by Task 2 `NodeStatusCaption` test `hides progress once the node is no longer running`.
3. A toolbar remounted mid-run (tab hidden and shown again) must keep receiving progress. Pinned by Task 2 `a remounted toolbar forwards progress for the resumed run`.
4. A progress event for a node id that is not on the canvas (flow edited mid-run) must be a no-op, not a crash. Pinned by Task 2 store test `patchFlowNodeProgress for an unknown node id is a safe no-op`.
5. Tests that render `FlowPane` without mocking the new listener would call the real Tauri `listen` in jsdom and break the Run path. Pinned by Task 2 Step 1, which adds `onFlowStepProgress` to every `vi.mock('@/lib/tauri-api')` that already mocks `onFlowStepStarted`.

---

### Task 1: Progress event in the engine

**Files:**
- Modify: `crates/rocket-shared/src/events.rs:231-234` (add the variant after `FlowStepStarted`) and its tests module (next to `flow_run_started_wire_shape`, ~:716)
- Modify: `src-tauri/src/tauri_event_bus.rs:26-31` (flow event arms)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`publish_progress` next to `publish_started`, ~:748; test next to `empty_flow_publishes_started_then_finished_with_zero_counts`)

**Interfaces:**
- Consumes (plan 01): `NodeRunContext { run_id, node_id, cancel }`, `cancel_pair()`.
- Produces (locked, `00-index.md`):
  - `DomainEvent::FlowStepProgress { run_id: String, node_id: String, attempt: Option<u32>, max_attempts: Option<u32>, message: String }`
  - channel name `flow-step-progress`
  - `FlowExecutionService::publish_progress(&self, ctx: &NodeRunContext, attempt: Option<u32>, max_attempts: Option<u32>, message: String)`

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-shared/src/events.rs`, tests module, after `flow_run_started_wire_shape`:

```rust
    #[test]
    fn flow_step_progress_wire_shape() {
        let event = DomainEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: Some(3),
            max_attempts: Some(30),
            message: "attempt 3/30".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepProgress","run_id":"01J","node_id":"n","attempt":3,"max_attempts":30,"message":"attempt 3/30"}"#
        );
    }

    #[test]
    fn flow_step_progress_without_attempts_sends_nulls() {
        let event = DomainEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: None,
            max_attempts: None,
            message: "waiting… 42s left".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""attempt":null,"max_attempts":null"#), "got {json}");
    }
```

In `crates/rocket-app/src/flow_execution_service.rs`, tests module, after `empty_flow_publishes_started_then_finished_with_zero_counts`:

```rust
    #[test]
    fn publish_progress_sends_the_nodes_ids_and_message() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            Flow {
                name: "empty".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            &publisher,
        );
        let (_handle, cancel) = cancel_pair();
        let ctx = NodeRunContext {
            run_id: "run-1".to_string(),
            node_id: "poll".to_string(),
            cancel,
        };

        service.publish_progress(&ctx, Some(3), Some(30), "attempt 3/30".to_string());

        let events = publisher.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::FlowStepProgress {
                run_id,
                node_id,
                attempt,
                max_attempts,
                message,
            } => {
                assert_eq!(run_id, "run-1");
                assert_eq!(node_id, "poll");
                assert_eq!(*attempt, Some(3));
                assert_eq!(*max_attempts, Some(30));
                assert_eq!(message, "attempt 3/30");
            }
            other => panic!("expected FlowStepProgress, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_step_progress`
Expected: compile error, `no variant named 'FlowStepProgress' found for enum 'DomainEvent'`.

- [ ] **Step 3: Add the event variant**

In `crates/rocket-shared/src/events.rs`, right after the `FlowStepStarted { .. }` variant:

```rust
    /// Emitted while a node is still running, to report progress such as a
    /// poll attempt or a callback wait. It never changes the node's status.
    FlowStepProgress {
        run_id: String,
        node_id: String,
        /// 1-based attempt number, or `None` when attempts do not apply.
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        /// Short text shown on the node, such as "attempt 3/30".
        message: String,
    },
```

Run: `cargo test -j4 -p rocket-shared flow_step_progress`
Expected: 2 passed.

- [ ] **Step 4: Map the event to its Tauri channel**

In `src-tauri/src/tauri_event_bus.rs`, after `DomainEvent::FlowStepStarted { .. } => "flow-step-started",`:

```rust
            DomainEvent::FlowStepProgress { .. } => "flow-step-progress",
```

Run: `cargo check -j4 -p rocket --tests`
Expected: compiles. (Without this arm the exhaustive `match` fails with `non-exhaustive patterns: '&DomainEvent::FlowStepProgress { .. }' not covered`.)

- [ ] **Step 5: Add `publish_progress`**

In `crates/rocket-app/src/flow_execution_service.rs`, after `fn publish_started`:

```rust
    /// Reports progress for the node `ctx` belongs to. Waiting nodes (plans
    /// 04 and 08) call this; until then only the tests do.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn publish_progress(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
    ) {
        self.events.publish(DomainEvent::FlowStepProgress {
            run_id: ctx.run_id.clone(),
            node_id: ctx.node_id.clone(),
            attempt,
            max_attempts,
            message,
        });
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app publish_progress`
Expected: 1 passed.

Run: `cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): add a step progress event`.

---

### Task 2: Show progress on the node card

**Files:**
- Modify: `src/lib/tauri-api.ts` (after `onFlowStepCompleted`, ~:1931)
- Modify: `src/types/pane-types.ts:156-165` (`FlowNodeDetail`)
- Modify: `src/stores/pane-store.ts` (interface ~:234, implementation after `patchFlowNodeStatus` ~:804)
- Modify: `src/components/flow/FlowToolbar.tsx` (props, resumed-run effect ~:108-132, `handleRun` ~:159-167)
- Modify: `src/components/flow/FlowPane.tsx:48` (selector) and `:351-357` (toolbar props)
- Modify: `src/components/flow/nodes/NodeStatusCaption.tsx`
- Modify: `src/components/flow/nodes/RequestNode.tsx:11-20,86`, `InputNode.tsx:9-16,35`, `OutputNode.tsx:13-22,62`, `IfNode.tsx:14-23,60`, `SwitchNode.tsx:17-26,108` (add `progress?: string` to each data type and pass it)
- Test: `src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx` (create)
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx` (extend)
- Test: `src/stores/__tests__/pane-store.test.ts` (extend `Flow tab actions`, ~:833)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (extend)
- Test: `src/components/flow/__tests__/FlowPane.test.tsx` (mock + one test in `FlowPane run logs`)

**Interfaces:**
- Consumes (Task 1): Tauri event `flow-step-progress` with the snake_case payload above.
- Produces (locked, `00-index.md`):
  - `export interface FlowStepProgressEvent { type: 'flowStepProgress'; run_id: string; node_id: string; attempt: number | null; max_attempts: number | null; message: string }`
  - `export const onFlowStepProgress: (handler: (e: FlowStepProgressEvent) => void) => Promise<UnlistenFn>`
  - `FlowNodeDetail.progress?: string`
  - `patchFlowNodeProgress: (tabId: string, nodeId: string, message: string) => void`
  - `FlowToolbar` prop `onPatchProgress?: (nodeId: string, message: string) => void` (optional so existing renders stay valid)
  - `NodeStatusCaption` prop `progress?: string`, rendered as `data-testid='node-progress'` while `status === 'running'`
  - Every node data type gains `progress?: string`. `FlowCanvas.toRfNodes` already spreads `nodeDetail[n.id]` into `data`, so no canvas change is needed. Plan 09's `WaitForCallbackNode` must pass `data.progress` the same way.

- [ ] **Step 1: Mock the new listener in the existing test files**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, add to the `vi.mock('@/lib/tauri-api', ...)` return object:

```ts
    onFlowStepProgress: vi.fn(),
```

add next to the other handler variables:

```ts
let progressHandler: Parameters<typeof tauriApi.onFlowStepProgress>[0] | undefined;
```

and in `beforeEach`, next to the other `mockImplementation` calls:

```ts
    progressHandler = undefined;
    vi.mocked(tauriApi.onFlowStepProgress).mockImplementation(async (h) => {
      progressHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
```

In `src/components/flow/__tests__/FlowPane.test.tsx`, add `onFlowStepProgress` to the named import from `@/lib/tauri-api` and to the `vi.mock` return object (`onFlowStepProgress: vi.fn(),`), and in the `FlowPane run logs` `beforeEach` add:

```ts
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
```

- [ ] **Step 2: Write the failing tests**

Create `src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { NodeStatusCaption } from '../NodeStatusCaption';

describe('NodeStatusCaption', () => {
  it('shows progress while the node is running', () => {
    render(<NodeStatusCaption status='running' progress='attempt 3/30' />);
    expect(screen.getByTestId('node-progress')).toHaveTextContent('attempt 3/30');
  });

  it('shows nothing for a running node without progress', () => {
    const { container } = render(<NodeStatusCaption status='running' />);
    expect(container).toBeEmptyDOMElement();
  });

  it('hides progress once the node is no longer running', () => {
    render(<NodeStatusCaption status='success' progress='attempt 3/30' />);
    expect(screen.queryByTestId('node-progress')).toBeNull();
  });
});
```

In `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, add a test inside the top-level `describe` (reuse the file's `renderNode` and its base data; the base kind object is named `baseKind`):

```tsx
  it('shows progress while running', () => {
    renderNode({ kind: baseKind, status: 'running', progress: 'attempt 2/5' });
    expect(screen.getByTestId('node-progress')).toHaveTextContent('attempt 2/5');
  });
```

In `src/stores/__tests__/pane-store.test.ts`, inside `describe('Flow tab actions', ...)`, after `patchFlowNodeStatus keeps skip reason, branch and value in the detail`:

```ts
  async function flowTabWithNode() {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    return tabId;
  }

  it('patchFlowNodeProgress merges progress into the node detail', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'running', { statusCode: 202 });
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'n1', 'attempt 3/30');
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus.n1).toBe('running');
    expect(tab?.nodeDetail?.n1).toEqual({ statusCode: 202, progress: 'attempt 3/30' });
  });

  it('a completed status patch clears the progress text', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'n1', 'attempt 3/30');
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', { statusCode: 200 });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ statusCode: 200 });
  });

  it('patchFlowNodeProgress for an unknown node id is a safe no-op', async () => {
    const tabId = await flowTabWithNode();
    const before = findFirstFlowTab();
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'does-not-exist', 'attempt 1/2');
    expect(findFirstFlowTab()?.nodeDetail).toEqual(before?.nodeDetail);
  });
```

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, at the end of `describe('FlowToolbar', ...)`:

```tsx
  it('forwards flow-step-progress messages for the active run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 3,
      max_attempts: 30,
      message: 'attempt 3/30',
    });
    expect(onPatchProgress).toHaveBeenCalledWith('node-a', 'attempt 3/30');
  });

  it('ignores flow-step-progress for another run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'other-run',
      node_id: 'node-a',
      attempt: null,
      max_attempts: null,
      message: 'waiting',
    });
    expect(onPatchProgress).not.toHaveBeenCalled();
  });

  it('a remounted toolbar forwards progress for the resumed run', async () => {
    const onPatchProgress = vi.fn();
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onPatchProgress={onPatchProgress}
        onRunStateChange={onRunStateChange}
        tabRunState='running'
        tabRunId='run-9'
      />,
    );
    await waitFor(() => expect(progressHandler).toBeDefined());
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-9',
      node_id: 'node-a',
      attempt: 1,
      max_attempts: 5,
      message: 'attempt 1/5',
    });
    expect(onPatchProgress).toHaveBeenCalledWith('node-a', 'attempt 1/5');
  });
```

In `src/components/flow/__tests__/FlowPane.test.tsx`, inside `describe('FlowPane run logs', ...)`:

```tsx
  it('stores flow-step-progress text on the node detail', async () => {
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
    // Keep the run pending so the progress arrives mid-run.
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
    });
    progress?.({
      type: 'flowStepProgress',
      run_id: 'r1',
      node_id: 'n1',
      attempt: 2,
      max_attempts: 10,
      message: 'attempt 2/10',
    });
    const { root } = usePaneStore.getState();
    const stored =
      root.type === 'leaf' ? root.tabs.find((t) => t.id === logTab.id) : undefined;
    expect(stored && 'nodeDetail' in stored ? stored.nodeDetail?.n1?.progress : undefined).toBe(
      'attempt 2/10',
    );
  });
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test src/components/flow src/stores/__tests__/pane-store.test.ts`
Expected: FAIL. `NodeStatusCaption` tests fail with `Unable to find an element by: [data-testid="node-progress"]`; the store tests fail with `patchFlowNodeProgress is not a function`; the toolbar tests fail because `onPatchProgress` is never called; `yarn tsc --noEmit` reports `Property 'onFlowStepProgress' does not exist`.

- [ ] **Step 4: Add the TS event, detail field and store action**

In `src/lib/tauri-api.ts`, after `onFlowStepCompleted`:

```ts
export interface FlowStepProgressEvent {
  type: 'flowStepProgress';
  run_id: string;
  node_id: string;
  /** 1-based attempt number, or null when attempts do not apply. */
  attempt: number | null;
  max_attempts: number | null;
  /** Short text shown on the node, such as "attempt 3/30". */
  message: string;
}

export const onFlowStepProgress = (
  handler: (event: FlowStepProgressEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepProgressEvent>('flow-step-progress', (e) => handler(e.payload));
```

In `src/types/pane-types.ts`, add to `FlowNodeDetail`:

```ts
  /** Progress text of a running node, such as "attempt 3/30". */
  progress?: string;
```

In `src/stores/pane-store.ts`, add to the state interface after `patchFlowNodeStatus`:

```ts
  patchFlowNodeProgress: (tabId: string, nodeId: string, message: string) => void;
```

and the implementation after `patchFlowNodeStatus(...) { ... },`:

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

- [ ] **Step 5: Subscribe in `FlowToolbar` and wire `FlowPane`**

In `src/components/flow/FlowToolbar.tsx`:
- add `onFlowStepProgress,` to the `@/lib/tauri-api` import list;
- add to `FlowToolbarProps`, after `onPatchStatus`:

```ts
  // Receives progress text for a running node, such as "attempt 3/30".
  onPatchProgress?: (nodeId: string, message: string) => void;
```

- destructure `onPatchProgress` in the component parameters;
- after `onPatchStatusRef.current = onPatchStatus;` add:

```ts
  const onPatchProgressRef = useRef(onPatchProgress);
  onPatchProgressRef.current = onPatchProgress;
```

- in the resumed-run effect, declare `let unlistenProgress: UnlistenFn | undefined;` next to the other two, add before `return () => {`:

```ts
    void onFlowStepProgress((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchProgressRef.current?.(event.node_id, event.message);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenProgress = fn;
    });
```

  and call `unlistenProgress?.();` in its cleanup.
- in `handleRun`, after `const unlistenStep = await onFlowStepCompleted(...)`:

```ts
    const unlistenProgress = await onFlowStepProgress((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchProgressRef.current?.(event.node_id, event.message);
    });
```

  and change the refs line to `unlistenRefs.current = [unlistenStarted, unlistenStepStarted, unlistenStep, unlistenProgress];`.

In `src/components/flow/FlowPane.tsx`, after `const patchFlowNodeStatus = usePaneStore((s) => s.patchFlowNodeStatus);`:

```ts
  const patchFlowNodeProgress = usePaneStore((s) => s.patchFlowNodeProgress);
```

and on `<FlowToolbar`, after `onPatchStatus={...}`:

```tsx
              onPatchProgress={(nodeId, message) =>
                patchFlowNodeProgress(tab.id, nodeId, message)
              }
```

- [ ] **Step 6: Show progress in `NodeStatusCaption` and pass it from every node**

Replace `src/components/flow/nodes/NodeStatusCaption.tsx` with:

```tsx
import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { nodeStatusCaption } from './nodeStatus';

// Shows why a node did not run normally: the error of a failed node, or the
// reason for a skipped one. While a node runs, it shows the node's progress
// text, such as "attempt 3/30". Long errors wrap and stop at three lines.
export function NodeStatusCaption({
  status,
  skipReason,
  error,
  progress,
}: {
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  progress?: string;
}) {
  if (status === 'failed') {
    const message = error ?? 'Error';
    return (
      <div
        data-testid='node-error'
        title={message}
        className='line-clamp-3 break-words px-2 pt-1 text-red-600'
      >
        ✕ {message}
      </div>
    );
  }
  if (status === 'running' && progress) {
    return (
      <div data-testid='node-progress' className='truncate px-2 pt-1 text-blue-500'>
        {progress}
      </div>
    );
  }
  const caption = nodeStatusCaption(status, { skipReason });
  if (!caption) return null;
  return (
    <div data-testid='node-status-caption' className='px-2 pt-1 italic text-muted-foreground'>
      {caption}
    </div>
  );
}
```

In each node component, add to its data type (after `error?: string;` or `skipReason`):

```ts
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
```

and add `progress={data.progress}` to its `<NodeStatusCaption ... />`:
- `RequestNode.tsx:86` → `<NodeStatusCaption status={status} skipReason={data.skipReason} progress={data.progress} />`
- `InputNode.tsx:35`, `OutputNode.tsx:62` → `... error={data.error} progress={data.progress} />`
- `IfNode.tsx:60`, `SwitchNode.tsx:108` → `... error={data.error} progress={data.progress} />`

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test src/components/flow src/stores/__tests__/pane-store.test.ts`
Expected: PASS, including the 3 caption tests, the RequestNode progress test, 3 store tests, 3 toolbar tests and the FlowPane progress test.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no Biome errors. If formatting differs, run `yarn format` on the touched files and re-run `yarn check`.

- [ ] **Step 8: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): show step progress on running nodes`.
