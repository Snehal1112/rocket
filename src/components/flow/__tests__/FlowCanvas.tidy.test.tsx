import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const { fitView } = vi.hoisted(() => ({ fitView: vi.fn() }));

// Keep the real React Flow, but record the calls to fitView.
vi.mock('@xyflow/react', async () => {
  const actual = await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react');
  return { ...actual, useReactFlow: () => ({ ...actual.useReactFlow(), fitView }) };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const out = (id: string, x = 0, y = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y },
});

const edge = (id: string, from: string, to: string): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'trigger',
  expression: 'response.body',
});

function renderCanvas(
  nodes: FlowNode[],
  edges: FlowEdge[],
  selected: string[] = [],
  onNodesChange = vi.fn(),
  onUndo?: () => void,
) {
  render(
    <FlowCanvas
      nodes={nodes}
      edges={edges}
      nodeStatus={{}}
      onNodesChange={onNodesChange}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      selectedNodeIds={new Set(selected)}
      onSelectedNodeIdsChange={vi.fn()}
      onUndo={onUndo}
      onRedo={onUndo ? vi.fn() : undefined}
    />,
  );
  return onNodesChange;
}

const tidyButton = () => screen.getByRole('button', { name: 'Tidy layout' });

describe('FlowCanvas tidy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('writes the new positions with exactly one nodes call and no options', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    expect(onNodes).toHaveBeenCalledTimes(1);
    const [next, options] = onNodes.mock.calls[0];
    expect(options).toBeUndefined();
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('then fits the view', async () => {
    const user = userEvent.setup();
    renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    await vi.waitFor(() => expect(fitView).toHaveBeenCalled());
    expect(fitView.mock.calls[0][0]).toMatchObject({ duration: 300 });
  });

  it('writes nothing and says so when the graph is already tidy', async () => {
    const user = userEvent.setup();
    const tidy = [out('a', 0, 0), out('b', 340, 0)];
    const onNodes = renderCanvas(tidy, [edge('e1', 'a', 'b')]);
    await user.click(tidyButton());
    expect(onNodes).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith('The layout is already tidy.');
  });

  it('lays out only a selection of two or more nodes', async () => {
    const user = userEvent.setup();
    const far = out('far', 5000, 5000);
    const onNodes = renderCanvas(
      [out('a', 100, 100), out('b', 100, 100), far],
      [edge('e1', 'a', 'b')],
      ['a', 'b'],
    );
    await user.click(tidyButton());
    const [next] = onNodes.mock.calls[0];
    expect(next[2]).toBe(far);
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('lays out the whole graph when only one node is selected', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')], ['a']);
    await user.click(tidyButton());
    const [next] = onNodes.mock.calls[0];
    expect(next[1].position.x).toBeGreaterThan(next[0].position.x);
  });

  it('hands the focus back so Ctrl+Z works right after Tidy', async () => {
    const user = userEvent.setup();
    const onUndo = vi.fn();
    renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b')], [], vi.fn(), onUndo);
    await user.click(tidyButton());
    fireEvent.keyDown(document.activeElement as Element, { key: 'z', ctrlKey: true });
    expect(onUndo).toHaveBeenCalledTimes(1);
  });

  it('hands the focus back when the graph is already tidy', async () => {
    const user = userEvent.setup();
    const onUndo = vi.fn();
    renderCanvas([out('a', 0, 0), out('b', 340, 0)], [edge('e1', 'a', 'b')], [], vi.fn(), onUndo);
    await user.click(tidyButton());
    fireEvent.keyDown(document.activeElement as Element, { key: 'z', ctrlKey: true });
    expect(onUndo).toHaveBeenCalledTimes(1);
  });

  it('is disabled for an empty flow', () => {
    renderCanvas([], []);
    expect(tidyButton()).toBeDisabled();
  });

  it('does not throw on a cycle', async () => {
    const user = userEvent.setup();
    const onNodes = renderCanvas([out('a'), out('b')], [edge('e1', 'a', 'b'), edge('e2', 'b', 'a')]);
    await user.click(tidyButton());
    expect(toast.error).not.toHaveBeenCalled();
    expect(onNodes).toHaveBeenCalledTimes(1);
  });
});
