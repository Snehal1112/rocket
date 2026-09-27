import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it } from 'vitest';
import { RequestNode } from '../RequestNode';

function renderNode(data: Parameters<typeof RequestNode>[0]['data']) {
  return render(
    <ReactFlowProvider>
      <RequestNode
        id='n1'
        data={data}
        selected={false}
        type='Request'
        dragging={false}
        draggable
        selectable
        deletable
        zIndex={0}
        isConnectable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />
    </ReactFlowProvider>,
  );
}

const baseKind = {
  kind: 'Request' as const,
  label: 'Get Auth Token',
  source: { type: 'Saved' as const, requestPath: 'auth/login.yml' },
};

describe('RequestNode', () => {
  it('renders idle state with method badge, label, and field rows', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    expect(screen.getByText('Get Auth Token')).toBeInTheDocument();
    expect(screen.getByText(/URL/)).toBeInTheDocument();
    expect(screen.getByText(/Headers/)).toBeInTheDocument();
    expect(screen.getByText(/Body/)).toBeInTheDocument();
  });

  it('renders a headers row even with zero headers configured', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    expect(screen.getByTestId('request-node-headers-row')).toBeInTheDocument();
  });

  it('renders success state with status glow and result text', () => {
    renderNode({
      kind: baseKind,
      status: 'success',
      statusCode: 200,
      durationMs: 184,
    });
    expect(screen.getByText(/200/)).toBeInTheDocument();
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'success');
  });

  it('renders failure state with status glow and error text', () => {
    renderNode({
      kind: baseKind,
      status: 'failed',
      statusCode: 401,
      durationMs: 92,
      error: 'Unauthorized',
    });
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'failed');
  });

  it('renders running state distinctly from idle', () => {
    renderNode({ kind: baseKind, status: 'running' });
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'running');
  });
});
