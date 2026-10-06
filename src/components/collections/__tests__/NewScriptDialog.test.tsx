import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NewScriptDialog } from '@/components/collections/NewScriptDialog';
import { createScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, createScriptFile: vi.fn(), readScriptFile: vi.fn().mockResolvedValue('x') };
});

describe('NewScriptDialog', () => {
  beforeEach(() => {
    vi.mocked(createScriptFile).mockReset();
    usePaneStore.getState().closeAll();
  });

  it('creates the script in the folder and opens it', async () => {
    vi.mocked(createScriptFile).mockResolvedValue('lib/utils.js');
    const onClose = vi.fn();
    render(<NewScriptDialog open collectionName='col' folderPath='lib' onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Script name'), { target: { value: 'utils' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(createScriptFile).toHaveBeenCalledWith('col', 'lib', 'utils'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('shows the backend error and stays open', async () => {
    vi.mocked(createScriptFile).mockRejectedValue('Invalid input: utils.js already exists');
    const onClose = vi.fn();
    render(<NewScriptDialog open collectionName='col' folderPath='' onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Script name'), { target: { value: 'utils' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    expect(await screen.findByText(/already exists/)).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('disables Create for an empty name', () => {
    render(<NewScriptDialog open collectionName='col' folderPath='' onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
  });
});
