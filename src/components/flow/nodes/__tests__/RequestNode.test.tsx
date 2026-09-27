import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it } from 'vitest';
import { RequestNode } from '../RequestNode';

type Data = Parameters<typeof RequestNode>[0]['data'];

function nodeElement(data: Data) {
  return (
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
    </ReactFlowProvider>
  );
}

function renderNode(data: Data) {
  return render(nodeElement(data));
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

  it('exposes url, headers, and body target handles plus one result source handle', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    const card = screen.getByTestId('request-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['url', 'headers', 'body']);
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(sources).toEqual(['result']);
  });

  it('does not guess a method for a Saved source', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    expect(screen.queryByText('GET')).not.toBeInTheDocument();
    expect(screen.getByText('SAVED')).toBeInTheDocument();
  });

  it('shows a Saved source method when the caller supplies one', () => {
    renderNode({ kind: baseKind, status: 'idle', method: 'POST' });
    expect(screen.getByText('POST')).toBeInTheDocument();
  });

  it('shows the method of an Inline source', () => {
    renderNode({
      kind: {
        kind: 'Request',
        label: 'Inline',
        source: { type: 'Inline', request: { method: 'PUT', url: '', headers: [] } },
      },
      status: 'idle',
    });
    expect(screen.getByText('PUT')).toBeInTheDocument();
  });

  it('shows only the final state after rapid status transitions', () => {
    const { rerender } = renderNode({ kind: baseKind, status: 'idle' });
    rerender(nodeElement({ kind: baseKind, status: 'running' }));
    rerender(nodeElement({ kind: baseKind, status: 'success', statusCode: 200, durationMs: 5 }));
    const card = screen.getByTestId('request-node-card');
    expect(card).toHaveAttribute('data-status', 'success');
    expect(card.className).not.toContain('animate-pulse');
    rerender(nodeElement({ kind: baseKind, status: 'failed', error: 'boom' }));
    expect(card).toHaveAttribute('data-status', 'failed');
    expect(screen.queryByText(/200/)).not.toBeInTheDocument();
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });

  it('outlines the card when it is part of a rejected cycle', () => {
    renderNode({ kind: baseKind, status: 'idle', hasCycleError: true });
    expect(screen.getByTestId('request-node-card').className).toContain('ring-red-500');
  });
});
