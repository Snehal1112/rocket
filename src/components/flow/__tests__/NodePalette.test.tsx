import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { NodePalette } from '../NodePalette';

describe('NodePalette routing entries', () => {
  it('adds an If node with the default condition', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'If' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^if-/),
        position: { x: 100, y: 100 },
        kind: { kind: 'If', label: 'New If', condition: 'response.status === 200' },
      }),
    );
  });

  it('adds a Switch node with a default value and one empty case', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Switch' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^switch-/);
    expect(node.kind).toMatchObject({
      kind: 'Switch',
      label: 'New Switch',
      value: 'response.body.type',
    });
    expect(node.kind.cases).toHaveLength(1);
    expect(node.kind.cases[0]).toMatchObject({ label: 'Case 1', matches: 'case-1' });
  });
});
