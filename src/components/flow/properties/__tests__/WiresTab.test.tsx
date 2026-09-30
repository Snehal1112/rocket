// src/components/flow/properties/__tests__/WiresTab.test.tsx
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { WiresTab } from '../WiresTab';

const n = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });
const login = n('login', {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'a.yml' },
});
const check = n('check', { kind: 'If', label: 'Ok?', condition: 'true' });
const users = n('users', {
  kind: 'Request',
  label: 'List Users',
  source: { type: 'Saved', requestPath: 'b.yml' },
});
const out = n('out', { kind: 'Output', label: 'Result' });
const nodes = [login, check, users, out];
const edges: FlowEdge[] = [
  { id: 'e1', sourceNodeId: 'login', targetNodeId: 'check', targetField: 'input', expression: '' },
  {
    id: 'e2',
    sourceNodeId: 'check',
    targetNodeId: 'users',
    targetField: 'trigger',
    expression: '',
    sourceHandle: 'true',
  },
  {
    id: 'e3',
    sourceNodeId: 'check',
    targetNodeId: 'out',
    targetField: 'value',
    expression: 'response.body',
    sourceHandle: 'false',
  },
];

function renderTab(node: FlowNode, extra: Partial<Parameters<typeof WiresTab>[0]> = {}) {
  const onEditWire = vi.fn();
  const onSelectNode = vi.fn();
  render(
    <WiresTab
      node={node}
      nodes={nodes}
      edges={edges}
      onEditWire={onEditWire}
      onSelectNode={onSelectNode}
      {...extra}
    />,
  );
  return { onEditWire, onSelectNode };
}

describe('WiresTab', () => {
  it('shows the empty state for a node without wires', () => {
    const lonely = n('x', { kind: 'Output', label: 'X' });
    render(
      <WiresTab
        node={lonely}
        nodes={[lonely]}
        edges={[]}
        onEditWire={vi.fn()}
        onSelectNode={vi.fn()}
      />,
    );
    expect(
      screen.getByText('No wires. Drag from a dot on the canvas to connect nodes.'),
    ).toBeInTheDocument();
  });

  it('lists incoming and outgoing wires', () => {
    renderTab(check);
    expect(within(screen.getByTestId('wires-incoming')).getByText('Login')).toBeInTheDocument();
    const outgoing = screen.getByTestId('wires-outgoing');
    expect(within(outgoing).getByText('true')).toBeInTheDocument();
    expect(within(outgoing).getByText('false')).toBeInTheDocument();
  });

  it('groups outgoing wires by exit', () => {
    renderTab(check);
    const groups = screen.getAllByTestId('wires-exit-group');
    expect(groups.map((g) => g.getAttribute('data-exit'))).toEqual(['true', 'false']);
    expect(within(groups[0]).getByText('List Users')).toBeInTheDocument();
    expect(within(groups[1]).getByText('Result')).toBeInTheDocument();
  });

  it('opens the wire dialog from a row with a script', async () => {
    const { onEditWire } = renderTab(out);
    await userEvent.click(screen.getByRole('button', { name: /Edit wire into Value/ }));
    expect(onEditWire).toHaveBeenCalledWith('e3');
  });

  it('a Run when row is not clickable', () => {
    renderTab(users);
    expect(
      screen.queryByRole('button', { name: /Edit wire into Run when/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByText('(no script)')).toBeInTheDocument();
  });

  it('a node link selects the node without opening the wire', async () => {
    const { onEditWire, onSelectNode } = renderTab(out);
    await userEvent.click(screen.getByRole('button', { name: 'Select node Ok?' }));
    expect(onSelectNode).toHaveBeenCalledWith('check');
    expect(onEditWire).not.toHaveBeenCalled();
  });

  it('fades wires the last run did not take', () => {
    renderTab(check, {
      nodeStatus: { check: 'success' },
      nodeDetail: { check: { branch: 'true' } },
    });
    const groups = screen.getAllByTestId('wires-exit-group');
    const falseRow = within(groups[1]).getByTestId('wire-row');
    expect(falseRow.className).toContain('opacity-50');
    expect(falseRow).toHaveTextContent('not taken');
    expect(within(groups[0]).getByTestId('wire-row').className).not.toContain('opacity-50');
  });

  it('shows a missing source node', () => {
    const orphan: FlowEdge[] = [
      {
        id: 'e9',
        sourceNodeId: 'gone',
        targetNodeId: 'out',
        targetField: 'value',
        expression: 'response.body',
      },
    ];
    render(
      <WiresTab
        node={out}
        nodes={[out]}
        edges={orphan}
        onEditWire={vi.fn()}
        onSelectNode={vi.fn()}
      />,
    );
    expect(screen.getByText('(missing node)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Edit wire/ })).not.toBeInTheDocument();
  });

  it('truncates a long node name and keeps the full label in a title', () => {
    const long = n('long', { kind: 'Input', label: 'A very long node label', value: '' });
    const target = n('t', { kind: 'Output', label: 'T' });
    render(
      <WiresTab
        node={target}
        nodes={[long, target]}
        edges={[
          {
            id: 'e',
            sourceNodeId: 'long',
            targetNodeId: 't',
            targetField: 'value',
            expression: '',
          },
        ]}
        onEditWire={vi.fn()}
        onSelectNode={vi.fn()}
      />,
    );
    const link = screen.getByRole('button', { name: 'Select node A very long node label' });
    expect(link).toHaveAttribute('title', 'A very long node label');
    expect(link).toHaveClass('truncate', 'min-w-0', 'max-w-full');
  });
});
