import { findRequestTab } from '@/lib/assistant/request-tabs';
import type { ReferenceItem } from '@/lib/assistant/types';
import type { CollectionItem, Environment, Folder } from '@/lib/tauri-api';
import type { PaneNode } from '@/types/pane-types';

/** Items the `#` list shows at once. */
export const MAX_REFERENCE_RESULTS = 50;

interface Focus {
  collection: string;
  path: string;
}

/**
 * The collection, its folders and its HTTP requests, with the same paths the sidebar
 * uses (`CollectionNode` and `FolderNode`). Other protocols are left out, as in the sidebar.
 */
export function flattenCollectionTree(collection: string, root: Folder): ReferenceItem[] {
  const out: ReferenceItem[] = [{ kind: 'collection', collection, label: collection }];
  const walk = (items: readonly CollectionItem[], basePath: string) => {
    for (const item of items) {
      if (item.type === 'folder') {
        const dir = item.dirName ?? item.name;
        const path = basePath ? `${basePath}/${dir}` : dir;
        out.push({ kind: 'folder', collection, path, label: item.name });
        walk(item.items, path);
      } else if (
        item.type === 'request' ||
        (item.type === 'summary' && (item.kind ?? 'http') === 'http')
      ) {
        const fileName = item.fileName ?? item.name;
        const path = basePath ? `${basePath}/${fileName}` : fileName;
        out.push({ kind: 'request', collection, path, label: `${item.method} ${item.name}` });
      }
    }
  };
  walk(root.items, '');
  return out;
}

/** One item per environment. The environment name goes in `path`. */
export function environmentReferences(
  collection: string,
  environments: readonly Environment[],
): ReferenceItem[] {
  return environments.map(
    (env): ReferenceItem => ({
      kind: 'environment',
      collection,
      path: env.name,
      label: `env: ${env.name}`,
    }),
  );
}

// Lower is better. -1 means no match.
function score(item: ReferenceItem, query: string): number {
  const label = item.label.toLowerCase();
  if (label.startsWith(query)) return 0;
  if (label.includes(query)) return 1;
  if (`${item.collection}/${item.path ?? ''}`.toLowerCase().includes(query)) return 2;
  return -1;
}

/** The items matching `query`, best first. Equal scores keep the tree order. */
export function filterReferences(
  items: readonly ReferenceItem[],
  query: string,
  limit = MAX_REFERENCE_RESULTS,
): ReferenceItem[] {
  const q = query.trim().toLowerCase();
  if (q === '') return items.slice(0, limit);
  return items
    .map((item) => ({ item, rank: score(item, q) }))
    .filter((entry) => entry.rank >= 0)
    .sort((a, b) => a.rank - b.rank)
    .slice(0, limit)
    .map((entry) => entry.item);
}

function fileLabel(path: string): string {
  const last = path.split('/').pop() ?? path;
  return last.replace(/\.yml$/, '');
}

/** The chip for the focused request, labelled with its tab title when the tab is open. */
export function focusReference(focus: Focus | undefined, root: PaneNode): ReferenceItem | null {
  if (!focus) return null;
  const tab = findRequestTab(root, focus.collection, focus.path);
  return {
    kind: 'request',
    collection: focus.collection,
    path: focus.path,
    label: tab?.title ?? fileLabel(focus.path),
  };
}

/** The focused request's last response, when its open tab holds one. */
export function lastResponseReference(
  focus: Focus | undefined,
  root: PaneNode,
): ReferenceItem | null {
  if (!focus) return null;
  const tab = findRequestTab(root, focus.collection, focus.path);
  if (!tab?.response) return null;
  return {
    kind: 'last-response',
    collection: focus.collection,
    path: focus.path,
    label: `Last response: ${tab.title}`,
  };
}
