import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { StoreApi } from 'zustand/vanilla';
import type { GitCredentials } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { createGitStore, type GitState } from '../git-store';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  // Keep real pure helpers (parseGitNetworkError, isGitSshTrustFailure, types)
  // so push/pull/fetch failure handling exercises actual parsing logic.
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
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

let store: StoreApi<GitState>;

beforeEach(() => {
  vi.resetAllMocks();
  store = createGitStore();

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

describe('git-store loadStatus', () => {
  it('sets loadStatus to "error" (not "not-repo") when gitIsRepo itself fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockRejectedValue(new Error('disk unreadable'));

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('error');
    expect(store.getState().error).toBe('Error: disk unreadable');
    expect(store.getState().isRepo).toBe(false);
  });

  it('sets loadStatus to "ready" (not "error") when the repo loads but a later refresh fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitStatus).mockRejectedValue(new Error('status unavailable'));

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('ready');
    expect(store.getState().isRepo).toBe(true);
    expect(store.getState().error).toBe('Error: status unavailable');
  });

  it('sets loadStatus to "not-repo" when gitIsRepo cleanly resolves false', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(false);

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('not-repo');
    expect(store.getState().error).toBeNull();
  });
});

describe('git-store clearError', () => {
  beforeEach(() => {
    store.setState({
      error: null,
      repositoryId: null,
      credentials: null,
      remotes: [],
    });
    vi.clearAllMocks();
  });

  it('clearError sets error to null', () => {
    store.setState({ error: 'previous error' });
    store.getState().clearError();
    expect(store.getState().error).toBeNull();
  });

  it('push clears stale error before executing', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockResolvedValueOnce(undefined);

    store.setState({
      error: 'stale error',
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await store.getState().push();

    expect(store.getState().error).toBeNull();
  });

  it('pull clears stale error before executing', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    store.setState({
      error: 'stale error',
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await store.getState().pull();

    expect(store.getState().error).toBeNull();
  });

  it('fetch clears stale error before executing', async () => {
    const { gitFetch } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockResolvedValueOnce({
      updatedRefs: [],
      receivedObjects: 0,
      receivedBytes: 0,
    });

    store.setState({
      error: 'stale error',
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await store.getState().fetch();

    expect(store.getState().error).toBeNull();
  });

  it('push sets error when operation fails', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockRejectedValueOnce(new Error('NotFastForward'));

    store.setState({
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await store.getState().push();

    expect(store.getState().error).toContain('NotFastForward');
  });
});

describe('git-store SSH trust failures', () => {
  beforeEach(() => {
    store.setState({
      error: null,
      trustFailure: null,
      repositoryId: 'repository-test',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
      pendingNetworkOp: null,
    });
    vi.clearAllMocks();
  });

  it('push sets trustFailure (not pendingNetworkOp) for an unknown SSH host', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockRejectedValueOnce({
      code: 'sshUnknownHost',
      message:
        'Unknown SSH host git.example.com:22 (algorithm ssh-ed25519, fingerprint SHA256:abc)',
      host: 'git.example.com',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:abc',
    });

    await store.getState().push();

    expect(store.getState().trustFailure).toEqual({
      code: 'sshUnknownHost',
      message:
        'Unknown SSH host git.example.com:22 (algorithm ssh-ed25519, fingerprint SHA256:abc)',
      host: 'git.example.com',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:abc',
    });
    expect(store.getState().error).toContain('Unknown SSH host');
    // Never treated as a retriable auth error.
    expect(store.getState().pendingNetworkOp).toBeNull();
  });

  it('pull clears a stale trustFailure once the operation succeeds', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);
    store.setState({
      trustFailure: {
        code: 'sshHostKeyChanged',
        message: 'stale',
        host: 'git.example.com',
        port: 22,
        algorithm: 'ssh-ed25519',
        fingerprint: 'SHA256:stale',
      },
    });

    await store.getState().pull();

    expect(store.getState().trustFailure).toBeNull();
  });

  it('fetch still treats a real auth failure as retriable', async () => {
    const { gitFetch } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockRejectedValueOnce(
      new Error('authentication failed: check credentials'),
    );

    await store.getState().fetch();

    expect(store.getState().trustFailure).toBeNull();
    expect(store.getState().pendingNetworkOp).toBe('fetch');
  });
});

describe('git-store pull refreshes the commit log', () => {
  it('calls refreshLog after a successful pull', async () => {
    store.setState({
      repositoryId: 'repo-1',
      isRepo: true,
      credentials: { type: 'token', token: 'tok' },
    });
    vi.mocked(tauriApi.gitPull).mockResolvedValue(undefined);
    vi.mocked(tauriApi.gitLog).mockResolvedValue([]);

    await store.getState().pull();

    expect(tauriApi.gitLog).toHaveBeenCalledWith('repo-1', 50);
  });
});

describe('setRepository', () => {
  beforeEach(() => {
    store.setState({
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
    store.setState({
      isRepo: true,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.mocked(gitIsRepo).mockResolvedValueOnce(false);

    await store.getState().setRepository('repository-not-git');

    expect(store.getState().isRepo).toBe(false);
    expect(store.getState().status).toBeNull();
  });

  it('valid repository ID loads status, branches, remotes, and stashes', async () => {
    const { gitIsRepo, gitStatus, gitBranches, gitListRemotes, gitStashList } = await import(
      '@/lib/tauri-api'
    );
    vi.mocked(gitIsRepo).mockResolvedValueOnce(true);

    await store.getState().setRepository('repository-test');

    expect(store.getState().isRepo).toBe(true);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
    // Verify state was actually stored, not just that functions were called.
    expect(store.getState().status).not.toBeNull();
    expect(store.getState().loading).toBe(false);
  });

  it('gitIsRepo throwing sets error state', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockRejectedValueOnce(new Error('disk error'));

    await store.getState().setRepository('repository-test');

    expect(store.getState().error).toContain('disk error');
  });

  it('initRepo initializes and selects the repository ID', async () => {
    const { gitInit, gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitInit).mockResolvedValueOnce(undefined);
    vi.mocked(gitIsRepo).mockResolvedValueOnce(true);

    await store.getState().initRepo('repository-test');

    expect(gitInit).toHaveBeenCalledWith('repository-test');
    expect(gitIsRepo).toHaveBeenCalledWith('repository-test');
    expect(store.getState().repositoryId).toBe('repository-test');
  });
});

describe('git-store setRepository clears stale data synchronously', () => {
  it('clears prior repository data the instant a new load starts, before the new load resolves', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    await store.getState().setRepository('repo-a');
    // Sanity: repo-a actually loaded some data.
    expect(store.getState().isRepo).toBe(true);

    store.setState({
      status: { branch: 'repo-a-branch', files: [], ahead: 3, behind: 0, isClean: true },
      branches: { current: 'repo-a-branch', local: [], remote: [] },
      remotes: [{ name: 'origin', url: 'https://a.example.com' }],
      stashes: [
        {
          index: 0,
          message: 'wip',
          timestamp: '2026-01-01',
          filesChanged: 1,
          insertions: 1,
          deletions: 0,
          changedFiles: ['a'],
          branch: 'repo-a-branch',
        },
      ],
      commitLog: [
        {
          id: 'abc',
          fullId: 'abc123',
          message: 'm',
          author: 'a',
          authorEmail: 'a@a.com',
          timestamp: '2026-01-01',
          filesChanged: 1,
        },
      ],
      conflicts: [
        { path: 'config.yml', ours: 'version: 1', theirs: 'version: 2', ancestor: 'version: 0' },
      ],
      credentials: { type: 'token', token: 'repo-a-secret' },
    });

    const deferredIsRepo = createDeferred<boolean>();
    vi.mocked(tauriApi.gitIsRepo).mockReturnValue(deferredIsRepo.promise);

    const loadPromise = store.getState().setRepository('repo-b');

    // Before repo-b's gitIsRepo call even resolves, all of repo-a's data must
    // already be gone — not left visible until repo-b's refreshes complete.
    expect(store.getState().status).toBeNull();
    expect(store.getState().branches).toBeNull();
    expect(store.getState().remotes).toEqual([]);
    expect(store.getState().stashes).toEqual([]);
    expect(store.getState().commitLog).toEqual([]);
    expect(store.getState().conflicts).toEqual([]);
    expect(store.getState().credentials).toBeNull();
    expect(store.getState().isRepo).toBe(false);

    deferredIsRepo.resolve(false);
    await loadPromise;
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

    const loadA = store.getState().setRepository('repository-a');
    const loadB = store.getState().setRepository('repository-b');

    repoB.resolve(true);
    await loadB;
    repoA.resolve(true);
    await loadA;

    expect(store.getState()).toMatchObject({
      repositoryId: 'repository-b',
      isRepo: true,
      status: expect.objectContaining({ branch: 'branch-b' }),
      loading: false,
    });
  });
});

describe('pendingNetworkOp and setCredentials', () => {
  beforeEach(() => {
    store.setState({
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
    await store.getState().pull();

    expect(store.getState().showCredentialsDialog).toBe(true);
    expect(store.getState().pendingNetworkOp).toBe('pull');
  });

  it('setCredentials auto-retries pull and clears pendingNetworkOp', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    store.setState({ pendingNetworkOp: 'pull' });

    store.getState().setCredentials({ type: 'sshAgent' });

    await vi.waitFor(() => {
      expect(gitPull).toHaveBeenCalledWith('repository-test', 'origin', { type: 'sshAgent' });
    });

    expect(store.getState().pendingNetworkOp).toBeNull();
    expect(store.getState().showCredentialsDialog).toBe(false);
  });

  it('push without credentials sets pendingNetworkOp=push', async () => {
    await store.getState().push();

    expect(store.getState().showCredentialsDialog).toBe(true);
    expect(store.getState().pendingNetworkOp).toBe('push');
  });

  it('fetch without credentials sets pendingNetworkOp=fetch', async () => {
    await store.getState().fetch();

    expect(store.getState().showCredentialsDialog).toBe(true);
    expect(store.getState().pendingNetworkOp).toBe('fetch');
  });

  it('dismissing dialog clears pendingNetworkOp without retrying', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    store.setState({ pendingNetworkOp: 'push' });

    store.getState().setShowCredentialsDialog(false);

    expect(store.getState().pendingNetworkOp).toBeNull();
    expect(gitPush).not.toHaveBeenCalled();
  });

  it('reset clears pendingNetworkOp', () => {
    store.setState({ pendingNetworkOp: 'fetch' });

    store.getState().reset();

    expect(store.getState().pendingNetworkOp).toBeNull();
  });
});

describe('staging', () => {
  beforeEach(() => {
    store.setState({
      repositoryId: 'repository-test',
      isRepo: true,
      error: null,
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    });
    vi.clearAllMocks();
  });

  it('stageFiles calls gitStage and refreshes status', async () => {
    const { gitStage, gitStatus } = await import('@/lib/tauri-api');

    await store.getState().stageFiles(['foo.bru']);

    expect(gitStage).toHaveBeenCalledWith('repository-test', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
  });

  it('unstageFiles calls gitUnstage and refreshes status', async () => {
    const { gitUnstage, gitStatus } = await import('@/lib/tauri-api');

    await store.getState().unstageFiles(['foo.bru']);

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

    await store.getState().stageAll();

    expect(gitStage).toHaveBeenCalledWith('repository-test', ['unstaged-modified.bru']);
  });

  it('discardFiles calls gitDiscard and refreshes status', async () => {
    const { gitDiscard, gitStatus } = await import('@/lib/tauri-api');

    await store.getState().discardFiles(['foo.bru']);

    expect(gitDiscard).toHaveBeenCalledWith('repository-test', ['foo.bru']);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
  });

  it('commitChanges calls gitCommit and refreshes status', async () => {
    const { gitCommit, gitStatus } = await import('@/lib/tauri-api');

    await store.getState().commitChanges('initial commit');

    expect(gitCommit).toHaveBeenCalledWith('repository-test', 'initial commit');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
  });

  it('stageFiles sets error on failure', async () => {
    const { gitStage } = await import('@/lib/tauri-api');
    vi.mocked(gitStage).mockRejectedValueOnce(new Error('permission denied'));

    await store.getState().stageFiles(['locked.bru']);

    expect(store.getState().error).toContain('permission denied');
  });
});

describe('branches', () => {
  beforeEach(() => {
    store.setState({
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

    await store.getState().switchBranch('feature');

    expect(gitSwitchBranch).toHaveBeenCalledWith('repository-test', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('createBranch calls api and refreshes branches', async () => {
    const { gitCreateBranch, gitBranches } = await import('@/lib/tauri-api');

    await store.getState().createBranch('new-branch');

    expect(gitCreateBranch).toHaveBeenCalledWith('repository-test', 'new-branch');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('deleteBranch calls api and refreshes branches', async () => {
    const { gitDeleteBranch, gitBranches } = await import('@/lib/tauri-api');

    await store.getState().deleteBranch('old-branch');

    expect(gitDeleteBranch).toHaveBeenCalledWith('repository-test', 'old-branch');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('mergeBranch calls api and refreshes status and branches', async () => {
    const { gitMergeBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await store.getState().mergeBranch('feature');

    expect(gitMergeBranch).toHaveBeenCalledWith('repository-test', 'feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });

  it('switchBranch sets error on failure', async () => {
    const { gitSwitchBranch } = await import('@/lib/tauri-api');
    vi.mocked(gitSwitchBranch).mockRejectedValueOnce(new Error('branch not found'));

    await store.getState().switchBranch('nonexistent');

    expect(store.getState().error).toContain('branch not found');
  });

  it('checkoutRemoteBranch calls api and refreshes status and branches', async () => {
    const { gitCheckoutRemoteBranch, gitStatus, gitBranches } = await import('@/lib/tauri-api');

    await store.getState().checkoutRemoteBranch('origin/feature');

    expect(gitCheckoutRemoteBranch).toHaveBeenCalledWith('repository-test', 'origin/feature');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitBranches).toHaveBeenCalledWith('repository-test');
  });
});

describe('stash', () => {
  beforeEach(() => {
    store.setState({
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

    await store.getState().saveStash('WIP');

    expect(gitStashSave).toHaveBeenCalledWith('repository-test', 'WIP');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('popStash calls api and refreshes status and stashes', async () => {
    const { gitStashPop, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await store.getState().popStash(0);

    expect(gitStashPop).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('applyStash calls api and refreshes status and stashes', async () => {
    const { gitStashApply, gitStatus, gitStashList } = await import('@/lib/tauri-api');

    await store.getState().applyStash(0);

    expect(gitStashApply).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
  });

  it('dropStash calls api and refreshes stashes but not status', async () => {
    const { gitStashDrop, gitStashList, gitStatus } = await import('@/lib/tauri-api');

    await store.getState().dropStash(0);

    expect(gitStashDrop).toHaveBeenCalledWith('repository-test', 0);
    expect(gitStashList).toHaveBeenCalledWith('repository-test');
    expect(gitStatus).not.toHaveBeenCalled();
  });
});

describe('remotes', () => {
  beforeEach(() => {
    store.setState({
      repositoryId: 'repository-test',
      isRepo: true,
      error: null,
      remotes: [],
    });
    vi.clearAllMocks();
  });

  it('addRemote calls api and refreshes remotes', async () => {
    const { gitAddRemote, gitListRemotes } = await import('@/lib/tauri-api');

    await store.getState().addRemote('upstream', 'https://github.com/org/repo.git');

    expect(gitAddRemote).toHaveBeenCalledWith(
      'repository-test',
      'upstream',
      'https://github.com/org/repo.git',
    );
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
  });

  it('removeRemote calls api and refreshes remotes', async () => {
    const { gitRemoveRemote, gitListRemotes } = await import('@/lib/tauri-api');

    await store.getState().removeRemote('upstream');

    expect(gitRemoveRemote).toHaveBeenCalledWith('repository-test', 'upstream');
    expect(gitListRemotes).toHaveBeenCalledWith('repository-test');
  });

  it('setRemoteUrl calls api and refreshes remotes', async () => {
    const { gitSetRemoteUrl, gitListRemotes } = await import('@/lib/tauri-api');

    await store.getState().setRemoteUrl('origin', 'https://github.com/org/new.git');

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

    await store.getState().addRemote('origin', 'https://github.com/org/repo.git');

    expect(store.getState().error).toContain('remote already exists');
  });
});

describe('conflicts', () => {
  beforeEach(() => {
    store.setState({
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

    await store.getState().resolveConflict('foo.bru', { resolution: 'ours' });

    expect(gitResolveConflict).toHaveBeenCalledWith('repository-test', 'foo.bru', {
      resolution: 'ours',
    });
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitConflicts).toHaveBeenCalledWith('repository-test');
  });

  it('abortMerge calls api and refreshes status and conflicts', async () => {
    const { gitAbortMerge, gitStatus, gitConflicts } = await import('@/lib/tauri-api');

    await store.getState().abortMerge();

    expect(gitAbortMerge).toHaveBeenCalledWith('repository-test');
    expect(gitStatus).toHaveBeenCalledWith('repository-test');
    expect(gitConflicts).toHaveBeenCalledWith('repository-test');
  });
});

describe('reset', () => {
  beforeEach(() => {
    store.getState().reset();
    vi.clearAllMocks();
  });

  it('reset clears all state to initial values including pendingNetworkOp', () => {
    store.setState({
      isRepo: true,
      repositoryId: 'repository-some',
      error: 'some error',
      credentials: { type: 'sshAgent' },
      showCredentialsDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'https://example.com' }],
      branches: { current: 'main', local: [], remote: [] },
    });

    store.getState().reset();

    const s = store.getState();
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
    store.setState({
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

    await store.getState().applyStashMany([2, 0, 1]);

    expect(order).toEqual([0, 1, 2]);
  });

  it('applyStashMany stops on first error and sets error with stash index', async () => {
    const { gitStashApply } = await import('@/lib/tauri-api');
    vi.mocked(gitStashApply)
      .mockResolvedValueOnce(undefined) // index 0 succeeds
      .mockRejectedValueOnce(new Error('conflict')); // index 1 fails

    await store.getState().applyStashMany([0, 1, 2]);

    expect(vi.mocked(gitStashApply)).toHaveBeenCalledTimes(2);
    expect(store.getState().error).toContain('stash@{1}');
    expect(store.getState().error).toContain('conflict');
  });

  it('applyStashMany refreshes stashes and status after completion', async () => {
    const { gitStashApply, gitStashList, gitStatus } = await import('@/lib/tauri-api');
    vi.mocked(gitStashApply).mockResolvedValue(undefined);

    await store.getState().applyStashMany([0]);

    expect(vi.mocked(gitStashList)).toHaveBeenCalled();
    expect(vi.mocked(gitStatus)).toHaveBeenCalled();
  });

  it('popStashMany applies indices in descending order (oldest first)', async () => {
    const { gitStashPop } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashPop).mockImplementation(async (_repositoryId, index) => {
      order.push(index);
    });

    await store.getState().popStashMany([1, 0]);

    expect(order).toEqual([1, 0]);
  });

  it('dropStashMany applies indices in descending order (oldest first)', async () => {
    const { gitStashDrop } = await import('@/lib/tauri-api');
    const order: number[] = [];
    vi.mocked(gitStashDrop).mockImplementation(async (_repositoryId, index) => {
      order.push(index);
    });

    await store.getState().dropStashMany([2, 0, 1]);

    expect(order).toEqual([2, 1, 0]);
  });

  it('dropStashMany does not call gitStatus (drop is not a working-tree change)', async () => {
    const { gitStashDrop, gitStatus } = await import('@/lib/tauri-api');
    vi.mocked(gitStashDrop).mockResolvedValue(undefined);
    vi.mocked(gitStatus).mockClear();

    await store.getState().dropStashMany([0]);

    expect(vi.mocked(gitStatus)).not.toHaveBeenCalled();
  });
});

describe('git-store credential auto-load', () => {
  beforeEach(() => {
    store.setState({
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

    await store.getState().setRepository('repository-some');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(store.getState().credentials).toEqual(savedCreds);
  });

  it('leaves credentials null when keychain returns null', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    vi.mocked(gitIsRepo).mockResolvedValue(true);
    vi.mocked(loadGitCredentials).mockResolvedValue(null);

    await store.getState().setRepository('repository-some');

    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(store.getState().credentials).toBeNull();
  });

  it('overwrites existing credentials with keychain result (always reloads on setRepository)', async () => {
    const { loadGitCredentials, gitIsRepo } = await import('@/lib/tauri-api');
    const existing = { type: 'token' as const, token: 'mytoken' };
    store.setState({ credentials: existing as unknown as GitCredentials });
    vi.mocked(loadGitCredentials).mockResolvedValue(null);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await store.getState().setRepository('repository-some');

    // Always reload repository-scoped credentials on setRepository;
    // keychain returning null clears any previously-set in-memory credentials.
    expect(vi.mocked(loadGitCredentials)).toHaveBeenCalledWith('repository-some');
    expect(store.getState().credentials).toBeNull();
  });
});

describe('git-store identity setup flow', () => {
  beforeEach(() => {
    store.setState({
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
    store.setState({ repositoryId: 'repository-some' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    store.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = store.getState();
    expect(state.credentials).toBeNull();
    expect(state.showCredentialsDialog).toBe(false);
    expect(state.showIdentitySetupDialog).toBe(true);
    expect(state.pendingCredentialsForIdentitySetup).toEqual(creds);
    expect(state.identitySetupInitialName).toBe('Snehal');
    expect(state.identitySetupInitialEmail).toBe('snehal@example.com');
  });

  it('setCredentials with sshKey but no repositoryId activates immediately', () => {
    store.setState({ repositoryId: null });
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };

    store.getState().setCredentials(creds);

    const state = store.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with token creds activates immediately without identity dialog', () => {
    store.setState({ repositoryId: 'repository-some' });
    const creds: GitCredentials = { type: 'token', token: 'ghp_xxx' };

    store.getState().setCredentials(creds);

    const state = store.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with sshKey falls back to immediate activation when gitGetIdentity throws', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockRejectedValue(new Error('no repo'));
    store.setState({ repositoryId: 'repository-some' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    store.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = store.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('activatePendingCredentials sets credentials and clears identity setup state', () => {
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    store.setState({
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      identitySetupInitialName: 'Snehal',
      identitySetupInitialEmail: 'snehal@example.com',
      pendingNetworkOp: null,
    });

    store.getState().activatePendingCredentials();

    const state = store.getState();
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
    store.setState({
      repositoryId: 'repository-some',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'git@github.com:test/test.git' }],
    });

    store.getState().activatePendingCredentials();
    await new Promise((r) => setTimeout(r, 0));

    expect(vi.mocked(gitPush)).toHaveBeenCalledWith('repository-some', 'origin', creds);
  });

  it('activatePendingCredentials is a no-op when no pending credentials exist', () => {
    store.setState({
      credentials: { type: 'token', token: 'existing' } as unknown as GitCredentials,
    });

    store.getState().activatePendingCredentials();

    // Should not overwrite existing credentials
    expect(store.getState().credentials).toEqual({ type: 'token', token: 'existing' });
  });

  it('cancelling identity setup does not activate credentials or retry the pending operation', async () => {
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '/home/user/.ssh/id_ed25519' };
    store.setState({
      repositoryId: 'repo-1',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      identitySetupInitialName: 'Some Name',
      identitySetupInitialEmail: 'some@example.com',
      pendingNetworkOp: 'push',
    });

    store.getState().discardPendingIdentitySetup();

    const state = store.getState();
    expect(state.credentials).toBeNull();
    expect(state.showIdentitySetupDialog).toBe(false);
    expect(state.pendingCredentialsForIdentitySetup).toBeNull();
    expect(state.pendingNetworkOp).toBeNull();
    expect(state.identitySetupInitialName).toBe('');
    expect(state.identitySetupInitialEmail).toBe('');
    expect(tauriApi.gitPush).not.toHaveBeenCalled();
  });
});

describe('setRepository generation guard', () => {
  it('does not let a slower first call overwrite a faster second call', async () => {
    const { gitIsRepo } = await import('@/lib/tauri-api');
    const gitIsRepoMock = vi.mocked(gitIsRepo);
    let resolveFirst!: (value: boolean) => void;
    gitIsRepoMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveFirst = resolve;
        }),
    );
    gitIsRepoMock.mockResolvedValueOnce(true);

    const firstCall = store.getState().setRepository('repo-a');
    // Let the second call's setRepository start and fully resolve before the first does.
    await store.getState().setRepository('repo-b');
    expect(store.getState().repositoryId).toBe('repo-b');

    // Now let the slow first call resolve. It must not clobber repo-b's state.
    resolveFirst(true);
    await firstCall;
    expect(store.getState().repositoryId).toBe('repo-b');
  });

  it('does not let a stale refreshBranches response from a superseded repository overwrite the current one', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    const staleBranches = createDeferred<tauriApi.BranchList>();
    // repo-a's own refreshBranches() call (fired inside setRepository's
    // Promise.all) hangs; repo-b's setRepository must be free to complete
    // fully in the meantime. Keyed on the argument (not call order) so the
    // test doesn't depend on the exact microtask interleaving of two
    // concurrently-running setRepository() calls.
    vi.mocked(tauriApi.gitBranches).mockImplementation((repositoryId) =>
      repositoryId === 'repo-a'
        ? staleBranches.promise
        : Promise.resolve({ current: 'main-b', local: [], remote: [] }),
    );

    const firstCall = store.getState().setRepository('repo-a');
    await store.getState().setRepository('repo-b');
    expect(store.getState().branches?.current).toBe('main-b');

    // The stale repo-a branch list arrives after repo-b is already active.
    // It must be discarded, not written over repo-b's branches.
    staleBranches.resolve({ current: 'main-a', local: [], remote: [] });
    await firstCall;
    expect(store.getState().repositoryId).toBe('repo-b');
    expect(store.getState().branches?.current).toBe('main-b');
  });
});
