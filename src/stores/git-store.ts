import { create } from 'zustand';
import {
  type BranchList,
  type CommitInfo,
  type ConflictFile,
  type ConflictResolution,
  type FileStatus,
  type GitCredentials,
  gitAbortMerge,
  gitAddRemote,
  gitBranches,
  gitCheckoutRemoteBranch,
  gitCommit,
  gitConflicts,
  gitCreateBranch,
  gitDeleteBranch,
  gitDiscard,
  gitFetch,
  gitGetIdentity,
  gitInit,
  gitIsRepo,
  gitListRemotes,
  gitLog,
  gitMergeBranch,
  gitPull,
  gitPush,
  gitRemoveRemote,
  gitResolveConflict,
  gitSetRemoteUrl,
  gitStage,
  gitStashApply,
  gitStashDrop,
  gitStashList,
  gitStashPop,
  gitStashSave,
  gitStatus,
  gitSwitchBranch,
  gitUnstage,
  type GitSshTrustFailure,
  loadGitCredentials,
  parseGitNetworkError,
  type RemoteInfo,
  type RepoStatus,
  type StashEntry,
} from '@/lib/tauri-api';

interface GitState {
  isRepo: boolean;
  repositoryId: string | null;
  status: RepoStatus | null;
  conflicts: ConflictFile[];
  stashes: StashEntry[];
  branches: BranchList | null;
  remotes: RemoteInfo[];
  commitLog: CommitInfo[];
  loading: boolean;
  error: string | null;
  /** Set only for SSH host-trust failures (unknown/changed/unavailable). Never treated as an auth error. */
  trustFailure: GitSshTrustFailure | null;
  credentials: GitCredentials | null;
  showCredentialsDialog: boolean;
  /** Operation that triggered the credentials dialog — auto-retried once credentials are saved. */
  pendingNetworkOp: 'pull' | 'push' | 'fetch' | null;
  showIdentitySetupDialog: boolean;
  identitySetupInitialName: string;
  identitySetupInitialEmail: string;
  pendingCredentialsForIdentitySetup: GitCredentials | null;
  activatePendingCredentials: () => void;

  setRepository: (repositoryId: string) => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshConflicts: () => Promise<void>;
  refreshStashes: () => Promise<void>;
  refreshBranches: () => Promise<void>;
  refreshRemotes: () => Promise<void>;
  refreshLog: (limit?: number) => Promise<void>;
  resolveConflict: (file: string, resolution: ConflictResolution) => Promise<void>;
  abortMerge: () => Promise<void>;
  stageFiles: (files: string[]) => Promise<void>;
  unstageFiles: (files: string[]) => Promise<void>;
  discardFiles: (files: string[]) => Promise<void>;
  commitChanges: (message: string) => Promise<void>;
  stageAll: () => Promise<void>;
  unstageAll: () => Promise<void>;
  saveStash: (message: string) => Promise<void>;
  popStash: (index: number) => Promise<void>;
  applyStash: (index: number) => Promise<void>;
  dropStash: (index: number) => Promise<void>;
  applyStashMany: (indices: number[]) => Promise<void>;
  popStashMany: (indices: number[]) => Promise<void>;
  dropStashMany: (indices: number[]) => Promise<void>;
  switchBranch: (name: string) => Promise<void>;
  checkoutRemoteBranch: (name: string) => Promise<void>;
  createBranch: (name: string) => Promise<void>;
  deleteBranch: (name: string) => Promise<void>;
  mergeBranch: (name: string) => Promise<void>;
  addRemote: (name: string, url: string) => Promise<void>;
  removeRemote: (name: string) => Promise<void>;
  setRemoteUrl: (name: string, url: string) => Promise<void>;
  setCredentials: (creds: GitCredentials) => void;
  setShowCredentialsDialog: (show: boolean) => void;
  clearPendingNetworkOp: () => void;
  push: (remote?: string) => Promise<void>;
  pull: (remote?: string) => Promise<void>;
  fetch: (remote?: string) => Promise<void>;
  clearError: () => void;
  reset: () => void;
  initRepo: (repositoryId: string) => Promise<void>;
  hasConflicts: () => boolean;
}

export const useGitStore = create<GitState>((set, get) => ({
  // Selector to determine if any file is in a conflicted state
  hasConflicts: () => {
    const { status } = get();
    return status?.files.some((f) => f.status === 'conflicted') ?? false;
  },
  isRepo: false,
  repositoryId: null,
  status: null,
  conflicts: [],
  stashes: [],
  branches: null,
  remotes: [],
  commitLog: [],
  loading: false,
  error: null,
  trustFailure: null,credentials: null,
  showCredentialsDialog: false,
  pendingNetworkOp: null,
  showIdentitySetupDialog: false,
  identitySetupInitialName: '',
  identitySetupInitialEmail: '',
  pendingCredentialsForIdentitySetup: null,

  // Set the active repository and check if it is a git repo.
  setRepository: async (repositoryId: string) => {
    set({ repositoryId, loading: true, error: null });
    try {
      const isRepo = await gitIsRepo(repositoryId);
      set({ isRepo });
      if (isRepo) {
        // Always reload repository-scoped credentials so switching repositories
        // picks up the right identity without requiring a manual re-entry.
        try {
          const saved = await loadGitCredentials(repositoryId);
          set({ credentials: saved ?? null });
        } catch {
          // Keychain unavailable — proceed without credentials.
        }
        const [status] = await Promise.all([
          gitStatus(repositoryId),
          get().refreshStashes(),
          get().refreshBranches(),
          get().refreshRemotes(),
        ]);
        set({ status, loading: false });
      } else {
        set({ status: null, loading: false });
      }
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  // Reload the current status from disk.
  refreshStatus: async () => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const status = await gitStatus(repositoryId);
      set({ status });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the conflict list from disk.
  refreshConflicts: async () => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const conflicts = await gitConflicts(repositoryId);
      set({ conflicts });
    } catch {
      set({ conflicts: [] });
    }
  },

  // Resolve a single conflicted file with the given strategy.
  resolveConflict: async (file, resolution) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitResolveConflict(repositoryId, file, resolution);
      await get().refreshStatus();
      await get().refreshConflicts();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Abort a merge and reset to HEAD.
  abortMerge: async () => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitAbortMerge(repositoryId);
      await get().refreshStatus();
      await get().refreshConflicts();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the stash list from disk.
  refreshStashes: async () => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const stashes = await gitStashList(repositoryId);
      set({ stashes });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the branch list from disk.
  refreshBranches: async () => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const branches = await gitBranches(repositoryId);
      set({ branches });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the remote list from disk.
  refreshRemotes: async () => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const remotes = await gitListRemotes(repositoryId);
      set({ remotes });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the commit log from disk, up to the given limit.
  refreshLog: async (limit) => {
    const { repositoryId, isRepo } = get();
    if (!repositoryId || !isRepo) return;
    try {
      const log = await gitLog(repositoryId, limit ?? 50);
      set({ commitLog: log });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Stage the given file paths.
  stageFiles: async (files: string[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitStage(repositoryId, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Unstage the given file paths.
  unstageFiles: async (files: string[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitUnstage(repositoryId, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Discard working-tree changes for the given file paths.
  discardFiles: async (files: string[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitDiscard(repositoryId, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Commit all currently staged changes with the provided message.
  commitChanges: async (message: string) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitCommit(repositoryId, message);
      await get().refreshStatus();
      await get().refreshLog();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Stage every modified file that is not yet staged.
  stageAll: async () => {
    // Always refresh before reading so we never stage from a stale cache
    // that could contain directory-level entries (trailing '/') from an
    // older status response.
    await get().refreshStatus();
    const { repositoryId, status } = get();
    if (!repositoryId || !status) return;
    const paths = status.files
      .filter((f: FileStatus) => !f.staged && f.status !== 'unchanged')
      .map((f: FileStatus) => f.path);
    if (paths.length === 0) return;
    try {
      await gitStage(repositoryId, paths);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Unstage every file that is currently staged.
  unstageAll: async () => {
    const { status } = get();
    if (!status) return;
    const staged = status.files.filter((f: FileStatus) => f.staged).map((f: FileStatus) => f.path);
    if (staged.length > 0) {
      await get().unstageFiles(staged);
    }
  },

  // Save current working-tree changes as a new stash entry.
  saveStash: async (message: string) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitStashSave(repositoryId, message);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Pop the stash at the given index and restore it to the working tree.
  popStash: async (index: number) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitStashPop(repositoryId, index);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Apply the stash at the given index without removing it.
  applyStash: async (index: number) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitStashApply(repositoryId, index);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Drop (delete) the stash at the given index.
  dropStash: async (index: number) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitStashDrop(repositoryId, index);
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Apply stashes ascending (stash@{0} first). Apply leaves the stack intact so no renumbering occurs.
  applyStashMany: async (indices: number[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    const sorted = [...indices].sort((a, b) => a - b);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashApply(repositoryId, index);
      } catch (e) {
        set({
          error: `Failed at stash@{${index}}: ${String(e)}. Stashes processed before this one were already applied.`,
        });
        break;
      }
    }
    await get().refreshStashes();
    await get().refreshStatus();
  },

  // Pop multiple stashes oldest-first (descending index) to avoid renumbering. Stops on first error.
  popStashMany: async (indices: number[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    const sorted = [...indices].sort((a, b) => b - a);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashPop(repositoryId, index);
      } catch (e) {
        set({
          error: `Failed at stash@{${index}}: ${String(e)}. Stashes processed before this one were already popped.`,
        });
        break;
      }
    }
    await get().refreshStashes();
    await get().refreshStatus();
  },

  // Drop multiple stashes oldest-first (descending index) to avoid renumbering. Stops on first error. No working-tree changes.
  dropStashMany: async (indices: number[]) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    const sorted = [...indices].sort((a, b) => b - a);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashDrop(repositoryId, index);
      } catch (e) {
        set({
          error: `Failed at stash@{${index}}: ${String(e)}. Stashes processed before this one were already dropped.`,
        });
        break;
      }
    }
    await get().refreshStashes();
  },

  // Switch to the named branch.
  switchBranch: async (name) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitSwitchBranch(repositoryId, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Check out a remote branch as a new local tracking branch.
  checkoutRemoteBranch: async (name) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitCheckoutRemoteBranch(repositoryId, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Create a new branch with the given name and switch to it.
  createBranch: async (name) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitCreateBranch(repositoryId, name);
      await Promise.all([get().refreshBranches(), get().refreshStatus()]);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Delete the named branch.
  deleteBranch: async (name) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitDeleteBranch(repositoryId, name);
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Merge the named branch into the current branch.
  mergeBranch: async (name) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitMergeBranch(repositoryId, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Add a new remote.
  addRemote: async (name: string, url: string) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitAddRemote(repositoryId, name, url);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Remove a remote.
  removeRemote: async (name: string) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitRemoveRemote(repositoryId, name);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Update the URL for an existing remote.
  setRemoteUrl: async (name: string, url: string) => {
    const { repositoryId } = get();
    if (!repositoryId) return;
    try {
      await gitSetRemoteUrl(repositoryId, name, url);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Store credentials, close the dialog, and auto-retry the operation that triggered it.
  setCredentials: (creds) => {
    const { pendingNetworkOp, repositoryId } = get();

    // Close the credentials dialog immediately so the user gets instant feedback and
    // so no user action (Escape/overlay click) can fire onOpenChange and clear
    // pendingNetworkOp while the async identity fetch is in flight.
    // Use set() directly — setShowCredentialsDialog() would clear pendingNetworkOp.
    set({ showCredentialsDialog: false });

    // SSH key: prompt user to confirm/update git identity before activating.
    if (creds.type === 'sshKey' && repositoryId) {
      void (async () => {
        try {
          const identity = await gitGetIdentity(repositoryId);
          set({
            pendingCredentialsForIdentitySetup: creds,
            showIdentitySetupDialog: true,
            identitySetupInitialName: identity.name,
            identitySetupInitialEmail: identity.email,
          });
        } catch {
          // Identity fetch failed — activate creds immediately rather than blocking.
          const currentOp = get().pendingNetworkOp;
          set({ credentials: creds, pendingNetworkOp: null });
          if (currentOp) get()[currentOp]();
        }
      })();
      return;
    }

    // All other credential types: activate immediately.
    set({ credentials: creds, pendingNetworkOp: null });
    if (pendingNetworkOp) get()[pendingNetworkOp]();
  },

  // Show or hide the credentials dialog.
  setShowCredentialsDialog: (show) =>
    set({ showCredentialsDialog: show, ...(show ? {} : { pendingNetworkOp: null }) }),

  clearPendingNetworkOp: () => set({ pendingNetworkOp: null }),

  activatePendingCredentials: () => {
    const { pendingCredentialsForIdentitySetup, pendingNetworkOp } = get();
    if (!pendingCredentialsForIdentitySetup) return;
    const creds = pendingCredentialsForIdentitySetup;
    set({
      credentials: creds,
      showIdentitySetupDialog: false,
      pendingCredentialsForIdentitySetup: null,
      identitySetupInitialName: '',
      identitySetupInitialEmail: '',
      pendingNetworkOp: null,
    });
    if (pendingNetworkOp && creds) {
      get()[pendingNetworkOp]();
    }
  },

  // Push local commits to the remote, prompting for credentials if needed.
  push: async (remote) => {
    const { repositoryId, credentials } = get();
    if (!repositoryId) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'push' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPush(repositoryId, resolvedRemote, credentials);
      await get().refreshStatus();
    } catch (e) {
      const msg = String(e);
      // SSH auth failures: wrong key, repo access denied, host key mismatch.
      // Set pendingNetworkOp so that "Change credentials" auto-retries the push.
      const isAuthError =
        msg.includes('class=Ssh') ||
        msg.includes('authentication failed') ||
        msg.includes('Repository not found') ||
        msg.includes('Permission denied');
      set({ error: msg, ...(isAuthError ? { pendingNetworkOp: 'push' } : {}) });
    }
  },

  // Pull remote commits into the current branch, prompting for credentials if needed.
  pull: async (remote) => {
    const { repositoryId, credentials } = get();
    if (!repositoryId) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'pull' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPull(repositoryId, resolvedRemote, credentials);
    } catch (e) {
      const msg = String(e);
      const isAuthError =
        msg.includes('class=Ssh') ||
        msg.includes('authentication failed') ||
        msg.includes('Repository not found') ||
        msg.includes('Permission denied');
      set({ error: msg, ...(isAuthError ? { pendingNetworkOp: 'pull' } : {}) });
    }
    // Always refresh status and conflicts after a pull attempt — whether it
    // succeeded or produced merge conflicts — so the UI reflects the real
    // repo state (behind count, conflict files, etc.).
    await get().refreshStatus();
    await get().refreshConflicts();
    await get().refreshBranches();
  },

  // Fetch remote refs without merging, prompting for credentials if needed.
  fetch: async (remote) => {
    const { repositoryId, credentials } = get();
    if (!repositoryId) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'fetch' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitFetch(repositoryId, resolvedRemote, credentials);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      const msg = String(e);
      const isAuthError =
        msg.includes('class=Ssh') ||
        msg.includes('authentication failed') ||
        msg.includes('Repository not found') ||
        msg.includes('Permission denied');
      set({ error: msg, ...(isAuthError ? { pendingNetworkOp: 'fetch' } : {}) });
    }
  },

  clearError: () => set({ error: null }),

  // Initialize a new git repository then load it into the store.
  initRepo: async (repositoryId: string) => {
    try {
      await gitInit(repositoryId);
      await get().setRepository(repositoryId);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reset the store back to its initial state.
  reset: () => {
    set({
      isRepo: false,
      repositoryId: null,
      status: null,
      conflicts: [],
      stashes: [],
      branches: null,
      remotes: [],
      commitLog: [],
      loading: false,
      error: null,
      credentials: null,
      showCredentialsDialog: false,
      pendingNetworkOp: null,
      showIdentitySetupDialog: false,
      identitySetupInitialName: '',
      identitySetupInitialEmail: '',
      pendingCredentialsForIdentitySetup: null,
    });
  },
}));
