import { act, fireEvent, render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { copyTextAsync } from '@/lib/clipboard';
import { WaitForCallbackNode, type WaitForCallbackNodeData } from '../WaitForCallbackNode';

vi.mock('@/lib/clipboard', () => ({ copyTextAsync: vi.fn(async () => undefined) }));

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
  beforeEach(() => vi.mocked(copyTextAsync).mockClear());

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

  it('copies the variable through the native-first clipboard helper', async () => {
    renderNode();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Copy variable' }));
    });
    expect(copyTextAsync).toHaveBeenCalledTimes(1);
    await expect(vi.mocked(copyTextAsync).mock.calls[0][0]).resolves.toBe('{{callback.payment}}');
  });

  it('announces the copy and clears the message after 1500 ms', async () => {
    vi.useFakeTimers();
    try {
      renderNode();
      expect(screen.getByRole('status')).toHaveTextContent('');
      await act(async () => {
        fireEvent.click(screen.getByRole('button', { name: 'Copy variable' }));
      });
      expect(screen.getByRole('status')).toHaveTextContent('Variable copied');
      await act(async () => {
        vi.advanceTimersByTime(1500);
      });
      expect(screen.getByRole('status')).toBeEmptyDOMElement();
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows the received method and time after a success', () => {
    renderNode({ status: 'success', value: 'POST', durationMs: 3100 });
    expect(screen.getByTestId('wait-node-result')).toHaveTextContent('✓ received POST · 3.1s');
  });

  it('shows progress while waiting', () => {
    renderNode({ status: 'running', progress: 'waiting… 42s left · 1 ignored call(s)' });
    expect(screen.getByTestId('node-progress')).toHaveTextContent('waiting… 42s left');
  });

  it('shows the live callback URL while the run holds one', () => {
    renderNode({ status: 'running', callbackUrl: 'http://10.0.0.5:4000/cb/tok' });
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
  });

  it('shows no callback URL when the tab holds none', () => {
    renderNode({ status: 'success' });
    expect(screen.queryByTestId('callback-url')).not.toBeInTheDocument();
  });

  it('shows the error of a failed wait', () => {
    renderNode({
      status: 'failed',
      error: 'Invalid input: no matching callback within 60s (0 ignored)',
    });
    expect(screen.getByTestId('node-error')).toHaveTextContent('no matching callback within 60s');
  });
});
