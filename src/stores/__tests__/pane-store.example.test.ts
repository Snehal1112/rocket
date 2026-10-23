import { beforeEach, describe, expect, it, vi } from 'vitest';
import { collectAllTabs } from '@/lib/pane-utils';
import type { ExampleTab } from '@/types/pane-types';
import { isExampleTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function exampleTabs(): ExampleTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(isExampleTab);
}

describe('pane-store example tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('opens a tab titled with the example name and never marks it dirty', () => {
    usePaneStore.getState().openExampleTab('col', 'users.yml', '200 OK');
    const tabs = exampleTabs();
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      tabType: 'example',
      title: '200 OK',
      exampleName: '200 OK',
      source: { collection: 'col', path: 'users.yml' },
      isDirty: false,
    });
  });

  it('opening the same example twice leaves one tab and activates it', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'users.yml', '200 OK');
    store.openExampleTab('col', 'users.yml', '401');
    store.openExampleTab('col', 'users.yml', '200 OK');
    const tabs = exampleTabs();
    expect(tabs).toHaveLength(2);
    const root = usePaneStore.getState().root;
    if (root.type !== 'leaf') throw new Error('Expected a single leaf');
    expect(root.activeTabId).toBe(tabs.find((t) => t.exampleName === '200 OK')?.id);
  });

  it('closeExampleTabs with a name closes only that example', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'users.yml', '200 OK');
    store.openExampleTab('col', 'users.yml', '401');
    store.closeExampleTabs('col', 'users.yml', '401');
    expect(exampleTabs().map((t) => t.exampleName)).toEqual(['200 OK']);
  });

  it('closeExampleTabs without a name closes all examples of that request only', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'users.yml', '200 OK');
    store.openExampleTab('col', 'users.yml', '401');
    store.openExampleTab('col', 'orders.yml', '200 OK');
    store.closeExampleTabs('col', 'users.yml');
    const left = exampleTabs();
    expect(left).toHaveLength(1);
    expect(left[0].source?.path).toBe('orders.yml');
  });

  it('retitleExampleTab changes the example name and the title', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'users.yml', '401');
    store.openExampleTab('col', 'users.yml', '200 OK');
    store.retitleExampleTab('col', 'users.yml', '401', 'Unauthorized');
    const names = exampleTabs().map((t) => [t.exampleName, t.title]);
    expect(names).toContainEqual(['Unauthorized', 'Unauthorized']);
    expect(names).toContainEqual(['200 OK', '200 OK']);
    expect(exampleTabs().every((t) => !t.isDirty)).toBe(true);
  });

  // Parks tabs by switching to another collection, as a real collection switch does.
  function parkTabs(): void {
    const state = usePaneStore.getState();
    const root = state.root;
    if (root.type !== 'leaf') throw new Error('Expected a single leaf');
    usePaneStore.setState({
      collectionTabState: {
        col: { tabs: root.tabs, activeTabId: root.activeTabId },
      } as never,
      root: { ...root, tabs: [], activeTabId: '' },
    });
  }

  function parkedExampleTabs(): ExampleTab[] {
    return Object.values(usePaneStore.getState().collectionTabState)
      .flatMap((entry) => entry.tabs)
      .filter(isExampleTab);
  }

  it('renameExampleTabPaths rewrites by whole segment, live and parked', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'auth/login.yml', '200 OK');
    store.openExampleTab('col', 'author/x.yml', '200 OK');
    parkTabs();
    usePaneStore.getState().openExampleTab('col', 'auth', '200 OK');
    usePaneStore.getState().renameExampleTabPaths('col', 'auth', 'identity');
    expect(exampleTabs().map((t) => t.source.path)).toEqual(['identity']);
    expect(parkedExampleTabs().map((t) => t.source.path).sort()).toEqual([
      'author/x.yml',
      'identity/login.yml',
    ]);
  });

  it('closeExampleTabsUnder closes live and parked tabs under a folder only', () => {
    const store = usePaneStore.getState();
    store.openExampleTab('col', 'auth/login.yml', '200 OK');
    store.openExampleTab('col', 'author/x.yml', '200 OK');
    parkTabs();
    usePaneStore.getState().openExampleTab('col', 'auth/me.yml', '200 OK');
    usePaneStore.getState().openExampleTab('col', 'other.yml', '200 OK');
    usePaneStore.getState().closeExampleTabsUnder('col', 'auth');
    expect(exampleTabs().map((t) => t.source.path)).toEqual(['other.yml']);
    expect(parkedExampleTabs().map((t) => t.source.path)).toEqual(['author/x.yml']);
  });
});
