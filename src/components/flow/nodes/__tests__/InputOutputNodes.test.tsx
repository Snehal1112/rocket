import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it } from 'vitest';
import { InputNode } from '../InputNode';
import { OutputNode } from '../OutputNode';

function wrap(children: React.ReactNode) {
  return render(<ReactFlowProvider>{children}</ReactFlowProvider>);
}

describe('InputNode', () => {
  it('renders its label and value with only a source handle', () => {
    wrap(
      <InputNode
        id='i1'
        data={{ kind: { kind: 'Input', label: 'API Key', value: 'sk-123' }, status: 'idle' }}
        selected={false}
        type='Input'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByText('API Key')).toBeInTheDocument();
    const card = screen.getByTestId('input-node-card');
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(1);
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(0);
    expect(card.querySelector('[data-handleid="result"]')).toBeInTheDocument();
  });

  it('renders without crashing when value is undefined', () => {
    wrap(
      <InputNode
        id='i1'
        data={{ kind: { kind: 'Input', label: 'API Key', value: undefined }, status: 'idle' }}
        selected={false}
        type='Input'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByTestId('input-node-card')).toBeInTheDocument();
  });
});

describe('OutputNode', () => {
  it('renders its label with value and Run when target handles and no source handle', () => {
    wrap(
      <OutputNode
        id='o1'
        data={{ kind: { kind: 'Output', label: 'Result' }, status: 'idle' }}
        selected={false}
        type='Output'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByText('Result')).toBeInTheDocument();
    const card = screen.getByTestId('output-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger', 'value']);
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(0);
    expect(card.querySelector('[data-handleid="trigger"]')?.getAttribute('title')).toBe('Run when');
  });
});

describe('run status on Input/Output nodes', () => {
  const props = {
    selected: false,
    dragging: false,
    zIndex: 0,
    isConnectable: true,
    draggable: true,
    selectable: true,
    deletable: true,
    positionAbsoluteX: 0,
    positionAbsoluteY: 0,
  };

  it('marks an Input node with its status', () => {
    wrap(
      <InputNode
        {...props}
        id='i1'
        type='Input'
        data={{ kind: { kind: 'Input', label: 'Key', value: 'k' }, status: 'success' }}
      />,
    );
    expect(screen.getByTestId('input-node-card')).toHaveAttribute('data-status', 'success');
  });

  it('captions a not-taken Output node', () => {
    wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{
          kind: { kind: 'Output', label: 'Result' },
          status: 'skipped',
          skipReason: 'branch_not_taken',
        }}
      />,
    );
    const card = screen.getByTestId('output-node-card');
    expect(card).toHaveAttribute('data-status', 'skipped');
    expect(card).toHaveClass('border-dashed');
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
  });
});
