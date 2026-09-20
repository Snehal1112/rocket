import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

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
