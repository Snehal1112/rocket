import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

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

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures a
// node after SwitchNode calls updateNodeInternals.
class FakeMatrix {
  m22 = 1;
}
vi.stubGlobal('DOMMatrixReadOnly', FakeMatrix);

const switchNode: FlowNode = {
  id: 'sw1',
  position: { x: 0, y: 0 },
  kind: {
    kind: 'Switch',
    label: 'Plan router',
    value: 'response.body.plan',
    cases: [{ id: 'c1', label: 'Free', matches: 'free' }],
  },
};

function Harness({ onRemoveSwitchCase }: { onRemoveSwitchCase: (n: string, c: string) => void }) {
  const [nodes, setNodes] = useState<FlowNode[]>([switchNode]);
  const [edges, setEdges] = useState<FlowEdge[]>([]);
  return (
    <FlowCanvas
      nodes={nodes}
      edges={edges}
      nodeStatus={{}}
      onNodesChange={setNodes}
      onEdgesChange={setEdges}
      onConnect={vi.fn()}
      onNodeKindChange={(id, kind) =>
        setNodes((prev) => prev.map((n) => (n.id === id ? { ...n, kind } : n)))
      }
      onRemoveSwitchCase={onRemoveSwitchCase}
    />
  );
}

describe('FlowCanvas with routing nodes', () => {
  it('renders If and Switch nodes through nodeTypes', () => {
    render(
      <FlowCanvas
        nodes={[
          switchNode,
          {
            id: 'if1',
            position: { x: 300, y: 0 },
            kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
          },
        ]}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    expect(screen.getByTestId('switch-node-card')).toBeInTheDocument();
    expect(screen.getByTestId('if-node-card')).toBeInTheDocument();
  });

  it('wires the remove-case button to onRemoveSwitchCase', () => {
    const onRemove = vi.fn();
    render(<Harness onRemoveSwitchCase={onRemove} />);
    // React Flow hides an unmeasured node in jsdom, so role queries cannot see it.
    fireEvent.click(screen.getByLabelText('Remove case Free'));
    expect(onRemove).toHaveBeenCalledWith('sw1', 'c1');
  });

  it('does not delete a selected Switch node when Backspace is typed in a case field', async () => {
    render(<Harness onRemoveSwitchCase={vi.fn()} />);
    fireEvent.click(screen.getByText('Plan router'));
    await waitFor(() =>
      expect(document.querySelector('.react-flow__node[data-id="sw1"]')).toHaveClass('selected'),
    );
    const field = screen.getByLabelText('Case 1 label');
    act(() => field.focus());
    await act(async () => {
      fireEvent.keyDown(field, { key: 'Backspace' });
    });
    await act(async () => {
      fireEvent.keyUp(field, { key: 'Backspace' });
    });
    expect(screen.getByTestId('switch-node-card')).toBeInTheDocument();
    expect(field.closest('.nokey')).not.toBeNull();
  });
});
