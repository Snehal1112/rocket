import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';

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
vi.mock('@/hooks/useFolderVariableContext', () => ({
  useFolderVariableContext: () => ({ variableContext: new Map(), environmentName: undefined }),
}));

import { HeadersSection } from '../HeadersSection';

const settings = (patch: Record<string, unknown> = {}): FolderSettings =>
  ({
    headers: [],
    auth: null,
    variables: [],
    preRequestScript: null,
    postResponseScript: null,
    testsScript: null,
    docs: null,
    ...patch,
  }) as unknown as FolderSettings;

describe('HeadersSection', () => {
  it('shows the folder headers', () => {
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Key for row 1')).toHaveValue('X-A');
  });

  it('editing a value keeps the header description', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({
          headers: [{ key: 'X-A', value: '1', enabled: true, description: 'why' }],
        })}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Value'), { target: { value: '2' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-A', value: '2', enabled: true, description: 'why' }],
    });
  });

  it('a disabled header keeps enabled false', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: false }] })}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Value'), { target: { value: '2' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-A', value: '2', enabled: false }],
    });
  });

  it('adding a blank row does not call onChange', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings()}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Add Header/ }));
    expect(screen.getByLabelText('Key for row 1')).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('naming a new row sends the header', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings()}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Add Header/ }));
    fireEvent.change(screen.getByLabelText('Key for row 1'), { target: { value: 'X-New' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-New', value: '', enabled: true }],
    });
  });

  it('resets the rows when settings.headers changes from outside', () => {
    const { rerender } = render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    rerender(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-Z', value: '9', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Key for row 1')).toHaveValue('X-Z');
  });
});
