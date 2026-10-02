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
    expect(node.kind.cases[0]).toMatchObject({ label: 'Case 1', matches: '' });
  });

  it('adds a Poll request node with repeat-until turned on', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Poll request' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^request-/);
    expect(node.kind).toEqual({
      kind: 'Request',
      label: 'New Poll',
      source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      repeatUntil: {
        condition: 'response.status === 200',
        intervalMs: 2000,
        maxAttempts: 30,
        timeoutMs: 60000,
      },
    });
  });
});

describe('NodePalette Wait for callback entry', () => {
  it('adds a Wait for callback node with the first free name', async () => {
    const onAddNode = vi.fn();
    render(
      <NodePalette
        onAddNode={onAddNode}
        nodes={[
          {
            id: 'w1',
            kind: { kind: 'WaitForCallback', label: 'Hook', name: 'callback', timeoutMs: 60000 },
            position: { x: 0, y: 0 },
          },
        ]}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Wait for callback' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^wait-/),
        position: { x: 100, y: 100 },
        kind: {
          kind: 'WaitForCallback',
          label: 'Wait for callback',
          name: 'callback_2',
          timeoutMs: 60000,
        },
      }),
    );
  });
});

describe('NodePalette Transform entry', () => {
  it('adds a Transform node with the default script', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Transform' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^transform-/),
        position: { x: 100, y: 100 },
        kind: { kind: 'Transform', label: 'New Transform', script: 'return response.body;' },
      }),
    );
  });

  it('adds an Auth node that applies to inherited auth by default', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Auth' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^auth-/);
    expect(node.position).toEqual({ x: 100, y: 100 });
    expect(node.kind).toEqual({
      kind: 'Auth',
      label: 'New Auth',
      auth: { authType: 'bearer', token: '' },
      applyToInherit: true,
    });
  });
});
