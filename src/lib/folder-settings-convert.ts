// Conversions between the Folder Settings payload and the editor state types.
// Auth conversion reuses persisted-auth.ts, header conversion reuses persisted-headers.ts.
import { type AuthTypeOption, NTLM_OPTION, OAUTH1_OPTION } from '@/lib/auth-type-options';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Auth, Header } from '@/lib/tauri-api';
import type { AuthState, KeyValueEntry } from '@/types/pane-types';

/** A persisted header, which can carry a description the editor rows do not show. */
export type FolderHeader = Header & { description?: unknown };

/** Editor rows for a folder's headers. Ids are positional, like the collection tab. */
export function headersToEntries(headers: readonly Header[]): KeyValueEntry[] {
  return headers.map((h, i) => ({
    id: String(i),
    key: h.key,
    value: h.value,
    enabled: h.enabled,
  }));
}

/**
 * Persisted headers for the editor rows. Blank-key rows are dropped and `enabled` is kept
 * as is. A description is carried over from `previous` by header name, because the rows
 * have no description column and saving would otherwise erase it.
 */
export function entriesToHeaders(
  entries: KeyValueEntry[],
  previous: readonly FolderHeader[],
): FolderHeader[] {
  const descriptions = new Map<string, unknown>();
  for (const h of previous) {
    if (h.description !== undefined && h.description !== null && !descriptions.has(h.key)) {
      descriptions.set(h.key, h.description);
    }
  }
  return toPersistedHeaders(entries).map((h) => {
    const description = descriptions.get(h.key);
    return description === undefined ? h : { ...h, description };
  });
}

/**
 * A synthetic request path inside the folder. The chain commands walk the parents of a
 * request path, so passing a file in the folder includes the folder itself. The root folder
 * has no parents to walk, so it maps to an empty path.
 */
export function folderChainPath(folderPath: string): string {
  return folderPath ? `${folderPath}/folder.yml` : '';
}

/** The editor state for a folder's persisted auth. No folder auth reads as Inherit. */
export function folderAuthToState(auth: Auth | null | undefined): AuthState {
  // fromPersistedAuth maps `none` to the fallback, so an on-disk `none` is kept here.
  if (auth?.authType === 'none') return { authType: 'none' };
  return fromPersistedAuth(auth, 'inherit');
}

/**
 * The persisted auth for the editor state. Inherit is stored as no folder auth (an absent
 * field), which is how the backend already reads both `none` and `inherit`
 * (`resolve_folder_auth`).
 */
export function stateToFolderAuth(state: AuthState): Auth | undefined {
  return state.authType === 'inherit' ? undefined : toPersistedAuth(state);
}

/**
 * Copies the OAuth2 token fields of an in-memory state onto the state read from disk, which
 * never holds tokens. Mirrors what CollectionOverviewTab does on load. A disk copy that
 * already has an access token is returned unchanged.
 */
export function restoreOAuth2Tokens(disk: AuthState, cached: AuthState | undefined): AuthState {
  if (disk.authType !== 'oauth2' || !disk.oauth2 || disk.oauth2.accessToken) return disk;
  if (cached?.authType !== 'oauth2' || !cached.oauth2?.accessToken) return disk;
  return {
    ...disk,
    oauth2: {
      ...disk.oauth2,
      accessToken: cached.oauth2.accessToken,
      refreshToken: cached.oauth2.refreshToken ?? '',
      expiresIn: cached.oauth2.expiresIn ?? null,
      tokenAcquiredAt: cached.oauth2.tokenAcquiredAt ?? null,
      idToken: cached.oauth2.idToken ?? '',
      idTokenClaims: cached.oauth2.idTokenClaims ?? null,
      accessTokenClaims: cached.oauth2.accessTokenClaims ?? null,
      tokenType: cached.oauth2.tokenType ?? '',
      responseScope: cached.oauth2.responseScope ?? '',
    },
  };
}

/**
 * Types a folder can be set to. There is no None: the backend skips None and Inherit alike
 * when it looks for folder auth, so a folder cannot switch auth off for its children.
 */
export const FOLDER_AUTH_TYPES: AuthTypeOption[] = [
  { label: 'Inherit', value: 'inherit' },
  { label: 'Basic', value: 'basic' },
  { label: 'Digest', value: 'digest' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  OAUTH1_OPTION,
  NTLM_OPTION,
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
  { label: 'WSSE', value: 'wsse' },
];

const LEGACY_NONE_OPTION: AuthTypeOption = { label: 'No Auth (set on disk)', value: 'none' };

/** The selector entries. An existing on-disk `none` stays visible so it keeps a label. */
export function folderAuthTypeOptions(current: AuthState['authType']): AuthTypeOption[] {
  return current === 'none' ? [LEGACY_NONE_OPTION, ...FOLDER_AUTH_TYPES] : FOLDER_AUTH_TYPES;
}
