import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, toRfEdges } from '../FlowCanvas';

// The real CodeMirror editor needs react-query and Tauri mocks.
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

const trio: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
  { id: 'c', kind: { kind: 'Output', label: 'Gamma' }, position: { x: 600, y: 0 } },
];

const nodeEl = (id: string) => document.querySelector<HTMLElement>(`.react-flow__node[data-id="${id}"]`);

function renderCanvas(props: Partial<React.ComponentProps<typeof FlowCanvas>> = {}) {
  return render(
    <FlowCanvas
      nodes={trio}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      {...props}
    />,
  );
}

describe('FlowCanvas accessible names', () => {
  it('gives each node a name with its kind and status', () => {
    renderCanvas({ nodeStatus: { b: 'success' } });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, not run');
    expect(nodeEl('b')).toHaveAttribute('aria-label', 'Beta, output node, succeeded');
  });

  it('adds the short error of a failed node', () => {
    renderCanvas({ nodeStatus: { a: 'failed' }, nodeDetail: { a: { error: 'boom\nmore' } } });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, failed: boom');
  });

  it('does not put progress text in the name', () => {
    renderCanvas({
      nodeStatus: { a: 'running' },
      nodeDetail: { a: { progress: 'attempt 3/30' } },
    });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, running');
  });

  it('names the canvas and describes the keyboard use', () => {
    renderCanvas();
    const wrapper = screen.getByTestId('rf__wrapper');
    expect(wrapper).toHaveAttribute('aria-label', 'Flow canvas');
    const describedBy = wrapper.getAttribute('aria-describedby');
    expect(describedBy).toBeTruthy();
    const description = document.getElementById(describedBy ?? '');
    expect(description).toHaveTextContent('Ctrl+A');
    expect(description).toHaveTextContent('Delete');
  });

  it('keeps the outer wrapper focusable by script only and without a role', () => {
    renderCanvas();
    const outer = screen.getByTestId('flow-canvas');
    expect(outer).toHaveAttribute('tabindex', '-1');
    expect(outer).not.toHaveAttribute('role');
  });

  it('describes wires in the keyboard help text with the flow wording', () => {
    renderCanvas();
    expect(document.body.textContent).toContain('Press Enter or Space to select this step');
    expect(document.body.textContent).toContain('Press Enter or Space to select this wire');
  });
});

describe('toRfEdges accessible names', () => {
  const edges: FlowEdge[] = [
    {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'value',
      expression: 'response.body',
    },
  ];

  it('sets an aria label that names both ends', () => {
    const rf = toRfEdges(edges, trio, {}, new Set());
    expect(rf[0].ariaLabel).toBe('Wire from Alpha to Beta, value');
  });
});

describe('FlowCanvas keyboard behaviour with names in place', () => {
  function SelectHarness({ onSelect }: { onSelect: (ids: ReadonlySet<string>) => void }) {
    const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
    return (
      <FlowCanvas
        nodes={trio}
        edges={[]}
        nodeStatus={{ a: 'success' }}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
        selectedNodeIds={selected}
        onSelectedNodeIdsChange={(ids) => {
          onSelect(ids);
          setSelected(ids);
        }}
      />
    );
  }

  it('still selects every node on Ctrl+A pressed on a focused node', async () => {
    const onSelect = vi.fn();
    render(<SelectHarness onSelect={onSelect} />);
    const node = nodeEl('a');
    expect(node).not.toBeNull();
    fireEvent.keyDown(node as HTMLElement, { key: 'a', ctrlKey: true });
    await waitFor(() => expect(onSelect).toHaveBeenLastCalledWith(new Set(['a', 'b', 'c'])));
  });
});
