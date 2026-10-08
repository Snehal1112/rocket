import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey, oauth2Fingerprint } from '@/lib/flow-auth';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import * as tauriApi from '@/lib/tauri-api';
import type { Auth, FlowNodeKind } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { createDeferred } from '@/test/deferred';
import { buildVariableContext, resolveWithContext } from '@/lib/variable-context';
import { useEnvStore } from '@/stores/env-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { AuthNodeEditor } from '../AuthNodeEditor';

// The real AuthEditor pulls in CodeMirror and OAuth2 sections. A small stand-in
// with the same props lets these tests drive onChange directly.
// The props the editor last received, so tests can inspect variableContext.
const authEditorProps = vi.hoisted(() => ({
  last: null as null | { variableContext?: Map<string, VariableScopeEntry> },
}));
// The active global environment name the editor sees.
const globalEnvState = vi.hoisted(() => ({ name: 'global' as string | null }));

vi.mock('@/components/request/AuthEditor', () => ({
  AuthEditor: (props: {
    auth: AuthState;
    onChange: (a: AuthState) => void;
    variableContext?: Map<string, VariableScopeEntry>;
  }) => {
    authEditorProps.last = props;
    return (
      <div>
        <span data-testid='auth-type'>{props.auth.authType}</span>
        <span data-testid='access-token'>{props.auth.oauth2?.accessToken ?? ''}</span>
        <span data-testid='header-prefix'>{props.auth.oauth2?.headerPrefix ?? ''}</span>
        <button
          type='button'
          onClick={() =>
            props.auth.oauth2 &&
            props.onChange({ ...props.auth, oauth2: { ...props.auth.oauth2, headerPrefix: '' } })
          }
        >
          clear prefix
        </button>
        <button
          type='button'
          onClick={() =>
            props.auth.oauth2 &&
            props.onChange({
              ...props.auth,
              oauth2: { ...props.auth.oauth2, accessToken: 'fetched-token-123456' },
            })
          }
        >
          fetch token
        </button>
        <button type='button' onClick={() => props.onChange(props.auth)}>
          no-op edit
        </button>
        <button
          type='button'
          onClick={() =>
            props.auth.oauth2 &&
            props.onChange({
              ...props.auth,
              oauth2: { ...props.auth.oauth2, accessToken: '', expiresIn: null },
            })
          }
        >
          clear token
        </button>
        <button
          type='button'
          onClick={() =>
            props.auth.oauth2 &&
            props.onChange({ ...props.auth, oauth2: { ...props.auth.oauth2, scope: 'changed' } })
          }
        >
          change scope
        </button>
        <button
          type='button'
          onClick={() =>
            props.onChange({ authType: 'bearer', bearer: { token: 'typed-token-123456' } })
          }
        >
          make bearer
        </button>
      </div>
    );
  },
}));

// Variable sources: the active collection's environments, the global
// environment, process env, and the collection's own variables.
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: (collection: string | null) => ({
    data:
      collection === 'api'
        ? [
            {
              name: 'dev',
              variables: [
                { key: 'clientId', value: 'dev-client', enabled: true, secret: false },
                { key: 'disabled', value: 'x', enabled: false, secret: false },
              ],
              // A vault binding: {{vault.secret}} is resolved by the backend.
              externalSecrets: [
                {
                  alias: 'vault',
                  connectionId: 'c1',
                  vaultName: 'v',
                  secretNames: [{ name: 'secret' }],
                },
              ],
            },
          ]
        : [],
  }),
  useGlobalEnvironmentName: () => ({ data: globalEnvState.name }),
  useGlobalEnvironment: () => ({
    data: {
      name: 'global',
      variables: [{ key: 'tenant', value: 'acme', enabled: true, secret: false }],
    },
  }),
  useProcessEnvVars: () => ({ data: { HOME: '/home/u' } }),
}));

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  oauth2GetToken: vi.fn(),
  oauth2RefreshToken: vi.fn(),
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

vi.mock('@/lib/execute-request', () => ({
  buildOAuth2VarContext: vi.fn(async () => ({
    clientId: 'dev-client',
    tokenUrl: 'https://idp/token',
    tenant: 'acme',
  })),
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

type AuthKind = Extract<FlowNodeKind, { kind: 'Auth' }>;
const kind: AuthKind = {
  kind: 'Auth',
  label: 'Sign in',
  auth: { authType: 'basic', username: 'u', password: 'p' },
  applyToInherit: true,
};

describe('AuthNodeEditor', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
    authEditorProps.last = null;
    globalEnvState.name = 'global';
  });

  it('shows the label and loads the persisted auth into the editor', () => {
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    expect(screen.getByLabelText('Label')).toHaveValue('Sign in');
    expect(screen.getByTestId('auth-type')).toHaveTextContent('basic');
  });

  it('reports the whole node with a persisted auth when the auth changes', async () => {
    const onChange = vi.fn();
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={onChange}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'make bearer' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...kind,
      auth: { authType: 'bearer', token: 'typed-token-123456' },
    });
  });

  it('keeps the full state, with any token, only in the in-memory store', async () => {
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'make bearer' }));
    const stored = useFlowAuthStore
      .getState()
      .getAuth(flowAuthKey('api', 'login', 'n1', null, 'global'));
    expect(stored?.authType).toBe('bearer');
  });

  it('toggles apply to inherited auth', async () => {
    const onChange = vi.fn();
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={onChange}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.click(screen.getByRole('switch', { name: 'Apply to inherited auth' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, applyToInherit: false });
  });

  it('edits the label', async () => {
    const onChange = vi.fn();
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={onChange}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.type(screen.getByLabelText('Label'), '!');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, label: 'Sign in!' });
  });

  it('shows the persisted auth when the stored state is for an older configuration', () => {
    // Stored state for config A (bearer); the node now holds config B (basic).
    useFlowAuthStore.getState().setAuth(flowAuthKey('api', 'login', 'n1', null, 'global'), {
      authType: 'bearer',
      bearer: { token: 'stale-token-123456' },
    });
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    expect(screen.getByTestId('auth-type')).toHaveTextContent('basic');
  });

  it('gives the auth editor the active collection and environment variables', async () => {
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await waitFor(() =>
      expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
        'https://idp/token',
      ),
    );
    const ctx = authEditorProps.last?.variableContext;
    expect(ctx?.get('clientId')).toEqual(
      expect.objectContaining({ value: 'dev-client', source: 'environment' }),
    );
    expect(ctx?.has('disabled')).toBe(false);
    expect(ctx?.get('tenant')?.value).toBe('acme');
    expect(ctx?.get('process.env.HOME')?.value).toBe('/home/u');
  });

  it('has an Auth type selector and switches to OAuth 2.0 without persisting a token', async () => {
    const onChange = vi.fn();
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={onChange}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.click(screen.getByRole('combobox', { name: 'Auth type' }));
    await userEvent.click(await screen.findByRole('option', { name: 'OAuth 2.0' }));
    const sent = onChange.mock.lastCall?.[0] as AuthKind;
    expect(sent.auth.authType).toBe('o-auth2');
    // No key named accessToken at any depth (accessTokenUrl etc. are fine).
    expect(JSON.stringify(sent.auth)).not.toMatch(/"(accessToken|access_token)"\s*:/);
  });

  it('switches to Basic', async () => {
    const onChange = vi.fn();
    render(
      <AuthNodeEditor
        kind={{ ...kind, auth: { authType: 'bearer', token: 'tok-12345678' } }}
        onChange={onChange}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    await userEvent.click(screen.getByRole('combobox', { name: 'Auth type' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Basic' }));
    const lastKind = onChange.mock.lastCall?.[0] as AuthKind | undefined;
    expect(lastKind?.auth.authType).toBe('basic');
  });

  describe('OAuth2 tokens', () => {
    const oauthKind: AuthKind = {
      ...kind,
      auth: {
        authType: 'o-auth2',
        flow: 'client_credentials',
        accessTokenUrl: '{{tokenUrl}}',
        credentials: { clientId: '{{clientId}}', clientSecret: 's' },
      } as unknown as Auth,
    };
    // The values the editor resolves with: 'dev' environment + collection variables.
    const devVars = { clientId: 'dev-client', tokenUrl: 'https://idp/token', tenant: 'acme' };
    const withToken = (): AuthState => {
      const base = fromPersistedAuth(oauthKind.auth);
      return {
        ...base,
        oauth2: {
          ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
          accessToken: 'stored-token-123456',
          expiresIn: 3600,
          tokenAcquiredAt: Math.floor(Date.now() / 1000),
        },
      };
    };
    const fingerprintFor = (state: AuthState, vars: Record<string, string>) =>
      oauth2Fingerprint(state.oauth2 as NonNullable<AuthState['oauth2']>, (s) =>
        resolveWithContext(s, vars),
      );
    const renderEditor = () =>
      render(
        <AuthNodeEditor
          kind={oauthKind}
          onChange={vi.fn()}
          collection='api'
          flowName='login'
          nodeId='n1'
        />,
      );

    beforeEach(() => {
      useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
    });

    it('shows a stored token fetched with the current variable values', async () => {
      const state = withToken();
      useFlowAuthStore
        .getState()
        .setAuth(
          flowAuthKey('api', 'login', 'n1', 'dev', 'global'),
          state,
          fingerprintFor(state, devVars),
        );
      renderEditor();
      await waitFor(() =>
        expect(screen.getByTestId('access-token')).toHaveTextContent('stored-token-123456'),
      );
    });

    it('does not show a stored token fetched with other variable values', async () => {
      const state = withToken();
      useFlowAuthStore
        .getState()
        .setAuth(
          flowAuthKey('api', 'login', 'n1', 'dev', 'global'),
          state,
          fingerprintFor(state, { ...devVars, clientId: 'prod-client' }),
        );
      renderEditor();
      // Wait for the collection variables to load, then the token is still hidden.
      await waitFor(() =>
        expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
          'https://idp/token',
        ),
      );
      expect(screen.getByTestId('access-token')).toHaveTextContent('');
    });

    it('does not show a token stored under another global environment', async () => {
      const state = withToken();
      useFlowAuthStore
        .getState()
        .setAuth(
          flowAuthKey('api', 'login', 'n1', 'dev', 'other-global'),
          state,
          fingerprintFor(state, devVars),
        );
      renderEditor();
      await waitFor(() =>
        expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
          'https://idp/token',
        ),
      );
      expect(screen.getByTestId('access-token')).toHaveTextContent('');
    });

    it('keys the stored state by the global environment and remembers the fingerprint of a fetched token', async () => {
      globalEnvState.name = null;
      renderEditor();
      await waitFor(() =>
        expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
          'https://idp/token',
        ),
      );
      await userEvent.click(screen.getByRole('button', { name: 'fetch token' }));
      const entry = useFlowAuthStore
        .getState()
        .getEntry(flowAuthKey('api', 'login', 'n1', 'dev', ''));
      expect(entry?.auth.oauth2?.accessToken).toBe('fetched-token-123456');
      expect(entry?.fingerprint).toBe(
        fingerprintFor(entry?.auth as AuthState, {
          clientId: 'dev-client',
          tokenUrl: 'https://idp/token',
        }),
      );
    });

    it('keeps an emptied header prefix and the token once the node saves it', async () => {
      const state = withToken();
      useFlowAuthStore
        .getState()
        .setAuth(
          flowAuthKey('api', 'login', 'n1', 'dev', 'global'),
          state,
          fingerprintFor(state, devVars),
        );
      // Feeds each reported node back in, as the flow pane does.
      function Harness() {
        const [current, setCurrent] = useState<AuthKind>(oauthKind);
        return (
          <AuthNodeEditor
            kind={current}
            onChange={(k) => setCurrent(k as AuthKind)}
            collection='api'
            flowName='login'
            nodeId='n1'
          />
        );
      }
      render(<Harness />);
      await waitFor(() =>
        expect(screen.getByTestId('access-token')).toHaveTextContent('stored-token-123456'),
      );

      await userEvent.click(screen.getByRole('button', { name: 'clear prefix' }));

      expect(screen.getByTestId('header-prefix')).toHaveTextContent(/^$/);
      expect(screen.getByTestId('access-token')).toHaveTextContent('stored-token-123456');
    });

    it('keeps a stored token that is hidden only because the variables changed when an edit leaves the configuration as it is', async () => {
      const state = withToken();
      const staleFingerprint = fingerprintFor(state, { ...devVars, clientId: 'prod-client' });
      const key = flowAuthKey('api', 'login', 'n1', 'dev', 'global');
      useFlowAuthStore.getState().setAuth(key, state, staleFingerprint);
      renderEditor();
      await waitFor(() =>
        expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
          'https://idp/token',
        ),
      );

      await userEvent.click(screen.getByRole('button', { name: 'no-op edit' }));

      // Still hidden, but kept with its own fingerprint, for when the values match again.
      expect(screen.getByTestId('access-token')).toHaveTextContent(/^$/);
      const entry = useFlowAuthStore.getState().getEntry(key);
      expect(entry?.auth.oauth2?.accessToken).toBe('stored-token-123456');
      expect(entry?.fingerprint).toBe(staleFingerprint);
    });

    it('clears a shown token when the user clears it', async () => {
      const state = withToken();
      const key = flowAuthKey('api', 'login', 'n1', 'dev', 'global');
      useFlowAuthStore.getState().setAuth(key, state, fingerprintFor(state, devVars));
      renderEditor();
      await waitFor(() =>
        expect(screen.getByTestId('access-token')).toHaveTextContent('stored-token-123456'),
      );

      await userEvent.click(screen.getByRole('button', { name: 'clear token' }));

      expect(useFlowAuthStore.getState().getEntry(key)?.auth.oauth2?.accessToken).toBe('');
    });
  });

  describe('OAuth2 tokens with a vault reference', () => {
    const vaultKind: AuthKind = {
      ...kind,
      auth: {
        authType: 'o-auth2',
        flow: 'client_credentials',
        accessTokenUrl: '{{tokenUrl}}',
        credentials: { clientId: '{{clientId}}', clientSecret: '{{vault.secret}}' },
      } as unknown as Auth,
    };
    const key = flowAuthKey('api', 'login', 'n1', 'dev', 'global');
    const withToken = (): AuthState => {
      const base = fromPersistedAuth(vaultKind.auth);
      return {
        ...base,
        oauth2: {
          ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
          accessToken: 'preflight-token-123456',
          expiresIn: 3600,
          tokenAcquiredAt: Math.floor(Date.now() / 1000),
        },
      };
    };
    // How the pre-run step fingerprints: buildVariableContext leaves
    // {{vault.secret}} as written, for the backend to resolve.
    const preflightFingerprint = (state: AuthState) =>
      oauth2Fingerprint(state.oauth2 as NonNullable<AuthState['oauth2']>, (s) =>
        resolveWithContext(
          s,
          buildVariableContext({
            processEnvVars: { HOME: '/home/u' },
            globalVars: { tenant: 'acme' },
            envVars: { clientId: 'dev-client' },
            collectionVars: [
              {
                key: 'tokenUrl',
                value: 'https://idp/token',
                initialValue: '',
                enabled: true,
                secret: false,
              },
            ],
          }),
        ),
      );

    beforeEach(() => {
      useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
    });

    it('shows a token the pre-run step stored', async () => {
      const state = withToken();
      useFlowAuthStore.getState().setAuth(key, state, preflightFingerprint(state));
      render(
        <AuthNodeEditor
          kind={vaultKind}
          onChange={vi.fn()}
          collection='api'
          flowName='login'
          nodeId='n1'
        />,
      );
      await waitFor(() =>
        expect(screen.getByTestId('access-token')).toHaveTextContent('preflight-token-123456'),
      );
    });

    it('keeps that token, with the same fingerprint, across an edit that leaves the configuration as it is', async () => {
      const state = withToken();
      useFlowAuthStore.getState().setAuth(key, state, preflightFingerprint(state));
      render(
        <AuthNodeEditor
          kind={vaultKind}
          onChange={vi.fn()}
          collection='api'
          flowName='login'
          nodeId='n1'
        />,
      );
      await waitFor(() =>
        expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
          'https://idp/token',
        ),
      );

      await userEvent.click(screen.getByRole('button', { name: 'no-op edit' }));

      const entry = useFlowAuthStore.getState().getEntry(key);
      expect(entry?.auth.oauth2?.accessToken).toBe('preflight-token-123456');
      expect(entry?.fingerprint).toBe(preflightFingerprint(state));
    });
  });

  it('looks environments up in the collection prop, not the env store active collection', async () => {
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'other' });
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    // The 'dev' environment exists only for 'api' in the mock.
    await waitFor(() =>
      expect(authEditorProps.last?.variableContext?.get('clientId')).toEqual(
        expect.objectContaining({ value: 'dev-client', source: 'environment' }),
      ),
    );
  });

  describe('apply to inherited auth', () => {
    const renderApply = (applyToInherit: boolean, otherNodeApplies?: boolean) =>
      render(
        <AuthNodeEditor
          kind={{ ...kind, applyToInherit }}
          onChange={vi.fn()}
          collection='api'
          flowName='login'
          nodeId='n1'
          otherNodeApplies={otherNodeApplies}
        />,
      );
    const note = /another auth node already applies/i;

    it('disables the switch and explains why when another node applies', () => {
      renderApply(false, true);
      expect(screen.getByRole('switch', { name: 'Apply to inherited auth' })).toBeDisabled();
      expect(screen.getByText(note)).toBeInTheDocument();
    });

    it('is enabled when no other node applies', () => {
      renderApply(false, false);
      expect(screen.getByRole('switch', { name: 'Apply to inherited auth' })).toBeEnabled();
      expect(screen.queryByText(note)).not.toBeInTheDocument();
    });

    it('stays enabled when this node itself applies', () => {
      renderApply(true, true);
      expect(screen.getByRole('switch', { name: 'Apply to inherited auth' })).toBeEnabled();
      expect(screen.queryByText(note)).not.toBeInTheDocument();
    });

    it('is unaffected for a single node', () => {
      renderApply(false);
      expect(screen.getByRole('switch', { name: 'Apply to inherited auth' })).toBeEnabled();
    });
  });
});

describe('AuthNodeEditor plaintext credential warning', () => {
  const SECRET = 'hunter2-literal-value';
  const renderKind = (auth: Auth) =>
    render(
      <AuthNodeEditor
        kind={{ ...kind, auth }}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );

  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
  });

  it('warns about a literal password and names the field, never the value', () => {
    renderKind({ authType: 'basic', username: 'u', password: SECRET });
    const note = screen.getByRole('note');
    expect(note).toHaveTextContent(
      'This credential is saved as plain text in the flow file. Use a {{variable}} or a RocketVault reference instead.',
    );
    expect(note).toHaveTextContent('Password');
    expect(document.body.innerHTML).not.toContain(SECRET);
  });

  it('lists every literal field of an OAuth 2.0 auth', () => {
    renderKind({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      accessTokenUrl: '{{tokenUrl}}',
      credentials: { clientId: '{{clientId}}', clientSecret: SECRET },
      resourceOwner: { username: 'u', password: 'also-literal' },
    } as unknown as Auth);
    const note = screen.getByRole('note');
    expect(note).toHaveTextContent('Client secret');
    expect(note).toHaveTextContent('Resource owner password');
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(document.body.innerHTML).not.toContain('also-literal');
  });

  it('does not warn when the credential is a variable reference', () => {
    renderKind({ authType: 'basic', username: 'u', password: '{{password}}' });
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });

  it('does not warn for an empty credential', () => {
    renderKind({ authType: 'bearer', token: '' });
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });
});

describe('AuthNodeEditor Authenticate button', () => {
  const grantKind = (flow: string): AuthKind => ({
    ...kind,
    auth: {
      authType: 'o-auth2',
      flow,
      accessTokenUrl: '{{tokenUrl}}',
      authorizationUrl: 'https://idp/authorize',
      credentials: { clientId: '{{clientId}}', clientSecret: '{{secret}}' },
    } as unknown as Auth,
  });
  const tokenResult = {
    access_token: 'authenticated-token-123456',
    token_type: 'Bearer',
    expires_in: 3600,
    refresh_token: 'refresh-123456',
  };
  const renderGrant = (flow: string) =>
    render(
      <AuthNodeEditor
        kind={grantKind(flow)}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );

  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
    globalEnvState.name = 'global';
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
  });

  it.each(['authorization_code', 'implicit'])('shows the button for the %s grant', (flow) => {
    renderGrant(flow);
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeInTheDocument();
    expect(screen.getByText('No token')).toBeInTheDocument();
  });

  it.each(['client_credentials', 'resource_owner_password_credentials'])(
    'shows no button for the %s grant',
    (flow) => {
      renderGrant(flow);
      expect(screen.queryByRole('button', { name: /^Authenticate/ })).not.toBeInTheDocument();
    },
  );

  it('shows no button for a static auth type', () => {
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    expect(screen.queryByRole('button', { name: /^Authenticate/ })).not.toBeInTheDocument();
  });

  it('signs in, stores the token under the displayed key and shows the status', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(tokenResult);
    renderGrant('authorization_code');
    await waitFor(() =>
      expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
        'https://idp/token',
      ),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('status')).toHaveTextContent('Signed in.');
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({ grantType: 'authorization_code', clientId: 'dev-client' }),
    );
    // The token sits under the key the editor reads, so the status and the editor see it.
    const entry = useFlowAuthStore
      .getState()
      .getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global'));
    expect(entry?.auth.oauth2?.accessToken).toBe('authenticated-token-123456');
    expect(screen.getByText(/^Token valid until/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate again' })).toBeInTheDocument();
    // The AuthEditor stand-in prints the token on purpose, so it is left out of the check.
    const page = document.body.cloneNode(true) as HTMLElement;
    page.querySelector('[data-testid="access-token"]')?.remove();
    expect(page.innerHTML).not.toContain('authenticated-token-123456');
  });

  it('discards the token when the active environment changes during sign-in', async () => {
    const pending = createDeferred<typeof tokenResult>();
    vi.mocked(tauriApi.oauth2GetToken).mockReturnValue(pending.promise);
    renderGrant('authorization_code');
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    act(() => useEnvStore.setState({ activeEnvId: 'other' }));
    pending.resolve(tokenResult);

    expect(await screen.findByRole('status')).toHaveTextContent(
      'The sign-in settings changed, so the new token was not saved.',
    );
    expect(useFlowAuthStore.getState().auths).toEqual({});
    expect(screen.getByText('No token')).toBeInTheDocument();
  });

  it('uses no more than one sign-in for two quick clicks', async () => {
    const pending = createDeferred<typeof tokenResult>();
    vi.mocked(tauriApi.oauth2GetToken).mockReturnValue(pending.promise);
    renderGrant('authorization_code');

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    await userEvent.click(screen.getByRole('button', { name: 'Signing in…' }));

    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
    pending.resolve(tokenResult);
    expect(await screen.findByRole('status')).toBeInTheDocument();
  });

  it('shows a failed sign-in as an alert and stores no token', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockRejectedValue(new Error('window closed'));
    renderGrant('authorization_code');

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Sign-in for Auth node "Sign in" failed: window closed',
    );
    expect(
      useFlowAuthStore.getState().getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global')),
    ).toBeUndefined();
  });

  it('drops the token when the configuration is edited while the sign-in window is open', async () => {
    const pending = createDeferred<typeof tokenResult>();
    vi.mocked(tauriApi.oauth2GetToken).mockReturnValue(pending.promise);
    // Feeds each reported node back in, as the flow pane does.
    function Harness() {
      const [current, setCurrent] = useState<AuthKind>(grantKind('authorization_code'));
      return (
        <AuthNodeEditor
          kind={current}
          onChange={(k) => setCurrent(k as AuthKind)}
          collection='api'
          flowName='login'
          nodeId='n1'
        />
      );
    }
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    await userEvent.click(screen.getByRole('button', { name: 'change scope' }));
    pending.resolve(tokenResult);

    expect(await screen.findByRole('status')).toHaveTextContent(
      'The sign-in settings changed, so the new token was not saved.',
    );
    const entry = useFlowAuthStore
      .getState()
      .getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global'));
    expect(entry?.auth.oauth2?.accessToken ?? '').toBe('');
    expect(screen.getByText('No token')).toBeInTheDocument();
  });
});
