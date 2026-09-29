import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { headerNameFromTarget, WireScriptDialog } from '../WireScriptDialog';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <div className='monaco-editor'>
      <textarea
        aria-label='Wire script'
        value={value}
        onChange={(e) => onChange?.(e.target.value)}
      />
    </div>
  ),
}));

const edge: FlowEdge = {
  id: 'edge-1',
  sourceNodeId: 'node-a',
  targetNodeId: 'node-c',
  targetField: 'url',
  expression: 'response.body',
};

const targetNode: FlowNode = {
  id: 'node-c',
  kind: {
    kind: 'Request',
    label: 'Target',
    source: { type: 'Inline', request: { method: 'GET', url: '', headers: [], body: undefined } },
  },
  position: { x: 0, y: 0 },
};

function setup(e: FlowEdge = edge) {
  const onCommit = vi.fn();
  const onOpenChange = vi.fn();
  render(
    <WireScriptDialog
      edge={e}
      targetNode={targetNode}
      open
      onOpenChange={onOpenChange}
      onCommit={onCommit}
    />,
  );
  return { onCommit, onOpenChange };
}

describe('WireScriptDialog', () => {
  it('shows the expression and commits an edited one on Save', async () => {
    const user = userEvent.setup();
    const { onCommit, onOpenChange } = setup();
    expect(screen.getByText('Value from source')).toBeTruthy();
    const editor = await screen.findByLabelText('Wire script');
    expect((editor as HTMLTextAreaElement).value).toBe('response.body');
    await user.clear(editor);
    await user.type(editor, 'response.status');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).toHaveBeenCalledWith({ ...edge, expression: 'response.status' });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('keeps the dialog open when Escape is pressed inside the editor', async () => {
    const { onOpenChange } = setup();
    const editor = await screen.findByLabelText('Wire script');
    fireEvent.keyDown(editor, { key: 'Escape' });
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it('closes the dialog when Escape is pressed outside the editor', async () => {
    const { onOpenChange } = setup();
    await screen.findByLabelText('Wire script');
    fireEvent.keyDown(screen.getByRole('button', { name: 'Cancel' }), { key: 'Escape' });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('closes without committing on Cancel', async () => {
    const user = userEvent.setup();
    const { onCommit, onOpenChange } = setup();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('requires a header name for a new headers edge', async () => {
    const user = userEvent.setup();
    const { onCommit } = setup({ ...edge, targetField: 'headers' });
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).not.toHaveBeenCalled();
    await user.type(screen.getByLabelText('Header name'), 'Authorization');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).toHaveBeenCalledWith({
      ...edge,
      targetField: 'headers[Authorization].value',
    });
  });

  it('starts an existing header edge with its name and keeps the target', async () => {
    const user = userEvent.setup();
    const existing = { ...edge, targetField: 'headers[X-Token].value' };
    const { onCommit } = setup(existing);
    expect((screen.getByLabelText('Header name') as HTMLInputElement).value).toBe('X-Token');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).toHaveBeenCalledWith(existing);
  });

  it('marks the dialog with nokey', () => {
    setup();
    expect(screen.getByRole('dialog').classList.contains('nokey')).toBe(true);
  });

  it('extracts the header name from a target field', () => {
    expect(headerNameFromTarget('headers[A].value')).toBe('A');
    expect(headerNameFromTarget('url')).toBe('');
  });
});
