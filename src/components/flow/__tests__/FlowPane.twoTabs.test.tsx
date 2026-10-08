import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import {
  cancelFlowRun,
  type FlowRunStartedEvent,
  type FlowRunSummary,
  type FlowStepCompletedEvent,
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    runFlow: vi.fn(),
    cancelFlowRun: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

// Delivers every event to every subscriber, like Tauri's global listen().
// Run ids are the real UUIDs from newFlowRunId.
function eventBus<T>() {
  const handlers = new Set<(event: T) => void>();
  return {
    listen: async (handler: (event: T) => void) => {
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    emit: (event: T) =>
      act(() => {
        for (const handler of [...handlers]) handler(event);
      }),
    size: () => handlers.size,
  };
}

let startedBus = eventBus<FlowRunStartedEvent>();
let completedBus = eventBus<FlowStepCompletedEvent>();
// Ends a pending run_flow call, by the run id it was sent with.
const resolvers = new Map<string, (summary: FlowRunSummary) => void>();

const flowTab = (id: string): FlowTab => ({
  id,
  tabType: 'flow',
  title: 'Flow: shared',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'shared',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

function stored(id: string): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, id);
  if (!found || !isFlowTab(found.tab)) throw new Error(`Expected flow tab ${id}`);
  return found.tab;
}

function Pane({ id }: { id: string }) {
  const tab = usePaneStore((s) => {
    const found = findTabInTree(s.root, id);
    return found && isFlowTab(found.tab) ? found.tab : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

function renderBoth() {
  render(
    <>
      <div data-testid='pane-a'>
        <Pane id='flow-a' />
      </div>
      <div data-testid='pane-b'>
        <Pane id='flow-b' />
      </div>
    </>,
  );
}

const pane = (which: 'a' | 'b') => within(screen.getByTestId(`pane-${which}`));
const sentRunId = (call: number) => vi.mocked(runFlow).mock.calls[call]?.[5]?.runId ?? '';

const startedEvent = (runId: string): FlowRunStartedEvent => ({
  type: 'flowRunStarted',
  run_id: runId,
  flow_name: 'shared',
  collection: 'demo',
  total_nodes: 1,
});

const completedEvent = (runId: string, status: 'success' | 'failed'): FlowStepCompletedEvent => ({
  type: 'flowStepCompleted',
  run_id: runId,
  node_id: 'out1',
  status,
  status_code: null,
  duration_ms: 3,
  error: status === 'failed' ? 'boom' : null,
  value: null,
});

const summary = (runId: string, status: 'success' | 'failed'): FlowRunSummary => ({
  runId,
  stoppedReason: 'completed',
  steps: [{ nodeId: 'out1', status, statusCode: null, durationMs: 3, error: null, value: null }],
});

// Clicks Run in one pane and returns the run id that pane sent.
async function runIn(which: 'a' | 'b', call: number): Promise<string> {
  await userEvent.click(pane(which).getByRole('button', { name: 'Run' }));
  await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(call + 1));
  return sentRunId(call);
}

describe('two tabs running the same flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab('flow-a'));
    usePaneStore.getState().openTab(flowTab('flow-b'));
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(cancelFlowRun).mockResolvedValue(undefined);
    startedBus = eventBus<FlowRunStartedEvent>();
    completedBus = eventBus<FlowStepCompletedEvent>();
    const quiet = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(startedBus.listen);
    vi.mocked(onFlowStepCompleted).mockImplementation(completedBus.listen);
    vi.mocked(onFlowRunFinished).mockImplementation(quiet);
    vi.mocked(onFlowStepStarted).mockImplementation(quiet);
    vi.mocked(onFlowStepProgress).mockImplementation(quiet);
    resolvers.clear();
    // Each run stays pending until the test ends it by id.
    vi.mocked(runFlow).mockImplementation(
      (_collection, _flowName, _env, _globalEnv, _tokens, options) =>
        new Promise((resolve) => {
          resolvers.set(options?.runId ?? '', resolve);
        }),
    );
  });

  it('each tab follows only its own run, whichever starts first', async () => {
    renderBoth();
    const idA = await runIn('a', 0);
    const idB = await runIn('b', 1);
    expect(idA).not.toBe('');
    expect(idA).not.toBe(idB);
    expect(stored('flow-a').pendingRunId).toBe(idA);
    expect(stored('flow-b').pendingRunId).toBe(idB);

    // B's run announces itself first. Tab A must not adopt it.
    startedBus.emit(startedEvent(idB));
    expect(stored('flow-b').runState).toBe('running');
    expect(stored('flow-b').runId).toBe(idB);
    expect(stored('flow-a').runState).toBe('idle');
    expect(stored('flow-a').pendingRunId).toBe(idA);

    startedBus.emit(startedEvent(idA));
    expect(stored('flow-a').runId).toBe(idA);

    completedBus.emit(completedEvent(idA, 'failed'));
    completedBus.emit(completedEvent(idB, 'success'));
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'failed' });
    expect(stored('flow-b').nodeStatus).toEqual({ out1: 'success' });
  });

  it("Stop in one tab cancels only that tab's run, before and after it started", async () => {
    renderBoth();
    const idA = await runIn('a', 0);
    const idB = await runIn('b', 1);

    // No event yet: the tab already knows its run.
    await userEvent.click(pane('b').getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledTimes(1);
    expect(cancelFlowRun).toHaveBeenLastCalledWith(idB);

    startedBus.emit(startedEvent(idB));
    startedBus.emit(startedEvent(idA));
    await userEvent.click(pane('a').getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledTimes(2);
    expect(cancelFlowRun).toHaveBeenLastCalledWith(idA);
  });

  it('events of a finished run change nothing, in the next run too', async () => {
    renderBoth();
    const first = await runIn('a', 0);
    startedBus.emit(startedEvent(first));
    await act(async () => {
      resolvers.get(first)?.(summary(first, 'success'));
    });
    await waitFor(() => expect(stored('flow-a').runState).toBe('done'));
    completedBus.emit(completedEvent(first, 'failed'));
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'success' });

    await waitFor(() => expect(pane('a').getByRole('button', { name: 'Run' })).toBeEnabled());
    const second = await runIn('a', 1);
    expect(second).not.toBe(first);
    startedBus.emit(startedEvent(second));
    completedBus.emit(completedEvent(first, 'failed'));
    expect(stored('flow-a').runId).toBe(second);
    expect(stored('flow-a').nodeStatus).toEqual({});
  });

  it('a refused start keeps the previous run id and results, and clears the pending id', async () => {
    render(<Pane id='flow-a' />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    const id = sentRunId(0);
    startedBus.emit(startedEvent(id));
    completedBus.emit(completedEvent(id, 'success'));
    await act(async () => {
      resolvers.get(id)?.(summary(id, 'success'));
    });
    await waitFor(() => expect(stored('flow-a').runState).toBe('done'));
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'success' });

    vi.mocked(runFlow).mockRejectedValueOnce('A run with this id exists');
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(stored('flow-a').pendingRunId).toBeUndefined());
    expect(stored('flow-a').runId).toBe(id);
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'success' });
    expect(stored('flow-a').runState).toBe('done');
  });

  it('a tab whose pane remounts before its run starts still follows it', async () => {
    const view = render(<Pane id='flow-a' />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    const id = sentRunId(0);
    view.unmount();
    expect(startedBus.size()).toBe(0);
    expect(stored('flow-a').pendingRunId).toBe(id);

    render(<Pane id='flow-a' />);
    await waitFor(() => expect(startedBus.size()).toBe(1));
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    startedBus.emit(startedEvent(id));
    expect(stored('flow-a').runState).toBe('running');
    expect(stored('flow-a').runId).toBe(id);
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledWith(id);
  });

  it('runs started with Ctrl+Enter get their own ids too', async () => {
    renderBoth();
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId: 'flow-a' } }));
    });
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId: 'flow-b' } }));
    });
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(2));
    const a = stored('flow-a').pendingRunId;
    const b = stored('flow-b').pendingRunId;
    expect(a).toBeDefined();
    expect(b).toBeDefined();
    expect(a).not.toBe(b);
    expect([sentRunId(0), sentRunId(1)].sort()).toEqual([a, b].sort());
  });
});
