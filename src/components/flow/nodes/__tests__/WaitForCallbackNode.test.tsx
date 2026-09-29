import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { WaitForCallbackNode, type WaitForCallbackNodeData } from '../WaitForCallbackNode';

const props = {
  selected: false,
  type: 'WaitForCallback',
  dragging: false,
  zIndex: 0,
  isConnectable: true,
  draggable: true,
  selectable: true,
  deletable: true,
  positionAbsoluteX: 0,
  positionAbsoluteY: 0,
};

function renderNode(data: Partial<WaitForCallbackNodeData> = {}) {
  return render(
    <ReactFlowProvider>
      <WaitForCallbackNode
        {...props}
        id='w1'
        data={{
          kind: {
            kind: 'WaitForCallback',
            label: 'Payment done',
            name: 'payment',
            timeoutMs: 60000,
          },
          status: 'idle',
          ...data,
        }}
      />
    </ReactFlowProvider>,
  );
}

describe('WaitForCallbackNode', () => {
  it('has a Run when input and a result exit, and shows name, timeout and variable', () => {
    renderNode();
    const card = screen.getByTestId('wait-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger']);
    expect(sources).toEqual(['result']);
    expect(screen.getByText('Payment done')).toBeInTheDocument();
    expect(card).toHaveTextContent('Wait for callback · payment · 60s');
    expect(screen.getByTestId('wait-node-variable')).toHaveTextContent('{{callback.payment}}');
  });

  it('truncates the summary line and keeps the full text in a title', () => {
    renderNode({
      kind: {
        kind: 'WaitForCallback',
        label: 'Payment done',
        name: 'a_very_long_callback_name_that_would_overflow',
        timeoutMs: 60000,
      },
    });
    const summary = screen.getByTestId('wait-node-summary');
    expect(summary).toHaveAttribute(
      'title',
      'Wait for callback · a_very_long_callback_name_that_would_overflow · 60s',
    );
    expect(summary).toHaveClass('truncate');
  });

  it('copies the variable', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    renderNode();
    fireEvent.click(screen.getByRole('button', { name: 'Copy variable' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('{{callback.payment}}'));
  });

  it('shows the received method and time after a success', () => {
    renderNode({ status: 'success', value: 'POST', durationMs: 3100 });
    expect(screen.getByTestId('wait-node-result')).toHaveTextContent('✓ received POST · 3.1s');
  });

  it('shows progress while waiting', () => {
    renderNode({ status: 'running', progress: 'waiting… 42s left · 1 ignored call(s)' });
    expect(screen.getByTestId('node-progress')).toHaveTextContent('waiting… 42s left');
  });

  it('shows the error of a failed wait', () => {
    renderNode({
      status: 'failed',
      error: 'Invalid input: no matching callback within 60s (0 ignored)',
    });
    expect(screen.getByTestId('node-error')).toHaveTextContent('no matching callback within 60s');
  });
});
