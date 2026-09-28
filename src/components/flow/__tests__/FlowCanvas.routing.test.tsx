import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
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

  describe('Backspace inside an inline field', () => {
    const ifNode: FlowNode = {
      id: 'if1',
      position: { x: 400, y: 0 },
      kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
    };

    function EditHarness() {
      const [nodes, setNodes] = useState<FlowNode[]>([switchNode, ifNode]);
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
          onRemoveSwitchCase={vi.fn()}
        />
      );
    }

    // React Flow hides an unmeasured node in jsdom, so the pointer check is off.
    const setup = () => userEvent.setup({ pointerEventsCheck: 0 });

    it.each([
      ['a Switch case input', 'Case 1 label', 'switch-node-card'],
      ['the If condition editor', 'Condition', 'if-node-card'],
    ])('keeps focus in %s and keeps the node', async (_name, fieldLabel, cardId) => {
      const user = setup();
      render(<EditHarness />);
      const field = screen.getByLabelText(fieldLabel);
      await user.click(field);
      expect(field.contains(document.activeElement)).toBe(true);
      await user.keyboard('{Backspace}');
      expect(screen.getByTestId(cardId)).toBeInTheDocument();
      expect(field.contains(document.activeElement)).toBe(true);
    });

    it('keeps the node when Backspace is pressed on a focused Remove button', async () => {
      const user = setup();
      render(<EditHarness />);
      // Selecting the node first makes it a delete target. A pointer click on
      // the header would start a d3 drag, which jsdom cannot run.
      fireEvent.click(screen.getByText('Plan router'));
      await waitFor(() =>
        expect(document.querySelector('.react-flow__node[data-id="sw1"]')).toHaveClass('selected'),
      );
      const button = screen.getByLabelText('Remove case Free');
      act(() => button.focus());
      await user.keyboard('{Backspace}');
      expect(screen.getByTestId('switch-node-card')).toBeInTheDocument();
    });
  });

  describe('edges after a run', () => {
    function stubLayout() {
      // jsdom has no SVG text measuring, which edge labels need.
      Object.defineProperty(SVGElement.prototype, 'getBBox', {
        configurable: true,
        value: () => ({ x: 0, y: 0, width: 20, height: 10 }),
      });
      vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(200);
      vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(80);
      vi.stubGlobal(
        'DOMMatrixReadOnly',
        class {
          m22 = 1;
        },
      );
      vi.stubGlobal(
        'ResizeObserver',
        class {
          constructor(private cb: ResizeObserverCallback) {}
          observe(target: Element) {
            if (!target.classList.contains('react-flow__node')) return;
            this.cb([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
          }
          unobserve() {
            // Not needed by these tests.
          }
          disconnect() {
            // Not needed by these tests.
          }
        },
      );
    }

    afterEach(() => {
      Reflect.deleteProperty(SVGElement.prototype, 'getBBox');
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
    });

    const graph: FlowNode[] = [
      {
        id: 'if1',
        position: { x: 0, y: 0 },
        kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
      },
      { id: 'yes', position: { x: 300, y: -100 }, kind: { kind: 'Output', label: 'Yes' } },
      { id: 'no', position: { x: 300, y: 100 }, kind: { kind: 'Output', label: 'No' } },
    ];
    const wires: FlowEdge[] = [
      {
        id: 'eT',
        sourceNodeId: 'if1',
        sourceHandle: 'true',
        targetNodeId: 'yes',
        targetField: 'trigger',
        expression: '',
      },
      {
        id: 'eF',
        sourceNodeId: 'if1',
        sourceHandle: 'false',
        targetNodeId: 'no',
        targetField: 'trigger',
        expression: '',
      },
    ];

    it('labels routing exits and styles taken vs not-taken edges', () => {
      stubLayout();
      render(
        <FlowCanvas
          nodes={graph}
          edges={wires}
          nodeStatus={{ if1: 'success', yes: 'success', no: 'skipped' }}
          nodeDetail={{ if1: { branch: 'true' }, no: { skipReason: 'branch_not_taken' } }}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByTestId('rf__edge-eT')).toHaveClass('flow-edge-taken');
      expect(screen.getByTestId('rf__edge-eF')).toHaveClass('flow-edge-not-taken');
      expect(screen.getByTestId('rf__edge-eT')).toHaveTextContent('true');
      expect(screen.getByTestId('rf__edge-eF')).toHaveTextContent('false');
    });

    it('renders both exits neutral before any run', () => {
      stubLayout();
      render(
        <FlowCanvas
          nodes={graph}
          edges={wires}
          nodeStatus={{}}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByTestId('rf__edge-eT')).not.toHaveClass('flow-edge-taken');
      expect(screen.getByTestId('rf__edge-eF')).not.toHaveClass('flow-edge-not-taken');
    });
  });
});
