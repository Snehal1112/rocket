import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { EditorGroup } from '../EditorGroup';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), saveFlow: vi.fn(), endAgentSession: vi.fn() };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
vi.mock('@/components/flow/FlowPane', () => ({ FlowPane: () => <div /> }));

const dirtyFlow: FlowTab = {
  id: 'flow-eg-1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function setup() {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab(dirtyFlow);
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  render(
    <QueryClientProvider client={new QueryClient()}>
      <EditorGroup node={root} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByLabelText(/close/i));
}

const tabStillOpen = () => findTabInTree(usePaneStore.getState().root, 'flow-eg-1') !== null;

describe('EditorGroup unsaved flow close dialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('uses flow wording', () => {
    setup();
    expect(screen.getByText(/This flow has unsaved changes/)).toBeInTheDocument();
  });

  it('saves the flow and then closes the tab', async () => {
    setup();
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith('demo', { name: 'my-flow', nodes: [], edges: [] }),
    );
    await waitFor(() => expect(tabStillOpen()).toBe(false));
  });

  it('keeps the tab and the dialog open when saving fails', async () => {
    setup();
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: flow contains a cycle');
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(tabStillOpen()).toBe(true);
    expect(screen.getByRole('button', { name: 'Save and close' })).toBeInTheDocument();
  });

  it('Close discards the changes without saving', async () => {
    setup();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(tabStillOpen()).toBe(false));
    expect(saveFlow).not.toHaveBeenCalled();
  });
});
