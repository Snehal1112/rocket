import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
];

describe('FlowCanvas minimap', () => {
  it('renders a labelled minimap beside the controls', () => {
    render(
      <FlowCanvas
        nodes={nodes}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    const minimap = document.querySelector('.react-flow__minimap');
    expect(minimap).toBeInTheDocument();
    expect(screen.getByLabelText('Flow minimap')).toBeInTheDocument();
    expect(document.querySelector('.react-flow__controls')).toBeInTheDocument();
    const style = minimap?.getAttribute('style') ?? '';
    expect(style).not.toMatch(/backdrop|color-mix/);
  });
});
