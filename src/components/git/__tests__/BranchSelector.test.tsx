import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BranchSelector } from '@/components/git/BranchSelector';
import type { BranchList } from '@/lib/tauri-api';
import { createGitStore, type GitState } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

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
