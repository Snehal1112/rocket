import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({
    variables: [
      {
        key: 'baseUrl',
        value: 'https://collection.example',
        initialValue: '',
        enabled: true,
        secret: false,
      },
    ],
    headers: [{ key: 'X-Collection', value: 'yes', enabled: true }],
  })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';

function baseRequest(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: '{{baseUrl}}/ping',
    pathParams: [],
    queryParams: [],
    headers: [{ id: '1', key: 'Accept', value: 'application/json', enabled: true }],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'none' },
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
  };
}

describe('resolveRequestFieldsForPath', () => {
  beforeEach(() => vi.clearAllMocks());

  it('resolves {{var}} placeholders using collection variables', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    expect(resolved.url).toBe('https://collection.example/ping');
  });

  it('merges collection headers under request headers, request wins on collision', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    const keys = resolved.headers.map((h) => h.key);
    expect(keys).toContain('X-Collection');
    expect(keys).toContain('Accept');
  });

  it('passes collection and requestPath through unchanged', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    expect(resolved.collection).toBe('demo');
    expect(resolved.requestPath).toBe('ping.yml');
  });

  it('works with collection and requestPath both undefined', async () => {
    const resolved = await resolveRequestFieldsForPath(undefined, undefined, baseRequest());
    expect(resolved.url).toBe('{{baseUrl}}/ping'); // no collection vars available, left unresolved
  });
});
