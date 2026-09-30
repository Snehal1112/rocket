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

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    onSubmit,
    variableContext,
    placeholder,
    'aria-label': ariaLabel,
  }: {
    value: string;
    onChange: (v: string) => void;
    onSubmit?: () => void;
    variableContext?: unknown;
    placeholder?: string;
    'aria-label'?: string;
  }) => (
    <input
      data-testid='single-line'
      data-has-context={variableContext ? 'yes' : 'no'}
      aria-label={ariaLabel}
      placeholder={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') onSubmit?.();
      }}
    />
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

  describe('header targets', () => {
    const headerEdge = { ...edge, targetField: 'headers[X-Token].value', expression: 'a' };

    it('opens compact with the single-line editor and no Monaco', () => {
      setup(headerEdge);
      const field = screen.getByLabelText('Wire script') as HTMLInputElement;
      expect(field.dataset.testid).toBe('single-line');
      expect(field.dataset.hasContext).toBe('no');
      expect(field.placeholder).toBe('response.body.token');
      expect(document.querySelector('.monaco-editor')).toBeNull();
    });

    it('saves on Enter in the compact field', async () => {
      const user = userEvent.setup();
      const { onCommit, onOpenChange } = setup(headerEdge);
      const field = screen.getByLabelText('Wire script');
      await user.clear(field);
      await user.type(field, 'response.body.token{Enter}');
      expect(onCommit).toHaveBeenCalledWith({
        ...headerEdge,
        expression: 'response.body.token',
      });
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });

    it('Expand keeps the text and shows Monaco', async () => {
      const user = userEvent.setup();
      setup(headerEdge);
      await user.click(screen.getByRole('button', { name: /expand/i }));
      const editor = (await screen.findByLabelText('Wire script')) as HTMLTextAreaElement;
      expect(editor.value).toBe('a');
      expect(document.querySelector('.monaco-editor')).not.toBeNull();
      expect(screen.queryByTestId('single-line')).toBeNull();
    });

    it('Collapse returns to the compact field', async () => {
      const user = userEvent.setup();
      setup(headerEdge);
      await user.click(screen.getByRole('button', { name: /expand/i }));
      await screen.findByLabelText('Wire script');
      await user.click(screen.getByRole('button', { name: /collapse/i }));
      expect((screen.getByLabelText('Wire script') as HTMLInputElement).value).toBe('a');
      expect(screen.getByTestId('single-line')).toBeTruthy();
    });

    it('opens a multi-line saved expression in Monaco', async () => {
      setup({ ...headerEdge, expression: 'const t = 1;\nreturn t;' });
      await screen.findByLabelText('Wire script');
      expect(document.querySelector('.monaco-editor')).not.toBeNull();
      expect(screen.queryByTestId('single-line')).toBeNull();
    });

    it('disables Collapse while the text is multi-line', async () => {
      setup({ ...headerEdge, expression: 'const t = 1;\nreturn t;' });
      await screen.findByLabelText('Wire script');
      const collapse = screen.getByRole('button', { name: /collapse/i }) as HTMLButtonElement;
      expect(collapse.disabled).toBe(true);
    });
  });

  it('opens a url wire in Monaco without an Expand button', async () => {
    setup();
    await screen.findByLabelText('Wire script');
    expect(document.querySelector('.monaco-editor')).not.toBeNull();
    expect(screen.queryByRole('button', { name: /expand/i })).toBeNull();
  });

  it('opens a body wire in Monaco', async () => {
    setup({ ...edge, targetField: 'body' });
    await screen.findByLabelText('Wire script');
    expect(document.querySelector('.monaco-editor')).not.toBeNull();
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
