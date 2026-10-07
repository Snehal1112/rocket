import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';

const api = vi.hoisted(() => ({
  getFolderSettings: vi.fn(),
  getCollectionSettings: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);

import {
  ancestorFolderPaths,
  describeInheritedAuthSource,
  resolveInheritedAuthSource,
  resolveInheritedFolderAuth,
} from '@/lib/inherited-auth';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

const folders: Record<string, unknown> = {};
const basic = { authType: 'basic', username: 'u', password: 'p' };

const oauth = authStateForType('oauth2', { authType: 'none' });
const oauthFields = oauth.oauth2 as NonNullable<typeof oauth.oauth2>;
const withToken = (patch: Partial<typeof oauthFields> = {}) => ({
  ...oauth,
  oauth2: { ...oauthFields, ...patch, accessToken: 'cached' },
});

describe('ancestorFolderPaths', () => {
  it('lists folders outermost first and excludes the root', () => {
    expect(ancestorFolderPaths('a/b/req.yml')).toEqual(['a', 'a/b']);
    expect(ancestorFolderPaths('a/req.yml')).toEqual(['a']);
    expect(ancestorFolderPaths('req.yml')).toEqual([]);
  });
});

describe('resolveInheritedAuthSource', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const k of Object.keys(folders)) delete folders[k];
    useFolderAuthStore.setState({ auths: {} });
    useCollectionAuthStore.setState({ auths: new Map() });
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: folders[path] ?? null,
    }));
    api.getCollectionSettings.mockResolvedValue({ auth: null });
  });

  it('uses the nearest folder with auth', async () => {
    folders.a = basic;
    folders['a/b'] = { authType: 'bearer', token: 't' };
    const out = await resolveInheritedAuthSource('demo', 'a/b/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a/b' });
  });

  it('skips folders that are Inherit or None', async () => {
    folders.a = basic;
    folders['a/b'] = { authType: 'none' };
    const out = await resolveInheritedAuthSource('demo', 'a/b/c/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a' });
  });

  it('falls back to the collection auth store, then to the collection on disk', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'x' },
    });
    expect((await resolveInheritedAuthSource('demo', 'a/req.yml')).kind).toBe('collection');

    useCollectionAuthStore.setState({ auths: new Map() });
    api.getCollectionSettings.mockResolvedValue({ auth: basic });
    expect((await resolveInheritedAuthSource('demo', 'a/req.yml')).kind).toBe('collection');
  });

  it('reports none when nothing sets auth', async () => {
    expect(await resolveInheritedAuthSource('demo', 'a/req.yml')).toEqual({ kind: 'none' });
  });

  it('skips a folder whose settings cannot be read', async () => {
    folders.a = basic;
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => {
      if (path === 'a/b') throw new Error('bad yaml');
      return { auth: folders[path] ?? null };
    });
    const out = await resolveInheritedAuthSource('demo', 'a/b/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a' });
  });

  it('puts a cached folder OAuth2 token on the folder auth', async () => {
    folders.a = stateToFolderAuth(oauth);
    useFolderAuthStore.getState().setFolderAuth('demo', 'a', withToken());
    const out = await resolveInheritedFolderAuth('demo', 'a/req.yml');
    expect(out?.auth.oauth2?.accessToken).toBe('cached');
  });

  it('does not use a cached token fetched for another OAuth2 config', async () => {
    folders.a = stateToFolderAuth({
      ...oauth,
      oauth2: { ...oauthFields, tokenUrl: 'https://new.example/token' },
    });
    useFolderAuthStore
      .getState()
      .setFolderAuth('demo', 'a', withToken({ tokenUrl: 'https://old.example/token' }));
    const out = await resolveInheritedFolderAuth('demo', 'a/req.yml');
    expect(out?.folderPath).toBe('a');
    expect(out?.auth.oauth2?.accessToken).toBe('');
  });

  it('treats an inherit entry in the folder auth store as no folder auth', async () => {
    useFolderAuthStore.getState().setFolderAuth('demo', 'a', { authType: 'inherit' });
    expect(await resolveInheritedFolderAuth('demo', 'a/req.yml')).toBeUndefined();
    expect(await resolveInheritedAuthSource('demo', 'a/req.yml')).toEqual({ kind: 'none' });
  });

  it('never takes folder auth from the store alone, only from folder.yml', async () => {
    useFolderAuthStore.getState().setFolderAuth('demo', 'a', withToken());
    expect(await resolveInheritedFolderAuth('demo', 'a/req.yml')).toBeUndefined();
  });
});

describe('describeInheritedAuthSource', () => {
  it('names the folder, the collection or nothing', () => {
    expect(
      describeInheritedAuthSource({
        kind: 'folder',
        folderPath: 'api/users',
        auth: { authType: 'bearer', bearer: { token: '' } },
      }),
    ).toBe('This request inherits authorization from the folder "api/users" (Bearer).');
    expect(describeInheritedAuthSource({ kind: 'collection', auth: { authType: 'basic' } })).toBe(
      'This request inherits authorization from the collection settings (Basic).',
    );
    expect(describeInheritedAuthSource({ kind: 'none' })).toBe(
      'No folder or collection sets authorization, so this request is sent without it.',
    );
  });
});
