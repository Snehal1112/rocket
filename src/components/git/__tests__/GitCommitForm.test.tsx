import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitGetIdentity: vi.fn().mockResolvedValue({ name: 'Test', email: 'test@example.com' }),
    gitSetIdentity: vi.fn(),
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

  it('marks the commit button as busy while committing', async () => {
    const deferred = createDeferred<void>();
    renderForm(() => deferred.promise);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    const commitButton = screen.getByRole('button', { name: /commit 1 file/i });
    await user.click(commitButton);

    expect(commitButton).toHaveAttribute('aria-busy', 'true');
    deferred.resolve();
    await vi.waitFor(() => expect(commitButton).toHaveAttribute('aria-busy', 'false'));
  });
});

describe('GitCommitForm identity setup', () => {
  it('prompts for identity before committing when none is configured, then commits after it is saved', async () => {
    vi.mocked(tauriApi.gitGetIdentity).mockResolvedValueOnce({ name: '', email: '' });
    vi.mocked(tauriApi.gitSetIdentity).mockResolvedValueOnce(undefined);
    const commitChanges = vi.fn().mockResolvedValue(undefined);
    renderForm(commitChanges);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    // GitIdentityDialog's actual title is "Git Author Identity" and its
    // default confirm button label is "Save & Commit" (see
    // src/components/git/GitIdentityDialog.tsx) — not the guessed
    // "git identity" / "Save identity" strings.
    expect(await screen.findByText(/git author identity/i)).toBeInTheDocument();
    expect(commitChanges).not.toHaveBeenCalled();

    await user.type(screen.getByLabelText(/name/i), 'Ada Lovelace');
    await user.type(screen.getByLabelText(/email/i), 'ada@example.com');
    await user.click(screen.getByRole('button', { name: /save & commit/i }));

    await vi.waitFor(() => expect(commitChanges).toHaveBeenCalledWith('fix: broken thing'));
  });
});
