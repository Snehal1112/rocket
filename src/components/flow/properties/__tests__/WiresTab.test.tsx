// src/components/flow/properties/__tests__/WiresTab.test.tsx
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { PanelFocusProvider } from '../panelFocus';
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

  it('a node link selects the node, then refocuses the panel', async () => {
    const order: string[] = [];
    const onSelectNode = vi.fn(() => order.push('select'));
    const refocus = vi.fn(() => order.push('refocus'));
    render(
      <PanelFocusProvider value={refocus}>
        <WiresTab
          node={out}
          nodes={nodes}
          edges={edges}
          onEditWire={vi.fn()}
          onSelectNode={onSelectNode}
        />
      </PanelFocusProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Select node Ok?' }));
    expect(onSelectNode).toHaveBeenCalledWith('check');
    expect(order).toEqual(['select', 'refocus']);
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

  it('keeps the last value collapsed until asked', async () => {
    renderTab(out, {
      nodeStatus: { out: 'success' },
      nodeDetail: {
        out: {
          trace: {
            wires: [
              { edgeId: 'e3', sourceNodeId: 'check', targetField: 'value', value: 'big value', truncated: true },
            ],
          },
        },
      },
    });
    expect(screen.queryByTestId('wire-value')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Last value/ }));
    expect(screen.getByTestId('wire-value')).toHaveTextContent('big value');
    expect(screen.getByText('Cut at 16 KB.')).toBeInTheDocument();
  });

  it('shows a credential wire as hidden', () => {
    const auth = n('signin', {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: false,
    });
    render(
      <WiresTab
        node={users}
        nodes={[auth, users]}
        edges={[
          { id: 'ea', sourceNodeId: 'signin', targetNodeId: 'users', targetField: 'auth', expression: '' },
        ]}
        nodeDetail={{
          users: {
            trace: {
              wires: [
                {
                  edgeId: 'ea',
                  sourceNodeId: 'signin',
                  targetField: 'auth',
                  credential: true,
                  value: 'leaked-token-123456',
                },
              ],
            },
          },
        }}
        onEditWire={vi.fn()}
        onSelectNode={vi.fn()}
      />,
    );
    expect(screen.getByTestId('wire-credential')).toHaveTextContent('Credential (hidden)');
    expect(screen.queryByRole('button', { name: /Last value/ })).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent('leaked-token-123456');
  });

  it('highlights only the wire that failed', () => {
    renderTab(check, {
      nodeStatus: { check: 'failed' },
      nodeDetail: {
        check: {
          trace: {
            wires: [{ edgeId: 'e1', sourceNodeId: 'login', targetField: 'input', error: 'boom' }],
            failedEdgeId: 'e1',
          },
        },
      },
    });
    const incoming = within(screen.getByTestId('wires-incoming')).getByTestId('wire-row');
    expect(incoming).toHaveAttribute('data-failed', 'true');
    expect(within(incoming).getByTestId('wire-error')).toHaveTextContent('boom');
    for (const row of within(screen.getByTestId('wires-outgoing')).getAllByTestId('wire-row')) {
      expect(row).not.toHaveAttribute('data-failed');
    }
  });
});
