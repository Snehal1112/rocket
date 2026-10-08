import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlowListItem } from '@/components/collections/FlowListItem';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import { getFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
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
  return onDelete;
}

describe('FlowListItem', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset().mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
    vi.mocked(toast.error).mockReset();
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
    const onDelete = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({ type: 'flow', collection: 'col', name: 'Login' });
  });

  it('refuses to delete a flow that is running', async () => {
    usePaneStore.getState().openTab(runningTab);
    const onDelete = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
  });
});
