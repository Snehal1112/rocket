import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';

const state = vi.hoisted(() => ({ cache: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({ variables: [], headers: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: 'dev', activeCollection: 'demo' }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({
    getQueryData: (key: readonly unknown[]) => state.cache.get(JSON.stringify(key)),
  }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { getCollectionSettings } from '@/lib/tauri-api';

function request(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://x.test/a',
    pathParams: [],
    queryParams: [],
    headers: [],
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

const devEnv = {
  name: 'dev',
  variables: [
    { key: 'API_TOKEN', value: 'tok-123456', enabled: true, secret: true },
    { key: 'SHORT', value: 'abc', enabled: true, secret: true },
    { key: 'HOST', value: 'h.test', enabled: true, secret: false },
  ],
};

describe('secret values of a send', () => {
  beforeEach(() => {
    state.cache.clear();
    vi.clearAllMocks();
  });

  it('collects the secret values of the active environment', async () => {
    state.cache.set(JSON.stringify(['environments', 'demo']), [devEnv]);
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['tok-123456']);
    expect(resolved.secretsUncertain).toBe(false);
  });

  it('collects secret collection variables', async () => {
    state.cache.set(JSON.stringify(['environments', 'demo']), [devEnv]);
    vi.mocked(getCollectionSettings).mockResolvedValueOnce({
      variables: [
        { key: 'K', value: 'collection-secret', initialValue: '', enabled: true, secret: true },
      ],
      headers: [],
    } as never);
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['collection-secret', 'tok-123456']);
  });

  it('flags the secret set as uncertain when the environment is not cached', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual([]);
    expect(resolved.secretsUncertain).toBe(true);
  });
});
