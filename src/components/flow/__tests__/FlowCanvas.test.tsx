import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

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
    // React Flow renders its background as an SVG pattern container with
    // this test id in @xyflow/react — confirmed via its own testing docs.
    expect(document.querySelector('.react-flow__background')).toBeInTheDocument();
    expect(screen.getByText('Result')).toBeInTheDocument();
  });
});
