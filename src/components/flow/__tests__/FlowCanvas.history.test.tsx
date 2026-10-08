import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, nodeWriteOptions } from '../FlowCanvas';

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
];

function renderCanvas(handlers: { onUndo?: () => void; onRedo?: () => void }) {
  return render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      {...handlers}
    />,
  );
}

describe('FlowCanvas undo keys', () => {
  it('undoes on Ctrl+Z and Cmd+Z', () => {
    const onUndo = vi.fn();
    renderCanvas({ onUndo, onRedo: vi.fn() });
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'z', ctrlKey: true });
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'z', metaKey: true });
    expect(onUndo).toHaveBeenCalledTimes(2);
  });

  it('redoes on Ctrl+Shift+Z, Cmd+Shift+Z and Ctrl+Y', () => {
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    renderCanvas({ onUndo, onRedo });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'Z', ctrlKey: true, shiftKey: true });
    fireEvent.keyDown(canvas, { key: 'z', metaKey: true, shiftKey: true });
    fireEvent.keyDown(canvas, { key: 'y', ctrlKey: true });
    expect(onRedo).toHaveBeenCalledTimes(3);
    expect(onUndo).not.toHaveBeenCalled();
  });

  it('prevents the browser default when it handles the key', () => {
    renderCanvas({ onUndo: vi.fn(), onRedo: vi.fn() });
    const notPrevented = fireEvent.keyDown(screen.getByTestId('flow-canvas'), {
      key: 'z',
      ctrlKey: true,
    });
    expect(notPrevented).toBe(false);
  });

  it('leaves Ctrl+Z to an input or a .nokey field', () => {
    const onUndo = vi.fn();
    renderCanvas({ onUndo, onRedo: vi.fn() });
    const canvas = screen.getByTestId('flow-canvas');
    const input = document.createElement('input');
    canvas.appendChild(input);
    fireEvent.keyDown(input, { key: 'z', ctrlKey: true });
    const wrapper = document.createElement('div');
    wrapper.className = 'nokey';
    const inner = document.createElement('span');
    wrapper.appendChild(inner);
    canvas.appendChild(wrapper);
    fireEvent.keyDown(inner, { key: 'z', ctrlKey: true });
    expect(onUndo).not.toHaveBeenCalled();
  });

  it('ignores a plain z and does nothing without handlers', () => {
    const onUndo = vi.fn();
    renderCanvas({ onUndo });
    fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'z' });
    expect(onUndo).not.toHaveBeenCalled();
    const { container } = renderCanvas({});
    expect(() =>
      fireEvent.keyDown(container.querySelector('[data-testid="flow-canvas"]') as Element, {
        key: 'z',
        ctrlKey: true,
      }),
    ).not.toThrow();
  });
});

describe('nodeWriteOptions', () => {
  it('marks removals so the node and edge writes of one Delete share a step', () => {
    expect(nodeWriteOptions([{ type: 'remove', id: 'a' }], false)).toEqual({
      coalesceKey: 'canvas-remove',
      coalesceMs: 50,
    });
  });

  it('marks position changes during a drag as a gesture', () => {
    expect(
      nodeWriteOptions(
        [{ type: 'position', id: 'a', position: { x: 1, y: 1 }, dragging: true }],
        true,
      ),
    ).toEqual({ gesture: true });
  });

  it('gives a position change outside a drag its own step', () => {
    expect(
      nodeWriteOptions([{ type: 'position', id: 'a', position: { x: 1, y: 1 } }], false),
    ).toBeUndefined();
  });

  it('ignores selection and size changes', () => {
    expect(
      nodeWriteOptions(
        [
          { type: 'select', id: 'a', selected: true },
          { type: 'dimensions', id: 'a', dimensions: { width: 1, height: 1 } },
        ],
        true,
      ),
    ).toBeUndefined();
  });
});
