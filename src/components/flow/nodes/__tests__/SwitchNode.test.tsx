import { fireEvent, render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { SwitchNode, type SwitchNodeData } from '../SwitchNode';

const { updateNodeInternals } = vi.hoisted(() => ({ updateNodeInternals: vi.fn() }));

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return { ...actual, useUpdateNodeInternals: () => updateNodeInternals };
});

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const kind = {
  kind: 'Switch' as const,
  label: 'Plan router',
  value: 'response.body.plan',
  cases: [
    { id: 'c1', label: 'Free', matches: 'free' },
    { id: 'c2', label: 'Pro plan', matches: 'pro' },
  ],
};

function element(data: SwitchNodeData, actions: ReturnType<typeof makeActions>) {
  return (
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <SwitchNode
          id='sw1'
          type='Switch'
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
    </ReactFlowProvider>
  );
}

function makeActions() {
  return { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn() };
}

describe('SwitchNode', () => {
  it('renders an input handle, one exit per case and a default exit', () => {
    render(element({ kind, status: 'idle' }, makeActions()));
    const card = screen.getByTestId('switch-node-card');
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="case:c1"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="case:c2"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="default"]')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(3);
  });

  it('edits the value and a case inline', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.change(screen.getByLabelText('Switch value'), {
      target: { value: 'response.body.tier' },
    });
    expect(actions.updateNodeKind).toHaveBeenLastCalledWith('sw1', {
      ...kind,
      value: 'response.body.tier',
    });
    fireEvent.change(screen.getByLabelText('Case 2 matches'), { target: { value: 'premium' } });
    expect(actions.updateNodeKind).toHaveBeenLastCalledWith('sw1', {
      ...kind,
      cases: [kind.cases[0], { ...kind.cases[1], matches: 'premium' }],
    });
  });

  it('adds a case with the next number and a unique placeholder match', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Add case' }));
    const next = actions.updateNodeKind.mock.calls[0][1];
    expect(next.cases).toHaveLength(3);
    expect(next.cases[2]).toMatchObject({ label: 'Case 3', matches: 'case-3' });
    expect(next.cases[2].id).toEqual(expect.any(String));
    expect(new Set(next.cases.map((c: { id: string }) => c.id)).size).toBe(3);
  });

  it('skips a placeholder match that is already taken', () => {
    const actions = makeActions();
    const taken = { ...kind, cases: [kind.cases[0], { ...kind.cases[1], matches: 'case-3' }] };
    render(element({ kind: taken, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Add case' }));
    const next = actions.updateNodeKind.mock.calls[0][1];
    expect(next.cases[2]).toMatchObject({ label: 'Case 4', matches: 'case-4' });
  });

  it('removes a case through removeSwitchCase, not a plain kind update', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Remove case Free' }));
    expect(actions.removeSwitchCase).toHaveBeenCalledWith('sw1', 'c1');
    expect(actions.updateNodeKind).not.toHaveBeenCalled();
  });

  it('flags duplicate match values before save', () => {
    const dup = {
      ...kind,
      cases: [
        { id: 'c1', label: 'Case 1', matches: '' },
        { id: 'c2', label: 'Case 2', matches: '' },
      ],
    };
    render(element({ kind: dup, status: 'idle' }, makeActions()));
    expect(screen.getByRole('alert')).toHaveTextContent('Two cases match the same value.');
    expect(screen.getByLabelText('Case 1 matches')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Case 2 matches')).toHaveAttribute('aria-invalid', 'true');
  });

  it('refreshes node internals when cases change', () => {
    updateNodeInternals.mockClear();
    const actions = makeActions();
    const { rerender } = render(element({ kind, status: 'idle' }, actions));
    const afterMount = updateNodeInternals.mock.calls.length;
    rerender(
      element(
        {
          kind: { ...kind, cases: [...kind.cases, { id: 'c3', label: 'Case 3', matches: '' }] },
          status: 'idle',
        },
        actions,
      ),
    );
    expect(updateNodeInternals.mock.calls.length).toBeGreaterThan(afterMount);
    expect(updateNodeInternals).toHaveBeenLastCalledWith('sw1');
  });

  it('badges the chosen case by its label', () => {
    render(element({ kind, status: 'success', branch: 'case:c2' }, makeActions()));
    expect(screen.getByTestId('branch-badge')).toHaveTextContent('→ Pro plan');
  });
});
