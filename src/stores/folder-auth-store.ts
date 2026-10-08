import { create } from 'zustand';
import type { AuthState } from '@/types/pane-types';

/** Key for one folder of one collection. A NUL separator cannot appear in either part. */
export function folderAuthKey(collection: string, folderPath: string): string {
  return `${collection}\u0000${folderPath}`;
}

// Holds the full AuthState of each folder that has authorization, including any fetched
// OAuth2 token. Tokens are never written to folder.yml, so this is the only place they live,
// like collection-auth-store for collections. In memory only: lost on reload by design.
interface FolderAuthStore {
  auths: Record<string, AuthState>;
  setFolderAuth: (collection: string, folderPath: string, auth: AuthState) => void;
  getFolderAuth: (collection: string, folderPath: string) => AuthState | undefined;
  clearFolderAuth: (collection: string, folderPath: string) => void;
}

export const useFolderAuthStore = create<FolderAuthStore>()((set, get) => ({
  auths: {},

  setFolderAuth(collection, folderPath, auth) {
    set({ auths: { ...get().auths, [folderAuthKey(collection, folderPath)]: auth } });
  },

  getFolderAuth(collection, folderPath) {
    return get().auths[folderAuthKey(collection, folderPath)];
  },

  clearFolderAuth(collection, folderPath) {
    const { [folderAuthKey(collection, folderPath)]: _removed, ...rest } = get().auths;
    set({ auths: rest });
  },
}));
