import { vi, describe, it, expect, beforeEach } from 'vitest';
import { useGitStore } from '../git-store';

vi.mock('@/lib/tauri-api', () => ({
  gitIsRepo: vi.fn(),
  gitStatus: vi.fn().mockResolvedValue({
    branch: 'main', files: [], ahead: 0, behind: 0, isClean: true,
  }),
  gitBranches: vi.fn().mockResolvedValue({ local: [], remote: [] }),
  gitRemotes: vi.fn().mockResolvedValue([]),
  gitStashes: vi.fn().mockResolvedValue([]),
  gitLog: vi.fn().mockResolvedValue([]),
  gitConflicts: vi.fn().mockResolvedValue([]),
  gitPush: vi.fn().mockResolvedValue(undefined),
  gitPull: vi.fn().mockResolvedValue(undefined),
  gitFetch: vi.fn().mockResolvedValue(undefined),
}));

describe('git-store clearError', () => {
  beforeEach(() => {
    useGitStore.setState({
      error: null,
      collectionPath: null,
      credentials: null,
      remotes: [],
    });
    vi.clearAllMocks();
  });

  it('clearError sets error to null', () => {
    useGitStore.setState({ error: 'previous error' });
    useGitStore.getState().clearError();
    expect(useGitStore.getState().error).toBeNull();
  });

  it('push clears stale error before executing', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().push();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('pull clears stale error before executing', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().pull();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('fetch clears stale error before executing', async () => {
    const { gitFetch } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().fetch();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('push sets error when operation fails', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockRejectedValueOnce(new Error('NotFastForward'));

    useGitStore.setState({
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().push();

    expect(useGitStore.getState().error).toContain('NotFastForward');
  });
});
