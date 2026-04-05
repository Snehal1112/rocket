import type { CollectionItem } from './tauri-api';

/**
 * Returns a new array with folders sorted before requests.
 * Within each group items are sorted alphabetically (case-insensitive).
 * The input array is not mutated.
 */
export function sortItemsFoldersFirst(items: CollectionItem[]): CollectionItem[] {
  return [...items].sort((a, b) => {
    const aIsFolder = a.type === 'folder' ? 0 : 1;
    const bIsFolder = b.type === 'folder' ? 0 : 1;
    if (aIsFolder !== bIsFolder) return aIsFolder - bIsFolder;
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}
