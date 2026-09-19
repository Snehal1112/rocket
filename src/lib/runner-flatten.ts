import { sortItemsFoldersFirst } from '@/lib/collection-utils';
import type { Collection, CollectionItem, Folder } from '@/lib/tauri-api';
import type { RunnerRequestEntry } from '@/types/pane-types';

export interface RunnerFolderOption {
  path: string;
  label: string;
}

function collectFolderOptions(
  folder: Folder,
  basePath: string,
  parentLabels: string[],
  out: RunnerFolderOption[],
): void {
  const folders = sortItemsFoldersFirst(folder.items).filter((item) => item.type === 'folder');
  for (const child of folders) {
    const segment = child.dirName ?? child.name;
    const path = basePath ? `${basePath}/${segment}` : segment;
    const labels = [...parentLabels, child.name];
    out.push({ path, label: labels.join(' / ') });
    collectFolderOptions(child, path, labels, out);
  }
}

export function getRunnerFolderOptions(collection: Collection): RunnerFolderOption[] {
  const out: RunnerFolderOption[] = [];
  collectFolderOptions(collection.root, '', [], out);
  return out;
}

// Finds the Folder at `folderPath` (relative to the collection root),
// walking the same dirName/name formula FolderNode.tsx uses to build
// paths while rendering the sidebar tree. Returns null if not found.
function findFolder(folder: Folder, basePath: string, targetPath: string): Folder | null {
  if (basePath === targetPath) return folder;
  for (const item of folder.items) {
    if (item.type !== 'folder') continue;
    const childPath = basePath
      ? `${basePath}/${item.dirName ?? item.name}`
      : (item.dirName ?? item.name);
    if (targetPath === childPath || targetPath.startsWith(`${childPath}/`)) {
      const found = findFolder(item, childPath, targetPath);
      if (found) return found;
    }
  }
  return null;
}

// Recursively collects every request under `folder` into `out`, in the
// same folders-first, alphabetical order the sidebar tree renders.
function collect(folder: Folder, basePath: string, out: RunnerRequestEntry[]): void {
  const items: CollectionItem[] = sortItemsFoldersFirst(folder.items);
  for (const item of items) {
    if (item.type === 'folder') {
      const childPath = basePath
        ? `${basePath}/${item.dirName ?? item.name}`
        : (item.dirName ?? item.name);
      collect(item, childPath, out);
    } else if (item.type === 'request') {
      const requestPath = basePath
        ? `${basePath}/${item.fileName ?? item.name}`
        : (item.fileName ?? item.name);
      out.push({
        requestPath,
        request: item,
        included: true,
        status: 'pending',
      });
    }
    // 'summary' items never appear in a getCollection() result (only in
    // getCollectionSummaries()); flattenRunnerEntries is only ever
    // called with a full Collection, so no 'summary' branch is needed.
  }
}

// Flattens a collection (or one folder within it) into an ordered list
// of runnable requests, matching the order the sidebar tree displays.
// Every entry starts out included and pending.
export function flattenRunnerEntries(
  collection: Collection,
  folderPath?: string,
): RunnerRequestEntry[] {
  const startFolder = folderPath ? findFolder(collection.root, '', folderPath) : collection.root;
  if (!startFolder) return [];
  const out: RunnerRequestEntry[] = [];
  collect(startFolder, folderPath ?? '', out);
  return out;
}
