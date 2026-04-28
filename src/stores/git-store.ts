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
  gitInit,
  gitBranches,
  gitCheckoutRemoteBranch,
  gitCommit,
  gitConflicts,
  gitCreateBranch,
  gitDeleteBranch,
  gitDiscard,
  gitFetch,
  gitIsRepo,
  gitListRemotes,
  gitLog,
  loadGitCredentials,
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
  type RemoteInfo,
  type RepoStatus,
  type StashEntry,
} from '@/lib/tauri-api';

interface GitState {
  isRepo: boolean;
  collectionPath: string | null;
  status: RepoStatus | null;
  conflicts: ConflictFile[];
  stashes: StashEntry[];
  branches: BranchList | null;
  remotes: RemoteInfo[];
  commitLog: CommitInfo[];
  loading: boolean;
  error: string | null;
  credentials: GitCredentials | null;
  showCredentialsDialog: boolean;
  /** Operation that triggered the credentials dialog — auto-retried once credentials are saved. */
  pendingNetworkOp: 'pull' | 'push' | 'fetch' | null;

  setCollection: (path: string) => Promise<void>;
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
  initRepo: (path: string) => Promise<void>;
  hasConflicts: () => boolean;
}

export const useGitStore = create<GitState>((set, get) => ({
  // Selector to determine if any file is in a conflicted state
  hasConflicts: () => {
    const { status } = get();
    return status?.files.some((f) => f.status === 'conflicted') ?? false;
  },
  isRepo: false,
  collectionPath: null,
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

  // Set the active collection path and check if it is a git repo.
  setCollection: async (path: string) => {
    set({ collectionPath: path, loading: true, error: null });
    try {
      const isRepo = await gitIsRepo(path);
      set({ isRepo });
      if (isRepo) {
        // Auto-load persisted credentials if none are set in memory.
        if (!get().credentials) {
          try {
            const saved = await loadGitCredentials();
            if (saved) set({ credentials: saved });
          } catch {
            // Keychain unavailable — proceed without credentials.
          }
        }
        const [status] = await Promise.all([
          gitStatus(path),
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
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const status = await gitStatus(collectionPath);
      set({ status });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the conflict list from disk.
  refreshConflicts: async () => {
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const conflicts = await gitConflicts(collectionPath);
      set({ conflicts });
    } catch {
      set({ conflicts: [] });
    }
  },

  // Resolve a single conflicted file with the given strategy.
  resolveConflict: async (file, resolution) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitResolveConflict(collectionPath, file, resolution);
      await get().refreshStatus();
      await get().refreshConflicts();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Abort a merge and reset to HEAD.
  abortMerge: async () => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitAbortMerge(collectionPath);
      await get().refreshStatus();
      await get().refreshConflicts();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the stash list from disk.
  refreshStashes: async () => {
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const stashes = await gitStashList(collectionPath);
      set({ stashes });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the branch list from disk.
  refreshBranches: async () => {
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const branches = await gitBranches(collectionPath);
      set({ branches });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the remote list from disk.
  refreshRemotes: async () => {
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const remotes = await gitListRemotes(collectionPath);
      set({ remotes });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reload the commit log from disk, up to the given limit.
  refreshLog: async (limit) => {
    const { collectionPath, isRepo } = get();
    if (!collectionPath || !isRepo) return;
    try {
      const log = await gitLog(collectionPath, limit ?? 50);
      set({ commitLog: log });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Stage the given file paths.
  stageFiles: async (files: string[]) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitStage(collectionPath, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Unstage the given file paths.
  unstageFiles: async (files: string[]) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitUnstage(collectionPath, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Discard working-tree changes for the given file paths.
  discardFiles: async (files: string[]) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitDiscard(collectionPath, files);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Commit all currently staged changes with the provided message.
  commitChanges: async (message: string) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitCommit(collectionPath, message);
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
    const { collectionPath, status } = get();
    if (!collectionPath || !status) return;
    const paths = status.files
      .filter((f: FileStatus) => !f.staged && f.status !== 'unchanged')
      .map((f: FileStatus) => f.path);
    if (paths.length === 0) return;
    try {
      await gitStage(collectionPath, paths);
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
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitStashSave(collectionPath, message);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Pop the stash at the given index and restore it to the working tree.
  popStash: async (index: number) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitStashPop(collectionPath, index);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Apply the stash at the given index without removing it.
  applyStash: async (index: number) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitStashApply(collectionPath, index);
      await get().refreshStatus();
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Drop (delete) the stash at the given index.
  dropStash: async (index: number) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitStashDrop(collectionPath, index);
      await get().refreshStashes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Apply stashes ascending (stash@{0} first). Apply leaves the stack intact so no renumbering occurs.
  applyStashMany: async (indices: number[]) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    const sorted = [...indices].sort((a, b) => a - b);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashApply(collectionPath, index);
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
    const { collectionPath } = get();
    if (!collectionPath) return;
    const sorted = [...indices].sort((a, b) => b - a);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashPop(collectionPath, index);
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
    const { collectionPath } = get();
    if (!collectionPath) return;
    const sorted = [...indices].sort((a, b) => b - a);
    set({ error: null });
    for (const index of sorted) {
      try {
        await gitStashDrop(collectionPath, index);
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
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitSwitchBranch(collectionPath, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Check out a remote branch as a new local tracking branch.
  checkoutRemoteBranch: async (name) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitCheckoutRemoteBranch(collectionPath, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Create a new branch with the given name and switch to it.
  createBranch: async (name) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitCreateBranch(collectionPath, name);
      await Promise.all([get().refreshBranches(), get().refreshStatus()]);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Delete the named branch.
  deleteBranch: async (name) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitDeleteBranch(collectionPath, name);
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Merge the named branch into the current branch.
  mergeBranch: async (name) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitMergeBranch(collectionPath, name);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Add a new remote.
  addRemote: async (name: string, url: string) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitAddRemote(collectionPath, name, url);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Remove a remote.
  removeRemote: async (name: string) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitRemoveRemote(collectionPath, name);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Update the URL for an existing remote.
  setRemoteUrl: async (name: string, url: string) => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitSetRemoteUrl(collectionPath, name, url);
      await get().refreshRemotes();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Store credentials, close the dialog, and auto-retry the operation that triggered it.
  setCredentials: (creds) => {
    const { pendingNetworkOp } = get();
    set({ credentials: creds, showCredentialsDialog: false, pendingNetworkOp: null });
    if (pendingNetworkOp) {
      get()[pendingNetworkOp]();
    }
  },

  // Show or hide the credentials dialog.
  setShowCredentialsDialog: (show) =>
    set({ showCredentialsDialog: show, ...(show ? {} : { pendingNetworkOp: null }) }),

  clearPendingNetworkOp: () => set({ pendingNetworkOp: null }),

  // Push local commits to the remote, prompting for credentials if needed.
  push: async (remote) => {
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'push' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPush(collectionPath, resolvedRemote, credentials);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Pull remote commits into the current branch, prompting for credentials if needed.
  pull: async (remote) => {
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'pull' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPull(collectionPath, resolvedRemote, credentials);
    } catch (e) {
      set({ error: String(e) });
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
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) {
      set({ showCredentialsDialog: true, pendingNetworkOp: 'fetch' });
      return;
    }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitFetch(collectionPath, resolvedRemote, credentials);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearError: () => set({ error: null }),

  // Initialize a new git repository at the given path then load it into the store.
  initRepo: async (path: string) => {
    try {
      await gitInit(path);
      await get().setCollection(path);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Reset the store back to its initial state.
  reset: () => {
    set({
      isRepo: false,
      collectionPath: null,
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
    });
  },
}));
