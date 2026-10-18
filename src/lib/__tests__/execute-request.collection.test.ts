import { beforeEach, describe, expect, it, vi } from 'vitest';

const cache = vi.hoisted(() => ({ data: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({ variables: [], headers: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getResolvedEnvironment: vi.fn(async (collection: string, name: string) => {
    const envs = (cache.data.get(JSON.stringify(['environments', collection])) ?? []) as {
      name: string;
      variables: unknown[];
    }[];
    return { name, variables: envs.find((e) => e.name === name)?.variables ?? [], warnings: [] };
  }),
}));
vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({
    getQueryData: (key: unknown) => cache.data.get(JSON.stringify(key)),
    fetchQuery: (opts: { queryFn: () => unknown }) => opts.queryFn(),
  }),
}));

import { buildOAuth2VarContext } from '@/lib/execute-request';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { useEnvStore } from '@/stores/env-store';

const env = (name: string, value: string) => ({
  name,
  variables: [{ key: 'clientId', value, enabled: true, secret: false }],
});

describe('buildOAuth2VarContext collection', () => {
  beforeEach(() => {
    cache.data.clear();
    cache.data.set(JSON.stringify(environmentKeys.collection('api')), [env('dev', 'api-dev')]);
    cache.data.set(JSON.stringify(environmentKeys.collection('other')), [env('dev', 'other-dev')]);
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'other' });
  });

  it("reads the given collection's active environment, not the store's active collection", async () => {
    const ctx = await buildOAuth2VarContext('api');
    expect(ctx.clientId).toBe('api-dev');
  });

  it('falls back to the active collection when none is given', async () => {
    const ctx = await buildOAuth2VarContext(undefined);
    expect(ctx.clientId).toBe('other-dev');
  });
});
