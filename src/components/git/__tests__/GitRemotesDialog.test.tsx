import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

function renderDialog(store: ReturnType<typeof createGitStore>) {
  return render(
    <GitStoreProvider store={store}>
      <GitRemotesDialog open onOpenChange={vi.fn()} />
    </GitStoreProvider>,
  );
}

describe('GitRemotesDialog failure handling', () => {
  it('keeps the name/url fields and shows the error when adding a remote fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      addRemote: async () => {
        store.setState({ error: 'remote origin already exists' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText('name'), 'origin');
    await user.type(
      screen.getByPlaceholderText('https://github.com/...'),
      'https://example.com/repo.git',
    );
    await user.click(screen.getByRole('button', { name: /add/i }));

    expect(await screen.findByText('remote origin already exists')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('name')).toHaveValue('origin');
    expect(screen.getByPlaceholderText('https://github.com/...')).toHaveValue(
      'https://example.com/repo.git',
    );
  });

  it('clears the fields when adding a remote succeeds', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      addRemote: async () => {
        store.setState({ error: null });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText('name'), 'origin');
    await user.type(
      screen.getByPlaceholderText('https://github.com/...'),
      'https://example.com/repo.git',
    );
    await user.click(screen.getByRole('button', { name: /add/i }));

    expect(await screen.findByPlaceholderText('name')).toHaveValue('');
  });

  it('stays in edit mode and shows the error when saving a remote URL fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [{ name: 'origin', url: 'https://old.example.com/repo.git' }],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      setRemoteUrl: async () => {
        store.setState({ error: 'invalid remote URL' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /pencil|edit/i }));
    const urlInput = screen.getByDisplayValue('https://old.example.com/repo.git');
    await user.clear(urlInput);
    await user.type(urlInput, 'not-a-url');
    await user.keyboard('{Enter}');

    expect(await screen.findByText('invalid remote URL')).toBeInTheDocument();
    expect(screen.getByDisplayValue('not-a-url')).toBeInTheDocument();
  });

  it('stays in delete-confirmation mode and shows the error when removing a remote fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [{ name: 'origin', url: 'https://example.com/repo.git' }],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      removeRemote: async () => {
        store.setState({ error: 'could not remove remote' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Delete remote' }));
    await user.click(screen.getByRole('button', { name: /^remove$/i }));

    expect(await screen.findByText('could not remove remote')).toBeInTheDocument();
    expect(screen.getByText(/Remove/, { selector: 'span' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^remove$/i })).toBeInTheDocument();
  });
});
