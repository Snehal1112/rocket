import { beforeEach, describe, expect, it } from 'vitest';
import { folderAuthKey, useFolderAuthStore } from '@/stores/folder-auth-store';

describe('folder-auth-store', () => {
  beforeEach(() => useFolderAuthStore.setState({ auths: {} }));

  it('keeps auth per collection and folder', () => {
    const { setFolderAuth, getFolderAuth } = useFolderAuthStore.getState();
    setFolderAuth('demo', 'api', { authType: 'bearer', bearer: { token: 'a' } });
    setFolderAuth('demo', 'api/users', { authType: 'bearer', bearer: { token: 'b' } });
    setFolderAuth('other', 'api', { authType: 'bearer', bearer: { token: 'c' } });
    expect(getFolderAuth('demo', 'api')?.bearer?.token).toBe('a');
    expect(getFolderAuth('demo', 'api/users')?.bearer?.token).toBe('b');
    expect(getFolderAuth('other', 'api')?.bearer?.token).toBe('c');
    expect(getFolderAuth('demo', 'missing')).toBeUndefined();
  });

  it('clears one folder', () => {
    const { setFolderAuth, clearFolderAuth, getFolderAuth } = useFolderAuthStore.getState();
    setFolderAuth('demo', 'api', { authType: 'none' });
    clearFolderAuth('demo', 'api');
    expect(getFolderAuth('demo', 'api')).toBeUndefined();
  });

  it('builds a key that cannot collide across collection and path', () => {
    expect(folderAuthKey('a', 'b/c')).not.toBe(folderAuthKey('a/b', 'c'));
  });
});
