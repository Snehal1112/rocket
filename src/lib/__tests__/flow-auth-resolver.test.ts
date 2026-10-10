import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildOAuth2VarContext } from '@/lib/execute-request';
import { flowAuthResolver, oauth2Fingerprint } from '@/lib/flow-auth';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { setQueryClient } from '@/lib/query-client';
import type { Auth, CollectionVariable, Environment } from '@/lib/tauri-api';
import { resolveWithContext } from '@/lib/variable-context';
import { useEnvStore } from '@/stores/env-store';
import type { AuthState } from '@/types/pane-types';

const collectionVars: CollectionVariable[] = [
  { key: 'tokenUrl', value: 'https://idp/token', initialValue: '', enabled: true, secret: false },
];

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getProcessEnvVars: vi.fn(async () => ({ HOME: '/home/u' })),
  getCollectionSettings: vi.fn(async () => ({
    headers: [],
    variables: [
      {
        key: 'tokenUrl',
        value: 'https://idp/token',
        initialValue: '',
        enabled: true,
        secret: false,
      },
    ],
    sandboxMode: 'safe',
  })),
}));

const oauth2 = fromPersistedAuth({
  authType: 'o-auth2',
  flow: 'client_credentials',
  accessTokenUrl: '{{tokenUrl}}',
  scope: '{{tenant}} {{process.env.HOME}}',
  credentials: { clientId: '{{clientId}}', clientSecret: '{{vault.secret}}' },
} as unknown as Auth).oauth2 as NonNullable<AuthState['oauth2']>;

describe('flowAuthResolver (editor) and buildOAuth2VarContext (pre-run step)', () => {
  beforeEach(() => {
    const qc = new QueryClient();
    const dev: Environment = {
      name: 'dev',
      variables: [{ key: 'clientId', value: 'dev-client', enabled: true, secret: false }],
      externalSecrets: [
        { alias: 'vault', connectionId: 'c1', vaultName: 'v', secretNames: [{ name: 'secret' }] },
      ],
    } as Environment;
    qc.setQueryData(environmentKeys.collection('api'), [dev]);
    qc.setQueryData(environmentKeys.globalName, 'global');
    qc.setQueryData(environmentKeys.global('global'), {
      name: 'global',
      variables: [{ key: 'tenant', value: 'acme', enabled: true, secret: false }],
    });
    setQueryClient(qc);
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
  });

  it('give the same fingerprint for the same inputs, leaving a vault reference as written', async () => {
    const editorRv = flowAuthResolver({
      processEnvVars: { HOME: '/home/u' },
      globalVars: { tenant: 'acme' },
      envVars: { clientId: 'dev-client' },
      collectionVars,
    });
    const ctx = await buildOAuth2VarContext('api');
    const preflightRv = (s: string) => resolveWithContext(s, ctx);

    expect(oauth2Fingerprint(oauth2, editorRv)).toBe(oauth2Fingerprint(oauth2, preflightRv));
    expect(editorRv('{{vault.secret}}')).toBe('{{vault.secret}}');
    expect(editorRv('{{clientId}} {{tokenUrl}} {{tenant}} {{process.env.HOME}}')).toBe(
      'dev-client https://idp/token acme /home/u',
    );
  });
});
