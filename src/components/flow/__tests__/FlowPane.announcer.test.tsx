import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
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

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const tabId = 'flow-announce-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: announce',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'announce',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const polite = () => screen.getByTestId('flow-announcer-status');
const alert = () => screen.getByTestId('flow-announcer-alert');

describe('FlowPane run announcer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('has both live regions in the document, empty, before anything happens', () => {
    render(<Harness />);
    expect(polite()).toBeEmptyDOMElement();
    expect(alert()).toBeEmptyDOMElement();
    expect(polite()).toHaveAttribute('aria-live', 'polite');
    expect(polite()).toHaveAttribute('aria-atomic', 'true');
    expect(alert()).toHaveAttribute('role', 'alert');
  });

  it('announces a run and keeps the same region elements throughout', () => {
    render(<Harness />);
    const status = polite();
    const failure = alert();
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1'));
    expect(polite()).toHaveTextContent('Run started.');
    act(() =>
      usePaneStore.getState().patchFlowNodeStatus(tabId, 'out1', 'success', { value: '"ok"' }),
    );
    expect(polite()).toHaveTextContent('Result succeeded.');
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1'));
    expect(polite()).toHaveTextContent('Run finished: 1 succeeded, 0 failed, 0 skipped.');
    expect(polite()).toBe(status);
    expect(alert()).toBe(failure);
  });

  it('puts a failed node in the alert region and clears it on the next run', () => {
    render(<Harness />);
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1'));
    act(() =>
      usePaneStore.getState().patchFlowNodeStatus(tabId, 'out1', 'failed', { error: 'boom' }),
    );
    expect(alert()).toHaveTextContent('Result failed: boom.');
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1'));
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2'));
    expect(alert()).toBeEmptyDOMElement();
  });

  it('does not announce an old result when the tab is opened', () => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({
      ...baseTab,
      runState: 'done',
      runId: 'run-0',
      nodeStatus: { out1: 'failed' },
      nodeDetail: { out1: { error: 'old' } },
    });
    render(<Harness />);
    expect(polite()).toBeEmptyDOMElement();
    expect(alert()).toBeEmptyDOMElement();
  });
});
