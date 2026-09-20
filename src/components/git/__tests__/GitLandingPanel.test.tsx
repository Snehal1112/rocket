import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

describe('GitLandingPanel workflow guards', () => {
  it('does not record a fetch timestamp when fetch fails', async () => {
    const store = createGitStore();
    // Mirrors the store's own contract: fetch never throws, it sets `error`.
    const fetch = async () => {
      store.setState({ error: 'network unreachable' });
    };
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 1, behind: 2, isClean: true },
      fetch,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^fetch$/i }));

    expect(await screen.findByText('network unreachable')).toBeInTheDocument();
    expect(screen.getByText('Never fetched')).toBeInTheDocument();
  });

  it('does not push after a fetch failure in the fetch-then-push flow', async () => {
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 1, behind: 0, isClean: true },
      fetchThenPush: vi.fn().mockImplementation(async () => {
        store.setState({ error: 'auth failed' });
        return false; // fetch failed
      }),
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    // lastFetched is null and behind > 0, so Push opens the fetch-first dialog.
    await user.click(screen.getByRole('button', { name: /^push/i }));
    await user.click(screen.getByRole('button', { name: /fetch & push/i }));

    expect(await screen.findByText('auth failed')).toBeInTheDocument();
  });

  it('does not pull after a failed auto-stash, and does not pop after a failed pull', async () => {
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
      stashThenPull: async () => {
        store.setState({ error: 'could not create stash' });
      },
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^pull/i }));
    await user.click(screen.getByRole('button', { name: /stash & pull/i }));

    expect(await screen.findByText('could not create stash')).toBeInTheDocument();
  });

  it('does not pop the auto-stash after an outright pull failure', async () => {
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
      stashThenPull: vi.fn().mockImplementation(async () => {
        store.setState({ error: 'authentication failed' });
      }),
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^pull/i }));
    await user.click(screen.getByRole('button', { name: /stash & pull/i }));

    expect(await screen.findByText('authentication failed')).toBeInTheDocument();
  });

  it('does not abort stash-then-pull due to a stale error left over from an earlier, unrelated failure', async () => {
    // Regression test: the store's `error` field can be non-null on entry
    // (e.g. a previously failed push) even though stashThenPull is
    // about to succeed. handleStashAndPull must clear that stale error
    // before calling stashThenPull, or stashThenPull's internal error checking
    // will misread the stale error as its own failure.
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
      error: 'stale error from an earlier push',
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^pull/i }));
    await user.click(screen.getByRole('button', { name: /stash & pull/i }));

    // After success, stale error should be cleared and not displayed
    expect(screen.queryByText('stale error from an earlier push')).not.toBeInTheDocument();
  });

  it('marks Fetch as busy while a fetch is in flight', async () => {
    const deferred = createDeferred<void>();
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      fetch: () => deferred.promise,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    const fetchButton = screen.getByRole('button', { name: /^fetch$/i });
    await user.click(fetchButton);

    expect(fetchButton).toHaveAttribute('aria-busy', 'true');
    deferred.resolve();
    await vi.waitFor(() => expect(fetchButton).toHaveAttribute('aria-busy', 'false'));
  });
});

describe('GitLandingPanel force-push confirmation', () => {
  it("names the branch's actual tracked remote, not just the first configured remote", async () => {
    const store = createGitStore();
    store.setState({
      status: { branch: 'main', files: [], ahead: 2, behind: 0, isClean: true },
      credentials: { type: 'sshAgent' },
      // "collections" is listed first, but the current branch tracks "origin" —
      // the dialog must never claim it will overwrite "collections/main" when
      // the push actually targets "origin/main".
      remotes: [
        { name: 'collections', url: 'git@github.com:test/collections.git' },
        { name: 'origin', url: 'git@github.com:test/repo.git' },
      ],
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false, upstream: 'origin/main' }],
        remote: [],
      },
    });

    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'More push options' }));
    await user.click(screen.getByRole('menuitem', { name: /Force Push/ }));

    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      "This will overwrite origin/main's history",
    );
    expect(screen.queryByText(/collections\/main/)).not.toBeInTheDocument();
  });
});

describe('GitLandingPanel force-push menu item', () => {
  function renderMenu(hasConflicts = false) {
    const store = createGitStore();
    store.setState({
      status: {
        branch: 'main',
        files: hasConflicts ? [{ path: 'a.txt', status: 'conflicted', staged: false }] : [],
        ahead: 2,
        behind: 0,
        isClean: !hasConflicts,
      },
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false }],
        remote: [],
      },
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
  }

  it('marks Force Push with a destructive warning icon distinct from the plain Push action', async () => {
    renderMenu();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'More push options' }));

    const item = screen.getByRole('menuitem', { name: /Force Push/ });
    const icon = item.querySelector('svg');
    expect(icon).toHaveClass('text-destructive');
  });

  it('separates Force Push from routine push options with a menu separator', async () => {
    renderMenu();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'More push options' }));

    expect(screen.getByRole('separator')).toBeInTheDocument();
  });

  it('explains inline why Force Push is disabled when there are unresolved conflicts', async () => {
    renderMenu(true);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'More push options' }));

    const item = screen.getByRole('menuitem', { name: /Force Push/ });
    expect(item).toHaveAttribute('aria-disabled', 'true');
    expect(item).toHaveTextContent('resolve conflicts first');
  });

  it('shows no conflict reason when Force Push is enabled', async () => {
    renderMenu(false);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'More push options' }));

    const item = screen.getByRole('menuitem', { name: /Force Push/ });
    expect(item).not.toHaveTextContent('resolve conflicts first');
  });
});
