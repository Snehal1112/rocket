import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { TransformNode, type TransformNodeData } from '../TransformNode';

const kind = {
  kind: 'Transform' as const,
  label: 'Pick token',
  script: 'return response.body.token;',
};

function renderTransform(data: TransformNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn(), openProperties: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <TransformNode
          id='tf1'
          type='Transform'
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

describe('TransformNode', () => {
  it('renders one input handle and one result handle', () => {
    renderTransform({ kind, status: 'idle' });
    const card = screen.getByTestId('transform-node-card');
    expect(screen.getByText('Pick token')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(1);
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(1);
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="result"]')).toBeInTheDocument();
  });

  it('shows only the first non-empty line of the script', () => {
    renderTransform({
      kind: { ...kind, script: '\n\nconst token = response.body.token;\nreturn token;' },
      status: 'idle',
    });
    expect(screen.getByTestId('transform-script-preview')).toHaveTextContent(
      'const token = response.body.token;',
    );
    expect(screen.queryByText(/return token/)).not.toBeInTheDocument();
  });

  it('puts the whole first line in the preview title', () => {
    const long = `const token = ${'x'.repeat(80)};`;
    renderTransform({ kind: { ...kind, script: `${long}\nreturn token;` }, status: 'idle' });
    const preview = screen.getByTestId('transform-script-preview');
    expect(preview).toHaveAttribute('title', long);
    expect(preview.textContent).not.toBe(long);
  });

  it('shows a placeholder for an empty script', () => {
    renderTransform({ kind: { ...kind, script: '  \n ' }, status: 'idle' });
    const preview = screen.getByTestId('transform-script-preview');
    expect(preview).toHaveTextContent('(empty)');
    expect(preview).toHaveClass('text-muted-foreground');
    expect(preview).not.toHaveAttribute('title');
  });

  it('says Not taken for a skipped branch', () => {
    renderTransform({ kind, status: 'skipped', skipReason: 'branch_not_taken' });
    expect(screen.getByText(/not taken/i)).toBeInTheDocument();
  });

  it('says upstream failed for a failure skip', () => {
    renderTransform({ kind, status: 'skipped', skipReason: 'upstream_failed' });
    expect(screen.getByText(/upstream failed/i)).toBeInTheDocument();
  });

  it('shows the error of a failed run', () => {
    renderTransform({ kind, status: 'failed', error: 'script returned no value' });
    expect(screen.getByTestId('node-error')).toHaveTextContent('script returned no value');
  });
});
