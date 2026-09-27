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
  it('renders its label with only a target handle', () => {
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
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(1);
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(0);
    expect(card.querySelector('[data-handleid="value"]')).toBeInTheDocument();
  });
});
