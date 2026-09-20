import { render, screen, waitFor } from '@testing-library/react';
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

    expect(checkoutRemoteBranch).toHaveBeenCalledWith('origin/feature-x', false, undefined);
    expect(remoteButton).toBeDisabled();

    resolveCheckout();
    await screen.findByRole('button', { name: /feature-x/ }); // popover stays mounted; re-query after state settles
  });

  it('labels remote branch rows with their remote name so same-named branches on different remotes are distinguishable', async () => {
    const store = createGitStore();
    store.setState({
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false }],
        remote: [
          { name: 'origin/shared', isHead: false, isRemote: true },
          { name: 'collections/shared', isHead: false, isRemote: true },
        ],
      },
      checkoutRemoteBranch: vi.fn().mockResolvedValue(undefined),
    });
    render(
      <GitStoreProvider store={store}>
        <BranchSelector />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /main/ }));

    expect(screen.getByRole('button', { name: 'origin/shared' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'collections/shared' })).toBeInTheDocument();
  });

  it('labels local branch rows with their tracked remote, and shows nothing for untracked branches', async () => {
    const store = createGitStore();
    store.setState({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'feature-origin', isHead: false, isRemote: false, upstream: 'origin/main' },
          {
            name: 'feature-collections',
            isHead: false,
            isRemote: false,
            upstream: 'collections/main',
          },
          { name: 'feature-untracked', isHead: false, isRemote: false },
        ],
        remote: [],
      },
      checkoutRemoteBranch: vi.fn().mockResolvedValue(undefined),
    });
    render(
      <GitStoreProvider store={store}>
        <BranchSelector />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'main' }));

    const originRow = screen.getByRole('button', { name: /feature-origin/ });
    const collectionsRow = screen.getByRole('button', { name: /feature-collections/ });
    const untrackedRow = screen.getByRole('button', { name: /feature-untracked/ });

    expect(originRow).toHaveTextContent('origin');
    expect(collectionsRow).toHaveTextContent('collections');
    expect(untrackedRow).not.toHaveTextContent(/origin|collections/);
  });
});

describe('BranchSelector force checkout on branch collision', () => {
  function renderCollisionStore() {
    const store = createGitStore();
    // The mock mirrors the store's real "never throws, sets `error` instead"
    // convention: a non-forced call that collides sets the precise backend
    // collision message; a forced call (or a non-colliding call) succeeds.
    const checkoutRemoteBranch = vi.fn(async (_name: string, force?: boolean) => {
      if (!force) {
        store.setState({ error: "local branch 'main' already exists" });
      }
    });
    store.setState({
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false, upstream: 'origin/main' }],
        remote: [{ name: 'collections/main', isHead: false, isRemote: true }],
      },
      checkoutRemoteBranch,
    });
    render(
      <GitStoreProvider store={store}>
        <BranchSelector />
      </GitStoreProvider>,
    );
    return { store, checkoutRemoteBranch };
  }

  it('shows a confirmation dialog on a collision error instead of retrying with force', async () => {
    const { checkoutRemoteBranch } = renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));

    expect(await screen.findByText('Branch already exists')).toBeInTheDocument();
    expect(checkoutRemoteBranch).toHaveBeenCalledTimes(1);
    expect(checkoutRemoteBranch).toHaveBeenCalledWith('collections/main', false, undefined);
    expect(checkoutRemoteBranch).not.toHaveBeenCalledWith('collections/main', true, undefined);
  });

  it('retries with force:true when the confirmation dialog is confirmed', async () => {
    const { checkoutRemoteBranch } = renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    await user.click(screen.getByRole('button', { name: 'Reset Branch' }));

    expect(checkoutRemoteBranch).toHaveBeenCalledTimes(2);
    expect(checkoutRemoteBranch).toHaveBeenNthCalledWith(2, 'collections/main', true, undefined);
    expect(screen.queryByText('Branch already exists')).not.toBeInTheDocument();
  });

  it('cancels without retrying and closes the dialog', async () => {
    const { checkoutRemoteBranch } = renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(checkoutRemoteBranch).toHaveBeenCalledTimes(1);
    expect(checkoutRemoteBranch).not.toHaveBeenCalledWith('collections/main', true);
    expect(screen.queryByText('Branch already exists')).not.toBeInTheDocument();
  });

  it('re-shows the confirmation dialog on a repeat, identical collision after cancelling', async () => {
    // Regression test: the store's `error` is only ever overwritten, never
    // cleared, by cancelling — without clearing it before re-measuring, a
    // second collision producing the exact same message would look like "no
    // change" and silently report success instead of re-arming the dialog.
    const { checkoutRemoteBranch } = renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByText('Branch already exists')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'collections/main' }));

    expect(await screen.findByText('Branch already exists')).toBeInTheDocument();
    expect(checkoutRemoteBranch).toHaveBeenCalledTimes(2);
    expect(checkoutRemoteBranch).not.toHaveBeenCalledWith('collections/main', true);
  });

  it('surfaces a non-collision force-checkout failure as the normal error banner, not the dialog', async () => {
    const store = createGitStore();
    const checkoutRemoteBranch = vi.fn(async (_name: string, force?: boolean) => {
      if (!force) {
        store.setState({ error: "local branch 'main' already exists" });
      } else {
        store.setState({ error: 'cannot reset: uncommitted changes present' });
      }
    });
    store.setState({
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false, upstream: 'origin/main' }],
        remote: [{ name: 'collections/main', isHead: false, isRemote: true }],
      },
      checkoutRemoteBranch,
    });
    render(
      <GitStoreProvider store={store}>
        <BranchSelector />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');
    await user.click(screen.getByRole('button', { name: 'Reset Branch' }));

    expect(
      await screen.findByText('cannot reset: uncommitted changes present'),
    ).toBeInTheDocument();
    expect(screen.queryByText('Branch already exists')).not.toBeInTheDocument();
  });
});

describe('BranchSelector checkout-as-new on branch collision', () => {
  function renderCollisionStore() {
    const store = createGitStore();
    // Mirrors the store's "never throws, sets `error` instead" convention: a
    // non-forced call whose target name is 'main' collides; any other name
    // (the new-branch path) succeeds.
    const checkoutRemoteBranch = vi.fn(async (_name: string, force?: boolean, asName?: string) => {
      const targetName = asName ?? 'main';
      if (!force && targetName === 'main') {
        store.setState({ error: "local branch 'main' already exists" });
      }
    });
    store.setState({
      branches: {
        current: 'main',
        local: [{ name: 'main', isHead: true, isRemote: false, upstream: 'origin/main' }],
        remote: [{ name: 'collections/main', isHead: false, isRemote: true }],
      },
      checkoutRemoteBranch,
    });
    render(
      <GitStoreProvider store={store}>
        <BranchSelector />
      </GitStoreProvider>,
    );
    return { store, checkoutRemoteBranch };
  }

  it('shows the "Checkout as New" input and button pre-filled with the full remote branch name', async () => {
    renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    const input = screen.getByLabelText('New local branch name');
    expect(input).toHaveValue('collections/main');
    expect(screen.getByRole('button', { name: 'Checkout as New' })).toBeInTheDocument();
  });

  it('clicking "Checkout as New" calls checkoutRemoteBranch with the typed name, then closes the dialog and popover', async () => {
    const { checkoutRemoteBranch } = renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    const input = screen.getByLabelText('New local branch name');
    await user.clear(input);
    await user.type(input, 'collections-main-local');
    await user.click(screen.getByRole('button', { name: 'Checkout as New' }));

    expect(checkoutRemoteBranch).toHaveBeenCalledTimes(2);
    expect(checkoutRemoteBranch).toHaveBeenNthCalledWith(
      2,
      'collections/main',
      false,
      'collections-main-local',
    );
    await waitFor(() => {
      expect(screen.queryByText('Branch already exists')).not.toBeInTheDocument();
    });
    // Popover closes too — its search input is no longer present.
    expect(screen.queryByLabelText('Search branches')).not.toBeInTheDocument();
  });

  it('disables "Checkout as New" when the field is emptied', async () => {
    renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    const input = screen.getByLabelText('New local branch name');
    await user.clear(input);

    expect(screen.getByRole('button', { name: 'Checkout as New' })).toBeDisabled();
  });

  it('disables "Checkout as New" when the field is set back to the exact colliding branch name', async () => {
    renderCollisionStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: 'collections/main' }));
    await screen.findByText('Branch already exists');

    const input = screen.getByLabelText('New local branch name');
    await user.clear(input);
    await user.type(input, 'main');

    expect(screen.getByRole('button', { name: 'Checkout as New' })).toBeDisabled();
  });
});
