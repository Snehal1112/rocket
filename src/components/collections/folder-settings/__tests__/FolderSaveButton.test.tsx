import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FolderSaveButton } from '../FolderSaveButton';

describe('FolderSaveButton', () => {
  it('is disabled and shows no indicator when nothing changed', () => {
    render(<FolderSaveButton isDirty={false} isLoaded saveState='idle' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument();
  });

  it('is disabled until the settings are loaded', () => {
    render(<FolderSaveButton isDirty isLoaded={false} saveState='idle' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('shows the indicator and calls onSave when dirty', async () => {
    const onSave = vi.fn();
    render(<FolderSaveButton isDirty isLoaded saveState='idle' onSave={onSave} />);
    expect(screen.getByText('Unsaved changes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('shows Saved after a successful save', () => {
    render(<FolderSaveButton isDirty={false} isLoaded saveState='success' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Saved' })).toBeInTheDocument();
  });

  it('is disabled while saving', () => {
    render(<FolderSaveButton isDirty isLoaded saveState='saving' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });
});
