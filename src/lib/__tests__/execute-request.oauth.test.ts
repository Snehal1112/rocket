import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const cache = vi.hoisted(() => ({ data: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({ variables: [], headers: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getWorkspaceConfig: vi.fn(async () => ({})),
  executeRequest: vi.fn(),
  oauth2GetToken: vi.fn(),
  oauth2RefreshToken: vi.fn(),
}));
vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({
    getQueryData: (key: unknown) => cache.data.get(JSON.stringify(key)),
    invalidateQueries: vi.fn(),
  }),
}));

import { sendRequest } from '@/lib/execute-request';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { executeRequest, oauth2GetToken, oauth2RefreshToken } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { AuthState, RequestState, RequestTab } from '@/types/pane-types';

const TAB_ID = 'tab-1';
const NOW_S = 1_800_000_000;

const execResult = {
  status: 200,
  statusText: 'OK',
  headers: [],
  body: '',
  durationMs: 1,
  ttfbMs: 1,
  sizeBytes: 0,
  testResults: [],
  consoleEntries: [],
};

type OAuth = NonNullable<AuthState['oauth2']>;

function oauthAuth(
  flow: string,
  settings: { autoFetchToken: boolean; autoRefreshToken: boolean },
  token: Partial<OAuth> = {},
): AuthState {
  const base = fromPersistedAuth({
    authType: 'o-auth2',
    flow,
    authorizationUrl: '{{idp}}/authorize',
    accessTokenUrl: '{{idp}}/token',
    refreshTokenUrl: '{{idp}}/refresh',
    callbackUrl: 'http://localhost/cb',
    credentials: { clientId: '{{cid}}', clientSecret: 's3cret', placement: 'body' },
    resourceOwner: { username: 'u', password: 'p' },
    settings,
  } as never);
  if (!base.oauth2) throw new Error('fixture must produce oauth2 state');
  return { ...base, oauth2: { ...base.oauth2, ...token } };
}

function requestWith(auth: AuthState): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://api.test/ping',
    pathParams: [],
    queryParams: [],
    headers: [],
    body: { mode: 'none', content: '', formData: [] },
    auth,
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

function openTab(request: RequestState) {
  const tab: RequestTab = {
    id: TAB_ID,
    title: 'ping',
    isDirty: false,
    tabType: 'request',
    request,
    response: null,
    source: { collection: 'api', path: 'ping.yml' },
  };
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab(tab);
  // Opening a tab resets the env store, so select the environment afterwards.
  useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
  return request;
}

function storedAuth(): AuthState {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('expected a single leaf');
  const tab = root.tabs.find((t) => t.id === TAB_ID);
  if (!tab || tab.tabType !== 'request') throw new Error('request tab missing');
  return tab.request.auth;
}

const sentAuth = () => vi.mocked(executeRequest).mock.calls[0]?.[0].auth;

describe('sendRequest OAuth2 auto-refresh / auto-fetch', () => {
  let warn: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW_S * 1000);
    warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.mocked(executeRequest).mockResolvedValue(execResult as never);
    cache.data.clear();
    cache.data.set(JSON.stringify(environmentKeys.collection('api')), [
      {
        name: 'dev',
        variables: [
          { key: 'idp', value: 'https://idp.test', enabled: true, secret: false },
          { key: 'cid', value: 'client-123', enabled: true, secret: false },
        ],
      },
    ]);
  });

  afterEach(() => {
    vi.useRealTimers();
    warn.mockRestore();
  });

  const expired = { expiresIn: 3600, tokenAcquiredAt: NOW_S - 7200 };

  it('(a) refreshes an expired token with a resolved request and stores the new token', async () => {
    vi.mocked(oauth2RefreshToken).mockResolvedValue({
      access_token: 'new-access',
      token_type: 'Bearer',
      expires_in: 600,
      refresh_token: 'new-refresh',
    });
    const req = openTab(
      requestWith(
        oauthAuth(
          'client_credentials',
          { autoFetchToken: true, autoRefreshToken: true },
          { accessToken: 'old-access', refreshToken: 'old-refresh', ...expired },
        ),
      ),
    );

    await sendRequest(TAB_ID, req);

    expect(oauth2RefreshToken).toHaveBeenCalledTimes(1);
    expect(oauth2RefreshToken).toHaveBeenCalledWith(
      expect.objectContaining({
        refreshToken: 'old-refresh',
        tokenUrl: 'https://idp.test/token',
        refreshTokenUrl: 'https://idp.test/refresh',
        clientId: 'client-123',
        clientSecret: 's3cret',
        collection: 'api',
        environmentName: 'dev',
        requestPath: 'ping.yml',
      }),
    );
    expect(oauth2GetToken).not.toHaveBeenCalled();
    expect(storedAuth().oauth2).toMatchObject({
      accessToken: 'new-access',
      refreshToken: 'new-refresh',
      expiresIn: 600,
      tokenAcquiredAt: NOW_S,
    });
    expect(executeRequest).toHaveBeenCalledTimes(1);
    expect(sentAuth()).toMatchObject({ authType: 'bearer', token: 'new-access' });
  });

  it.each([
    'client_credentials',
    'resource_owner_password_credentials',
  ])('(b) auto-fetches a missing token for %s with a resolved request', async (flow) => {
    vi.mocked(oauth2GetToken).mockResolvedValue({
      access_token: 'fetched',
      token_type: 'Bearer',
      expires_in: 300,
    });
    const req = openTab(
      requestWith(oauthAuth(flow, { autoFetchToken: true, autoRefreshToken: false })),
    );

    await sendRequest(TAB_ID, req);

    expect(oauth2GetToken).toHaveBeenCalledTimes(1);
    expect(oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({
        grantType: flow === 'client_credentials' ? flow : 'password',
        tokenUrl: 'https://idp.test/token',
        clientId: 'client-123',
        clientSecret: 's3cret',
        collection: 'api',
        environmentName: 'dev',
        requestPath: 'ping.yml',
      }),
    );
    expect(oauth2RefreshToken).not.toHaveBeenCalled();
    expect(storedAuth().oauth2).toMatchObject({
      accessToken: 'fetched',
      expiresIn: 300,
      tokenAcquiredAt: NOW_S,
    });
    expect(sentAuth()).toMatchObject({ authType: 'bearer', token: 'fetched' });
  });

  it.each([
    'authorization_code',
    'implicit',
  ])('(c) never auto-fetches the interactive %s grant on send', async (flow) => {
    const req = openTab(
      requestWith(oauthAuth(flow, { autoFetchToken: true, autoRefreshToken: true })),
    );

    await sendRequest(TAB_ID, req);

    expect(oauth2GetToken).not.toHaveBeenCalled();
    expect(oauth2RefreshToken).not.toHaveBeenCalled();
    expect(executeRequest).toHaveBeenCalledTimes(1);
  });

  it('(d) a failed refresh is non-fatal: warns and sends with the original auth', async () => {
    vi.mocked(oauth2RefreshToken).mockRejectedValue(new Error('idp down'));
    const req = openTab(
      requestWith(
        oauthAuth(
          'client_credentials',
          { autoFetchToken: true, autoRefreshToken: true },
          { accessToken: 'old-access', refreshToken: 'old-refresh', ...expired },
        ),
      ),
    );

    await sendRequest(TAB_ID, req);

    expect(warn).toHaveBeenCalledTimes(1);
    expect(executeRequest).toHaveBeenCalledTimes(1);
    expect(sentAuth()).toMatchObject({ authType: 'bearer', token: 'old-access' });
    expect(storedAuth().oauth2?.accessToken).toBe('old-access');
  });

  it('(d) a failed fetch is non-fatal: warns and still sends the request', async () => {
    vi.mocked(oauth2GetToken).mockRejectedValue(new Error('invalid_client'));
    const req = openTab(
      requestWith(
        oauthAuth('client_credentials', { autoFetchToken: true, autoRefreshToken: false }),
      ),
    );

    await sendRequest(TAB_ID, req);

    expect(warn).toHaveBeenCalledTimes(1);
    expect(executeRequest).toHaveBeenCalledTimes(1);
    expect(storedAuth().oauth2?.accessToken).toBe('');
  });

  it('(e) leaves a valid unexpired token alone', async () => {
    const req = openTab(
      requestWith(
        oauthAuth(
          'client_credentials',
          { autoFetchToken: true, autoRefreshToken: true },
          {
            accessToken: 'still-good',
            refreshToken: 'r',
            expiresIn: 3600,
            tokenAcquiredAt: NOW_S - 10,
          },
        ),
      ),
    );

    await sendRequest(TAB_ID, req);

    expect(oauth2GetToken).not.toHaveBeenCalled();
    expect(oauth2RefreshToken).not.toHaveBeenCalled();
    expect(sentAuth()).toMatchObject({ authType: 'bearer', token: 'still-good' });
  });

  it('(f) ignores non-OAuth2 auth, even when stale OAuth2 state is left on it', async () => {
    const stale = oauthAuth('client_credentials', { autoFetchToken: true, autoRefreshToken: true });
    const req = openTab(requestWith({ ...stale, authType: 'bearer', bearer: { token: 'abc' } }));

    await sendRequest(TAB_ID, req);

    expect(oauth2GetToken).not.toHaveBeenCalled();
    expect(oauth2RefreshToken).not.toHaveBeenCalled();
    expect(executeRequest).toHaveBeenCalledTimes(1);
    expect(sentAuth()).toMatchObject({ authType: 'bearer', token: 'abc' });
  });
});
