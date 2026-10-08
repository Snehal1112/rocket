import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
  saveFlow,
} from '@/lib/tauri-api';
import { findTabInTree } from '@/lib/pane-utils';
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
    saveFlow: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const outputNode = {
  id: 'a',
  kind: { kind: 'Output' as const, label: 'Out a' },
  position: { x: 0, y: 0 },
};

const flowTab = (over: Partial<FlowTab> = {}): FlowTab => ({
  id: 'flow-save-1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [outputNode],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
  ...over,
});

function pressSave(tabId: string) {
  act(() => {
    window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId } }));
  });
}

function storedTab(id: string): FlowTab | undefined {
  const found = findTabInTree(usePaneStore.getState().root, id);
  return found && isFlowTab(found.tab) ? found.tab : undefined;
}

function openAndRender(tab: FlowTab) {
  usePaneStore.getState().openTab(tab);
  // The picker tab reads its flow list through TanStack Query.
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />
    </QueryClientProvider>,
  );
}

describe('FlowPane save shortcut', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockResolvedValue(undefined);
  });

  it('saves a dirty flow when its tab gets rocket:save-draft', async () => {
    openAndRender(flowTab());
    pressSave('flow-save-1');
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith('demo', {
        name: 'my-flow',
        nodes: [outputNode],
        edges: [],
      }),
    );
    await waitFor(() => expect(storedTab('flow-save-1')?.isDirty).toBe(false));
  });

  it('ignores the event for another tab', async () => {
    openAndRender(flowTab());
    pressSave('some-other-tab');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });

  it('does nothing for a clean flow', async () => {
    openAndRender(flowTab({ isDirty: false }));
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });

  it('saves once when the shortcut is pressed twice quickly', async () => {
    let finish: () => void = () => undefined;
    vi.mocked(saveFlow).mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          finish = () => resolve(undefined);
        }),
    );
    openAndRender(flowTab());
    pressSave('flow-save-1');
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).toHaveBeenCalledTimes(1);
    await act(async () => finish());
  });

  it('does nothing on a picker tab that has no flow yet', async () => {
    openAndRender(flowTab({ flowName: null, nodes: [] }));
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });
});

describe('FlowPane run auto-save', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    // Keep the run pending so the test ends mid-run.
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
  });

  it('tells the user when Run saved unsaved edits first', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    openAndRender(flowTab());
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(toast.info).toHaveBeenCalledWith('Flow saved before run.'));
    expect(runFlow).toHaveBeenCalled();
  });

  it('does not toast when the flow was already saved', async () => {
    openAndRender(flowTab({ isDirty: false }));
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('does not run or toast info when the auto-save fails', async () => {
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: bad graph');
    openAndRender(flowTab());
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
    expect(runFlow).not.toHaveBeenCalled();
  });
});
