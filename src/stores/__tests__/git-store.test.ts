import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { GitCredentials } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { useGitStore } from '../git-store';

vi.mock('@/lib/tauri-api', () => ({
  gitIsRepo: vi.fn(),
  gitInit: vi.fn().mockResolvedValue(undefined),
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

const knownRedDescribe = process.env.GIT_SAFETY_CONTRACTS === '1' ? describe : describe.skip;

beforeEach(() => {
  vi.resetAllMocks();
  useGitStore.setState(useGitStore.getInitialState(), true);

  vi.mocked(tauriApi.gitStatus).mockResolvedValue({
    branch: 'main',
    files: [],
    ahead: 0,
    behind: 0,
    isClean: true,
  });
  vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
  vi.mocked(tauriApi.gitListRemotes).mockResolvedValue([]);
  vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);
  vi.mocked(tauriApi.gitLog).mockResolvedValue([]);
  vi.mocked(tauriApi.gitConflicts).mockResolvedValue([]);
  vi.mocked(tauriApi.gitCommit).mockResolvedValue({
    id: 'abc1234',
    fullId: 'abc1234abc1234',
    message: 'test commit',
    author: 'Test',
    authorEmail: 'test@test.com',
    timestamp: '2026-01-01',
    filesChanged: 1,
  });
  vi.mocked(tauriApi.loadGitCredentials).mockResolvedValue(null);
  vi.mocked(tauriApi.gitGetIdentity).mockResolvedValue({
    name: 'Test User',
    email: 'test@example.com',
  });
});

describe('git-store clearError', () => {
  beforeEach(() => {
    useGitStore.setState({
      error: null,
      repositoryId: null,
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
      repositoryId: 'repository-test',
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
      repositoryId: 'repository-test',
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
      repositoryId: 'repository-test',
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
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().push();

    expect(useGitStore.getState().error).toContain('NotFastForward');
  });
});

describe('setRepository', () => {
  beforeEach(() => {
    useGitStore.setState({
      repositoryId: null,
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

  it('non-repository ID sets isRepo=false and status=null', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    // Seed non-default state so assertions are meaningful.
    useGitStore.setState({
      isRepo: true,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.mocked(gitIsRepo).mockResolvedValueOnce(false);

    await useGitStore.getState().setRepository('repository-not-git');

    expect(useGitStore.getState().isRepo).toBe(false);
    expect(useGitStore.getState().status).toBeNull();
  });

  it('valid repository ID loads status, branches, remotes, and stashes', async () => {
    const { gitIsRepo, gitStatus, gitBranches, gitListRemotes, gitStashList } = await import(
      '@/lib/tauri-api'
    );
    vi.mocked(gitIsRepo).mockResolvedValueOnce(true);

    await useGitStore.getState().setRepository('repository-test');

    expect(useGitStore.getState().isRepo).toBe(true);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
    // Verify state was actually stored, not just that functions were called.
    expect(useGitStore.getState().status).not.toBeNull();
    expect(useGitStore.getState().loading).toBe(false);
  });

  it('gitIsRepo throwing sets error state', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockRejectedValueOnce(new Error('disk error'));

    await useGitStore.getState().setRepository('repository-test');

    expect(useGitStore.getState().error).toContain('disk error');
  });

  it('initRepo initializes and selects the repository ID', async () => {
    const { gitInit, gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitInit).mockResolvedValueOnce(undefined);
    vi.mocked(gitIsRepo).mockResolvedValueOnce(true);

    await useGitStore.getState().initRepo('repository-test');

    expect(gitInit).toHaveBeenCalledWith('repository-test');
    expect(gitIsRepo).toHaveBeenCalledWith('repository-test');
    expect(useGitStore.getState().repositoryId).toBe('repository-test');
  });
});

// Phase 3 repository-scoping work will make late responses unable to overwrite the active repo.
knownRedDescribe('known-red: setRepository repository race contracts', () => {
  it('keeps B authoritative when delayed A resolves after B', async () => {
    const { gitIsRepo, gitStatus } = await import('@/lib/tauri-api');
    const repoA = createDeferred<boolean>();
    const repoB = createDeferred<boolean>();

    vi.mocked(gitIsRepo).mockImplementation((repositoryId) => {
      if (repositoryId === 'repository-a') return repoA.promise;
      if (repositoryId === 'repository-b') return repoB.promise;
      throw new Error(`Unexpected repository ID: ${repositoryId}`);
    });
    vi.mocked(gitStatus).mockImplementation(async (repositoryId) => ({
      branch: repositoryId === 'repository-a' ? 'branch-a' : 'branch-b',
      files: [],
      ahead: 0,
      behind: 0,
      isClean: true,
    }));

    const loadA = useGitStore.getState().setRepository('repository-a');
    const loadB = useGitStore.getState().setRepository('repository-b');

    repoB.resolve(true);
    await loadB;
    repoA.resolve(true);
    await loadA;

    expect(useGitStore.getState()).toMatchObject({
      repositoryId: 'repository-b',
      isRepo: true,
      status: expect.objectContaining({ branch: 'branch-b' }),
      loading: false,
    });
  });
});

describe('pendingNetworkOp and setCredentials', () => {
  beforeEach(() => {
    useGitStore.setState({
      repositoryId: 'repository-test',
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
      expect(gitPull).toHaveBeenCalledWith('repository-test', 'origin', { type: 'sshAgent' });
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
      repositoryId: 'repository-test',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.clearAllMocks();
  });

  it('stageFiles calls gitStage and refreshes status', async () => {
    const { gitStage, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().stageFiles(['foo.bru']);

    expect(gitStage).toHaveBeenCalledWith('repository-test', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
  });

  it('unstageFiles calls gitUnstage and refreshes status', async () => {
    const { gitUnstage, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().unstageFiles(['foo.bru']);

    expect(gitUnstage).toHaveBeenCalledWith('repository-test', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
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

    expect(gitStage).toHaveBeenCalledWith('repository-test', ['unstaged-modified.bru']);
  });

  it('discardFiles calls gitDiscard and refreshes status', async () => {
    const { gitDiscard, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().discardFiles(['foo.bru']);

    expect(gitDiscard).toHaveBeenCalledWith('repository-test', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
  });

  it('commitChanges calls gitCommit and refreshes status', async () => {
    const { gitCommit, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().commitChanges('initial commit');

    expect(gitCommit).toHaveBeenCalledWith('repository-test', 'initial commit');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
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
      repositoryId: 'repository-test',
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

    expect(gitSwitchBranch).toHaveBeenCalledWith('repository-test', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('createBranch calls api and refreshes branches', async () => {
    const { gitCreateBranch, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().createBranch('new-branch');

    expect(gitCreateBranch).toHaveBeenCalledWith('repository-test', 'new-branch');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('deleteBranch calls api and refreshes branches', async () => {
    const { gitDeleteBranch, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().deleteBranch('old-branch');

    expect(gitDeleteBranch).toHaveBeenCalledWith('repository-test', 'old-branch');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('mergeBranch calls api and refreshes status and branches', async () => {
    const { gitMergeBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await useGitStore.getState().mergeBranch('feature');

    expect(gitMergeBranch).toHaveBeenCalledWith('repository-test', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
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

    expect(gitCheckoutRemoteBranch).toHaveBeenCalledWith('repository-test', 'origin/feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });
});

describe('stash', () => {
  beforeEach(() => {
    useGitStore.setState({
      repositoryId: 'repository-test',
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

    expect(gitStashSave).toHaveBeenCalledWith('repository-test', 'WIP');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('popStash calls api and refreshes status and stashes', async () => {
    const { gitStashPop, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await useGitStore.getState().popStash(0);

    expect(gitStashPop).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('applyStash calls api and refreshes status and stashes', async () => {
    const { gitStashApply, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await useGitStore.getState().applyStash(0);

    expect(gitStashApply).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('dropStash calls api and refreshes stashes but not status', async () => {
    const { gitStashDrop, gitStashList, gitStatus } = await import('@/lib/tauri-api');

    await useGitStore.getState().dropStash(0);

    expect(gitStashDrop).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
    expect(gitStatus).not.toHaveBeenCalled();
  });
});

describe('remotes', () => {
  beforeEach(() => {
    useGitStore.setState({
      repositoryId: 'repository-test',
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
      'repository-test',
      'upstream',
      'https://github.com/org/repo.git',
    );
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
  });

  it('removeRemote calls api and refreshes remotes', async () => {
    const { gitRemoveRemote, gitListRemotes } = await import('@/lib/tauri-api');

    await useGitStore.getState().removeRemote('upstream');

    expect(gitRemoveRemote).toHaveBeenCalledWith('repository-test', 'upstream');
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
  });

  it('setRemoteUrl calls api and refreshes remotes', async () => {
    const { gitSetRemoteUrl, gitListRemotes } = await import('@/lib/tauri-api');

    await useGitStore.getState().setRemoteUrl('origin', 'https://github.com/org/new.git');

    expect(gitSetRemoteUrl).toHaveBeenCalledWith(
      'repository-test',
      'origin',
      'https://github.com/org/new.git',
    );
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
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
      repositoryId: 'repository-test',
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

    expect(gitResolveConflict).toHaveBeenCalledWith('repository-test', 'foo.bru', {
      resolution: 'ours',
    });
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitConflicts).toHaveBeenCalledWith('repository-test');
  });

  it('abortMerge calls api and refreshes status and conflicts', async () => {
    const { gitAbortMerge, gitStatus, gitConflicts } = await import('@/lib/tauri-api');

    await useGitStore.getState().abortMerge();

    expect(gitAbortMerge).toHaveBeenCalledWith('repository-test');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitConflicts).toHaveBeenCalledWith('repository-test');
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
      repositoryId: 'repository-some',
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
    expect(s.repositoryId).toBeNull();
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
      repositoryId: 'repository-test',
      isRepo: true,
      stashes: [],
    });
    vi.clearAllMocks();
  });

  it('applyStashMany applies indices in ascending order (newest first)', async () => {
    const { gitStashApply } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashApply).mockImplementation(async (_repositoryId, index) => {
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
    vi.mocked(gitStashPop).mockImplementation(async (_repositoryId, index) => {
      order.push(index);
    });

    await useGitStore.getState().popStashMany([1, 0]);

    expect(order).toEqual([1, 0]);
  });

  it('dropStashMany applies indices in descending order (oldest first)', async () => {
    const { gitStashDrop } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashDrop).mockImplementation(async (_repositoryId, index) => {
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
      repositoryId: null,
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

  it('auto-loads saved credentials from keychain when repository ID is a repo', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    const savedCreds = {
      type: 'sshKey',
      privateKeyPath: '~/.ssh/id_ed25519',
      passphrase: undefined,
    };
    vi.mocked(gitIsRepo).mockResolvedValue(true);
    vi.mocked(loadGitCredentials).mockResolvedValue(savedCreds as unknown as GitCredentials);

    await useGitStore.getState().setRepository('repository-some');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(useGitStore.getState().credentials).toEqual(savedCreds);
  });

  it('leaves credentials null when keychain returns null', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockResolvedValue(true);
    vi.mocked(loadGitCredentials).mockResolvedValue(null);

    await useGitStore.getState().setRepository('repository-some');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(useGitStore.getState().credentials).toBeNull();
  });

  it('overwrites existing credentials with keychain result (always reloads on setRepository)', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    const existing = { type: 'token' as const, token: 'mytoken' };
    useGitStore.setState({ credentials: existing as unknown as GitCredentials });
    vi.mocked(loadGitCredentials).mockResolvedValue(null);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await useGitStore.getState().setRepository('repository-some');

    // Always reload repository-scoped credentials on setRepository;
    // keychain returning null clears any previously-set in-memory credentials.
    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(useGitStore.getState().credentials).toBeNull();
  });
});

describe('git-store identity setup flow', () => {
  beforeEach(() => {
    useGitStore.setState({
      repositoryId: null,
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

  it('setCredentials with sshKey and repositoryId shows identity setup dialog', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockResolvedValue({ name: 'Snehal', email: 'snehal@example.com' });
    useGitStore.setState({ repositoryId: 'repository-some' });

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

  it('setCredentials with sshKey but no repositoryId activates immediately', () => {
    useGitStore.setState({ repositoryId: null });
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with token creds activates immediately without identity dialog', () => {
    useGitStore.setState({ repositoryId: 'repository-some' });
    const creds: GitCredentials = { type: 'token', token: 'ghp_xxx' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with sshKey falls back to immediate activation when gitGetIdentity throws', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockRejectedValue(new Error('no repo'));
    useGitStore.setState({ repositoryId: 'repository-some' });

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
      repositoryId: 'repository-some',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'git@github.com:test/test.git' }],
    });

    useGitStore.getState().activatePendingCredentials();
    await new Promise((r) => setTimeout(r, 0));

    expect(vi.mocked(gitPush)).toHaveBeenCalledWith('repository-some', 'origin', creds);
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
