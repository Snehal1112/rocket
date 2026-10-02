import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { AuthNodeEditor } from '../AuthNodeEditor';

// The real AuthEditor pulls in CodeMirror and OAuth2 sections. A small stand-in
// with the same props lets these tests drive onChange directly.
vi.mock('@/components/request/AuthEditor', () => ({
  AuthEditor: (props: { auth: AuthState; onChange: (a: AuthState) => void }) => (
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
  ),
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
});
