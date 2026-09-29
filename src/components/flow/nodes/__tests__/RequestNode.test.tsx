import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { RequestNode } from '../RequestNode';

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

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
  it('shows progress while running', () => {
    renderNode({ kind: baseKind, status: 'running', progress: 'attempt 2/5' });
    expect(screen.getByTestId('node-progress')).toHaveTextContent('attempt 2/5');
  });

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

  it('shows one failure line, not a second generic error caption', () => {
    renderNode({
      kind: baseKind,
      status: 'failed',
      statusCode: 400,
      durationMs: 92,
      error: 'non-2xx response: 400',
    });
    expect(screen.getByText(/non-2xx response: 400/)).toBeInTheDocument();
    expect(screen.queryByTestId('node-error')).not.toBeInTheDocument();
  });

  it('renders running state distinctly from idle', () => {
    renderNode({ kind: baseKind, status: 'running' });
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'running');
  });

  it('exposes trigger, url, headers, and body target handles plus one result source handle', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    const card = screen.getByTestId('request-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger', 'url', 'headers', 'body']);
    expect(screen.getByText('Run when')).toBeInTheDocument();
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

  it('shows a debug badge only while debug mode is on', () => {
    const { rerender } = renderNode({ kind: { ...baseKind, debug: true }, status: 'idle' });
    expect(screen.getByTestId('request-node-debug-badge')).toHaveAccessibleName('Debug mode on');
    rerender(nodeElement({ kind: { ...baseKind, debug: false }, status: 'idle' }));
    expect(screen.queryByTestId('request-node-debug-badge')).not.toBeInTheDocument();
    rerender(nodeElement({ kind: baseKind, status: 'idle' }));
    expect(screen.queryByTestId('request-node-debug-badge')).not.toBeInTheDocument();
  });

  describe('node menu', () => {
    function renderWithActions() {
      const actions = {
        updateNodeKind: vi.fn(),
        removeSwitchCase: vi.fn(),
        openProperties: vi.fn(),
      };
      render(
        <FlowNodeActionsContext.Provider value={actions}>
          {nodeElement({ kind: baseKind, status: 'idle' })}
        </FlowNodeActionsContext.Provider>,
      );
      return actions;
    }

    it('offers Edit properties and an unchecked Debug mode item', async () => {
      const actions = renderWithActions();
      const user = userEvent.setup();
      await user.click(screen.getByLabelText('Edit Get Auth Token'));
      expect(screen.getByRole('menuitemcheckbox', { name: 'Debug mode' })).toHaveAttribute(
        'aria-checked',
        'false',
      );
      await user.click(screen.getByRole('menuitem', { name: 'Edit properties' }));
      expect(actions.openProperties).toHaveBeenCalledWith('n1');
    });

    it('turns debug mode on through updateNodeKind', async () => {
      const actions = renderWithActions();
      const user = userEvent.setup();
      await user.click(screen.getByLabelText('Edit Get Auth Token'));
      await user.click(screen.getByRole('menuitemcheckbox', { name: 'Debug mode' }));
      expect(actions.updateNodeKind).toHaveBeenCalledWith('n1', { ...baseKind, debug: true });
    });

    it('marks the portalled menu nokey', async () => {
      renderWithActions();
      const user = userEvent.setup();
      await user.click(screen.getByLabelText('Edit Get Auth Token'));
      expect(screen.getByRole('menu')).toHaveClass('nokey');
    });
  });

  it('captions an upstream-failed skip and a not-taken skip differently', () => {
    const { rerender } = renderNode({
      kind: baseKind,
      status: 'skipped',
      skipReason: 'upstream_failed',
    });
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent(
      'Skipped — upstream failed',
    );
    expect(screen.getByTestId('request-node-card')).not.toHaveClass('border-dashed');

    rerender(nodeElement({ kind: baseKind, status: 'skipped', skipReason: 'branch_not_taken' }));
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
    expect(screen.getByTestId('request-node-card')).toHaveClass('border-dashed');
  });

  const pollingKind = {
    ...baseKind,
    repeatUntil: {
      condition: 'response.body.status === "done"',
      intervalMs: 2000,
      maxAttempts: 30,
      timeoutMs: 60000,
    },
  };

  it('shows the repeat-until row for a polling request', () => {
    renderNode({ kind: pollingKind, status: 'idle' });
    expect(screen.getByTestId('request-node-repeat-row')).toHaveTextContent(
      'until response.body.status === "done" · 2s · max 30',
    );
  });

  it('shows no repeat row for a plain request', () => {
    renderNode({ kind: baseKind, status: 'success', statusCode: 200, durationMs: 184 });
    expect(screen.queryByTestId('request-node-repeat-row')).toBeNull();
    expect(screen.getByText((_, el) => el?.textContent === '✓ 200 · 184ms')).toBeInTheDocument();
  });

  it('truncates a long condition and keeps it in the title', () => {
    const condition = `response.body.${'x'.repeat(200)} === "done"`;
    renderNode({
      kind: { ...pollingKind, repeatUntil: { ...pollingKind.repeatUntil, condition } },
      status: 'idle',
    });
    const row = screen.getByTestId('request-node-repeat-row');
    expect(row).toHaveAttribute('title', condition);
    expect(row.querySelector('.truncate')).not.toBeNull();
  });

  it('shows the attempt count and total time after a poll', () => {
    renderNode({
      kind: pollingKind,
      status: 'success',
      statusCode: 200,
      durationMs: 14200,
      attempts: 7,
    });
    expect(screen.getByText('✓ 200 · 7 attempts · 14.2s')).toBeInTheDocument();
  });

  it('says 1 attempt in the singular', () => {
    renderNode({
      kind: pollingKind,
      status: 'success',
      statusCode: 200,
      durationMs: 300,
      attempts: 1,
    });
    expect(screen.getByText('✓ 200 · 1 attempt · 0.3s')).toBeInTheDocument();
  });
});
