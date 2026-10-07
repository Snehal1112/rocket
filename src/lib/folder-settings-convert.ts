// Conversions between the Folder Settings payload and the editor state types.
// Auth conversion reuses persisted-auth.ts, header conversion reuses persisted-headers.ts.
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Header } from '@/lib/tauri-api';
import type { KeyValueEntry } from '@/types/pane-types';

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
