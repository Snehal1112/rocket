import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';

const state = vi.hoisted(() => ({
  cache: new Map<string, unknown>(),
  resolved: null as null | { name: string; variables: unknown[]; warnings: string[] },
  resolveError: null as null | Error,
  processEnv: { vars: {} as Record<string, string>, dotEnvKeys: [] as string[] },
}));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({ variables: [], headers: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getResolvedEnvironment: vi.fn(async () => {
    if (state.resolveError) throw state.resolveError;
    return state.resolved;
  }),
  getProcessEnvVars: vi.fn(async () => state.processEnv),
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
    fetchQuery: (opts: { queryFn: () => unknown }) => opts.queryFn(),
  }),
}));

import { buildMaskOptions, maskHttpExchange } from '@/lib/console-redaction';
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

const inherited = (key: string, value: string, secret: boolean) => ({
  key,
  value,
  enabled: true,
  secret,
  source: 'base',
});

describe('secret values of a send', () => {
  beforeEach(() => {
    state.cache.clear();
    state.resolveError = null;
    state.resolved = { name: 'dev', variables: devEnv.variables, warnings: [] };
    state.processEnv = { vars: {}, dotEnvKeys: [] };
    vi.clearAllMocks();
  });

  it('collects the secret values of the active environment', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['tok-123456']);
    expect(resolved.secretsUncertain).toBe(false);
  });

  it('collects secret collection variables', async () => {
    vi.mocked(getCollectionSettings).mockResolvedValueOnce({
      variables: [
        { key: 'K', value: 'collection-secret', initialValue: '', enabled: true, secret: true },
      ],
      headers: [],
    } as never);
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['collection-secret', 'tok-123456']);
  });

  it('getActiveVariables_reads_the_resolved_environment_so_an_inherited_variable_resolves', async () => {
    state.resolved = {
      name: 'dev',
      variables: [inherited('baseVar', 'from-base', false), inherited('PARENT_KEY', 'parent-secret', true)],
      warnings: [],
    };
    const req = { ...request(), url: 'https://{{baseVar}}.test/a' };
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', req);
    expect(resolved.url).toBe('https://from-base.test/a');
    // An inherited secret keeps its flag and is masked.
    expect(resolved.secretValues).toEqual(['parent-secret']);
  });

  it('a_resolved_query_error_is_reported_and_the_send_does_not_use_partial_variables', async () => {
    state.resolveError = new Error('environment cycle: a -> b -> a');
    await expect(resolveRequestFieldsForPath('demo', 'a.yml', request())).rejects.toThrow(
      'environment cycle: a -> b -> a',
    );
  });

  it('dot_env_values_are_in_the_secret_values_and_masked_in_the_console_row', async () => {
    state.processEnv = {
      vars: { DB_PASSWORD: 'dotenv-secret', HOME: '/home/u' },
      dotEnvKeys: ['DB_PASSWORD'],
    };
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['dotenv-secret', 'tok-123456']);
    const { masked } = maskHttpExchange(
      {
        url: 'https://x.test/?p=dotenv-secret',
        requestHeaders: [{ key: 'X-Pass', value: 'dotenv-secret' }],
        requestBody: '',
        responseHeaders: [],
        responseBody: '',
      },
      buildMaskOptions({ authType: 'none' }, resolved.secretValues ?? [], false),
    );
    expect(masked.url).not.toContain('dotenv-secret');
    expect(masked.requestHeaders[0].value).not.toContain('dotenv-secret');
  });

  it('a_dot_env_value_shorter_than_six_characters_stays_visible', async () => {
    // The existing minimum of 6 characters applies to every secret, .env values included.
    state.processEnv = { vars: { PIN: '1234' }, dotEnvKeys: ['PIN'] };
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['tok-123456']);
  });
});
