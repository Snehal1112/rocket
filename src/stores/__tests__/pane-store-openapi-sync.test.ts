import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function allTabs() {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  return root;
}

describe('pane-store OpenAPI sync tab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('opens_one_tab_per_collection', () => {
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    const leaf = allTabs();
    const tabs = leaf.tabs.filter((t) => t.tabType === 'openapi-sync');
    expect(tabs).toHaveLength(1);
    expect(tabs[0].id).toBe('openapi-sync:Orders');
    expect(leaf.activeTabId).toBe('openapi-sync:Orders');
  });

  it('title_names_the_collection', () => {
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    const found = findTabInTree(usePaneStore.getState().root, 'openapi-sync:Orders');
    expect(found?.tab.title).toBe('OpenAPI sync — Orders');
  });

  it('opening_switches_the_active_collection', () => {
    const switchCollection = vi.fn();
    usePaneStore.setState({ switchCollection });
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    expect(switchCollection).toHaveBeenCalledWith('Orders');
  });
});
