import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';
import type { RequestState } from '@/types/pane-types';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderChainVariables: vi.fn(),
  getRequestVariables: vi.fn(),
  getFolderSettings: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));
vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

const request = (authType: 'inherit' | 'none'): RequestState => ({
  requestType: 'http',
  method: 'GET',
  url: 'https://api.example/ping',
  pathParams: [],
  queryParams: [],
  headers: [],
  body: { mode: 'none', content: '', formData: [] },
  auth: { authType },
  settings: {
    verifySsl: true,
    followRedirects: true,
    maxRedirects: 5,
    timeoutMs: 0,
    encodeUrl: true,
  },
  docs: null,
  tags: [],
  assertions: [],
  actions: [],
});

const oauth = authStateForType('oauth2', { authType: 'none' });
const oauthFields = oauth.oauth2 as NonNullable<typeof oauth.oauth2>;

/** Makes folder `api` hold the given OAuth2 fields on disk. */
const folderOAuthOnDisk = (patch: Partial<typeof oauthFields> = {}) => {
  api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
    auth:
      path === 'api' ? stateToFolderAuth({ ...oauth, oauth2: { ...oauthFields, ...patch } }) : null,
  }));
};

/** Caches a folder token for `api`, fetched with the given OAuth2 fields. */
const cacheFolderToken = (patch: Partial<typeof oauthFields> = {}) => {
  useFolderAuthStore.getState().setFolderAuth('demo', 'api', {
    ...oauth,
    oauth2: { ...oauthFields, ...patch, accessToken: 'folder-token' },
  });
};

describe('resolveRequestFieldsForPath inherited folder auth', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useFolderAuthStore.setState({ auths: {} });
    useCollectionAuthStore.setState({ auths: new Map() });
    api.getCollectionSettings.mockResolvedValue({ variables: [], headers: [], auth: null });
    api.getFolderChainVariables.mockResolvedValue([]);
    api.getRequestVariables.mockResolvedValue([]);
    api.getFolderSettings.mockResolvedValue({ auth: null });
  });

  it('uses the nearest folder auth for an inheriting request', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: path === 'api' ? { authType: 'basic', username: 'u', password: 'p' } : null,
    }));
    const out = await resolveRequestFieldsForPath('demo', 'api/users/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'basic', username: 'u', password: 'p' });
  });

  it('a folder OAuth2 token reaches the wire as a bearer token', async () => {
    folderOAuthOnDisk();
    cacheFolderToken();
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'folder-token' });
  });

  it('a folder OAuth2 token wins over the collection auth', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    folderOAuthOnDisk();
    cacheFolderToken();
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'folder-token' });
  });

  it.each([
    ['grant type', { grantType: 'password' as const }],
    ['token url', { tokenUrl: 'https://other.example/token' }],
    ['client id', { clientId: 'other-client' }],
  ])('never sends a cached token whose %s differs from folder.yml', async (_name, patch) => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    folderOAuthOnDisk();
    cacheFolderToken(patch);
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    // The folder still wins, so the backend resolves it from folder.yml without the token.
    expect(out.auth).toEqual({ authType: 'inherit' });
  });

  it('leaves a folder OAuth2 auth without a token to the backend', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    folderOAuthOnDisk();
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'inherit' });
  });

  it('ignores an inherit entry in the folder auth store', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    useFolderAuthStore.getState().setFolderAuth('demo', 'api', { authType: 'inherit' });
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'collection-token' });
  });

  it('keeps inherit on the wire when no folder sets auth and the collection has none cached', async () => {
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'inherit' });
  });

  it('still uses the cached collection auth when no folder sets auth', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'collection-token' });
  });

  it('keeps a request own auth over a folder auth', async () => {
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: path === 'api' ? { authType: 'basic', username: 'u', password: 'p' } : null,
    }));
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('none'));
    expect(out.auth).toEqual({ authType: 'none' });
  });

  it('sends no folder auth for a request outside any collection', async () => {
    const out = await resolveRequestFieldsForPath(undefined, undefined, request('inherit'));
    expect(out.auth).toEqual({ authType: 'inherit' });
    expect(api.getFolderSettings).not.toHaveBeenCalled();
  });
});
