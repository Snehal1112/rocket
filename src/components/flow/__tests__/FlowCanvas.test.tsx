import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, toRfEdges } from '../FlowCanvas';

// Holds nodes/edges in state, like FlowPane does through the pane store.
function Harness({
  initialNodes,
  initialEdges,
  onEdges,
}: {
  initialNodes: FlowNode[];
  initialEdges: FlowEdge[];
  onEdges?: (edges: FlowEdge[]) => void;
}) {
  const [nodes, setNodes] = useState(initialNodes);
  const [edges, setEdges] = useState(initialEdges);
  return (
    <FlowCanvas
      nodes={nodes}
      edges={edges}
      nodeStatus={{}}
      onNodesChange={setNodes}
      onEdgesChange={(next) => {
        onEdges?.(next);
        setEdges(next);
      }}
      onConnect={vi.fn()}
    />
  );
}

const trio: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
  { id: 'c', kind: { kind: 'Output', label: 'Gamma' }, position: { x: 600, y: 0 } },
];

describe('FlowCanvas', () => {
  const nodes: FlowNode[] = [
    { id: 'n1', kind: { kind: 'Output', label: 'Result' }, position: { x: 0, y: 0 } },
  ];
  const edges: FlowEdge[] = [];

  it('renders the dotted background and the given nodes', () => {
    render(
      <FlowCanvas
        nodes={nodes}
        edges={edges}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    // React Flow renders its dotted background inside this container.
    expect(document.querySelector('.react-flow__background')).toBeInTheDocument();
    expect(screen.getByText('Result')).toBeInTheDocument();
  });

  it('deletes a selected node and drops the edges that touched it', async () => {
    const graph: FlowNode[] = [
      { id: 'in', kind: { kind: 'Input', label: 'Key', value: 'k' }, position: { x: 0, y: 0 } },
      { id: 'out', kind: { kind: 'Output', label: 'Shown' }, position: { x: 300, y: 0 } },
      { id: 'out2', kind: { kind: 'Output', label: 'Other' }, position: { x: 300, y: 200 } },
    ];
    const wires: FlowEdge[] = [
      {
        id: 'e1',
        sourceNodeId: 'in',
        targetNodeId: 'out',
        targetField: 'value',
        expression: 'response.body',
      },
      {
        id: 'e2',
        sourceNodeId: 'in',
        targetNodeId: 'out2',
        targetField: 'value',
        expression: 'response.body',
      },
    ];
    const onEdges = vi.fn();
    render(<Harness initialNodes={graph} initialEdges={wires} onEdges={onEdges} />);

    // A click must select the node. Selection is canvas-local state; without
    // it the delete key has nothing to delete.
    fireEvent.click(screen.getByText('Shown'));
    await waitFor(() =>
      expect(document.querySelector('.react-flow__node[data-id="out"]')).toHaveClass('selected'),
    );

    // Separate acts, so React Flow sees the key as pressed before release.
    await act(async () => {
      fireEvent.keyDown(document.body, { key: 'Backspace' });
    });
    await act(async () => {
      fireEvent.keyUp(document.body, { key: 'Backspace' });
    });

    await waitFor(() => expect(screen.queryByText('Shown')).not.toBeInTheDocument());
    expect(screen.getByText('Other')).toBeInTheDocument();
    expect(onEdges).toHaveBeenLastCalledWith([wires[1]]);
  });

  it('deletes a clicked node even when an unrelated input held focus beforehand', async () => {
    // React Flow's delete-key handler is gated on document.activeElement:
    // it no-ops if focus is on an input/textarea/contenteditable. Chromium
    // auto-focuses (and blurs the prior element for) any clicked tabIndex
    // div, so this never surfaces there — but WebKitGTK (the engine Tauri
    // actually runs on Linux) does not, so a node click that never moves
    // focus off a still-focused text field silently breaks delete.
    render(
      <>
        <input aria-label='distraction' />
        <Harness initialNodes={nodes} initialEdges={edges} />
      </>,
    );
    act(() => screen.getByLabelText('distraction').focus());
    expect(document.activeElement).toBe(screen.getByLabelText('distraction'));

    fireEvent.click(screen.getByText('Result'));
    await waitFor(() =>
      expect(document.querySelector('.react-flow__node[data-id="n1"]')).toHaveClass('selected'),
    );

    // A real keydown always targets whatever currently has focus, then
    // bubbles to `document` where React Flow's delete handler listens. If
    // the click above never moved focus off the distraction input, this is
    // still where the event originates — exactly like a real browser.
    await act(async () => {
      fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Backspace' });
    });
    await act(async () => {
      fireEvent.keyUp(document.activeElement ?? document.body, { key: 'Backspace' });
    });

    await waitFor(() => expect(screen.queryByText('Result')).not.toBeInTheDocument());
  });

  describe('with measurable nodes', () => {
    const observed: Element[] = [];

    // jsdom has no layout, so give every element a size and make the
    // ResizeObserver report each observed node at once.
    function stubLayout() {
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
            // Only node elements matter here; other observers are left idle.
            if (!target.classList.contains('react-flow__node')) return;
            observed.push(target);
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
      observed.length = 0;
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
    });

    it('keeps measured nodes visible when their status changes', async () => {
      stubLayout();
      const props = {
        nodes,
        edges,
        onNodesChange: vi.fn(),
        onEdgesChange: vi.fn(),
        onConnect: vi.fn(),
      };
      const { rerender } = render(<FlowCanvas {...props} nodeStatus={{}} />);
      const node = () => document.querySelector<HTMLElement>('.react-flow__node[data-id="n1"]');
      await waitFor(() => expect(node()?.style.visibility).toBe('visible'));
      const observeCalls = observed.length;

      // A run patches status many times. Each patch builds new React Flow
      // node objects; they must keep their size instead of being hidden and
      // re-measured.
      rerender(<FlowCanvas {...props} nodeStatus={{ n1: 'running' }} />);
      expect(node()?.style.visibility).toBe('visible');
      rerender(<FlowCanvas {...props} nodeStatus={{ n1: 'success' }} />);
      expect(node()?.style.visibility).toBe('visible');
      expect(observed.length).toBe(observeCalls);
    });

    it('gives a cycle edge a distinct stroke style', () => {
      // Edge paths only render once React Flow has measured both endpoint
      // handles, hence the layout stub (same requirement as the node
      // measurement test above). An Output node has no source ("result")
      // handle, so the source side must be a node type that has one.
      stubLayout();
      const graph: FlowNode[] = [
        { id: 'a', kind: { kind: 'Input', label: 'A', value: 'x' }, position: { x: 0, y: 0 } },
        { id: 'b', kind: { kind: 'Output', label: 'B' }, position: { x: 200, y: 0 } },
      ];
      const graphEdges: FlowEdge[] = [
        {
          id: 'e1',
          sourceNodeId: 'a',
          targetNodeId: 'b',
          targetField: 'value',
          expression: 'response.body',
        },
      ];
      render(
        <FlowCanvas
          nodes={graph}
          edges={graphEdges}
          nodeStatus={{}}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
          cycleEdgeIds={['e1']}
        />,
      );
      const path = screen.getByTestId('rf__edge-e1').querySelector('path');
      expect(path).toHaveStyle({ stroke: '#ef4444' });
    });

    it('reports a double-clicked wire through onEdgeEdit', () => {
      stubLayout();
      const graph: FlowNode[] = [
        { id: 'a', kind: { kind: 'Input', label: 'A', value: 'x' }, position: { x: 0, y: 0 } },
        {
          id: 'b',
          kind: {
            kind: 'Request',
            label: 'B',
            source: {
              type: 'Inline',
              request: { method: 'GET', url: '', headers: [], body: undefined },
            },
          },
          position: { x: 200, y: 0 },
        },
      ];
      const graphEdges: FlowEdge[] = [
        {
          id: 'e1',
          sourceNodeId: 'a',
          targetNodeId: 'b',
          targetField: 'url',
          expression: 'response.body',
        },
      ];
      const onEdgeEdit = vi.fn();
      const { container } = render(
        <FlowCanvas
          nodes={graph}
          edges={graphEdges}
          nodeStatus={{}}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
          onEdgeEdit={onEdgeEdit}
        />,
      );
      const edge = container.querySelector('.react-flow__edge');
      expect(edge).not.toBeNull();
      fireEvent.doubleClick(edge as Element);
      expect(onEdgeEdit).toHaveBeenCalledWith('e1');
    });

    it('marks wires nopan so a double-click does not zoom the canvas', () => {
      const rf = toRfEdges(
        [
          {
            id: 'e1',
            sourceNodeId: 'a',
            targetNodeId: 'b',
            targetField: 'url',
            expression: 'response.body',
          },
        ],
        [],
        {},
        new Set(),
      );
      expect(rf[0].className).toContain('nopan');
    });
  });

  describe('multi-select', () => {
    // Keeps the selection in state, like FlowPane does, and reports each change.
    function SelectHarness({ onSelect }: { onSelect: (ids: ReadonlySet<string>) => void }) {
      const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
      return (
        <>
          <input aria-label='outside-field' />
          <FlowCanvas
            nodes={trio}
            edges={[]}
            nodeStatus={{}}
            onNodesChange={vi.fn()}
            onEdgesChange={vi.fn()}
            onConnect={vi.fn()}
            selectedNodeIds={selected}
            onSelectedNodeIdsChange={(ids) => {
              onSelect(ids);
              setSelected(ids);
            }}
          />
        </>
      );
    }

    const selectedOf = (id: string) =>
      document.querySelector(`.react-flow__node[data-id="${id}"]`)?.classList.contains('selected');

    it('selects every node on Ctrl+A', async () => {
      const onSelect = vi.fn();
      render(<SelectHarness onSelect={onSelect} />);
      fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'a', ctrlKey: true });
      await waitFor(() => expect(onSelect).toHaveBeenLastCalledWith(new Set(['a', 'b', 'c'])));
    });

    it('selects every node on Cmd+A', async () => {
      const onSelect = vi.fn();
      render(<SelectHarness onSelect={onSelect} />);
      fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'a', metaKey: true });
      await waitFor(() => expect(onSelect).toHaveBeenLastCalledWith(new Set(['a', 'b', 'c'])));
    });

    it('does not select all when Ctrl+A comes from an input', () => {
      const onSelect = vi.fn();
      render(<SelectHarness onSelect={onSelect} />);
      const input = document.createElement('input');
      screen.getByTestId('flow-canvas').appendChild(input);
      fireEvent.keyDown(input, { key: 'a', ctrlKey: true });
      expect(onSelect).not.toHaveBeenCalled();
    });

    it('does not select all when Ctrl+A comes from inside a .nokey element', () => {
      const onSelect = vi.fn();
      render(<SelectHarness onSelect={onSelect} />);
      const wrapper = document.createElement('div');
      wrapper.className = 'nokey';
      const inner = document.createElement('span');
      wrapper.appendChild(inner);
      screen.getByTestId('flow-canvas').appendChild(wrapper);
      fireEvent.keyDown(inner, { key: 'a', ctrlKey: true });
      expect(onSelect).not.toHaveBeenCalled();
    });

    it('adds a second node to the selection on Ctrl+click', async () => {
      const onSelect = vi.fn();
      render(<SelectHarness onSelect={onSelect} />);
      fireEvent.click(screen.getByText('Alpha'));
      await waitFor(() => expect(selectedOf('a')).toBe(true));
      await act(async () => {
        fireEvent.keyDown(document.body, { key: 'Control', ctrlKey: true });
      });
      fireEvent.click(screen.getByText('Beta'), { ctrlKey: true });
      await waitFor(() => expect(selectedOf('b')).toBe(true));
      expect(selectedOf('a')).toBe(true);
    });

    it('shows the selection hint', () => {
      render(<SelectHarness onSelect={vi.fn()} />);
      expect(screen.getByText(/Drag to select/)).toBeInTheDocument();
    });
  });

  describe('Output value wiring', () => {
    it('tells an Output with only a Run when wire that no value is wired', () => {
      render(
        <FlowCanvas
          nodes={[
            { id: 'a', kind: { kind: 'Input', label: 'In', value: 'x' }, position: { x: 0, y: 0 } },
            { id: 'o', kind: { kind: 'Output', label: 'Out' }, position: { x: 300, y: 0 } },
          ]}
          edges={[
            {
              id: 'e1',
              sourceNodeId: 'a',
              targetNodeId: 'o',
              targetField: 'trigger',
              expression: '',
            },
          ]}
          nodeStatus={{ a: 'success', o: 'success' }}
          nodeDetail={{ o: { value: '' } }}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByText('(no value wired)')).toBeInTheDocument();
    });

    it('keeps (empty) for an Output whose value wire returned nothing', () => {
      render(
        <FlowCanvas
          nodes={[
            { id: 'a', kind: { kind: 'Input', label: 'In', value: '' }, position: { x: 0, y: 0 } },
            { id: 'o', kind: { kind: 'Output', label: 'Out' }, position: { x: 300, y: 0 } },
          ]}
          edges={[
            {
              id: 'e1',
              sourceNodeId: 'a',
              targetNodeId: 'o',
              targetField: 'value',
              expression: 'response.body',
            },
          ]}
          nodeStatus={{ a: 'success', o: 'success' }}
          nodeDetail={{ o: { value: '' } }}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByText('(empty)')).toBeInTheDocument();
    });
  });

  describe('toRfEdges', () => {
    it('maps a missing sourceHandle to result and keeps a routing exit', () => {
      const rf = toRfEdges(
        [
          {
            id: 'e1',
            sourceNodeId: 'a',
            targetNodeId: 'b',
            targetField: 'headers[Authorization].value',
            expression: 'response.body',
          },
          {
            id: 'e2',
            sourceNodeId: 'if1',
            targetNodeId: 'b',
            targetField: 'trigger',
            expression: '',
            sourceHandle: 'true',
          },
        ],
        [],
        {},
        new Set(),
      );
      expect(rf.map((e) => [e.id, e.sourceHandle, e.targetHandle])).toEqual([
        ['e1', 'result', 'headers'],
        ['e2', 'true', 'trigger'],
      ]);
    });

    it('draws Run when wires dotted and leaves data wires plain', () => {
      const rf = toRfEdges(
        [
          {
            id: 'data',
            sourceNodeId: 'a',
            targetNodeId: 'b',
            targetField: 'value',
            expression: 'response.body',
          },
          {
            id: 'gate',
            sourceNodeId: 'a',
            targetNodeId: 'b',
            targetField: 'trigger',
            expression: '',
          },
        ],
        [],
        {},
        new Set(),
      );
      const [data, gate] = rf;
      expect(data.style?.strokeDasharray).toBeUndefined();
      expect(gate.style?.strokeDasharray).toBeDefined();
      expect(gate.className).toContain('flow-edge-trigger');
    });

    it('keeps a taken Run when wire dotted', () => {
      const rf = toRfEdges(
        [
          {
            id: 'gate',
            sourceNodeId: 'if1',
            targetNodeId: 'b',
            targetField: 'trigger',
            expression: '',
            sourceHandle: 'true',
          },
        ],
        [{ id: 'if1', kind: { kind: 'If', label: 'If', condition: '' }, position: { x: 0, y: 0 } }],
        { if1: 'success' },
        new Set(),
        { if1: { branch: 'true' } },
      );
      expect(rf[0].style?.stroke).toBe('#22c55e');
      expect(rf[0].style?.strokeDasharray).toBeDefined();
    });
  });
});
