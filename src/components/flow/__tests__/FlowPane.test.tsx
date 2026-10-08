import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
import { collectFlowAuthTokens } from '@/lib/flow-auth-preflight';
import { flowKeys } from '@/lib/queries/flow-queries';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import {
  getFlow,
  listCollections,
  listFlows,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
  saveFlow,
} from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    getFlow: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
    onFlowRunFinished: vi.fn(async () => () => undefined),
  };
});

// FlowToolbar reads the active global environment from the query cache.
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));

// The Auth pre-run step is covered by its own tests; here only its input matters.
vi.mock('@/lib/flow-auth-preflight', () => ({
  collectFlowAuthTokens: vi.fn(async () => ({})),
}));

// `usePaneStore.setState({ openFlowTab: vi.fn(...) })` in the 'FlowPane
// picker' tests below replaces the store's `openFlowTab` action permanently
// (zustand `set` merges, and `reset()` does not restore actions), so later
// tests that call `usePaneStore.getState().openFlowTab` would silently run
// that stub instead of the real implementation. Capture the genuine action
// here, before any test can override it, so the save/reload test below
// exercises real store behavior regardless of test order.
const realOpenFlowTab = usePaneStore.getState().openFlowTab;

function pickerTab(collectionName: string | null): FlowTab {
  return {
    id: 'flow-picker-1',
    title: 'Flow',
    isDirty: false,
    tabType: 'flow',
    collectionName,
    flowName: null,
    nodes: [],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
  };
}

// The picker reads its flow list through TanStack Query, so it needs a provider.
function renderPicker() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <FlowPane tab={pickerTab('demo')} groupId='g1' />
    </QueryClientProvider>,
  );
  return { client, ...view };
}

describe('FlowPane picker', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
  });

  it('lists flows for the tab collection without a manual pick', async () => {
    renderPicker();
    await waitFor(() => expect(listFlows).toHaveBeenCalledWith('demo'));
  });

  it('creates an empty flow and opens it', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    const closeTab = vi.fn();
    usePaneStore.setState({ openFlowTab, closeTab });

    renderPicker();
    await userEvent.type(screen.getByLabelText('New flow name'), 'Login flow');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() => expect(openFlowTab).toHaveBeenCalledWith('demo', 'Login flow'));
    expect(saveFlow).toHaveBeenCalledWith('demo', { name: 'Login flow', nodes: [], edges: [] });
    expect(closeTab).toHaveBeenCalledWith('flow-picker-1', 'g1');
  });

  it('keeps the picker open when creating the flow fails', async () => {
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: flow name is empty');
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    usePaneStore.setState({ openFlowTab });

    renderPicker();
    await userEvent.type(screen.getByLabelText('New flow name'), 'Broken flow');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(openFlowTab).not.toHaveBeenCalled();
  });

  it('shows a flow that appears after the flow list is invalidated', async () => {
    vi.mocked(listFlows).mockResolvedValueOnce([]).mockResolvedValue(['Alpha']);
    const { client } = renderPicker();
    expect(await screen.findByText('No flows yet')).toBeInTheDocument();

    await act(async () => {
      await client.invalidateQueries({ queryKey: flowKeys.collection('demo') });
    });

    expect(await screen.findByText('Select flow')).toBeInTheDocument();
  });

  it('invalidates the flow list after creating a flow', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    usePaneStore.setState({
      openFlowTab: vi.fn().mockResolvedValue(undefined),
      closeTab: vi.fn(),
    });
    const { client } = renderPicker();
    const spy = vi.spyOn(client, 'invalidateQueries');

    await userEvent.type(screen.getByLabelText('New flow name'), 'Login flow');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({ queryKey: flowKeys.collection('demo') }),
    );
  });

  it('rejects a name containing "::" without calling the backend', async () => {
    renderPicker();
    await userEvent.type(screen.getByLabelText('New flow name'), 'a::b');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("cannot contain '::'");
    expect(saveFlow).not.toHaveBeenCalled();
  });
});

describe('FlowPane save', () => {
  const outputNode = (id: string) => ({
    id,
    kind: { kind: 'Output' as const, label: `Out ${id}` },
    position: { x: 0, y: 0 },
  });
  const flowTab: FlowTab = {
    id: 'flow-open-1',
    title: 'Flow: my-flow',
    isDirty: true,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'my-flow',
    nodes: [outputNode('a'), outputNode('b'), outputNode('c')],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
  };

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    usePaneStore.getState().openTab(flowTab);
  });

  it('flags the node ids named in a cycle rejection', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2',
    );
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      const cards = screen.getAllByTestId('output-node-card');
      const flagged = cards.filter((c) => c.className.includes('ring-red-500'));
      expect(flagged.map((c) => c.textContent)).toEqual([
        'Out aRun whenValue—',
        'Out bRun whenValue—',
      ]);
    });
  });

  it('does not show a save error from another flow tab', async () => {
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow contains a cycle through node(s): a; edge(s): e1',
    );
    const groupId = usePaneStore.getState().activeGroupId;
    const { rerender } = render(<FlowPane tab={flowTab} groupId={groupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await userEvent.click(screen.getByLabelText('Edit Out a'));
    expect(await screen.findByTestId('node-save-error')).toBeInTheDocument();

    rerender(<FlowPane tab={{ ...flowTab, id: 'flow-other' }} groupId={groupId} />);
    await userEvent.click(screen.getByLabelText('Edit Out a'));
    expect(screen.getByTestId('node-properties-panel')).toBeInTheDocument();
    expect(screen.queryByTestId('node-save-error')).not.toBeInTheDocument();
  });

  it('flags the node named in a non-cycle validation error', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow is invalid: the If node needs exactly one input wire, found 0 — node(s): b; edge(s): ',
    );
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      const cards = screen.getAllByTestId('output-node-card');
      const flagged = cards.filter((c) => c.className.includes('ring-red-500'));
      expect(flagged.map((c) => c.textContent)).toEqual(['Out bRun whenValue—']);
    });
  });

  it('marks the tab clean after a successful save', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    const { root } = usePaneStore.getState();
    const tab = root.type === 'leaf' ? root.tabs.find((t) => t.id === flowTab.id) : undefined;
    expect(tab?.isDirty).toBe(false);
  });

  it('a saved flow reloads with the same nodes and edges when reopened', async () => {
    const store = new Map<string, { name: string; nodes: FlowNode[]; edges: FlowEdge[] }>();
    vi.mocked(saveFlow).mockImplementation(async (collection, flow) => {
      store.set(`${collection}/${flow.name}`, flow);
    });
    vi.mocked(getFlow).mockImplementation(async (collection, name) => {
      const saved = store.get(`${collection}/${name}`);
      if (!saved) throw new Error(`no such flow: ${collection}/${name}`);
      return saved;
    });

    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());

    // The pre-existing flowTab already carries flowName 'my-flow', so a naive
    // find-by-flowName would match it instead of the newly reopened tab.
    // Track which tab ids exist before reopening and require the match to be new.
    const rootBefore = usePaneStore.getState().root;
    const idsBefore = new Set(rootBefore.type === 'leaf' ? rootBefore.tabs.map((t) => t.id) : []);

    await realOpenFlowTab('demo', 'my-flow');

    const { root } = usePaneStore.getState();
    const reopened =
      root.type === 'leaf'
        ? root.tabs.find(
            (t) => t.tabType === 'flow' && t.flowName === 'my-flow' && !idsBefore.has(t.id),
          )
        : undefined;
    expect(reopened).toBeDefined();
    expect(reopened && 'nodes' in reopened ? reopened.nodes : undefined).toEqual(flowTab.nodes);
    expect(reopened && 'edges' in reopened ? reopened.edges : undefined).toEqual(flowTab.edges);
  });

  it('saves callbackHost when it is set', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const withHost: FlowTab = { ...flowTab, id: 'flow-host', callbackHost: 'host.docker.internal' };
    usePaneStore.getState().openTab(withHost);
    render(<FlowPane tab={withHost} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith(
        'demo',
        expect.objectContaining({ callbackHost: 'host.docker.internal' }),
      ),
    );
  });

  it('saves exactly name, nodes and edges when no callback host is set', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(vi.mocked(saveFlow).mock.calls[0][1]).toEqual({
      name: 'my-flow',
      nodes: flowTab.nodes,
      edges: [],
    });
  });

  it('shows the callback host setting only when the flow has a Wait node', () => {
    const { unmount } = render(
      <FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />,
    );
    expect(screen.queryByRole('button', { name: 'Callback host' })).toBeNull();
    unmount();

    const withWait: FlowTab = {
      ...flowTab,
      id: 'flow-wait',
      nodes: [
        ...flowTab.nodes,
        {
          id: 'w',
          kind: { kind: 'WaitForCallback', label: 'Hook', name: 'payment', timeoutMs: 60000 },
          position: { x: 0, y: 0 },
        },
      ],
    };
    usePaneStore.getState().openTab(withWait);
    render(<FlowPane tab={withWait} groupId={usePaneStore.getState().activeGroupId} />);
    expect(screen.getByRole('button', { name: 'Callback host' })).toBeInTheDocument();
  });
});

describe('FlowPane run logs', () => {
  const logTab: FlowTab = {
    id: 'flow-logs-1',
    title: 'Flow: login-flow',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'login-flow',
    nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Show token' }, position: { x: 0, y: 0 } }],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
  };

  beforeEach(() => {
    usePaneStore.getState().reset();
    useConsoleStore.getState().clearEntries();
    vi.clearAllMocks();
    usePaneStore.getState().openTab(logTab);
    const unlisten = async () => () => {
      // Fake unlisten.
    };
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
  });

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
    const stored = root.type === 'leaf' ? root.tabs.find((t) => t.id === logTab.id) : undefined;
    expect(stored && 'nodeDetail' in stored ? stored.nodeDetail?.n1?.progress : undefined).toBe(
      'attempt 2/10',
    );
  });

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

  it('pushes step logs from the run summary to the Console', async () => {
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'n1',
          status: 'success',
          statusCode: null,
          durationMs: null,
          error: null,
          value: null,
          logs: [{ level: 'warn', message: 'hi' }],
        },
      ],
    });
    render(<FlowPane tab={logTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() =>
      expect(useConsoleStore.getState().entries).toContainEqual(
        expect.objectContaining({
          kind: 'script',
          level: 'warn',
          message: 'hi',
          requestName: 'login-flow › Show token',
        }),
      ),
    );
  });

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

  it('passes the active global environment to the Auth pre-run step', async () => {
    vi.mocked(getActiveGlobalEnvName).mockReturnValue('g1');
    vi.mocked(runFlow).mockResolvedValue({ runId: 'r1', stoppedReason: 'completed', steps: [] });
    render(<FlowPane tab={logTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() =>
      expect(collectFlowAuthTokens).toHaveBeenCalledWith(
        expect.objectContaining({ flowName: 'login-flow', globalEnvName: 'g1' }),
      ),
    );
  });

  it('hands the toolbar a pre-run step that collects tokens for the latest nodes and active env', async () => {
    const authNode: FlowNode = {
      id: 'auth1',
      kind: {
        kind: 'Auth',
        label: 'Sign in',
        auth: { authType: 'bearer', token: '' },
        applyToInherit: true,
      },
      position: { x: 0, y: 0 },
    } as FlowNode;
    vi.mocked(getActiveGlobalEnvName).mockReturnValue('g1');
    vi.mocked(collectFlowAuthTokens).mockResolvedValue({ auth1: { accessToken: 'tok-abcdef123' } });
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    vi.mocked(runFlow).mockResolvedValue({ runId: 'r1', stoppedReason: 'completed', steps: [] });
    useEnvStore.setState({ activeEnvId: 'dev' });
    // The rendered `tab` prop is stale; the store holds the newer nodes.
    usePaneStore.getState().updateFlowNodes(logTab.id, [authNode]);

    render(<FlowPane tab={logTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    expect(collectFlowAuthTokens).toHaveBeenCalledTimes(1);
    expect(collectFlowAuthTokens).toHaveBeenCalledWith({
      collection: 'demo',
      flowName: 'login-flow',
      nodes: [authNode],
      environmentName: 'dev',
      globalEnvName: 'g1',
    });
    expect(runFlow).toHaveBeenCalledWith('demo', 'login-flow', 'dev', 'g1', {
      auth1: { accessToken: 'tok-abcdef123' },
    });
    useEnvStore.setState({ activeEnvId: null });
  });

  const runWithDebug = async (debugRequest: object, error: string | null = null) => {
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'n1',
          status: error ? 'failed' : 'success',
          statusCode: null,
          durationMs: null,
          error,
          value: null,
          debugRequest,
        },
      ],
    } as never);
    render(<FlowPane tab={logTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(useConsoleStore.getState().entries).toHaveLength(1));
    return useConsoleStore.getState().entries[0];
  };

  it('pushes a debug request from the run summary as a Console HTTP row', async () => {
    const entry = await runWithDebug({
      method: 'POST',
      url: 'https://x.test/login',
      headers: [{ key: 'Authorization', value: 'Bearer ••••••' }],
      body: '{"u":"a"}',
      response: {
        status: 400,
        statusText: 'Bad Request',
        durationMs: 12,
        sizeBytes: 20,
        headers: [],
        body: '{"error":"bad"}',
      },
    });
    expect(entry).toEqual(
      expect.objectContaining({
        kind: 'http',
        method: 'POST',
        url: 'https://x.test/login',
        status: 400,
        requestHeaders: [{ key: 'Authorization', value: 'Bearer ••••••' }],
        requestBody: '{"u":"a"}',
        responseBody: '{"error":"bad"}',
        requestName: 'login-flow › Show token',
      }),
    );
  });

  it('pushes an error row when the debug request has no response', async () => {
    const entry = await runWithDebug(
      { method: 'GET', url: 'https://x.test/down', headers: [], error: 'connection refused' },
      'connection refused',
    );
    expect(entry).toEqual(
      expect.objectContaining({
        kind: 'http',
        status: 0,
        statusText: 'Error',
        durationMs: 0,
        sizeBytes: 0,
        responseHeaders: [],
        responseBody: 'connection refused',
      }),
    );
  });
});
