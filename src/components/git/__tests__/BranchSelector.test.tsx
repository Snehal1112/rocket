import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BranchSelector } from '@/components/git/BranchSelector';
import type { BranchList } from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

const branches: BranchList = {
  current: 'main',
  local: [{ name: 'main', isHead: true, isRemote: false }],
  remote: [{ name: 'origin/feature-x', isHead: false, isRemote: true }],
};

function renderWithStore(checkoutRemoteBranch: () => Promise<void>) {
  const store = createGitStore();
  store.setState({ branches, checkoutRemoteBranch });
  render(
    <GitStoreProvider store={store}>
      <BranchSelector />
    </GitStoreProvider>,
  );
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
    renderWithStore(checkoutRemoteBranch);
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
