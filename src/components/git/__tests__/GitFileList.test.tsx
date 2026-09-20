import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitFileList } from '@/components/git/GitFileList';
import type { RepoStatus } from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

const baseStatus: RepoStatus = {
  branch: 'main',
  isClean: false,
  ahead: 0,
  behind: 0,
  files: [{ path: 'notes.txt', status: 'modified', staged: false }],
};

function renderWithStore() {
  const store = createGitStore();
  store.setState({ status: baseStatus });
  const discardFiles = vi.fn();
  store.setState({ discardFiles });
  render(
    <GitStoreProvider store={store}>
      <GitFileList onFileClick={vi.fn()} onConflictClick={vi.fn()} />
    </GitStoreProvider>,
  );
  return { discardFiles };
}

describe('GitFileList individual discard confirmation', () => {
  it('does not call discardFiles until the confirmation dialog is confirmed', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /discard/i }));
    expect(discardFiles).not.toHaveBeenCalled();
    expect(screen.getByText('Discard Changes?')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(discardFiles).toHaveBeenCalledWith(['notes.txt']);
  });

  it('cancel performs no discard', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /discard/i }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(discardFiles).not.toHaveBeenCalled();
  });
});
