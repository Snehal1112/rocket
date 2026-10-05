import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { KeyValueEntry } from '@/types/pane-types';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => <input aria-label={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

const open = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (...a: unknown[]) => open(...a) }));

import { FormDataEditor } from '../FormDataEditor';

const textRow: KeyValueEntry = { id: '1', key: 'name', value: 'x', enabled: true };
const fileRow: KeyValueEntry = {
  id: '2',
  key: 'doc',
  value: '/tmp/dir/report.pdf',
  enabled: true,
  entryType: 'file',
};

describe('FormDataEditor', () => {
  beforeEach(() => vi.clearAllMocks());

  it('shows a file row with the file name and a text row with an editor', () => {
    render(<FormDataEditor entries={[textRow, fileRow]} onChange={vi.fn()} />);
    expect(screen.getByDisplayValue('x')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Choose file for row 2' })).toHaveTextContent(
      'report.pdf',
    );
  });

  it('switches a row to a file row and clears its value', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[textRow]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Field type for row 1: Text' }));
    expect(onChange).toHaveBeenLastCalledWith([{ ...textRow, entryType: 'file', value: '' }]);
  });

  it('disables the file picker until the workspace path is known', () => {
    render(<FormDataEditor entries={[{ ...fileRow, value: '' }]} onChange={vi.fn()} />);
    const choose = screen.getByRole('button', { name: 'Choose file for row 1' });
    expect(choose).toBeDisabled();
    fireEvent.click(choose);
    expect(open).not.toHaveBeenCalled();
  });

  it('stores a workspace-relative path for a file inside the workspace', async () => {
    open.mockResolvedValue('/home/me/ws/assets/pic.png');
    const onChange = vi.fn();
    render(
      <FormDataEditor
        entries={[{ ...fileRow, value: '' }]}
        onChange={onChange}
        workspacePath='/home/me/ws'
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Choose file for row 1' }));
    await waitFor(() =>
      expect(onChange).toHaveBeenLastCalledWith([{ ...fileRow, value: 'assets/pic.png' }]),
    );
  });

  it('normalizes Windows-style paths inside the workspace', async () => {
    open.mockResolvedValue('C:\\ws\\assets\\pic.png');
    const onChange = vi.fn();
    render(
      <FormDataEditor
        entries={[{ ...fileRow, value: '' }]}
        onChange={onChange}
        workspacePath={'C:\\ws'}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Choose file for row 1' }));
    await waitFor(() =>
      expect(onChange).toHaveBeenLastCalledWith([{ ...fileRow, value: 'assets/pic.png' }]),
    );
  });

  it('stores nothing and explains when the file is outside the workspace', async () => {
    open.mockResolvedValue('/home/me/other/pic.png');
    const onChange = vi.fn();
    render(
      <FormDataEditor
        entries={[{ ...fileRow, value: '' }]}
        onChange={onChange}
        workspacePath='/home/me/ws'
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Choose file for row 1' }));
    expect(await screen.findByText('Files must be inside the workspace folder')).toBeVisible();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('edits the content type of a row', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[fileRow]} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Content type for row 1'), {
      target: { value: 'application/pdf' },
    });
    expect(onChange).toHaveBeenLastCalledWith([{ ...fileRow, contentType: 'application/pdf' }]);
  });

  it('adds an empty text row', () => {
    const onChange = vi.fn();
    render(<FormDataEditor entries={[]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: /add field/i }));
    expect(onChange.mock.calls[0][0]).toHaveLength(1);
    expect(onChange.mock.calls[0][0][0]).toMatchObject({ key: '', value: '', enabled: true });
  });
});
