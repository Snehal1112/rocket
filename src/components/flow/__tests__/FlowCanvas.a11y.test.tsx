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

describe('FlowCanvas status icons', () => {
  const at = { x: 0, y: 0 };
  const everyKind: FlowNode[] = [
    {
      id: 'auth',
      position: at,
      kind: {
        kind: 'Auth',
        label: 'Sign in',
        auth: { authType: 'bearer', token: 't' },
        applyToInherit: false,
      },
    },
    {
      id: 'req',
      position: at,
      kind: {
        kind: 'Request',
        label: 'Fetch',
        source: { type: 'Inline', request: { method: 'GET', url: 'https://x.test', headers: [] } },
      },
    },
    { id: 'in', position: at, kind: { kind: 'Input', label: 'Key', value: 'k' } },
    { id: 'out', position: at, kind: { kind: 'Output', label: 'Shown' } },
    { id: 'if', position: at, kind: { kind: 'If', label: 'Check', condition: 'true' } },
    {
      id: 'sw',
      position: at,
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [{ id: 'c1', label: 'One', matches: '1' }],
      },
    },
    { id: 'tf', position: at, kind: { kind: 'Transform', label: 'Pick', script: 'return 1;' } },
    {
      id: 'wait',
      position: at,
      kind: { kind: 'WaitForCallback', label: 'Hook', name: 'cb', timeoutMs: 60000 },
    },
  ];
  const cardIds = [
    'auth-node-card',
    'request-node-card',
    'input-node-card',
    'output-node-card',
    'if-node-card',
    'switch-node-card',
    'transform-node-card',
    'wait-node-card',
  ];
  const card = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`);

  function renderKinds(status: 'idle' | 'success' | 'failed' | 'skipped' | 'running') {
    return render(
      <FlowCanvas
        nodes={everyKind}
        edges={[]}
        nodeStatus={Object.fromEntries(everyKind.map((n) => [n.id, status]))}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
  }

  it('shows no icon on idle nodes', () => {
    renderKinds('idle');
    for (const id of cardIds) {
      expect(card(id)?.querySelector('[data-testid="node-status-icon"]'), id).toBeNull();
    }
  });

  it.each(['running', 'success', 'failed', 'skipped'] as const)(
    'shows a %s icon in the header of all eight node kinds',
    (status) => {
      renderKinds(status);
      for (const id of cardIds) {
        const icon = card(id)?.querySelector('[data-testid="node-status-icon"]');
        expect(icon, id).toHaveAttribute('data-status', status);
        expect(icon, id).toHaveAttribute('aria-hidden', 'true');
      }
    },
  );

  it('keeps the status in each node name, so the icon is not the only carrier', () => {
    renderKinds('failed');
    expect(nodeEl('req')).toHaveAttribute('aria-label', 'Fetch, request node, failed');
    expect(nodeEl('wait')).toHaveAttribute('aria-label', 'Hook, wait for callback node, failed');
  });
});
