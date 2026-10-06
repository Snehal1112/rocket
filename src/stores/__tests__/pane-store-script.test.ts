import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile, renameRequest } from '@/lib/tauri-api';
import { isScriptTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn(),
    renameRequest: vi.fn().mockResolvedValue(undefined),
    endAgentSession: vi.fn(),
  };
});

function findTab(collection: string, path: string) {
  return findScriptTab(usePaneStore.getState().root, collection, path);
}

describe('pane-store script tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(readScriptFile).mockReset();
  });

  it('opens a script tab with the file content and a clean state', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
    await usePaneStore.getState().openScriptTab('col', 'lib/utils.js');
    const found = findTab('col', 'lib/utils.js');
    expect(found).not.toBeNull();
    expect(found?.tab.title).toBe('utils.js');
    expect(found?.tab.content).toBe('module.exports = 1;');
    expect(found?.tab.isDirty).toBe(false);
    expect(found?.tab.source).toEqual({ collection: 'col', path: 'lib/utils.js' });
  });

  it('focuses the existing tab instead of reading the file again', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('a');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    expect(readScriptFile).toHaveBeenCalledTimes(1);
  });

  it('does not open a tab when the read fails', async () => {
    vi.mocked(readScriptFile).mockRejectedValue('boom');
    await expect(usePaneStore.getState().openScriptTab('col', 'x.js')).rejects.toBe('boom');
    expect(findTab('col', 'x.js')).toBeNull();
  });

  it('tracks dirty state against the last saved content', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('one');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const id = findTab('col', 'a.js')?.tab.id ?? '';

    usePaneStore.getState().updateScriptContent(id, 'two');
    expect(findTab('col', 'a.js')?.tab.isDirty).toBe(true);

    usePaneStore.getState().updateScriptContent(id, 'one');
    expect(findTab('col', 'a.js')?.tab.isDirty).toBe(false);

    usePaneStore.getState().updateScriptContent(id, 'three');
    usePaneStore.getState().markScriptSaved(id, 'three');
    const tab = findTab('col', 'a.js')?.tab;
    expect(tab?.isDirty).toBe(false);
    expect(tab?.savedContent).toBe('three');
  });

  it('retargets tabs when a script is renamed and keeps the id', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('x');
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const before = findTab('col', 'lib/a.js')?.tab;

    usePaneStore.getState().renameScriptTabs('col', 'lib/a.js', 'lib/b.js');

    expect(findTab('col', 'lib/a.js')).toBeNull();
    const after = findTab('col', 'lib/b.js')?.tab;
    expect(after?.id).toBe(before?.id);
    expect(after?.title).toBe('b.js');
    expect(after && isScriptTab(after) && after.source).toEqual({
      collection: 'col',
      path: 'lib/b.js',
    });
  });

  it('never renames a request file when a script tab title changes', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('x');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const id = findTab('col', 'a.js')?.tab.id ?? '';
    usePaneStore.getState().updateTabTitle(id, 'other.js');
    expect(renameRequest).not.toHaveBeenCalled();
  });
  it('retargets script tabs under a renamed folder and keeps ids', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('x');
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    await usePaneStore.getState().openScriptTab('col', 'lib/deep/b.js');
    await usePaneStore.getState().openScriptTab('col', 'lib2/x.js');
    const idA = findTab('col', 'lib/a.js')?.tab.id;
    expect(idA).toBeDefined();

    usePaneStore.getState().renameScriptTabs('col', 'lib', 'helpers');

    expect(findTab('col', 'helpers/a.js')?.tab.id).toBe(idA);
    expect(findTab('col', 'helpers/a.js')?.tab.source).toEqual({
      collection: 'col',
      path: 'helpers/a.js',
    });
    expect(findTab('col', 'helpers/deep/b.js')).not.toBeNull();
    expect(findTab('col', 'lib2/x.js')).not.toBeNull();
    expect(findTab('col', 'lib/a.js')).toBeNull();
  });
});
