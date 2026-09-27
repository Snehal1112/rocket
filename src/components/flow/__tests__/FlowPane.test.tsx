import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), listFlows: vi.fn(), saveFlow: vi.fn() };
});

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

describe('FlowPane picker', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
  });

  it('lists flows for the tab collection without a manual pick', async () => {
    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
    await waitFor(() => expect(listFlows).toHaveBeenCalledWith('demo'));
  });

  it('creates an empty flow and opens it', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    const closeTab = vi.fn();
    usePaneStore.setState({ openFlowTab, closeTab });

    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
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

    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
    await userEvent.type(screen.getByLabelText('New flow name'), '!!!');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(openFlowTab).not.toHaveBeenCalled();
  });
});
