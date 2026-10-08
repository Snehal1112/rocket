import { beforeEach, describe, expect, it, vi } from 'vitest';
import { collectAllTabs } from '@/lib/pane-utils';
import { getFlow } from '@/lib/tauri-api';
import { isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const flowTabs = () => collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);

describe('openFlowTab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset();
    vi.mocked(getFlow).mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
  });

  it('focuses the tab already open for the flow instead of opening a copy', async () => {
    await usePaneStore.getState().openFlowTab('col', 'Login');
    const firstId = flowTabs()[0]?.id;
    await usePaneStore.getState().openFlowTab('col', 'Login');
    expect(flowTabs()).toHaveLength(1);
    expect(flowTabs()[0]?.id).toBe(firstId);
    expect(getFlow).toHaveBeenCalledTimes(1);
  });

  it('opens separate tabs for different flows and for the same name in another collection', async () => {
    await usePaneStore.getState().openFlowTab('col', 'Login');
    await usePaneStore.getState().openFlowTab('col', 'Sync');
    await usePaneStore.getState().openFlowTab('other', 'Login');
    expect(flowTabs()).toHaveLength(3);
  });
});
