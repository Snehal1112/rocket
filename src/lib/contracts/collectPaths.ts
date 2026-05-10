import type { CollectionItem } from '@/lib/tauri-api';

/**
 * Recursively walks a collection item tree and appends relative
 * folder paths and request file paths to the supplied arrays.
 * Call with `collectPaths(collection.root.items, '', folders, requests)`.
 */
export function collectPaths(
  items: CollectionItem[],
  prefix: string,
  folders: string[],
  requests: string[],
): void {
  for (const item of items) {
    if (item.type === 'folder') {
      const seg = item.dirName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      folders.push(path);
      collectPaths(item.items, path, folders, requests);
    } else {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
  }
}
