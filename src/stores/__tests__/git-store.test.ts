import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { GitCredentials } from '@/lib/tauri-api';
import { useGitStore } from '../git-store';

vi.mock('@/lib/tauri-api', () => ({
  gitIsRepo: vi.fn(),
  gitStatus: vi.fn().mockResolvedValue({
    branch: 'main',
    files: [],
    ahead: 0,
    behind: 0,
    isClean: true,
  }),
  gitBranches: vi.fn().mockResolvedValue({ current: 'main', local: [], remote: [] }),
  gitListRemotes: vi.fn().mockResolvedValue([]),
  gitStashList: vi.fn().mockResolvedValue([]),
  gitLog: vi.fn().mockResolvedValue([]),
  gitConflicts: vi.fn().mockResolvedValue([]),
  gitPush: vi.fn().mockResolvedValue(undefined),
  gitPull: vi.fn().mockResolvedValue(undefined),
  gitFetch: vi.fn().mockResolvedValue(undefined),
  gitStage: vi.fn().mockResolvedValue(undefined),
  gitUnstage: vi.fn().mockResolvedValue(undefined),
  gitDiscard: vi.fn().mockResolvedValue(undefined),
  gitCommit: vi.fn().mockResolvedValue({
    id: 'abc1234',
    fullId: 'abc1234abc1234',
    message: 'test commit',
    author: 'Test',
    authorEmail: 'test@test.com',
    timestamp: '2026-01-01',
    filesChanged: 1,
  }),
  gitStashSave: vi.fn().mockResolvedValue(undefined),
  gitStashPop: vi.fn().mockResolvedValue(undefined),
  gitStashApply: vi.fn().mockResolvedValue(undefined),
  gitStashDrop: vi.fn().mockResolvedValue(undefined),
  gitSwitchBranch: vi.fn().mockResolvedValue(undefined),
  gitCheckoutRemoteBranch: vi.fn().mockResolvedValue(undefined),
  gitCreateBranch: vi.fn().mockResolvedValue(undefined),
  gitDeleteBranch: vi.fn().mockResolvedValue(undefined),
  gitMergeBranch: vi.fn().mockResolvedValue(undefined),
  gitResolveConflict: vi.fn().mockResolvedValue(undefined),
  gitAbortMerge: vi.fn().mockResolvedValue(undefined),
  gitAddRemote: vi.fn().mockResolvedValue(undefined),
  gitRemoveRemote: vi.fn().mockResolvedValue(undefined),
  gitSetRemoteUrl: vi.fn().mockResolvedValue(undefined),
  loadGitCredentials: vi.fn().mockResolvedValue(null),
  gitGetIdentity: vi.fn().mockResolvedValue({ name: 'Test User', email: 'test@example.com' }),
}));

vi.mock('@/stores/workspace-store', () => ({
  useWorkspaceStore: Object.assign(
    (selector: (s: { activeWorkspaceId: string; multiWorkspaceMode: boolean }) => unknown) =>
      selector({ activeWorkspaceId: 'ws-test', multiWorkspaceMode: false }),
    {
      getState: () => ({ activeWorkspaceId: 'ws-test', multiWorkspaceMode: false }),
    },
  ),
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
    vi.mocked(gitFetch).mockResolvedValueOnce({
      updatedRefs: [],
      receivedObjects: 0,
      receivedBytes: 0,
    });

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

describe('setCollection', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: null,
      isRepo: false,
      error: null,
      status: null,
      branches: null,
      remotes: [],
      stashes: [],
      loading: false,
    });
    vi.clearAllMocks();
  });

  it('non-repo path sets isRepo=false and status=null', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    // Seed non-default state so assertions are meaningful.
    useGitStore.setState({
      isRepo: true,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.mocked(gitIsRepo).mockResolvedValueOnce(false);

    await useGitStore.getState().setCollection('/not/a/repo');

    expect(useGitStore.getState().isRepo).toBe(false);
    expect(useGitStore.getState().status).toBeNull();
  });

  it('valid repo path loads status, branches, remotes, and stashes', async () => {
    const { gitIsRepo, gitStatus, gitBranches, gitListRemotes, gitStashList } = await import(
      '@/lib/tauri-api'
    );
    vi.mocked(gitIsRepo).mockResolvedValueOnce(true);

    await useGitStore.getState().setCollection('/test/repo');

    expect(useGitStore.getState().isRepo).toBe(true);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
    expect(gitListRemotes).toHaveBeenCalledWith('/test/repo');
    expect(gitStashList).toHaveBeenCalledWith('/test/repo');
    // Verify state was actually stored, not just that functions were called.
    expect(useGitStore.getState().status).not.toBeNull();
    expect(useGitStore.getState().loading).toBe(false);
  });

  it('gitIsRepo throwing sets error state', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockRejectedValueOnce(new Error('disk error'));

    await useGitStore.getState().setCollection('/test/repo');

    expect(useGitStore.getState().error).toContain('disk error');
  });
});

describe('pendingNetworkOp and setCredentials', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      credentials: null,
      error: null,
      showCredentialsDialog: false,
      pendingNetworkOp: null,
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      branches: { current: 'main', local: [], remote: [] },
    });
    vi.clearAllMocks();
  });

  it('pull without credentials opens dialog and sets pendingNetworkOp=pull', async () => {
    await useGitStore.getState().pull();

    expect(useGitStore.getState().showCredentialsDialog).toBe(true);
    expect(useGitStore.getState().pendingNetworkOp).toBe('pull');
  });

  it('setCredentials auto-retries pull and clears pendingNetworkOp', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    useGitStore.setState({ pendingNetworkOp: 'pull' });

    useGitStore.getState().setCredentials({ type: 'sshAgent' });

    await vi.waitFor(() => {
      expect(gitPull).toHaveBeenCalledWith('/test/repo', 'origin', { type: 'sshAgent' });
    });

    expect(useGitStore.getState().pendingNetworkOp).toBeNull();
    expect(useGitStore.getState().showCredentialsDialog).toBe(false);
  });

  it('push without credentials sets pendingNetworkOp=push', async () => {
    await useGitStore.getState().push();

    expect(useGitStore.getState().showCredentialsDialog).toBe(true);
    expect(useGitStore.getState().pendingNetworkOp).toBe('push');
  });

  it('fetch without credentials sets pendingNetworkOp=fetch', async () => {
    await useGitStore.getState().fetch();

    expect(useGitStore.getState().showCredentialsDialog).toBe(true);
    expect(useGitStore.getState().pendingNetworkOp).toBe('fetch');
  });

  it('dismissing dialog clears pendingNetworkOp without retrying', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    useGitStore.setState({ pendingNetworkOp: 'push' });

    useGitStore.getState().setShowCredentialsDialog(false);

    expect(useGitStore.getState().pendingNetworkOp).toBeNull();
    expect(gitPush).not.toHaveBeenCalled();
  });

  it('reset clears pendingNetworkOp', () => {
    useGitStore.setState({ pendingNetworkOp: 'fetch' });

    useGitStore.getState().reset();

    expect(useGitStore.getState().pendingNetworkOp).toBeNull();
  });
});

describe('staging', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.clearAllMocks();
  });

  it('stageFiles calls gitStage and refreshes status', async () => {
    const { gitStage, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().stageFiles(['foo.bru']);

    expect(gitStage).toHaveBeenCalledWith('/test/repo', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
  });

  it('unstageFiles calls gitUnstage and refreshes status', async () => {
    const { gitUnstage, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().unstageFiles(['foo.bru']);

    expect(gitUnstage).toHaveBeenCalledWith('/test/repo', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
  });

  it('stageAll stages only unstaged non-unchanged files', async () => {
    const { gitStage, gitStatus } = await import('@/lib/tauri-api');
    const files: import('@/lib/tauri-api').FileStatus[] = [
      { path: 'already-staged.bru', status: 'modified', staged: true },
      { path: 'unstaged-modified.bru', status: 'modified', staged: false },
      { path: 'unchanged.bru', status: 'unchanged', staged: false },
    ];
    // stageAll calls refreshStatus first, so mock gitStatus to return these files.
    vi.mocked(gitStatus).mockResolvedValueOnce({
      branch: 'main',
      ahead: 0,
      behind: 0,
      isClean: false,
      files,
    });

    await useGitStore.getState().stageAll();

    expect(gitStage).toHaveBeenCalledWith('/test/repo', ['unstaged-modified.bru']);
  });

  it('discardFiles calls gitDiscard and refreshes status', async () => {
    const { gitDiscard, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().discardFiles(['foo.bru']);

    expect(gitDiscard).toHaveBeenCalledWith('/test/repo', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
  });

  it('commitChanges calls gitCommit and refreshes status', async () => {
    const { gitCommit, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().commitChanges('initial commit');

    expect(gitCommit).toHaveBeenCalledWith('/test/repo', 'initial commit');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
  });

  it('stageFiles sets error on failure', async () => {
    const { gitStage } = await import('@/lib/tauri-api');
    vi.mocked(gitStage).mockRejectedValueOnce(new Error('permission denied'));

    await useGitStore.getState().stageFiles(['locked.bru']);

    expect(useGitStore.getState().error).toContain('permission denied');
  });
});

describe('branches', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      branches: { current: 'main', local: [], remote: [] },
    });
    vi.clearAllMocks();
  });

  it('switchBranch calls api and refreshes status and branches', async () => {
    const { gitSwitchBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().switchBranch('feature');

    expect(gitSwitchBranch).toHaveBeenCalledWith('/test/repo', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
  });

  it('createBranch calls api and refreshes branches', async () => {
    const { gitCreateBranch, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().createBranch('new-branch');

    expect(gitCreateBranch).toHaveBeenCalledWith('/test/repo', 'new-branch');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
  });

  it('deleteBranch calls api and refreshes branches', async () => {
    const { gitDeleteBranch, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().deleteBranch('old-branch');

    expect(gitDeleteBranch).toHaveBeenCalledWith('/test/repo', 'old-branch');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
  });

  it('mergeBranch calls api and refreshes status and branches', async () => {
    const { gitMergeBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().mergeBranch('feature');

    expect(gitMergeBranch).toHaveBeenCalledWith('/test/repo', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
  });

  it('switchBranch sets error on failure', async () => {
    const { gitSwitchBranch } = await import('@/lib/tauri-api');
    vi.mocked(gitSwitchBranch).mockRejectedValueOnce(new Error('branch not found'));

    await useGitStore.getState().switchBranch('nonexistent');

    expect(useGitStore.getState().error).toContain('branch not found');
  });

  it('checkoutRemoteBranch calls api and refreshes status and branches', async () => {
    const { gitCheckoutRemoteBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().checkoutRemoteBranch('origin/feature');

    expect(gitCheckoutRemoteBranch).toHaveBeenCalledWith('/test/repo', 'origin/feature');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitBranches).toHaveBeenCalledWith('/test/repo');
  });
});

describe('stash', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      stashes: [],
    });
    vi.clearAllMocks();
  });

  it('saveStash calls api and refreshes status and stashes', async () => {
    const { gitStashSave, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await useGitStore.getState().saveStash('WIP');

    expect(gitStashSave).toHaveBeenCalledWith('/test/repo', 'WIP');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitStashList).toHaveBeenCalledWith('/test/repo');
  });

  it('popStash calls api and refreshes status and stashes', async () => {
    const { gitStashPop, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await useGitStore.getState().popStash(0);

    expect(gitStashPop).toHaveBeenCalledWith('/test/repo', 0);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitStashList).toHaveBeenCalledWith('/test/repo');
  });

  it('applyStash calls api and refreshes status and stashes', async () => {
    const { gitStashApply, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await useGitStore.getState().applyStash(0);

    expect(gitStashApply).toHaveBeenCalledWith('/test/repo', 0);
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitStashList).toHaveBeenCalledWith('/test/repo');
  });

  it('dropStash calls api and refreshes stashes but not status', async () => {
    const { gitStashDrop, gitStashList, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().dropStash(0);

    expect(gitStashDrop).toHaveBeenCalledWith('/test/repo', 0);
    expect(gitStashList).toHaveBeenCalledWith('/test/repo');
    expect(gitStatus).not.toHaveBeenCalled();
  });
});

describe('remotes', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      error: null,
      remotes: [],
    });
    vi.clearAllMocks();
  });

  it('addRemote calls api and refreshes remotes', async () => {
    const { gitAddRemote, gitListRemotes } = await import('@/lib/tauri-api');

    await useGitStore.getState().addRemote('upstream', 'https://github.com/org/repo.git');

    expect(gitAddRemote).toHaveBeenCalledWith(
      '/test/repo',
      'upstream',
      'https://github.com/org/repo.git',
    );
    expect(gitListRemotes).toHaveBeenCalledWith('/test/repo');
  });

  it('removeRemote calls api and refreshes remotes', async () => {
    const { gitRemoveRemote, gitListRemotes } = await import('@/lib/tauri-api');

    await useGitStore.getState().removeRemote('upstream');

    expect(gitRemoveRemote).toHaveBeenCalledWith('/test/repo', 'upstream');
    expect(gitListRemotes).toHaveBeenCalledWith('/test/repo');
  });

  it('setRemoteUrl calls api and refreshes remotes', async () => {
    const { gitSetRemoteUrl, gitListRemotes } = await import('@/lib/tauri-api');

    await useGitStore.getState().setRemoteUrl('origin', 'https://github.com/org/new.git');

    expect(gitSetRemoteUrl).toHaveBeenCalledWith(
      '/test/repo',
      'origin',
      'https://github.com/org/new.git',
    );
    expect(gitListRemotes).toHaveBeenCalledWith('/test/repo');
  });

  it('addRemote sets error on failure', async () => {
    const { gitAddRemote } = await import('@/lib/tauri-api');
    vi.mocked(gitAddRemote).mockRejectedValueOnce(new Error('remote already exists'));

    await useGitStore.getState().addRemote('origin', 'https://github.com/org/repo.git');

    expect(useGitStore.getState().error).toContain('remote already exists');
  });
});

describe('conflicts', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: '/test/repo',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      conflicts: [],
    });
    vi.clearAllMocks();
  });

  it('resolveConflict calls api and refreshes status and conflicts', async () => {
    const { gitResolveConflict, gitStatus, gitConflicts } = await import('@/lib/tauri-api');

    await useGitStore.getState().resolveConflict('foo.bru', { resolution: 'ours' });

    expect(gitResolveConflict).toHaveBeenCalledWith('/test/repo', 'foo.bru', {
      resolution: 'ours',
    });
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitConflicts).toHaveBeenCalledWith('/test/repo');
  });

  it('abortMerge calls api and refreshes status and conflicts', async () => {
    const { gitAbortMerge, gitStatus, gitConflicts } = await import('@/lib/tauri-api');

    await useGitStore.getState().abortMerge();

    expect(gitAbortMerge).toHaveBeenCalledWith('/test/repo');
    expect(gitStatus).toHaveBeenCalledWith('/test/repo');
    expect(gitConflicts).toHaveBeenCalledWith('/test/repo');
  });
});

describe('reset', () => {
  beforeEach(() => {
    useGitStore.getState().reset();
    vi.clearAllMocks();
  });

  it('reset clears all state to initial values including pendingNetworkOp', () => {
    useGitStore.setState({
      isRepo: true,
      collectionPath: '/some/path',
      error: 'some error',
      credentials: { type: 'sshAgent' },
      showCredentialsDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'https://example.com' }],
      branches: { current: 'main', local: [], remote: [] },
    });

    useGitStore.getState().reset();

    const s = useGitStore.getState();
    expect(s.isRepo).toBe(false);
    expect(s.collectionPath).toBeNull();
    expect(s.status).toBeNull();
    expect(s.conflicts).toEqual([]);
    expect(s.stashes).toEqual([]);
    expect(s.branches).toBeNull();
    expect(s.remotes).toEqual([]);
    expect(s.commitLog).toEqual([]);
    expect(s.error).toBeNull();
    expect(s.credentials).toBeNull();
    expect(s.showCredentialsDialog).toBe(false);
    expect(s.pendingNetworkOp).toBeNull();
  });
});

describe('git-store stash batch operations', () => {
  beforeEach(() => {
    useGitStore.setState({
      error: null,
      collectionPath: '/test/repo',
      isRepo: true,
      stashes: [],
    });
    vi.clearAllMocks();
  });

  it('applyStashMany applies indices in ascending order (newest first)', async () => {
    const { gitStashApply } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashApply).mockImplementation(async (_path, index) => {
      order.push(index);
    });

    await useGitStore.getState().applyStashMany([2, 0, 1]);

    expect(order).toEqual([0, 1, 2]);
  });

  it('applyStashMany stops on first error and sets error with stash index', async () => {
    const { gitStashApply } = await import('@/lib/tauri-api');
    vi.mocked(gitStashApply)
      .mockResolvedValueOnce(undefined) // index 0 succeeds
      .mockRejectedValueOnce(new Error('conflict')); // index 1 fails

    await useGitStore.getState().applyStashMany([0, 1, 2]);

    expect(vi.mocked(gitStashApply)).toHaveBeenCalledTimes(2);
    expect(useGitStore.getState().error).toContain('stash@{1}');
    expect(useGitStore.getState().error).toContain('conflict');
  });

  it('applyStashMany refreshes stashes and status after completion', async () => {
    const { gitStashApply, gitStashList, gitStatus } = await import('@/lib/tauri-api');
    vi.mocked(gitStashApply).mockResolvedValue(undefined);

    await useGitStore.getState().applyStashMany([0]);

    expect(vi.mocked(gitStashList)).toHaveBeenCalled();
    expect(vi.mocked(gitStatus)).toHaveBeenCalled();
  });

  it('popStashMany applies indices in descending order (oldest first)', async () => {
    const { gitStashPop } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashPop).mockImplementation(async (_path, index) => {
      order.push(index);
    });

    await useGitStore.getState().popStashMany([1, 0]);

    expect(order).toEqual([1, 0]);
  });

  it('dropStashMany applies indices in descending order (oldest first)', async () => {
    const { gitStashDrop } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashDrop).mockImplementation(async (_path, index) => {
      order.push(index);
    });

    await useGitStore.getState().dropStashMany([2, 0, 1]);

    expect(order).toEqual([2, 1, 0]);
  });

  it('dropStashMany does not call gitStatus (drop is not a working-tree change)', async () => {
    const { gitStashDrop, gitStatus } = await import('@/lib/tauri-api');
    vi.mocked(gitStashDrop).mockResolvedValue(undefined);
    vi.mocked(gitStatus).mockClear();

    await useGitStore.getState().dropStashMany([0]);

    expect(vi.mocked(gitStatus)).not.toHaveBeenCalled();
  });
});

describe('git-store credential auto-load', () => {
  beforeEach(() => {
    useGitStore.setState({
      isRepo: false,
      collectionPath: null,
      credentials: null,
      status: null,
      branches: null,
      remotes: [],
      stashes: [],
      commitLog: [],
      conflicts: [],
      loading: false,
      error: null,
      showCredentialsDialog: false,
      pendingNetworkOp: null,
    });
    vi.clearAllMocks();
  });

  it('auto-loads saved credentials from keychain when collection is a repo', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    const savedCreds = {
      type: 'sshKey',
      privateKeyPath: '~/.ssh/id_ed25519',
      passphrase: undefined,
    };
    vi.mocked(gitIsRepo).mockResolvedValue(true);
    vi.mocked(loadGitCredentials).mockResolvedValue(savedCreds as unknown as GitCredentials);

    await useGitStore.getState().setCollection('/some/path');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('ws-test');
    expect(useGitStore.getState().credentials).toEqual(savedCreds);
  });

  it('leaves credentials null when keychain returns null', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockResolvedValue(true);
    vi.mocked(loadGitCredentials).mockResolvedValue(null);

    await useGitStore.getState().setCollection('/some/path');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('ws-test');
    expect(useGitStore.getState().credentials).toBeNull();
  });

  it('overwrites existing credentials with keychain result (always reloads on setCollection)', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    const existing = { type: 'token' as const, token: 'mytoken' };
    useGitStore.setState({ credentials: existing as unknown as GitCredentials });
    vi.mocked(loadGitCredentials).mockResolvedValue(null);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await useGitStore.getState().setCollection('/some/collection');

    // New design: always reloads workspace-scoped credentials on setCollection;
    // keychain returning null clears any previously-set in-memory credentials.
    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('ws-test');
    expect(useGitStore.getState().credentials).toBeNull();
  });
});

describe('git-store identity setup flow', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: null,
      credentials: null,
      showCredentialsDialog: false,
      showIdentitySetupDialog: false,
      identitySetupInitialName: '',
      identitySetupInitialEmail: '',
      pendingCredentialsForIdentitySetup: null,
      pendingNetworkOp: null,
      remotes: [],
      error: null,
    });
    vi.clearAllMocks();
  });

  it('setCredentials with sshKey and collectionPath shows identity setup dialog', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockResolvedValue({ name: 'Snehal', email: 'snehal@example.com' });
    useGitStore.setState({ collectionPath: '/some/repo' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = useGitStore.getState();
    expect(state.credentials).toBeNull();
    expect(state.showCredentialsDialog).toBe(false);
    expect(state.showIdentitySetupDialog).toBe(true);
    expect(state.pendingCredentialsForIdentitySetup).toEqual(creds);
    expect(state.identitySetupInitialName).toBe('Snehal');
    expect(state.identitySetupInitialEmail).toBe('snehal@example.com');
  });

  it('setCredentials with sshKey but no collectionPath activates immediately', () => {
    useGitStore.setState({ collectionPath: null });
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with token creds activates immediately without identity dialog', () => {
    useGitStore.setState({ collectionPath: '/some/repo' });
    const creds: GitCredentials = { type: 'token', token: 'ghp_xxx' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with sshKey falls back to immediate activation when gitGetIdentity throws', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockRejectedValue(new Error('no repo'));
    useGitStore.setState({ collectionPath: '/some/repo' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('activatePendingCredentials sets credentials and clears identity setup state', () => {
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.setState({
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      identitySetupInitialName: 'Snehal',
      identitySetupInitialEmail: 'snehal@example.com',
      pendingNetworkOp: null,
    });

    useGitStore.getState().activatePendingCredentials();

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
    expect(state.pendingCredentialsForIdentitySetup).toBeNull();
    expect(state.identitySetupInitialName).toBe('');
    expect(state.identitySetupInitialEmail).toBe('');
  });

  it('activatePendingCredentials retries pending push after activating', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockResolvedValue(undefined);
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.setState({
      collectionPath: '/some/repo',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'git@github.com:test/test.git' }],
    });

    useGitStore.getState().activatePendingCredentials();
    await new Promise((r) => setTimeout(r, 0));

    expect(vi.mocked(gitPush)).toHaveBeenCalledWith('/some/repo', 'origin', creds);
  });

  it('activatePendingCredentials is a no-op when no pending credentials exist', () => {
    useGitStore.setState({
      credentials: { type: 'token', token: 'existing' } as unknown as GitCredentials,
    });

    useGitStore.getState().activatePendingCredentials();

    // Should not overwrite existing credentials
    expect(useGitStore.getState().credentials).toEqual({ type: 'token', token: 'existing' });
  });
});
