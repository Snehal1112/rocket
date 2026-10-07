// Where a request set to Inherit gets its authorization: the nearest folder with auth, else
// the collection, else nothing. Used by the Auth tab hint and by execute-request.ts.
import { ancestorFolderPaths } from '@/lib/folder-inheritance';
import {
  FOLDER_AUTH_TYPES,
  folderAuthToState,
  restoreOAuth2Tokens,
  sameOAuth2Config,
} from '@/lib/folder-settings-convert';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import { getCollectionSettings, getFolderSettings } from '@/lib/tauri-api';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import type { AuthState } from '@/types/pane-types';

export { ancestorFolderPaths };

export type InheritedAuthSource =
  | { kind: 'folder'; folderPath: string; auth: AuthState }
  | { kind: 'collection'; auth: AuthState }
  | { kind: 'none' };

const hasAuth = (auth: AuthState) => auth.authType !== 'none' && auth.authType !== 'inherit';

/**
 * The innermost ancestor folder whose auth is neither None nor Inherit, matching the backend
 * `resolve_folder_auth`. The folder.yml decides whether a folder has auth. `folder-auth-store`
 * only supplies a cached OAuth2 token, and only one fetched for the same OAuth2 config. An
 * `inherit` entry in the store therefore never counts as folder auth. A folder that cannot be read is skipped: the
 * backend reports a broken folder.yml when the request runs, and a hint must not fail on it.
 */
export async function resolveInheritedFolderAuth(
  collection: string,
  requestPath: string,
): Promise<{ folderPath: string; auth: AuthState } | undefined> {
  for (const folderPath of ancestorFolderPaths(requestPath).reverse()) {
    try {
      const settings = await getFolderSettings(collection, folderPath);
      const disk = folderAuthToState(settings.auth);
      if (!hasAuth(disk)) continue;
      const cached = useFolderAuthStore.getState().getFolderAuth(collection, folderPath);
      // restoreOAuth2Tokens checks the config as well. The check is repeated here so a
      // token for another config never even reaches the merge.
      const matching = cached && sameOAuth2Config(disk, cached) ? cached : undefined;
      return { folderPath, auth: restoreOAuth2Tokens(disk, matching) };
    } catch {
      // Unreadable folder settings are skipped here on purpose.
    }
  }
  return undefined;
}

export async function resolveInheritedAuthSource(
  collection: string,
  requestPath: string,
): Promise<InheritedAuthSource> {
  const folder = await resolveInheritedFolderAuth(collection, requestPath);
  if (folder) return { kind: 'folder', ...folder };

  const cached = useCollectionAuthStore.getState().getCollectionAuth(collection);
  if (cached && hasAuth(cached)) return { kind: 'collection', auth: cached };
  try {
    const disk = fromPersistedAuth((await getCollectionSettings(collection)).auth);
    if (hasAuth(disk)) return { kind: 'collection', auth: disk };
  } catch {
    // Collection settings are unavailable: treat the collection as having no auth.
  }
  return { kind: 'none' };
}

const typeLabel = (auth: AuthState) =>
  FOLDER_AUTH_TYPES.find((t) => t.value === auth.authType)?.label ?? auth.authType;

export function describeInheritedAuthSource(source: InheritedAuthSource): string {
  switch (source.kind) {
    case 'folder':
      return `This request inherits authorization from the folder "${source.folderPath}" (${typeLabel(source.auth)}).`;
    case 'collection':
      return `This request inherits authorization from the collection settings (${typeLabel(source.auth)}).`;
    case 'none':
      return 'No folder or collection sets authorization, so this request is sent without it.';
  }
}
