import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { AuthNode, type AuthNodeData } from '../AuthNode';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';

type FlowAuth = Extract<FlowNodeKind, { kind: 'Auth' }>['auth'];

const kind = {
  kind: 'Auth' as const,
  label: 'Sign in',
  auth: { authType: 'bearer' as const, token: 't' },
  applyToInherit: true,
};

function renderAuth(data: AuthNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn(), openProperties: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <AuthNode
          id='a1'
          type='Auth'
          data={data}
          selected={false}
          dragging={false}
          zIndex={0}
          isConnectable
          draggable
          selectable
          deletable
          positionAbsoluteX={0}
          positionAbsoluteY={0}
        />
      </FlowNodeActionsContext.Provider>
    </ReactFlowProvider>,
  );
}

describe('AuthNode', () => {
  it('shows the label, the auth summary and one result handle, no inputs', () => {
    renderAuth({ kind, status: 'idle' });
    const card = screen.getByTestId('auth-node-card');
    expect(screen.getByText('Sign in')).toBeInTheDocument();
    expect(screen.getByTestId('auth-node-summary')).toHaveTextContent('Bearer');
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(0);
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(sources).toEqual(['result']);
  });

  it('says when it applies to inherited auth', () => {
    renderAuth({ kind, status: 'idle' });
    expect(screen.getByTestId('auth-node-applies')).toHaveTextContent('Applies to inherited auth');
  });

  it('does not say so when it does not apply', () => {
    renderAuth({ kind: { ...kind, applyToInherit: false }, status: 'idle' });
    expect(screen.queryByTestId('auth-node-applies')).not.toBeInTheDocument();
  });

  describe('never renders a secret value', () => {
    // Checks the whole card, text and attributes, not just text nodes.
    const cases: [string, FlowAuth, string[]][] = [
      ['Bearer', { authType: 'bearer', token: 'SECRET-bearer-1' }, ['SECRET-bearer-1']],
      [
        'Basic',
        { authType: 'basic', username: 'alice', password: 'SECRET-basic-2' },
        ['SECRET-basic-2'],
      ],
      [
        'API key',
        { authType: 'api-key', key: 'X-Key', value: 'SECRET-apikey-3', placement: 'header' },
        ['SECRET-apikey-3'],
      ],
      [
        'OAuth2',
        {
          authType: 'o-auth2',
          flow: 'client_credentials',
          accessTokenUrl: 'https://idp.test/token',
          credentials: { clientId: 'cid', clientSecret: 'SECRET-oauth-4', placement: 'body' },
        },
        ['SECRET-oauth-4'],
      ],
      [
        'AWS',
        {
          authType: 'aws-sig-v4',
          accessKey: 'AKIAEXAMPLE',
          secretKey: 'SECRET-aws-5',
          sessionToken: 'SECRET-aws-session-6',
          region: 'us-east-1',
          service: 's3',
        },
        ['SECRET-aws-5', 'SECRET-aws-session-6'],
      ],
      [
        'NTLM',
        { authType: 'ntlm', username: 'bob', password: 'SECRET-ntlm-7', domain: 'CORP' },
        ['SECRET-ntlm-7'],
      ],
      [
        'Digest',
        { authType: 'digest', username: 'carol', password: 'SECRET-digest-8' },
        ['SECRET-digest-8'],
      ],
      [
        'WSSE',
        { authType: 'wsse', username: 'dave', password: 'SECRET-wsse-9' },
        ['SECRET-wsse-9'],
      ],
    ];

    it.each(cases)('%s', (_name, auth, secrets) => {
      renderAuth({ kind: { ...kind, auth }, status: 'idle' });
      const card = screen.getByTestId('auth-node-card');
      for (const secret of secrets) {
        expect(card.textContent).not.toContain(secret);
        expect(card.innerHTML).not.toContain(secret);
      }
    });
  });
});
