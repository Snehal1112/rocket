import { fireEvent, render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { IfNode, type IfNodeData } from '../IfNode';

// The real CodeMirror editor needs react-query and Tauri mocks. A plain input
// with the same value/onChange contract is enough to test the node.
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

const kind = { kind: 'If' as const, label: 'Logged in?', condition: 'response.status === 200' };

function renderIf(data: IfNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn(), openProperties: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <IfNode
          id='if1'
          type='If'
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
  return actions;
}

describe('IfNode', () => {
  it('renders one input handle and true/false exits', () => {
    renderIf({ kind, status: 'idle' });
    const card = screen.getByTestId('if-node-card');
    expect(screen.getByText('Logged in?')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(1);
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="true"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="false"]')).toBeInTheDocument();
  });

  it('edits the condition inline through the node actions', () => {
    const actions = renderIf({ kind, status: 'idle' });
    fireEvent.change(screen.getByLabelText('Condition'), {
      target: { value: 'response.status === 201' },
    });
    expect(actions.updateNodeKind).toHaveBeenCalledWith('if1', {
      ...kind,
      condition: 'response.status === 201',
    });
  });

  it('keeps the condition editor out of canvas drag, wheel and key handling', () => {
    renderIf({ kind, status: 'idle' });
    const wrapper = screen.getByLabelText('Condition').closest('.nodrag');
    expect(wrapper).toHaveClass('nowheel');
    expect(wrapper).toHaveClass('nokey');
  });

  it('shows the chosen exit after a successful run', () => {
    renderIf({ kind, status: 'success', branch: 'false' });
    expect(screen.getByTestId('branch-badge')).toHaveTextContent('→ false');
  });

  it('shows the evaluation error when the condition throws', () => {
    renderIf({ kind, status: 'failed', error: 'ReferenceError: x is not defined' });
    expect(screen.getByText(/ReferenceError/)).toBeInTheDocument();
  });

  it('captions a not-taken skip', () => {
    renderIf({ kind, status: 'skipped', skipReason: 'branch_not_taken' });
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
  });

  it('shows how long the last run took', () => {
    renderIf({ kind, status: 'success', branch: 'true', durationMs: 12 });
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('12ms');
  });

  it('shows no duration before a run', () => {
    renderIf({ kind, status: 'idle' });
    expect(screen.queryByTestId('duration-chip')).not.toBeInTheDocument();
  });
});
