import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
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
            },
          ]
        : [],
  }),
  useGlobalEnvironmentName: () => ({ data: 'global' }),
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
    const stored = useFlowAuthStore.getState().getAuth(flowAuthKey('api', 'login', 'n1', null));
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
    useFlowAuthStore.getState().setAuth(flowAuthKey('api', 'login', 'n1', null), {
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
});
