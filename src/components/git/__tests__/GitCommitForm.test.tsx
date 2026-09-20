import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import type * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitGetIdentity: vi.fn().mockResolvedValue({ name: 'Test', email: 'test@example.com' }),
  };
});

function renderForm(commitChanges: (message: string) => Promise<void>) {
  const store = createGitStore();
  store.setState({
    repositoryId: 'repo-1',
    status: {
      branch: 'main',
      files: [{ path: 'a.txt', staged: true, status: 'modified' }],
      ahead: 0,
      behind: 0,
      isClean: false,
    },
    commitChanges,
  });
  render(
    <GitStoreProvider store={store}>
      <GitCommitForm />
    </GitStoreProvider>,
  );
  return store;
}

describe('GitCommitForm failure handling', () => {
  it('keeps the typed message and shows the error when commit fails', async () => {
    const store = renderForm(async () => {
      store.setState({ error: 'commit failed: nothing to commit' });
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    expect(await screen.findByText('commit failed: nothing to commit')).toBeInTheDocument();
    expect(screen.getByLabelText('Commit message')).toHaveValue('fix: broken thing');
  });

  it('clears the message on a successful commit', async () => {
    const store = renderForm(async () => {
      store.setState({ error: null });
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    expect(await screen.findByLabelText('Commit message')).toHaveValue('');
  });
});
