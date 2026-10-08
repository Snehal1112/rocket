import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
];

function renderCanvas(selected: string[], handlers: Record<string, () => void>) {
  return render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      selectedNodeIds={new Set(selected)}
      onSelectedNodeIdsChange={vi.fn()}
      {...handlers}
    />,
  );
}

describe('FlowCanvas copy, paste and duplicate keys', () => {
  it('copies and duplicates only with a selection', () => {
    const onCopy = vi.fn();
    const onDuplicate = vi.fn();
    renderCanvas([], { onCopy, onDuplicate });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onDuplicate).not.toHaveBeenCalled();
  });

  it('calls the handlers on Ctrl and Cmd', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    const onDuplicate = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste, onDuplicate });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas, { key: 'v', metaKey: true });
    fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true });
    expect(onCopy).toHaveBeenCalledTimes(1);
    expect(onPaste).toHaveBeenCalledTimes(1);
    expect(onDuplicate).toHaveBeenCalledTimes(1);
  });

  it('prevents the browser default for paste and duplicate', () => {
    renderCanvas(['a'], { onPaste: vi.fn(), onDuplicate: vi.fn() });
    const canvas = screen.getByTestId('flow-canvas');
    expect(fireEvent.keyDown(canvas, { key: 'v', ctrlKey: true })).toBe(false);
    expect(fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true })).toBe(false);
  });

  it('leaves the keys to a field inside the canvas', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste });
    const input = document.createElement('input');
    screen.getByTestId('flow-canvas').appendChild(input);
    fireEvent.keyDown(input, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(input, { key: 'v', ctrlKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onPaste).not.toHaveBeenCalled();
  });

  it('ignores Ctrl+Shift+C and Ctrl+Shift+V', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'C', ctrlKey: true, shiftKey: true });
    fireEvent.keyDown(canvas, { key: 'V', ctrlKey: true, shiftKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onPaste).not.toHaveBeenCalled();
  });

  it('does nothing without handlers', () => {
    renderCanvas(['a'], {});
    expect(() =>
      fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'v', ctrlKey: true }),
    ).not.toThrow();
  });
});
