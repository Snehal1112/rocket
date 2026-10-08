import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const { fitView } = vi.hoisted(() => ({ fitView: vi.fn() }));

// Keep the real React Flow, but record the calls to fitView.
vi.mock('@xyflow/react', async () => {
  const actual = await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react');
  return { ...actual, useReactFlow: () => ({ ...actual.useReactFlow(), fitView }) };
});

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
  { id: 'c', kind: { kind: 'Output', label: 'Gamma' }, position: { x: 600, y: 0 } },
];

function Harness({ onSelect }: { onSelect: (ids: ReadonlySet<string>) => void }) {
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
  return (
    <FlowCanvas
      nodes={nodes}
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
  );
}

const searchBox = () => screen.queryByRole('textbox', { name: 'Search nodes' });

describe('FlowCanvas search', () => {
  beforeEach(() => {
    fitView.mockClear();
  });

  it('opens on Ctrl+F with the field focused and the browser default stopped', () => {
    render(<Harness onSelect={vi.fn()} />);
    expect(searchBox()).not.toBeInTheDocument();
    const notPrevented = fireEvent.keyDown(screen.getByTestId('flow-canvas'), {
      key: 'f',
      ctrlKey: true,
    });
    expect(notPrevented).toBe(false);
    expect(searchBox()).toHaveFocus();
  });

  it('opens on Cmd+F and from the Search button', async () => {
    const user = userEvent.setup();
    render(<Harness onSelect={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', metaKey: true });
    expect(searchBox()).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(searchBox()).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Search nodes' }));
    expect(searchBox()).toHaveFocus();
  });

  it('selects the match and zooms to it, then cycles with Enter', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'a');
    expect(onSelect).toHaveBeenLastCalledWith(new Set(['a']));
    expect(fitView).toHaveBeenLastCalledWith({
      nodes: [{ id: 'a' }],
      duration: 300,
      maxZoom: 1.2,
    });
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenLastCalledWith(new Set(['b']));
    expect(fitView).toHaveBeenLastCalledWith({
      nodes: [{ id: 'b' }],
      duration: 300,
      maxZoom: 1.2,
    });
  });

  it('selects nothing and does not zoom when there is no match', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'zzz');
    expect(screen.getByRole('status')).toHaveTextContent('0 of 0');
    expect(onSelect).not.toHaveBeenCalled();
    expect(fitView).not.toHaveBeenCalled();
  });

  it('closes on Escape and returns focus to the canvas', async () => {
    const user = userEvent.setup();
    render(<Harness onSelect={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.keyboard('{Escape}');
    expect(searchBox()).not.toBeInTheDocument();
    expect(screen.getByTestId('flow-canvas')).toHaveFocus();
  });

  it('leaves Ctrl+F to a field inside a node', () => {
    render(<Harness onSelect={vi.fn()} />);
    const input = document.createElement('input');
    screen.getByTestId('flow-canvas').appendChild(input);
    fireEvent.keyDown(input, { key: 'f', ctrlKey: true });
    expect(searchBox()).not.toBeInTheDocument();
  });

  it('does not delete a node when Backspace is pressed in the search field', async () => {
    const user = userEvent.setup();
    const onNodes = vi.fn();
    render(
      <FlowCanvas
        nodes={nodes}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={onNodes}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
        selectedNodeIds={new Set(['a'])}
        onSelectedNodeIdsChange={vi.fn()}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'f', ctrlKey: true });
    await user.type(searchBox() as HTMLElement, 'ab{Backspace}');
    expect(onNodes).not.toHaveBeenCalled();
  });
});
