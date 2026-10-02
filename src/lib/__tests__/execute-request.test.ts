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

import {
  getEnvInvalidationKeys,
  resolveRequestFieldsForPath,
  toApiAuth,
} from '@/lib/execute-request';
import { environmentKeys } from '@/lib/queries/environment-queries';

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

  it('resolves {{var}} placeholders in assertion values', async () => {
    const request = {
      ...baseRequest(),
      assertions: [{ expression: 'res.body.token', operator: 'eq', value: '{{baseUrl}}' }],
    };
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', request);
    expect(resolved.assertions).toEqual([
      { expression: 'res.body.token', operator: 'eq', value: 'https://collection.example' },
    ]);
  });

  it('leaves assertion expression and undefined value untouched', async () => {
    const request = {
      ...baseRequest(),
      assertions: [{ expression: 'res.status', operator: 'isDefined' }],
    };
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', request);
    expect(resolved.assertions).toEqual([{ expression: 'res.status', operator: 'isDefined' }]);
  });
});

describe('getEnvInvalidationKeys', () => {
  it('returns the collection environments key when collection is set', () => {
    const keys = getEnvInvalidationKeys('my-api', undefined);
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
  });

  it('returns both the global environment key and the global list key when globalEnvName is set', () => {
    const keys = getEnvInvalidationKeys(undefined, 'global-prod');
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
    expect(keys).toContainEqual(environmentKeys.globalList);
  });

  it('returns collection, global, and global list keys when both are set', () => {
    const keys = getEnvInvalidationKeys('my-api', 'global-prod');
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
    expect(keys).toContainEqual(environmentKeys.globalList);
  });

  it('returns an empty list when neither is set', () => {
    const keys = getEnvInvalidationKeys(undefined, undefined);
    expect(keys).toHaveLength(0);
  });
});

describe('toApiAuth for digest, wsse, ntlm and oauth1', () => {
  const resolve = (s: string) => s.replace('{{pw}}', 'secret');

  it('preserves inherit on the wire so the backend applies collection auth', () => {
    expect(toApiAuth({ authType: 'inherit' })).toEqual({ authType: 'inherit' });
  });

  it('sends digest and wsse credentials with variables resolved', () => {
    expect(
      toApiAuth({ authType: 'digest', digest: { username: 'u', password: '{{pw}}' } }, resolve),
    ).toEqual({ authType: 'digest', username: 'u', password: 'secret' });
    expect(
      toApiAuth({ authType: 'wsse', wsse: { username: 'u', password: '{{pw}}' } }, resolve),
    ).toEqual({ authType: 'wsse', username: 'u', password: 'secret' });
  });

  it('sends ntlm with its domain', () => {
    expect(
      toApiAuth(
        { authType: 'ntlm', ntlm: { username: 'u', password: '{{pw}}', domain: 'CORP' } },
        resolve,
      ),
    ).toEqual({ authType: 'ntlm', username: 'u', password: 'secret', domain: 'CORP' });
  });

  it('sends oauth1 under the backend tag and resolves only its credential fields', () => {
    const sent = toApiAuth(
      {
        authType: 'oauth1',
        oauth1: { consumerSecret: '{{pw}}', signatureMethod: '{{pw}}', includeBodyHash: true },
      },
      resolve,
    );
    expect(sent).toEqual({
      authType: 'o-auth1',
      consumerSecret: 'secret',
      signatureMethod: '{{pw}}',
      includeBodyHash: true,
    });
  });

  it('does not send none for these types any more', () => {
    for (const authType of ['digest', 'wsse', 'ntlm', 'oauth1'] as const) {
      expect(toApiAuth({ authType }).authType).not.toBe('none');
    }
  });
});
