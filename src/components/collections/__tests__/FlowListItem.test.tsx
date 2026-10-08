import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlowListItem } from '@/components/collections/FlowListItem';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import { flowAuthKey } from '@/lib/flow-auth';
import { flowKeys } from '@/lib/queries/flow-queries';
import { getFlow, renameFlow } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import { type AuthState, type FlowTab, isFlowTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), renameFlow: vi.fn(), endAgentSession: vi.fn() };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));

const runningTab: FlowTab = {
  id: 'run-1',
  title: 'Flow: Login',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'col',
  flowName: 'Login',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'running',
};

function renderItem(onDelete = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Tree aria-label='tree'>
        <FlowListItem name='Login' collectionName='col' onDelete={onDelete} />
      </Tree>
    </QueryClientProvider>,
  );
  return { onDelete, client };
}

describe('FlowListItem', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset().mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
    vi.mocked(toast.error).mockReset();
    vi.mocked(renameFlow).mockReset();
    vi.mocked(toast.info).mockReset();
    useFlowAuthStore.setState({ auths: {} });
  });

  it('opens a flow tab on click', async () => {
    renderItem();
    fireEvent.click(screen.getByText('Login'));
    await waitFor(() => {
      const tabs = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);
      expect(tabs.map((t) => t.flowName)).toEqual(['Login']);
    });
  });

  it('asks the sidebar to delete with a flow target', async () => {
    const { onDelete } = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({ type: 'flow', collection: 'col', name: 'Login' });
  });

  it('refuses to delete a flow that is running', async () => {
    usePaneStore.getState().openTab(runningTab);
    const { onDelete } = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
  });

  const idleTab = (patch: Partial<FlowTab> = {}): FlowTab => ({ ...runningTab, runState: 'idle', ...patch });
  const authKey = flowAuthKey('col', 'Login', 'a1', null, null);

  async function renameTo(value: string) {
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('Login');
    fireEvent.change(input, { target: { value } });
    fireEvent.keyDown(input, { key: 'Enter' });
  }

  it('renames the flow, retargets a dirty tab, clears tokens, tells the user and refreshes the list', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    usePaneStore.getState().openTab(idleTab({ isDirty: true }));
    useFlowAuthStore.setState({ auths: { [authKey]: { auth: { authType: 'bearer' } as AuthState } } });
    const { client } = renderItem();
    const spy = vi.spyOn(client, 'invalidateQueries');

    await renameTo('  Sign In  ');

    await waitFor(() => expect(renameFlow).toHaveBeenCalledWith('col', 'Login', 'Sign In'));
    await waitFor(() => {
      const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
      expect(tab?.flowName).toBe('Sign In');
      expect(tab?.isDirty).toBe(true);
    });
    expect(useFlowAuthStore.getState().auths).toEqual({});
    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining('Authenticate again'));
    expect(spy).toHaveBeenCalledWith({ queryKey: flowKeys.collection('col') });
  });

  it('does not mention tokens when the flow held none', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    renderItem();
    await renameTo('Sign In');
    await waitFor(() => expect(renameFlow).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('passes a case-only rename to the backend unchanged', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    usePaneStore.getState().openTab(idleTab());
    renderItem();

    await renameTo('login');

    await waitFor(() => expect(renameFlow).toHaveBeenCalledWith('col', 'Login', 'login'));
    await waitFor(() => {
      const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
      expect(tab?.flowName).toBe('login');
    });
  });

  it('rejects a name containing "::" without calling the backend', async () => {
    renderItem();
    await renameTo('a::b');
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("A flow name cannot contain '::'."));
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('does not call the backend for an unchanged name', async () => {
    renderItem();
    await renameTo('  Login ');
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('refuses to rename a running flow', async () => {
    usePaneStore.getState().openTab(runningTab);
    renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
    expect(screen.queryByDisplayValue('Login')).not.toBeInTheDocument();
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('refuses when a run starts while the name is being typed', async () => {
    usePaneStore.getState().openTab(idleTab());
    renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('Login');

    usePaneStore.getState().setFlowRunState('run-1', 'running', 'r1');
    fireEvent.change(input, { target: { value: 'Sign In' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run')));
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('keeps the tab and the tokens, and shows the backend message, when the rename fails', async () => {
    vi.mocked(renameFlow).mockRejectedValue('Conflict: Flow name collides with an existing flow');
    usePaneStore.getState().openTab(idleTab());
    useFlowAuthStore.setState({ auths: { [authKey]: { auth: { authType: 'bearer' } as AuthState } } });
    renderItem();

    await renameTo('Taken');

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Conflict: Flow name collides')),
    );
    const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
    expect(tab?.flowName).toBe('Login');
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([authKey]);
  });
});
