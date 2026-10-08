import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { NodePropertiesPanel } from '../NodePropertiesPanel';

const scope = vi.hoisted(() => ({ variableContext: new Map<string, unknown>() }));
const editorProps = vi.hoisted(() => ({ last: null as null | Record<string, unknown> }));

vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => scope,
}));

// The real CodeMirror editor needs Tauri and react-query. A plain input with
// the same value/onChange contract is enough here.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => {
    editorProps.last = props as unknown as Record<string, unknown>;
    return (
      <input
        aria-label={props['aria-label']}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    );
  },
}));

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

const node = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });

function renderPanel(n: FlowNode, extra: Partial<Parameters<typeof NodePropertiesPanel>[0]> = {}) {
  const onChange = vi.fn();
  const onClose = vi.fn();
  const onDelete = vi.fn();
  const onTabChange = vi.fn();
  render(
    <NodePropertiesPanel
      node={n}
      edges={[]}
      nodes={[n]}
      collection='demo'
      status='idle'
      activeTab='settings'
      onTabChange={onTabChange}
      onEditWire={vi.fn()}
      onSelectNode={vi.fn()}
      onChange={onChange}
      onClose={onClose}
      onDelete={onDelete}
      {...extra}
    />,
  );
  return { onChange, onClose, onDelete, onTabChange };
}

describe('NodePropertiesPanel', () => {
  it('asks for the Last run tab when it is clicked', async () => {
    const { onTabChange } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.click(screen.getByRole('tab', { name: 'Last run' }));
    expect(onTabChange).toHaveBeenCalledWith('last-run');
  });

  it('renders the Last run tab when it is active', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }), { activeTab: 'last-run' });
    expect(screen.getByRole('tab', { name: 'Last run' })).toHaveAttribute('data-state', 'active');
    expect(screen.getByText('Not run yet. Run the flow to see results here.')).toBeInTheDocument();
  });

  it('shows a Settings tab holding the editors', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.getByRole('tab', { name: 'Settings' })).toHaveAttribute('data-state', 'active');
    expect(screen.getByRole('tabpanel')).toContainElement(screen.getByLabelText('Label'));
  });

  it('keeps the tab bar inside the nokey panel', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.getByRole('tab', { name: 'Settings' }).closest('.nokey')).not.toBeNull();
  });

  it('shows the save error only for a flagged node', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }), {
      saveError: 'flow contains a cycle through node(s): o1',
    });
    expect(screen.getByTestId('node-save-error')).toHaveTextContent(
      'flow contains a cycle through node(s): o1',
    );
  });

  it('shows no save error box without an error', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    expect(screen.queryByTestId('node-save-error')).not.toBeInTheDocument();
  });

  it('calls onDelete from the Delete node button', async () => {
    const { onDelete } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete node' }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it('edits the label of any node kind with one change per keystroke', async () => {
    const { onChange } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Output', label: 'Outx' });
  });

  it('edits an Input value through the variable-aware editor', async () => {
    const { onChange } = renderPanel(node('i1', { kind: 'Input', label: 'User', value: 'al' }));
    await userEvent.type(screen.getByLabelText('Input value'), 'i');
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Input', label: 'User', value: 'ali' });
  });

  it('shows a structured value read-only', () => {
    renderPanel(node('i1', { kind: 'Input', label: 'User', value: { secret: true } }));
    expect(screen.queryByLabelText('Input value')).not.toBeInTheDocument();
    expect(screen.getByTestId('input-value-readonly')).toHaveTextContent('{"secret":true}');
  });

  it('shows an If condition read-only with a pointer to the node', () => {
    renderPanel(node('if1', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' }));
    expect(screen.getByTestId('if-details-condition')).toHaveTextContent('response.status === 200');
    expect(screen.getByText('Edit on the node.')).toBeInTheDocument();
  });

  it('shows a Switch value and each case', () => {
    renderPanel(
      node('s1', {
        kind: 'Switch',
        label: 'Route',
        value: 'response.body.type',
        cases: [
          { id: 'c1', label: 'Card', matches: 'card' },
          { id: 'c2', label: 'Cash', matches: 'cash' },
        ],
      }),
    );
    expect(screen.getByTestId('switch-details-value')).toHaveTextContent('response.body.type');
    const cases = screen.getAllByTestId('switch-details-case').map((c) => c.textContent);
    expect(cases).toEqual(['Card = card', 'Cash = cash']);
  });

  it('shows which wire feeds an Output', () => {
    const out = node('o1', { kind: 'Output', label: 'Result' });
    const login = node('r1', {
      kind: 'Request',
      label: 'Login',
      source: { type: 'Saved', requestPath: 'auth/login.yml' },
    });
    renderPanel(out, {
      nodes: [out, login],
      edges: [
        {
          id: 'e1',
          sourceNodeId: 'r1',
          targetNodeId: 'o1',
          targetField: 'value',
          expression: 'response.body.token',
        },
      ],
    });
    expect(screen.getByTestId('output-details')).toHaveTextContent('response.body.token');
    expect(screen.getByTestId('output-details')).toHaveTextContent('Login');
  });

  it('says when an Output has no value wire', () => {
    renderPanel(node('o1', { kind: 'Output', label: 'Result' }));
    expect(screen.getByTestId('output-details')).toHaveTextContent('No value wire.');
  });

  it('shows "(missing node)" when Output wire source is not in nodes', () => {
    const out = node('o1', { kind: 'Output', label: 'Result' });
    renderPanel(out, {
      nodes: [out],
      edges: [
        {
          id: 'e1',
          sourceNodeId: 'r1',
          targetNodeId: 'o1',
          targetField: 'value',
          expression: 'response.body.token',
        },
      ],
    });
    expect(screen.getByTestId('output-details')).toHaveTextContent('(missing node)');
  });

  it('shows "—" when If condition is empty', () => {
    renderPanel(node('if1', { kind: 'If', label: 'Check', condition: '' }));
    expect(screen.getByTestId('if-details-condition')).toHaveTextContent('—');
  });

  it('shows "No cases." when Switch has zero cases', () => {
    renderPanel(node('s1', { kind: 'Switch', label: 'Route', value: 'x', cases: [] }));
    expect(screen.getByText('No cases.')).toBeInTheDocument();
  });

  it('does not truncate long Switch case lines', () => {
    renderPanel(
      node('s1', {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [
          {
            id: 'c1',
            label: 'Very Long Case Label',
            matches: 'a_very_long_matching_value_that_should_wrap',
          },
        ],
      }),
    );
    const caseElement = screen.getByTestId('switch-details-case');
    expect(caseElement).toHaveClass('break-words');
    expect(caseElement).not.toHaveClass('truncate');
  });

  it('shows the kind and label in its header and closes on ✕', async () => {
    const { onClose } = renderPanel(node('o1', { kind: 'Output', label: 'Result' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · Result');
    await userEvent.click(screen.getByRole('button', { name: 'Close properties' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
describe('Wait for callback editor', () => {
  const waitKind = {
    kind: 'WaitForCallback' as const,
    label: 'Hook',
    name: 'payment',
    timeoutMs: 60000,
    acceptWhen: "request.body.event === 'done'",
  };

  it('edits the name', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    await userEvent.type(screen.getByLabelText('Name'), '_2');
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, name: 'payment2' });
  });

  it('shows a hint for an invalid name', () => {
    renderPanel(node('w', { ...waitKind, name: 'pay ment' }));
    expect(screen.getByTestId('callback-name-hint')).toHaveTextContent(
      'Use letters, digits and _ only.',
    );
  });

  it('edits the timeout in seconds', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    // The field shows 60; typing 0 makes it 600 seconds.
    await userEvent.type(screen.getByLabelText('Timeout (seconds)'), '0');
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, timeoutMs: 600000 });
  });

  it('clearing accept_when stores null', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    await userEvent.clear(screen.getByLabelText('Accept when'));
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, acceptWhen: null });
  });

  it('shows the variable and the local network note', () => {
    renderPanel(node('w', waitKind));
    expect(screen.getByText('{{callback.payment}}')).toBeInTheDocument();
    expect(screen.getByText(/reachable from your local network/)).toBeInTheDocument();
  });

  it('shows the live callback URL in Settings', () => {
    const url = 'http://10.0.0.5:4000/cb/tok';
    renderPanel(node('w', waitKind), { callbackUrl: url });
    expect(screen.getByTestId('callback-url')).toHaveTextContent(url);
  });

  it('shows the live callback URL on the Last run tab of a running wait', () => {
    const url = 'http://10.0.0.5:4000/cb/tok';
    renderPanel(node('w', waitKind), {
      callbackUrl: url,
      activeTab: 'last-run',
      status: 'running',
    });
    expect(screen.getByTestId('callback-url')).toHaveTextContent(url);
  });

  it('shows no callback URL when none is passed', () => {
    renderPanel(node('w', waitKind));
    expect(screen.queryByTestId('callback-url')).not.toBeInTheDocument();
  });
});

describe('Wires tab in the panel', () => {
  it('lists the wires of the selected node and opens one', async () => {
    const outNode = node('out', { kind: 'Output', label: 'Result' });
    const src = node('src', { kind: 'Input', label: 'Token', value: 'x' });
    const onEditWire = vi.fn();
    renderPanel(outNode, {
      nodes: [src, outNode],
      edges: [
        {
          id: 'w1',
          sourceNodeId: 'src',
          targetNodeId: 'out',
          targetField: 'value',
          expression: 'response.body',
        },
      ],
      activeTab: 'wires',
      onEditWire,
    });
    expect(screen.getByText('Token')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Edit wire into value/i }));
    expect(onEditWire).toHaveBeenCalledWith('w1');
  });

  it('shows the label and script sections for a Transform node', () => {
    renderPanel(node('t1', { kind: 'Transform', label: 'Pick', script: 'return 1;' }));
    expect(screen.getByLabelText('Label')).toHaveValue('Pick');
    expect(screen.getByText('Script')).toBeInTheDocument();
  });

  it('edits the label of a Transform node', async () => {
    const { onChange } = renderPanel(
      node('t1', { kind: 'Transform', label: 'Pick', script: 'return 1;' }),
    );
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({
      kind: 'Transform',
      label: 'Pickx',
      script: 'return 1;',
    });
  });

  it('gives an Input node value editor the collection variables, read-only for saving', () => {
    scope.variableContext.set('user', {
      value: 'alice',
      source: 'environment',
      label: 'dev',
      secret: false,
    });
    renderPanel(node('in1', { kind: 'Input', label: 'User', value: '{{user}}' }));
    expect(editorProps.last?.variableContext).toBe(scope.variableContext);
    expect(editorProps.last?.readOnlyVariables).toBe(true);
  });
});
