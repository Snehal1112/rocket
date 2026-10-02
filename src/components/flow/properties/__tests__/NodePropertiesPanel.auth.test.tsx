import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { NodePropertiesPanel } from '../NodePropertiesPanel';

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

// The editor's own behaviour is tested elsewhere; here only the prop the panel
// computes for it matters.
vi.mock('../AuthNodeEditor', () => ({
  AuthNodeEditor: (props: { nodeId: string; otherNodeApplies?: boolean }) => (
    <div data-testid={`other-applies-${props.nodeId}`}>{String(props.otherNodeApplies)}</div>
  ),
}));

const authNode = (id: string, applyToInherit: boolean): FlowNode => ({
  id,
  position: { x: 0, y: 0 },
  kind: {
    kind: 'Auth',
    label: id,
    auth: { authType: 'bearer', token: '' },
    applyToInherit,
  },
});

function renderFor(selected: FlowNode, nodes: FlowNode[]) {
  render(
    <NodePropertiesPanel
      node={selected}
      edges={[]}
      nodes={nodes}
      collection='demo'
      flowName='f'
      status='idle'
      activeTab='settings'
      onTabChange={vi.fn()}
      onEditWire={vi.fn()}
      onSelectNode={vi.fn()}
      onChange={vi.fn()}
      onClose={vi.fn()}
      onDelete={vi.fn()}
    />,
  );
}

describe('NodePropertiesPanel Auth otherNodeApplies', () => {
  const applying = authNode('a', true);
  const idle = authNode('b', false);

  it('is true for the node that does not apply when another one does', () => {
    renderFor(idle, [applying, idle]);
    expect(screen.getByTestId('other-applies-b')).toHaveTextContent('true');
  });

  it('is false for the applying node itself', () => {
    renderFor(applying, [applying, idle]);
    expect(screen.getByTestId('other-applies-a')).toHaveTextContent('false');
  });

  it('is false for a single node', () => {
    renderFor(idle, [idle]);
    expect(screen.getByTestId('other-applies-b')).toHaveTextContent('false');
  });
});
