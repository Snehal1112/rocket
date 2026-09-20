import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitPanel } from '@/components/git/GitPanel';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitIsRepo: vi.fn(),
    gitStatus: vi
      .fn()
      .mockResolvedValue({ branch: 'main', files: [], ahead: 0, behind: 0, isClean: true }),
    gitBranches: vi.fn().mockResolvedValue({ current: 'main', local: [], remote: [] }),
    gitListRemotes: vi.fn().mockResolvedValue([]),
    gitStashList: vi.fn().mockResolvedValue([]),
    loadGitCredentials: vi.fn().mockResolvedValue(null),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub for onCollectionChanged.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

describe('GitPanel repository scoping', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(false);
  });

  it('two panels for different repositories never share state', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockImplementation(async (id: string) => id === 'repo-a');

    // GitCloneDialog (rendered unconditionally inside GitPanel) uses react-query
    // mutations, so a QueryClientProvider must be present in the tree.
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
        <GitPanel repositoryId='repo-b' repositoryLabel='Repo B' />
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(screen.getAllByText(/Repo A|not a Git repository/).length).toBeGreaterThan(0);
    });
    // repo-a is a repo (shows its normal panel with the label), repo-b is not
    // (shows the non-repository message) — proving the two instances resolved
    // independently rather than one overwriting the other.
    expect(screen.getByText('Repo A')).toBeInTheDocument();
    expect(screen.getByText('This collection is not a Git repository.')).toBeInTheDocument();
  });
});

describe('GitPanel remount on repositoryId change', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitStatus).mockResolvedValue({
      branch: 'main',
      files: [],
      ahead: 0,
      behind: 0,
      isClean: true,
    });
    vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
    vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);
  });

  it('does not carry loaded credentials over when the mounted repository changes', async () => {
    vi.mocked(tauriApi.loadGitCredentials).mockImplementation(async (id: string) =>
      id === 'repo-a' ? { type: 'token', token: 'secret-a' } : null,
    );

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { rerender: rerenderComponent } = render(
      <QueryClientProvider client={queryClient}>
        <GitPanel key='repo-a' repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );
    await screen.findByText('Repo A');

    // Simulate the app switching the active Git tab to a different repository —
    // same JSX position, different key, exactly what EditorGroup/WorkspaceGitTab do.
    rerenderComponent(
      <QueryClientProvider client={queryClient}>
        <GitPanel key='repo-b' repositoryId='repo-b' repositoryLabel='Repo B' />
      </QueryClientProvider>,
    );
    await screen.findByText('Repo B');

    // Open the credentials dialog for repo-b and confirm it never shows repo-a's
    // loaded credentials — this proves the store instance was not reused.
    await userEvent.click(screen.getByRole('button', { name: /change ssh credentials|set credentials/i }));
    const tokenField = screen.queryByLabelText(/token/i);
    // repo-b has no saved credentials and the dialog defaults to SSH Key, so the
    // token field may not even be present; when it is, it must be empty.
    if (tokenField) {
      expect(tokenField).toHaveValue('');
    }
    expect(vi.mocked(tauriApi.loadGitCredentials)).toHaveBeenCalledWith('repo-b');
  });
});

describe('GitPanel load error rendering', () => {
  it('renders a retryable error state, not the Initialize/Clone prompt, when the load fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockRejectedValue(new Error('disk unreadable'));
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );

    expect(await screen.findByText(/failed to load this repository/i)).toBeInTheDocument();
    expect(screen.getByText(/disk unreadable/i)).toBeInTheDocument();
    expect(screen.queryByText('Initialize Git')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });
});
