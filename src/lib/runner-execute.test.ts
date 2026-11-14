import { describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn(async () => ({
    status: 200,
    statusText: 'OK',
    headers: [],
    body: '',
    durationMs: 1,
    ttfbMs: 1,
    sizeBytes: 0,
    testResults: [],
    consoleEntries: [],
    scriptError: null,
  })),
  executeGraphQlRequest: vi.fn(),
  getCollectionSettings: vi.fn(async () => ({
    variables: [{ key: 'id', value: '1', initialValue: '', enabled: true, secret: false }],
    headers: [],
  })),
  getProcessEnvVars: vi.fn(async () => ({ vars: {}, dotEnvKeys: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getFolderSettings: vi.fn(async () => {
    throw new Error('no folder settings');
  }),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({
    getQueryData: () => undefined,
    fetchQuery: (opts: { queryFn: () => unknown }) => opts.queryFn(),
  }),
}));

import { executeRunnerEntry } from '@/lib/runner-execute';
import { executeRequest } from '@/lib/tauri-api';

describe('executeRunnerEntry placeholders', () => {
  it('runner_tab_leaves_collection_placeholders_for_the_backend', async () => {
    const request: Request = {
      uid: 'r1',
      name: 'Get',
      method: 'GET',
      url: 'https://x.test/{{id}}',
      headers: [],
      auth: { authType: 'none' },
      fileName: 'get.yml',
    };
    await executeRunnerEntry('demo', 'get.yml', request, undefined, undefined, {
      index: 0,
      total: 1,
      data: { id: '7' },
    });
    expect(executeRequest).toHaveBeenCalledWith(
      expect.objectContaining({ url: expect.stringContaining('{{id}}') }),
    );
  });
});
