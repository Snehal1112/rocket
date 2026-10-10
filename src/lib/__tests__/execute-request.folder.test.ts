import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import type { AuthState, RequestState } from '@/types/pane-types';

const state = vi.hoisted(() => ({
  folders: new Map<string, unknown>(),
  collectionAuth: undefined as unknown,
}));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({
    variables: [{ key: 'token', value: 'tok-123', initialValue: '', enabled: true, secret: false }],
    headers: [
      { key: 'X-Team', value: 'core', enabled: true },
      { key: 'X-Env', value: 'collection', enabled: true },
    ],
  })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getFolderSettings: vi.fn(async (_collection: string, folderPath: string) => {
    if (!state.folders.has(folderPath)) throw new Error(`cannot read ${folderPath}/folder.yml`);
    return state.folders.get(folderPath);
  }),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: {
    getState: () => ({
      getCollectionAuth: () => state.collectionAuth as AuthState | undefined,
    }),
  },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';

const PATH = 'users/admin/get.yml';

function folder(partial: Partial<FolderSettings>): FolderSettings {
  return { headers: [], variables: [], ...partial } as FolderSettings;
}

function request(overrides: Partial<RequestState> = {}): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://api.example.com/users',
    pathParams: [],
    queryParams: [],
    headers: [],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'inherit' },
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
    ...overrides,
  };
}

const values = (headers: { key: string; value: string }[], key: string) =>
  headers.filter((h) => h.key.toLowerCase() === key.toLowerCase()).map((h) => h.value);

describe('resolveRequestFieldsForPath with a pre-request rok.setVar', () => {
  beforeEach(() => {
    state.folders.clear();
    state.collectionAuth = undefined;
  });

  it('leaves a name the script sets as a placeholder for the backend', async () => {
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({
        url: 'https://api.example.com/{{token}}',
        headers: [{ id: '1', key: 'X-T', value: '{{token}}', enabled: true }],
        preRequestScript: "rok.setVar('token', 'fresh');",
      }),
      true,
    );
    expect(resolved.url).toBe('https://api.example.com/{{token}}');
    expect(values(resolved.headers, 'X-T')).toEqual(['{{token}}']);
  });

  it('resolves the name for a path that runs no script, such as a cURL copy', async () => {
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({
        url: 'https://api.example.com/{{token}}',
        preRequestScript: "rok.setVar('token', 'fresh');",
      }),
    );
    expect(resolved.url).toBe('https://api.example.com/tok-123');
  });

  it('resolves the name as before when no script sets it', async () => {
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({
        url: 'https://api.example.com/{{token}}',
        preRequestScript: "rok.setVar('other', 'x');",
      }),
      true,
    );
    expect(resolved.url).toBe('https://api.example.com/tok-123');
  });
});

describe('resolveRequestFieldsForPath with folder settings', () => {
  beforeEach(() => {
    state.folders.clear();
    state.collectionAuth = undefined;
    vi.clearAllMocks();
  });

  it('lets a folder header beat a collection header and a request header beat the folder', async () => {
    state.folders.set(
      'users',
      folder({
        headers: [
          { key: 'X-Env', value: 'folder', enabled: true },
          { key: 'X-Trace', value: 'folder', enabled: true },
        ],
      }),
    );
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({ headers: [{ id: '1', key: 'X-Trace', value: 'request', enabled: true }] }),
    );
    expect(values(resolved.headers, 'X-Team')).toEqual(['core']);
    expect(values(resolved.headers, 'X-Env')).toEqual(['folder']);
    expect(values(resolved.headers, 'X-Trace')).toEqual(['request']);
  });

  it('lets the inner folder header win over the outer one', async () => {
    state.folders.set(
      'users',
      folder({ headers: [{ key: 'X-Env', value: 'outer', enabled: true }] }),
    );
    state.folders.set(
      'users/admin',
      folder({ headers: [{ key: 'X-Env', value: 'inner', enabled: true }] }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(values(resolved.headers, 'X-Env')).toEqual(['inner']);
  });

  it('never lets a disabled folder header hide the collection header', async () => {
    state.folders.set(
      'users/admin',
      folder({ headers: [{ key: 'X-Env', value: 'off', enabled: false }] }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(values(resolved.headers, 'X-Env')).toEqual(['collection']);
  });

  it('gives an inheriting request the nearest folder auth, with placeholders resolved', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set(
      'users',
      folder({ auth: { authType: 'basic', username: 'outer', password: 'x' } }),
    );
    state.folders.set('users/admin', folder({ auth: { authType: 'bearer', token: '{{token}}' } }));
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'tok-123' });
  });

  it('falls back to the collection auth when no folder sets auth', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set('users/admin', folder({ auth: { authType: 'inherit' } }));
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'from-collection' });
  });

  it('leaves an OAuth2 folder auth as inherit so the backend resolves it', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set(
      'users',
      folder({
        auth: {
          authType: 'o-auth2',
          flow: 'client_credentials',
          accessTokenUrl: 'https://auth.example.com/token',
          credentials: { clientId: 'c', clientSecret: 's' },
        },
      }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'inherit' });
  });

  it('keeps an explicit request auth over every folder', async () => {
    state.folders.set('users', folder({ auth: { authType: 'bearer', token: 'from-folder' } }));
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({ auth: { authType: 'bearer', bearer: { token: 'mine' } } }),
    );
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'mine' });
  });
});
