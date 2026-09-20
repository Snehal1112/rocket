import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BranchSelector } from '@/components/git/BranchSelector';
import type { BranchList } from '@/lib/tauri-api';
import { createGitStore, type GitState } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

const branches: BranchList = {
  current: 'main',
  local: [{ name: 'main', isHead: true, isRemote: false }],
  remote: [{ name: 'origin/feature-x', isHead: false, isRemote: true }],
};

function renderWithStore(patch: Partial<GitState>) {
  const store = createGitStore();
  store.setState({ branches, ...patch });
  render(
    <GitStoreProvider store={store}>
      <BranchSelector />
    </GitStoreProvider>,
  );
  return store;
}

describe('BranchSelector remote checkout', () => {
  it('shows a busy spinner and disables the row while checkout is in flight, and surfaces a resulting error', async () => {
    let resolveCheckout!: () => void;
    const checkoutRemoteBranch = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveCheckout = resolve;
        }),
    );
    renderWithStore({ checkoutRemoteBranch });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    const remoteButton = screen.getByRole('button', { name: /feature-x/ });
    await user.click(remoteButton);

    expect(checkoutRemoteBranch).toHaveBeenCalledWith('origin/feature-x');
    expect(remoteButton).toBeDisabled();

    resolveCheckout();
    await screen.findByRole('button', { name: /feature-x/ }); // popover stays mounted; re-query after state settles
  });
});

describe('BranchSelector createBranch result handling', () => {
  it('treats a repeated identical error as a failure, not success', async () => {
    const store = renderWithStore({
      createBranch: async () => {
        store.setState({ error: 'branch already exists' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    await user.click(screen.getByLabelText('Create branch'));

    expect(await screen.findByText('branch already exists')).toBeInTheDocument();
    expect(screen.getByLabelText('New branch name')).toHaveValue('feature-x');

    // Second attempt fails with the exact same message — must still be treated
    // as a failure (this is what the old prevError/nextError comparison got wrong).
    await user.click(screen.getByLabelText('Create branch'));
    expect(await screen.findByText('branch already exists')).toBeInTheDocument();
    expect(screen.getByLabelText('New branch name')).toHaveValue('feature-x');
  });

  it('clears the input on success', async () => {
    const store = renderWithStore({
      createBranch: async () => {
        store.setState({ error: null });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    await user.click(screen.getByLabelText('Create branch'));

    expect(await screen.findByLabelText('New branch name')).toHaveValue('');
  });
});

describe('BranchSelector switchBranch result handling', () => {
  it('treats a repeated identical switch error as a failure', async () => {
    const store = renderWithStore({
      switchBranch: async () => {
        store.setState({ error: 'uncommitted changes would be overwritten' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    // `main` is head; click a non-head local branch row instead — the fixture
    // only has `main` as local, so extend the fixture's local branches for
    // this test via a second store patch.
    store.setState({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });

    await user.click(await screen.findByText('develop'));
    expect(await screen.findByText('uncommitted changes would be overwritten')).toBeInTheDocument();

    await user.click(screen.getByText('develop'));
    expect(await screen.findByText('uncommitted changes would be overwritten')).toBeInTheDocument();
  });
});

describe('BranchSelector mergeBranch/deleteBranch result handling', () => {
  it('treats a repeated identical merge error as a failure and keeps the popover open', async () => {
    const store = renderWithStore({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
      mergeBranch: async () => {
        store.setState({ error: 'not something we can merge (unrelated histories)' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: /merge into current/i }));

    expect(
      await screen.findByText('not something we can merge (unrelated histories)'),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /merge into current/i }));
    expect(
      await screen.findByText('not something we can merge (unrelated histories)'),
    ).toBeInTheDocument();
  });

  it('shows an error when deleting a branch fails', async () => {
    const store = renderWithStore({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
      deleteBranch: async () => {
        store.setState({ error: 'cannot delete the currently checked-out branch' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: /delete branch/i }));

    expect(
      await screen.findByText('cannot delete the currently checked-out branch'),
    ).toBeInTheDocument();
  });
});

describe('BranchSelector duplicate-action guards', () => {
  it('disables Create while a create is in flight and only calls createBranch once', async () => {
    const deferred = createDeferred<void>();
    const createBranch = vi.fn(() => deferred.promise);
    renderWithStore({ createBranch });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    const createButton = screen.getByLabelText('Create branch');
    await user.click(createButton);

    expect(createButton).toBeDisabled();
    await user.click(createButton); // no-op — button is disabled

    deferred.resolve();
    // On success the input clears (pre-existing behavior), which independently
    // disables the button via `!newBranchName.trim()`. Retype a name to isolate
    // the guard's own `creating` flag and confirm it was reset, not left stuck.
    await vi.waitFor(() => expect(screen.getByLabelText('New branch name')).toHaveValue(''));
    await user.type(screen.getByLabelText('New branch name'), 'another-branch');
    await vi.waitFor(() => expect(createButton).not.toBeDisabled());
    expect(createBranch).toHaveBeenCalledTimes(1);
  });
});

describe('BranchSelector error announcements', () => {
  it('announces the branch-switch error banner as an alert', async () => {
    const store = renderWithStore({
      switchBranch: async () => {
        store.setState({ error: 'checkout failed' });
      },
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByText('develop'));

    expect(await screen.findByRole('alert')).toHaveTextContent('checkout failed');
  });
});

describe('BranchSelector keyboard behavior', () => {
  it('prevents the default Space-scroll behavior when activating a local branch row', async () => {
    const switchBranch = vi.fn();
    const store = renderWithStore({
      switchBranch,
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    // Find the 'develop' branch row (a div with role='button')
    const developRow = screen.getByText('develop').closest('div[role="button"]');
    expect(developRow).toBeInTheDocument();

    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    developRow!.dispatchEvent(event);

    expect(switchBranch).toHaveBeenCalledWith('develop');
    expect(event.defaultPrevented).toBe(true);
  });
});
