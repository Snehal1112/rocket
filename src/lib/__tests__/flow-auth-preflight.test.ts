import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildOAuth2VarContext } from '@/lib/execute-request';
import { flowAuthKey, oauth2Fingerprint } from '@/lib/flow-auth';
import { collectFlowAuthTokens } from '@/lib/flow-auth-preflight';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import type { Auth, FlowNode } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { resolveWithContext } from '@/lib/variable-context';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';

vi.mock('@/lib/execute-request', () => ({
  buildOAuth2VarContext: vi.fn().mockResolvedValue({ cid: 'resolved-cid' }),
}));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, oauth2GetToken: vi.fn(), oauth2RefreshToken: vi.fn() };
});

const oauthAuth = (flow: string, clientId = '{{cid}}'): Auth =>
  ({
    authType: 'o-auth2',
    flow,
    authorizationUrl: 'https://idp.example.com/authorize',
    accessTokenUrl: 'https://idp.example.com/token',
    callbackUrl: 'https://app.example.com/cb',
    credentials: { clientId, clientSecret: 's' },
  }) as unknown as Auth;

const authNode = (id: string, auth: Auth, label = 'Sign in'): FlowNode => ({
  id,
  kind: { kind: 'Auth', label, auth, applyToInherit: true },
  position: { x: 0, y: 0 },
});

const input = (nodes: FlowNode[]) => ({ collection: 'api', flowName: 'login', nodes });
const key = (
  nodeId: string,
  env: string | null | undefined = undefined,
  globalEnv: string | null | undefined = undefined,
) => flowAuthKey('api', 'login', nodeId, env, globalEnv);

// The variables buildOAuth2VarContext resolves by default (see the mock above).
const DEFAULT_VARS = { cid: 'resolved-cid' };

const result = (over: Partial<tauriApi.OAuth2TokenResult> = {}): tauriApi.OAuth2TokenResult => ({
  access_token: 'new-access-123456',
  token_type: 'Bearer',
  expires_in: 3600,
  refresh_token: 'new-refresh-123456',
  ...over,
});

/**
 * Seeds the in-memory store with a token state for `nodeId`, fingerprinted
 * against `vars` (the variable values the token was fetched with).
 */
function seed(
  nodeId: string,
  auth: Auth,
  patch: Record<string, unknown>,
  env: string | undefined = undefined,
  opts: { globalEnv?: string; vars?: Record<string, string> } = {},
) {
  const base = fromPersistedAuth(auth);
  const oauth2 = { ...(base.oauth2 as NonNullable<AuthState['oauth2']>), ...patch };
  const state: AuthState = { ...base, oauth2 };
  const vars = opts.vars ?? DEFAULT_VARS;
  const fingerprint = oauth2Fingerprint(oauth2, (s) => resolveWithContext(s, vars));
  useFlowAuthStore.getState().setAuth(key(nodeId, env, opts.globalEnv), state, fingerprint);
}

describe('collectFlowAuthTokens', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
    vi.mocked(tauriApi.oauth2RefreshToken).mockReset();
    vi.mocked(buildOAuth2VarContext).mockResolvedValue(DEFAULT_VARS);
  });

  it('returns nothing and calls nothing for a flow without Auth nodes', async () => {
    const tokens = await collectFlowAuthTokens(input([]));
    expect(tokens).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('ignores static auth types', async () => {
    const node = authNode('a', { authType: 'bearer', token: 't' });
    expect(await collectFlowAuthTokens(input([node]))).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('prompts for an interactive grant with no token, with variables resolved, and keeps the token in memory', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const node = authNode('a', oauthAuth('authorization_code'));

    const tokens = await collectFlowAuthTokens({ ...input([node]), environmentName: 'dev' });

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({
        grantType: 'authorization_code',
        clientId: 'resolved-cid',
        collection: 'api',
        environmentName: 'dev',
      }),
    );
    expect(useFlowAuthStore.getState().getAuth(key('a', 'dev'))?.oauth2?.accessToken).toBe(
      'new-access-123456',
    );
  });

  it('remembers the fingerprint of the resolved configuration with the token it fetched', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const auth = oauthAuth('authorization_code');

    await collectFlowAuthTokens({ ...input([authNode('a', auth)]), globalEnvName: 'g1' });

    const entry = useFlowAuthStore.getState().getEntry(key('a', undefined, 'g1'));
    const expected = oauth2Fingerprint(
      fromPersistedAuth(auth).oauth2 as NonNullable<AuthState['oauth2']>,
      (s) => resolveWithContext(s, DEFAULT_VARS),
    );
    expect(entry?.fingerprint).toBe(expected);
    // A second run with nothing changed reuses it.
    const again = await collectFlowAuthTokens({
      ...input([authNode('a', auth)]),
      globalEnvName: 'g1',
    });
    expect(again).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
  });

  it('does not reuse a token fetched under another global environment', async () => {
    const auth = oauthAuth('authorization_code');
    seed(
      'a',
      auth,
      {
        accessToken: 'g1-token-123456',
        expiresIn: 3600,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
      },
      undefined,
      { globalEnv: 'g1' },
    );
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(
      result({ access_token: 'g2-token-123456' }),
    );

    const g2 = await collectFlowAuthTokens({
      ...input([authNode('a', auth)]),
      globalEnvName: 'g2',
    });
    expect(g2).toEqual({ a: { accessToken: 'g2-token-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);

    const g1 = await collectFlowAuthTokens({
      ...input([authNode('a', auth)]),
      globalEnvName: 'g1',
    });
    expect(g1).toEqual({ a: { accessToken: 'g1-token-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
  });

  it('does not reuse a token after the variable behind the client id changed value', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-client-token-123456',
      refreshToken: 'old-client-refresh-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    vi.mocked(buildOAuth2VarContext).mockResolvedValue({ cid: 'other-cid' });
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2RefreshToken).not.toHaveBeenCalled();
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({ clientId: 'other-cid' }),
    );
  });

  it('does not refresh a token fetched for other variable values', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(buildOAuth2VarContext).mockResolvedValue({ cid: 'other-cid' });
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(
      result({ access_token: 'prompted-123456' }),
    );

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'prompted-123456' } });
    expect(tauriApi.oauth2RefreshToken).not.toHaveBeenCalled();
  });

  it('does not pass a stale token for a non-interactive grant, leaving it to the backend', async () => {
    const auth = oauthAuth('client_credentials');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    vi.mocked(buildOAuth2VarContext).mockResolvedValue({ cid: 'other-cid' });

    expect(await collectFlowAuthTokens(input([authNode('a', auth)]))).toEqual({});
  });

  it('reuses a valid stored token without prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'stored-123456' } });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
    expect(tauriApi.oauth2RefreshToken).not.toHaveBeenCalled();
  });

  it('ignores a stored token when the node persisted auth changed since', async () => {
    seed('a', oauthAuth('authorization_code', 'client-A'), {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());

    const tokens = await collectFlowAuthTokens(
      input([authNode('a', oauthAuth('authorization_code', 'client-B'))]),
    );

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({ clientId: 'client-B' }),
    );
  });

  it('does not reuse a token fetched under another environment', async () => {
    const auth = oauthAuth('authorization_code');
    seed(
      'a',
      auth,
      {
        accessToken: 'prod-token-123456',
        expiresIn: 3600,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
      },
      'prod',
    );
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(
      result({ access_token: 'staging-token-123456' }),
    );

    const staging = await collectFlowAuthTokens({
      ...input([authNode('a', auth)]),
      environmentName: 'staging',
    });
    expect(staging).toEqual({ a: { accessToken: 'staging-token-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);

    const prod = await collectFlowAuthTokens({
      ...input([authNode('a', auth)]),
      environmentName: 'prod',
    });
    expect(prod).toEqual({ a: { accessToken: 'prod-token-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
  });

  it('refreshes an expired token that has a refresh token, without prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockResolvedValue(result());

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2RefreshToken).toHaveBeenCalledTimes(1);
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('falls back to a prompt when the refresh fails', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockRejectedValue(new Error('invalid_grant'));
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(
      result({ access_token: 'prompted-123456' }),
    );

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'prompted-123456' } });
  });

  it('leaves a non-interactive grant without a token to the backend', async () => {
    const node = authNode('a', oauthAuth('client_credentials'));
    expect(await collectFlowAuthTokens(input([node]))).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('passes a valid stored token for a non-interactive grant', async () => {
    const auth = oauthAuth('client_credentials');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));
    expect(tokens).toEqual({ a: { accessToken: 'stored-123456' } });
  });

  it('rejects with the node label when a sign-in fails', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockRejectedValue(new Error('window closed'));
    const node = authNode('a', oauthAuth('implicit'), 'Corporate SSO');

    await expect(collectFlowAuthTokens(input([node]))).rejects.toThrow(
      'Sign-in for Auth node "Corporate SSO" failed: window closed',
    );
  });
});
