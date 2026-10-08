import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
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

  it('shows how long the last run took', () => {
    wrap(
      <InputNode
        id='i1'
        data={{
          kind: { kind: 'Input', label: 'API Key', value: 'sk-123' },
          status: 'success',
          durationMs: 1500,
        }}
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
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('1.5s');
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

  it('labels the Run when and Value inputs next to their handles', () => {
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
    const trigger = screen.getByTestId('output-node-trigger-row');
    expect(trigger).toHaveTextContent('Run when');
    expect(trigger.querySelector('[data-handleid="trigger"]')).toBeInTheDocument();
    const value = screen.getByTestId('output-node-value-row');
    expect(value).toHaveTextContent('Value');
    expect(value.querySelector('[data-handleid="value"]')).toBeInTheDocument();
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

  it('shows the error of a failed Output node', () => {
    wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{
          kind: { kind: 'Output', label: 'Result' },
          status: 'failed',
          error: "field 'value' has 2 live inputs",
        }}
      />,
    );
    expect(screen.getByTestId('node-error')).toHaveTextContent("field 'value' has 2 live inputs");
  });

  it('shows the error of a failed Input node', () => {
    wrap(
      <InputNode
        {...props}
        id='i1'
        type='Input'
        data={{
          kind: { kind: 'Input', label: 'Key', value: 'k' },
          status: 'failed',
          error: 'bad input',
        }}
      />,
    );
    expect(screen.getByTestId('node-error')).toHaveTextContent('bad input');
  });

  it('falls back to a generic message when a failed node has no error', () => {
    wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{ kind: { kind: 'Output', label: 'Result' }, status: 'failed' }}
      />,
    );
    expect(screen.getByTestId('node-error')).toHaveTextContent('Error');
  });
});

describe('OutputNode value display', () => {
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

  function renderOutput(value?: string) {
    return wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{ kind: { kind: 'Output', label: 'Result' }, status: 'success', value }}
      />,
    );
  }

  function mockClipboard(writeText: () => Promise<void>) {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  }

  it('pretty-prints a JSON value', () => {
    renderOutput('{"token":"abc"}');
    expect(screen.getByTestId('output-node-value').textContent).toContain('\n  "token": "abc"\n');
  });

  it('shows (empty) and no copy button for an empty value', () => {
    renderOutput('');
    expect(screen.getByText('(empty)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Copy value' })).toBeNull();
  });

  it('says no value is wired when the Output has no value wire', () => {
    wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{
          kind: { kind: 'Output', label: 'Result' },
          status: 'success',
          value: '',
          hasValueWire: false,
        }}
      />,
    );
    expect(screen.getByText('(no value wired)')).toBeInTheDocument();
    expect(screen.queryByText('(empty)')).toBeNull();
  });

  it('shows a dash and no copy button for an undefined value', () => {
    renderOutput(undefined);
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Copy value' })).toBeNull();
  });

  it('copies the raw value and shows a check icon', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    mockClipboard(writeText);
    renderOutput('{"token":"abc"}');
    const btn = screen.getByRole('button', { name: 'Copy value' });
    expect(btn.querySelector('.lucide-copy')).not.toBeNull();
    fireEvent.click(btn);
    expect(writeText).toHaveBeenCalledWith('{"token":"abc"}');
    await waitFor(() => expect(btn.querySelector('.lucide-check')).not.toBeNull());
  });

  it('does not crash when the clipboard write is rejected', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'));
    mockClipboard(writeText);
    renderOutput('x');
    const btn = screen.getByRole('button', { name: 'Copy value' });
    fireEvent.click(btn);
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    expect(btn.querySelector('.lucide-copy')).not.toBeNull();
  });

  it('marks the value area for wheel, drag and key isolation', () => {
    renderOutput('x');
    expect(screen.getByTestId('output-node-value')).toHaveClass('nowheel', 'nodrag', 'nokey');
  });
});
