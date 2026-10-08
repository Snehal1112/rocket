import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findAffectedTabs } from '@/components/collections/tree-utils';
import { collectAllTabs, findFolderTab } from '@/lib/pane-utils';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function folderTabs(): FolderTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(isFolderTab);
}

function activeTabId(): string {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected a single leaf');
  return root.activeTabId;
}

describe('pane-store folder tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('opens a new folder tab', () => {
    const reused = usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    expect(reused).toBe(false);
    const tabs = folderTabs();
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      tabType: 'folder',
      title: 'oauth',
      collectionName: 'col',
      folderPath: 'auth/oauth',
      activeSection: 'headers',
      isDirty: false,
    });
    expect(tabs[0].source).toBeUndefined();
    expect(tabs[0].id.startsWith('folder:')).toBe(true);
    expect(activeTabId()).toBe(tabs[0].id);
  });

  it('opens a new folder tab on the requested section', () => {
    usePaneStore.getState().openFolderTab('col', 'auth', 'vars');
    expect(folderTabs()[0].activeSection).toBe('vars');
  });

  it('reuses the open tab for the same collection and folder', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    const firstId = folderTabs()[0].id;
    usePaneStore.getState().openFolderTab('col', 'other');
    expect(activeTabId()).not.toBe(firstId);

    const reused = usePaneStore.getState().openFolderTab('col', 'auth');
    expect(reused).toBe(true);
    expect(folderTabs().filter((t) => t.folderPath === 'auth')).toHaveLength(1);
    expect(activeTabId()).toBe(firstId);
  });

  it('switches the section of a reused tab only when one is given', () => {
    usePaneStore.getState().openFolderTab('col', 'auth', 'docs');
    usePaneStore.getState().openFolderTab('col', 'auth');
    expect(folderTabs()[0].activeSection).toBe('docs');
    usePaneStore.getState().openFolderTab('col', 'auth', 'vars');
    expect(folderTabs()[0].activeSection).toBe('vars');
  });

  it('keeps same-named folders of two collections apart', () => {
    usePaneStore.getState().openFolderTab('col-a', 'auth');
    usePaneStore.getState().openFolderTab('col-b', 'auth');
    expect(usePaneStore.getState().activeCollection).toBe('col-b');
    expect(folderTabs().map((t) => t.collectionName)).toEqual(['col-b']);
    expect(findFolderTab(usePaneStore.getState().root, 'col-a', 'auth')).toBeNull();
  });

  it('finds a tab parked in a collection snapshot', () => {
    usePaneStore.getState().openFolderTab('col-a', 'auth');
    const id = folderTabs()[0].id;
    usePaneStore.getState().switchCollection('col-b');
    expect(folderTabs()).toHaveLength(0);

    const reused = usePaneStore.getState().openFolderTab('col-a', 'auth');
    expect(reused).toBe(true);
    expect(folderTabs().map((t) => t.id)).toEqual([id]);
  });

  it('leaves workspace mode', () => {
    usePaneStore.getState().openWorkspaceTabs('ws-1');
    expect(usePaneStore.getState().isWorkspaceMode()).toBe(true);
    usePaneStore.getState().openFolderTab('col', 'auth');
    expect(usePaneStore.getState().isWorkspaceMode()).toBe(false);
    expect(folderTabs()).toHaveLength(1);
  });

  it('updateFolderSection changes only the named tab', () => {
    usePaneStore.getState().openFolderTab('col', 'a');
    usePaneStore.getState().openFolderTab('col', 'b');
    const [a, b] = folderTabs();
    usePaneStore.getState().updateFolderSection(a.id, 'auth');
    const after = folderTabs();
    expect(after.find((t) => t.id === a.id)?.activeSection).toBe('auth');
    expect(after.find((t) => t.id === b.id)?.activeSection).toBe('headers');
  });

  it('retargets the renamed folder and nested folder tabs by whole segments', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    usePaneStore.getState().openFolderTab('col', 'authx');
    usePaneStore.getState().openFolderTab('col', 'auth');
    const idsBefore = folderTabs().map((t) => t.id);

    usePaneStore.getState().renameFolderTabs('col', 'auth', 'login');

    const tabs = folderTabs();
    expect(tabs.map((t) => t.id)).toEqual(idsBefore);
    expect(tabs.map((t) => t.folderPath)).toEqual(['login', 'login/oauth', 'authx']);
    expect(tabs.map((t) => t.title)).toEqual(['login', 'oauth', 'authx']);
  });

  it('findAffectedTabs matches folder tabs', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    usePaneStore.getState().openFolderTab('col', 'authx');
    const { root } = usePaneStore.getState();
    const paths = (target: Parameters<typeof findAffectedTabs>[1]) =>
      findAffectedTabs(root, target).map(({ tab }) => (isFolderTab(tab) ? tab.folderPath : '?'));

    expect(paths({ type: 'folder', collection: 'col', path: 'auth', name: 'auth' })).toEqual([
      'auth',
      'auth/oauth',
    ]);
    expect(paths({ type: 'collection', collection: 'col', name: 'col' })).toEqual([
      'auth',
      'auth/oauth',
      'authx',
    ]);
    expect(paths({ type: 'folder', collection: 'other', path: 'auth', name: 'auth' })).toEqual([]);
    expect(paths({ type: 'request', collection: 'col', path: 'auth', name: 'r' })).toEqual([]);
  });
});
