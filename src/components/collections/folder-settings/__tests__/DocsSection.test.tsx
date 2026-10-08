import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { DocsSection } from '../DocsSection';

function renderSection(docs: string | undefined, folderPath = 'a/b') {
  const onChange = vi.fn();
  const settings: FolderSettings = { headers: [], variables: [], docs };
  const utils = render(
    <DocsSection
      collectionName='col'
      folderPath={folderPath}
      settings={settings}
      onChange={onChange}
    />,
  );
  return { onChange, settings, ...utils };
}

describe('DocsSection', () => {
  it('starts in preview mode and renders the markdown', () => {
    renderSection('# Folder notes');
    expect(screen.getByRole('heading', { name: 'Folder notes' })).toBeInTheDocument();
  });

  it('shows the empty state when there are no docs', () => {
    renderSection(undefined);
    expect(screen.getByText('No documentation yet')).toBeInTheDocument();
  });

  it('switches to edit mode with the Edit tab', async () => {
    renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.getByPlaceholderText(/Add documentation/)).toHaveValue('hello');
  });

  it('shows no Save button of its own', async () => {
    renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.queryByRole('button', { name: /save/i })).not.toBeInTheDocument();
  });

  it('typing patches only docs', async () => {
    const { onChange } = renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    fireEvent.change(screen.getByPlaceholderText(/Add documentation/), {
      target: { value: 'hello world' },
    });
    expect(onChange).toHaveBeenCalledWith({ docs: 'hello world' });
  });

  it('clearing the editor stores undefined', async () => {
    const { onChange } = renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    fireEvent.change(screen.getByPlaceholderText(/Add documentation/), { target: { value: '' } });
    expect(onChange).toHaveBeenCalledWith({ docs: undefined });
  });

  it('returns to preview mode when the folder changes', async () => {
    const { rerender, settings, onChange } = renderSection('hello', 'a');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.getByPlaceholderText(/Add documentation/)).toBeInTheDocument();
    rerender(
      <DocsSection collectionName='col' folderPath='b' settings={settings} onChange={onChange} />,
    );
    expect(screen.queryByPlaceholderText(/Add documentation/)).not.toBeInTheDocument();
  });
});
