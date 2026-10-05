import { buildSchema, introspectionFromSchema } from 'graphql';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/graphql-schema-input', () => ({
  buildSchemaRequestInput: vi.fn(async () => ({
    method: 'POST',
    url: 'https://x/graphql',
    headers: [],
    queryParams: [],
    auth: { authType: 'none' },
    options: { followRedirects: true, timeoutMs: 1000, verifySsl: true },
    collection: 'api',
    environmentName: 'dev',
  })),
}));

vi.mock('@/lib/tauri-api', () => ({
  fetchGraphQlSchema: vi.fn(),
  getCachedGraphQlSchema: vi.fn(),
}));

import { createDefaultRequestFor } from '@/lib/pane-utils';
import { fetchGraphQlSchema, getCachedGraphQlSchema } from '@/lib/tauri-api';
import { useGraphQlSchemaStore } from '../graphql-schema-store';

const introspection = introspectionFromSchema(buildSchema('type Query { a: String }'));
const result = { key: 'k', fetchedAt: '2026-10-05T10:00:00Z', introspection };

describe('graphql schema store', () => {
  beforeEach(() => {
    useGraphQlSchemaStore.setState({ entries: {} });
    vi.mocked(fetchGraphQlSchema).mockReset();
    vi.mocked(getCachedGraphQlSchema).mockReset();
  });

  it('fetchSchema goes loading then ready and keeps a built schema', async () => {
    vi.mocked(fetchGraphQlSchema).mockResolvedValue(result);
    const p = useGraphQlSchemaStore
      .getState()
      .fetchSchema('t1', createDefaultRequestFor('graphql'), false);
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('loading');
    await p;
    const entry = useGraphQlSchemaStore.getState().entries.t1;
    expect(entry?.status).toBe('ready');
    expect(entry?.schema?.getQueryType()?.name).toBe('Query');
    expect(entry?.fetchedAt).toBe('2026-10-05T10:00:00Z');
    expect(fetchGraphQlSchema).toHaveBeenCalledWith(expect.objectContaining({ refresh: false }));
  });

  it('fetchSchema records a readable error and keeps no schema', async () => {
    vi.mocked(fetchGraphQlSchema).mockRejectedValue('Invalid input: introspection is disabled');
    await useGraphQlSchemaStore
      .getState()
      .fetchSchema('t1', createDefaultRequestFor('graphql'), true);
    const entry = useGraphQlSchemaStore.getState().entries.t1;
    expect(entry?.status).toBe('error');
    expect(entry?.error).toContain('introspection is disabled');
    expect(entry?.schema).toBeUndefined();
  });

  it('loadCached uses the backend cache and never calls fetch', async () => {
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(result);
    await useGraphQlSchemaStore.getState().loadCached('t1', createDefaultRequestFor('graphql'));
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('ready');
    expect(fetchGraphQlSchema).not.toHaveBeenCalled();
  });

  it('loadCached leaves the entry idle when nothing is cached', async () => {
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(null);
    await useGraphQlSchemaStore.getState().loadCached('t1', createDefaultRequestFor('graphql'));
    expect(useGraphQlSchemaStore.getState().entries.t1?.status ?? 'idle').toBe('idle');
  });

  it('a broken introspection result becomes an error, not a crash', async () => {
    vi.mocked(fetchGraphQlSchema).mockResolvedValue({ ...result, introspection: { nope: true } });
    await useGraphQlSchemaStore
      .getState()
      .fetchSchema('t1', createDefaultRequestFor('graphql'), false);
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('error');
  });

  it('a slow fetch does not overwrite a newer state for the same tab', async () => {
    let finish: ((r: typeof result) => void) | undefined;
    vi.mocked(fetchGraphQlSchema).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(null);
    const req = createDefaultRequestFor('graphql');
    const slow = useGraphQlSchemaStore.getState().fetchSchema('t1', req, false);
    await useGraphQlSchemaStore.getState().loadCached('t1', req);
    finish?.(result);
    await slow;
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('idle');
  });

  it('keeps the current schema while a refresh is loading and after it fails', async () => {
    vi.mocked(fetchGraphQlSchema).mockResolvedValueOnce(result);
    const req = createDefaultRequestFor('graphql');
    await useGraphQlSchemaStore.getState().fetchSchema('t1', req, false);

    let fail: ((e: unknown) => void) | undefined;
    vi.mocked(fetchGraphQlSchema).mockReturnValueOnce(
      new Promise((_resolve, reject) => {
        fail = reject;
      }),
    );
    const refresh = useGraphQlSchemaStore.getState().fetchSchema('t1', req, true);
    const loading = useGraphQlSchemaStore.getState().entries.t1;
    expect(loading?.status).toBe('loading');
    expect(loading?.schema?.getQueryType()?.name).toBe('Query');

    fail?.('server down');
    await refresh;
    const failed = useGraphQlSchemaStore.getState().entries.t1;
    expect(failed?.status).toBe('error');
    expect(failed?.error).toContain('server down');
    expect(failed?.schema?.getQueryType()?.name).toBe('Query');
  });

  it('asks the backend cache with the whole request, so auth is part of the lookup', async () => {
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(null);
    await useGraphQlSchemaStore.getState().loadCached('t1', createDefaultRequestFor('graphql'));
    expect(getCachedGraphQlSchema).toHaveBeenCalledWith(
      expect.objectContaining({ url: 'https://x/graphql', collection: 'api' }),
    );
  });
});
