import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitClone: vi.fn(),
    detectClonedStructure: vi.fn(),
    selectCloneDestination: vi.fn(),
  };
});

vi.mock('@/lib/queries/workspace-queries', async () => {
  const actual = await vi.importActual<typeof import('@/lib/queries/workspace-queries')>(
    '@/lib/queries/workspace-queries',
  );
  return {
    ...actual,
    useOpenWorkspaceFromDisk: () => ({ mutateAsync: openFromDiskMock }),
    useSwitchWorkspace: () => ({ mutate: switchWorkspaceMock }),
  };
});

const openFromDiskMock = vi.fn();
const switchWorkspaceMock = vi.fn();

function renderDialog(onOpenChange: (open: boolean) => void, open: boolean) {
  const store = createGitStore();
  store.setState({ credentials: { type: 'token', token: 'tok' } });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <GitStoreProvider store={store}>
        <GitCloneDialog open={open} onOpenChange={onOpenChange} />
      </GitStoreProvider>
    </QueryClientProvider>,
  );
}

describe('GitCloneDialog stale completion', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(tauriApi.selectCloneDestination).mockResolvedValue({
      capability: 'cap-1',
      displayPath: '/tmp/dest-1',
      expiresInSeconds: 300,
    });
    vi.mocked(tauriApi.detectClonedStructure).mockResolvedValue({
      kind: 'workspace',
      workspacePath: '/tmp/dest-1',
      collections: [],
    });
  });

  it('ignores a clone that resolves after the dialog was closed and does not switch workspaces', async () => {
    const deferredClone = createDeferred<void>();
    vi.mocked(tauriApi.gitClone).mockReturnValue(deferredClone.promise);

    let open = true;
    const onOpenChange = vi.fn((next: boolean) => {
      open = next;
    });
    const { rerender } = renderDialog(onOpenChange, open);

    await userEvent.click(screen.getByRole('button', { name: /browse/i }));
    await userEvent.type(
      screen.getByPlaceholderText(/github.com/i),
      'https://example.com/repo.git',
    );
    await userEvent.click(screen.getByRole('button', { name: /^clone$/i }));

    // Dialog closes (e.g. user hits escape) while the clone is still in flight.
    rerender(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <GitStoreProvider store={createGitStore()}>
          <GitCloneDialog open={false} onOpenChange={onOpenChange} />
        </GitStoreProvider>
      </QueryClientProvider>,
    );

    // The stale clone now resolves.
    deferredClone.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(openFromDiskMock).not.toHaveBeenCalled();
    expect(switchWorkspaceMock).not.toHaveBeenCalled();
  });

  it('does not continue a credentials-pending clone after the dialog was closed', async () => {
    // Regression test: closing the dialog (e.g. via Escape) while
    // awaitingCredentials was true used to leave the credentials-continuation
    // effect armed. If credentials then arrived — from some other, still-open
    // credentials UI — performClone would run to completion with no clone
    // dialog visible on screen, silently switching the user's workspace.
    vi.mocked(tauriApi.gitClone).mockResolvedValue(undefined);

    const store = createGitStore();
    // No credentials yet — Clone will trigger the credentials dialog instead
    // of cloning immediately.
    store.setState({ credentials: null });

    const onOpenChange = vi.fn();
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { rerender } = render(
      <QueryClientProvider client={queryClient}>
        <GitStoreProvider store={store}>
          <GitCloneDialog open={true} onOpenChange={onOpenChange} />
        </GitStoreProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(screen.getByRole('button', { name: /browse/i }));
    await userEvent.type(
      screen.getByPlaceholderText(/github.com/i),
      'https://example.com/repo.git',
    );
    await userEvent.click(screen.getByRole('button', { name: /^clone$/i }));

    // Clone is now awaiting credentials (progress step shown, dialog still open).
    expect(await screen.findByText(/cloning repository/i)).toBeInTheDocument();

    // User presses Escape — dialog closes while still awaiting credentials.
    rerender(
      <QueryClientProvider client={queryClient}>
        <GitStoreProvider store={store}>
          <GitCloneDialog open={false} onOpenChange={onOpenChange} />
        </GitStoreProvider>
      </QueryClientProvider>,
    );

    // Credentials now arrive via whatever other UI surface prompted for them.
    store.setState({ credentials: { type: 'token', token: 'tok' } });
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(tauriApi.gitClone).not.toHaveBeenCalled();
    expect(openFromDiskMock).not.toHaveBeenCalled();
    expect(switchWorkspaceMock).not.toHaveBeenCalled();
  });

  it('gives the URL and destination fields accessible names', () => {
    renderDialog(vi.fn(), true);
    expect(screen.getByLabelText('Repository URL')).toBeInTheDocument();
    expect(screen.getByLabelText('Destination')).toBeInTheDocument();
  });
});
