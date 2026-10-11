import { create } from 'zustand';

/** One key per folder of a collection. */
export function folderEditKey(collection: string, folderPath: string): string {
  return JSON.stringify([collection, folderPath]);
}

interface FolderEditState {
  /** Folders whose open Folder Settings tab has unsaved edits. */
  dirty: Record<string, true>;
  /** How many times a reload was requested, per folder. */
  reloads: Record<string, number>;
  setDirty: (key: string, dirty: boolean) => void;
  requestReload: (key: string) => void;
}

/**
 * Bridges the Folder Settings tab and the assistant panel. A folder tab keeps
 * its edits in hook state, so this store carries only its dirty flag and the
 * reload requests that an accepted assistant proposal raises.
 */
export const useFolderEditStore = create<FolderEditState>()((set) => ({
  dirty: {},
  reloads: {},
  setDirty: (key, dirty) =>
    set((state) => {
      if (dirty === (state.dirty[key] === true)) return state;
      const next = { ...state.dirty };
      if (dirty) next[key] = true;
      else delete next[key];
      return { dirty: next };
    }),
  requestReload: (key) =>
    set((state) => ({ reloads: { ...state.reloads, [key]: (state.reloads[key] ?? 0) + 1 } })),
}));
