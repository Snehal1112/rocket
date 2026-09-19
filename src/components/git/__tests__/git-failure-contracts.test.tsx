import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import { createDeferred } from '@/test/deferred';
import { GitCloneDialog } from '../GitCloneDialog';
import { GitCommitForm } from '../GitCommitForm';
import { GitLandingPanel } from '../GitLandingPanel';
import { GitRemotesDialog } from '../GitRemotesDialog';

const workspaceQueries = vi.hoisted(() => ({
  openFromDisk: vi.fn(),
  switchWorkspace: vi.fn(),
}));

vi.mock('@/lib/queries/workspace-queries', () => ({
  useOpenWorkspaceFromDisk: () => ({ mutateAsync: workspaceQueries.openFromDisk }),
  useSwitchWorkspace: () => ({ mutate: workspaceQueries.switchWorkspace }),
}));

vi.mock('@/lib/tauri-api', () => ({
  detectClonedStructure: vi.fn(),
  gitAbortMerge: vi.fn(),
  gitAddRemote: vi.fn(),
  gitBranches: vi.fn(),
  gitCheckoutRemoteBranch: vi.fn(),
  gitClone: vi.fn(),
  gitCommit: vi.fn(),
  gitConflicts: vi.fn(),
  gitCreateBranch: vi.fn(),
  gitDeleteBranch: vi.fn(),
  gitDiscard: vi.fn(),
  gitFetch: vi.fn(),
  gitGetIdentity: vi.fn(),
  gitInit: vi.fn(),
  gitIsRepo: vi.fn(),
  gitListRemotes: vi.fn(),
  gitLog: vi.fn(),
  gitMergeBranch: vi.fn(),
  gitPull: vi.fn(),
  gitPush: vi.fn(),
  gitRemoveRemote: vi.fn(),
  gitResolveConflict: vi.fn(),
  gitSetRemoteUrl: vi.fn(),
  gitStage: vi.fn(),
  gitStashApply: vi.fn(),
  gitStashDrop: vi.fn(),
  gitStashList: vi.fn(),
  gitStashPop: vi.fn(),
  gitStashSave: vi.fn(),
  gitStatus: vi.fn(),
  gitSwitchBranch: vi.fn(),
  gitUnstage: vi.fn(),
  loadGitCredentials: vi.fn(),
  openFolderPicker: vi.fn(),
  selectCloneDestination: vi.fn(),
}));

const knownRedDescribe = process.env.GIT_SAFETY_CONTRACTS === '1' ? describe : describe.skip;

const cleanStatus: tauriApi.RepoStatus = {
  branch: 'main',
  files: [],
  ahead: 0,
  behind: 0,
  isClean: true,
};

function setRepositoryState(overrides: Partial<ReturnType<typeof useGitStore.getState>> = {}) {
  useGitStore.setState({
    collectionPath: '/collections/active',
    isRepo: true,
    credentials: { type: 'sshAgent' },
    remotes: [{ name: 'origin', url: 'git@example.com:team/repo.git' }],
    status: cleanStatus,
    ...overrides,
  });
}

async function fillCloneForm() {
  vi.mocked(tauriApi.selectCloneDestination).mockResolvedValue({
    capability: 'clone-capability',
    displayPath: '/tmp/repo',
    expiresInSeconds: 300,
  });
  fireEvent.change(screen.getByPlaceholderText('https://github.com/user/repo.git'), {
    target: { value: 'https://example.com/team/repo.git' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Browse' }));
  await waitFor(() => expect(screen.getByDisplayValue('/tmp/repo')).toBeInTheDocument());
}

beforeEach(() => {
  vi.resetAllMocks();
  useGitStore.setState(useGitStore.getInitialState(), true);

  vi.mocked(tauriApi.gitStatus).mockResolvedValue(cleanStatus);
  vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
  vi.mocked(tauriApi.gitListRemotes).mockResolvedValue([]);
  vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);
  vi.mocked(tauriApi.gitLog).mockResolvedValue([]);
  vi.mocked(tauriApi.gitConflicts).mockResolvedValue([]);
  vi.mocked(tauriApi.gitGetIdentity).mockResolvedValue({
    name: 'Test User',
    email: 'test@example.com',
  });
  vi.mocked(tauriApi.loadGitCredentials).mockResolvedValue(null);
  vi.mocked(tauriApi.detectClonedStructure).mockResolvedValue({
    kind: 'unknown',
    workspacePath: null,
    collections: [],
  });
  workspaceQueries.openFromDisk.mockResolvedValue({ id: 'workspace-id' });
});

afterEach(() => {
  cleanup();
});

// Phase 2.2 will make failed store actions observable to callers.
knownRedDescribe('known-red: Git UI failure-chain contracts', () => {
  it('failed stash prevents pull', async () => {
    vi.mocked(tauriApi.gitStashSave).mockRejectedValueOnce(new Error('stash failed'));
    setRepositoryState({ status: { ...cleanStatus, isClean: false } });
    render(<GitLandingPanel />);

    fireEvent.click(screen.getByRole('button', { name: /^pull/i }));
    fireEvent.click(await screen.findByRole('button', { name: 'Stash & Pull' }));

    await waitFor(() => expect(useGitStore.getState().error).toContain('stash failed'));
    await waitFor(() => expect(screen.getByRole('button', { name: /^pull/i })).toBeEnabled());
    expect(tauriApi.gitPull).not.toHaveBeenCalled();
  });

  it('failed fetch prevents push', async () => {
    vi.mocked(tauriApi.gitFetch).mockRejectedValueOnce(new Error('fetch failed'));
    setRepositoryState();
    render(<GitLandingPanel />);

    fireEvent.click(screen.getByRole('button', { name: /^push/i }));
    fireEvent.click(await screen.findByRole('button', { name: 'Fetch & Push' }));

    await waitFor(() => expect(useGitStore.getState().error).toContain('fetch failed'));
    await waitFor(() => expect(screen.getByRole('button', { name: /^push/i })).toBeEnabled());
    expect(tauriApi.gitPush).not.toHaveBeenCalled();
  });

  it('failed commit preserves the commit message and reports failure', async () => {
    vi.mocked(tauriApi.gitCommit).mockRejectedValueOnce(new Error('commit failed'));
    setRepositoryState({
      status: {
        ...cleanStatus,
        isClean: false,
        files: [{ path: 'request.yml', status: 'modified', staged: true }],
      },
    });
    render(<GitCommitForm />);

    const message = screen.getByRole('textbox', { name: 'Commit message' });
    fireEvent.change(message, { target: { value: 'Preserve this message' } });
    fireEvent.click(screen.getByRole('button', { name: 'Commit 1 file' }));

    await waitFor(() => expect(useGitStore.getState().error).toContain('commit failed'));
    expect(message).toHaveValue('Preserve this message');
  });

  it('failed remote edit preserves the dialog input and edit state', async () => {
    vi.mocked(tauriApi.gitSetRemoteUrl).mockRejectedValueOnce(new Error('remote edit failed'));
    setRepositoryState();
    const onOpenChange = vi.fn();
    render(<GitRemotesDialog open onOpenChange={onOpenChange} />);

    const remoteRow = screen.getByText('origin').closest('.remote-row');
    if (!(remoteRow instanceof HTMLElement)) throw new Error('Remote row not found');
    fireEvent.click(within(remoteRow).getAllByRole('button')[0]);

    const editInput = screen.getByDisplayValue('git@example.com:team/repo.git');
    fireEvent.change(editInput, { target: { value: 'git@example.com:team/new-repo.git' } });
    const editRow = editInput.parentElement;
    if (!editRow) throw new Error('Remote edit row not found');
    fireEvent.click(within(editRow).getAllByRole('button')[0]);

    await waitFor(() => expect(useGitStore.getState().error).toContain('remote edit failed'));
    expect(screen.getByRole('dialog', { name: 'Manage Remotes' })).toBeInTheDocument();
    expect(screen.getByDisplayValue('git@example.com:team/new-repo.git')).toBeInTheDocument();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});

describe('Git clone ownership contract', () => {
  it('starts clone exactly once when credentials are preloaded', async () => {
    const clone = createDeferred<void>();
    vi.mocked(tauriApi.gitClone).mockReturnValue(clone.promise);
    setRepositoryState();
    render(<GitCloneDialog open onOpenChange={vi.fn()} />);
    await fillCloneForm();

    fireEvent.click(screen.getByRole('button', { name: 'Clone' }));

    await waitFor(() => expect(tauriApi.gitClone).toHaveBeenCalled());
    clone.resolve();
    await waitFor(() => expect(tauriApi.detectClonedStructure).toHaveBeenCalled());
    expect(tauriApi.gitClone).toHaveBeenCalledTimes(1);
    expect(tauriApi.gitClone).toHaveBeenCalledWith(
      'https://example.com/team/repo.git',
      'clone-capability',
      { type: 'sshAgent' },
    );
  });
});

describe('Git clone credential handoff characterization', () => {
  it('starts clone exactly once after credential submission', async () => {
    const clone = createDeferred<void>();
    vi.mocked(tauriApi.gitClone).mockReturnValue(clone.promise);
    render(<GitCloneDialog open onOpenChange={vi.fn()} />);
    await fillCloneForm();

    fireEvent.click(screen.getByRole('button', { name: 'Clone' }));
    expect(screen.getByText('Cloning repository...')).toBeInTheDocument();
    expect(tauriApi.gitClone).not.toHaveBeenCalled();

    act(() => {
      useGitStore.getState().setCredentials({ type: 'sshAgent' });
    });

    await waitFor(() => expect(tauriApi.gitClone).toHaveBeenCalledTimes(1));
    expect(tauriApi.gitClone).toHaveBeenCalledWith(
      'https://example.com/team/repo.git',
      'clone-capability',
      { type: 'sshAgent' },
    );

    clone.resolve();
    await waitFor(() => expect(tauriApi.detectClonedStructure).toHaveBeenCalledTimes(1));
    expect(tauriApi.gitClone).toHaveBeenCalledTimes(1);
  });
});
