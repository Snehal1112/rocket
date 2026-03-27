import { create } from 'zustand';
import {
  gitIsRepo,
  gitStatus,
  gitStage,
  gitUnstage,
  gitDiscard,
  gitCommit,
  gitStashList,
  gitStashSave,
  gitStashPop,
  gitStashApply,
  gitStashDrop,
  type RepoStatus,
  type FileStatus,
  type StashEntry,
} from '@/lib/tauri-api';

interface GitState {
  isRepo: boolean;
  collectionPath: string | null;
  status: RepoStatus | null;
  stashes: StashEntry[];
  loading: boolean;
  error: string | null;

  setCollection: (path: string) => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshStashes: () => Promise<void>;
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
  reset: () => void;
}

export const useGitStore = create<GitState>((set, get) => ({
  isRepo: false,
  collectionPath: null,
  status: null,
  stashes: [],
  loading: false,
  error: null,

  // Set the active collection path and check if it is a git repo.
  setCollection: async (path: string) => {
    set({ collectionPath: path, loading: true, error: null });
    try {
      const isRepo = await gitIsRepo(path);
      set({ isRepo });
      if (isRepo) {
        const status = await gitStatus(path);
        set({ status, loading: false });
        await get().refreshStashes();
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
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Stage every modified file that is not yet staged.
  stageAll: async () => {
    const { status } = get();
    if (!status) return;
    const unstaged = status.files
      .filter((f: FileStatus) => !f.staged && f.status !== 'unchanged')
      .map((f: FileStatus) => f.path);
    if (unstaged.length > 0) {
      await get().stageFiles(unstaged);
    }
  },

  // Unstage every file that is currently staged.
  unstageAll: async () => {
    const { status } = get();
    if (!status) return;
    const staged = status.files
      .filter((f: FileStatus) => f.staged)
      .map((f: FileStatus) => f.path);
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

  // Reset the store back to its initial state.
  reset: () => {
    set({
      isRepo: false,
      collectionPath: null,
      status: null,
      stashes: [],
      loading: false,
      error: null,
    });
  },
}));
