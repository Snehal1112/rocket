import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
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
