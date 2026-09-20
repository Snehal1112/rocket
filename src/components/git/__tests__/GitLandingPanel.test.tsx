import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

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
    const push = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 1, behind: 1, isClean: true },
      push,
      fetch: async () => {
        store.setState({ error: 'auth failed' });
      },
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
    expect(push).not.toHaveBeenCalled();
  });
});
