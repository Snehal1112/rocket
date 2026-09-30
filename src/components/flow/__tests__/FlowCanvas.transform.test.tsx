import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, toRfEdges } from '../FlowCanvas';

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it measures a node.
class FakeMatrix {
  m22 = 1;
}
vi.stubGlobal('DOMMatrixReadOnly', FakeMatrix);

const request: FlowNode = {
  id: 'req',
  position: { x: 0, y: 0 },
  kind: { kind: 'Request', label: 'Login', source: { type: 'Saved', requestPath: 'a.yml' } },
};
const transform: FlowNode = {
  id: 'tf',
  position: { x: 300, y: 0 },
  kind: { kind: 'Transform', label: 'Pick token', script: 'return response.body.token;' },
};
const output: FlowNode = {
  id: 'out',
  position: { x: 600, y: 0 },
  kind: { kind: 'Output', label: 'Out' },
};
const edges: FlowEdge[] = [
  { id: 'e1', sourceNodeId: 'req', targetNodeId: 'tf', targetField: 'input', expression: '' },
  {
    id: 'e2',
    sourceNodeId: 'tf',
    targetNodeId: 'out',
    targetField: 'value',
    expression: 'response.body',
  },
];

describe('FlowCanvas with a Transform node', () => {
  it('renders the Transform node through nodeTypes', () => {
    render(
      <FlowCanvas
        nodes={[request, transform, output]}
        edges={edges}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    expect(screen.getByTestId('transform-node-card')).toBeInTheDocument();
    expect(screen.getByText('Pick token')).toBeInTheDocument();
  });

  it('maps the wires onto the input and result handles', () => {
    const rf = toRfEdges(edges, [request, transform, output], {}, new Set());
    const into = rf.find((e) => e.id === 'e1');
    const outOf = rf.find((e) => e.id === 'e2');
    expect(into).toMatchObject({ source: 'req', target: 'tf', targetHandle: 'input' });
    expect(outOf).toMatchObject({
      source: 'tf',
      sourceHandle: 'result',
      target: 'out',
      targetHandle: 'value',
    });
    expect(outOf?.label).toBeUndefined();
  });

  it('a Transform never gets taken or not-taken styling', () => {
    const rf = toRfEdges(edges, [request, transform, output], { tf: 'success' }, new Set(), {
      tf: { branch: 'true' },
    });
    const outOf = rf.find((e) => e.id === 'e2');
    expect(outOf?.className).not.toMatch(/flow-edge-(taken|not-taken)/);
  });
});
