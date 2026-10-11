import { beforeEach, describe, expect, it } from 'vitest';
import { folderEditKey, useFolderEditStore } from '../folder-edit-store';

describe('folder-edit-store', () => {
  beforeEach(() => {
    useFolderEditStore.setState({ dirty: {}, reloads: {} });
  });

  it('keys a folder by collection and path', () => {
    expect(folderEditKey('c', 'a/b')).not.toBe(folderEditKey('c', 'a'));
    expect(folderEditKey('c', 'a/b')).toBe(folderEditKey('c', 'a/b'));
  });

  it('marks and clears a dirty folder', () => {
    const key = folderEditKey('c', 'a');
    useFolderEditStore.getState().setDirty(key, true);
    expect(useFolderEditStore.getState().dirty[key]).toBe(true);
    useFolderEditStore.getState().setDirty(key, false);
    expect(useFolderEditStore.getState().dirty[key]).toBeUndefined();
  });

  it('counts reload requests per folder', () => {
    const key = folderEditKey('c', 'a');
    useFolderEditStore.getState().requestReload(key);
    useFolderEditStore.getState().requestReload(key);
    expect(useFolderEditStore.getState().reloads[key]).toBe(2);
    expect(useFolderEditStore.getState().reloads[folderEditKey('c', 'b')]).toBeUndefined();
  });
});
