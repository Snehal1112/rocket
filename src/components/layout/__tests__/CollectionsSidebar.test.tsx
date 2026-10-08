import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { flowAuthKey } from '@/lib/flow-auth';
import { collectAllTabs, findScriptTab } from '@/lib/pane-utils';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { useWorkspaceStore } from '@/stores/workspace-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listWorkspaces: vi.fn(),
    getCollectionSummaries: vi.fn(),
    deleteScriptFile: vi.fn(),
    listFlows: vi.fn().mockResolvedValue([]),
    deleteFlow: vi.fn(),
    readScriptFile: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

// Marker component so the test can assert on mount/unmount without pulling in
// HistoryPanel's own dependencies (tauri-api's listHistory/searchHistory).
vi.mock('@/components/history/HistoryPanel', () => ({
  HistoryPanel: () => <div data-testid='history-panel-marker' />,
}));

vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setQueryClient(queryClient);
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('CollectionsSidebar History panel deferred mount', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listCollections).mockResolvedValue([]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([]);
  });

  it('does not mount HistoryPanel on initial render (Collections tab active)', async () => {
    render(<CollectionsSidebar />, { wrapper });

    // Positive control: wait for the sidebar to finish its initial data load
    // before asserting on absence, so the check below isn't trivially true.
    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    expect(screen.queryByTestId('history-panel-marker')).not.toBeInTheDocument();
  });

  it('mounts HistoryPanel after the History tab is activated for the first time', async () => {
    render(<CollectionsSidebar />, { wrapper });

    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));

    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();
  });

  it('keeps HistoryPanel mounted after switching back to Collections (only the first mount is deferred)', async () => {
    render(<CollectionsSidebar />, { wrapper });

    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));
    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Collections' }));
    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();
  });
});

describe('CollectionsSidebar script delete', () => {
  const summary: tauriApi.CollectionSummary = {
    uid: 'c1',
    repositoryId: 'r1',
    name: 'col',
    path: '/w/col',
    requestCount: 0,
  };

  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(tauriApi.listCollections).mockResolvedValue([summary]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([
      { id: 'ws1', repositoryId: 'r1', name: 'WS', path: '/w', pinned: false },
    ]);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'col',
      root: {
        uid: 'root',
        name: 'col',
        items: [{ type: 'scriptFile', fileName: 'utils.js', name: 'utils.js' }],
      },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.readScriptFile).mockResolvedValue('x');
    vi.mocked(tauriApi.deleteScriptFile).mockReset().mockResolvedValue(undefined);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    usePaneStore.setState({ activeCollection: 'col' });
  });

  it('warns about unsaved edits and closes the script tab on confirm', async () => {
    await usePaneStore.getState().openScriptTab('col', 'utils.js');
    const tabId = findScriptTab(usePaneStore.getState().root, 'col', 'utils.js')?.tab.id ?? '';
    usePaneStore.getState().updateScriptContent(tabId, 'edited');

    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for utils.js' }));
    await userEvent.click(await screen.findByText('Delete'));

    expect(await screen.findByText(/unsaved changes that will be lost/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteScriptFile).toHaveBeenCalledWith('col', 'utils.js'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'utils.js')).toBeNull(),
    );
  });
});

describe('CollectionsSidebar flow delete', () => {
  const summary: tauriApi.CollectionSummary = {
    uid: 'c1',
    repositoryId: 'r1',
    name: 'col',
    path: '/w/col',
    requestCount: 0,
  };
  const loginTab = (patch: Partial<FlowTab> = {}): FlowTab => ({
    id: 'flow-login',
    title: 'Flow: Login',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'col',
    flowName: 'Login',
    nodes: [],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
    ...patch,
  });
  const flowTabs = () => collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(toast.error).mockReset();
    vi.mocked(tauriApi.listCollections).mockResolvedValue([summary]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([
      { id: 'ws1', repositoryId: 'r1', name: 'WS', path: '/w', pinned: false },
    ]);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'col',
      root: { uid: 'root', name: 'col', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.listFlows).mockReset().mockResolvedValue(['Login']);
    vi.mocked(tauriApi.deleteFlow).mockReset().mockResolvedValue(undefined);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    usePaneStore.setState({ activeCollection: 'col' });
  });

  it('warns about unsaved edits, deletes the flow, closes its tab and refreshes the list', async () => {
    usePaneStore.getState().openTab(loginTab({ isDirty: true }));
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(await screen.findByText(/Delete flow 'Login'/)).toBeInTheDocument();
    expect(screen.getByText(/unsaved changes that will be lost/)).toBeInTheDocument();

    vi.mocked(tauriApi.listFlows).mockResolvedValue([]);
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteFlow).toHaveBeenCalledWith('col', 'Login'));
    await waitFor(() => expect(flowTabs()).toHaveLength(0));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Actions for Login' })).not.toBeInTheDocument(),
    );
  });

  it('blocks delete while the flow runs and leaves the dialog closed', async () => {
    usePaneStore.getState().openTab(loginTab({ runState: 'running' }));
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));

    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
    expect(screen.queryByText('Confirm Delete')).not.toBeInTheDocument();
    expect(tauriApi.deleteFlow).not.toHaveBeenCalled();
  });

  it('re-checks the run at confirm time, in case the run started while the dialog was open', async () => {
    usePaneStore.getState().openTab(loginTab());
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    await screen.findByText('Confirm Delete');

    usePaneStore.getState().setFlowRunState('flow-login', 'running', 'r1');
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run')),
    );
    expect(tauriApi.deleteFlow).not.toHaveBeenCalled();
    expect(flowTabs()).toHaveLength(1);
  });

  it('clears the flow auth tokens and the parked tab when the only tab sits in a snapshot', async () => {
    const key = flowAuthKey('col', 'Login', 'n1', null, null);
    useFlowAuthStore.setState({ auths: { [key]: { auth: { authType: 'none' } as never } } });
    usePaneStore.setState({
      collectionTabState: {
        other: { tabs: [loginTab({ id: 'parked' })], activeTabId: 'parked' },
      },
    });
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteFlow).toHaveBeenCalledWith('col', 'Login'));
    expect(useFlowAuthStore.getState().auths[key]).toBeUndefined();
    expect(usePaneStore.getState().collectionTabState.other?.tabs).toEqual([]);
  });
});
