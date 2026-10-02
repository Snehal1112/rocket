import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { AuthNode, type AuthNodeData } from '../AuthNode';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';

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

  it('never renders token or secret values', () => {
    renderAuth({
      kind: { ...kind, auth: { authType: 'bearer', token: 'super-secret-token' } },
      status: 'idle',
    });
    expect(screen.queryByText(/super-secret-token/)).not.toBeInTheDocument();
  });
});
