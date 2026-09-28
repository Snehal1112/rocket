import { useWorkspaceConfig, useWorkspaces } from '@/lib/queries/workspace-queries';
import type { CollectionReference } from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';

// Resolves a collection name to its filesystem path. Mirrors the embedded-
// collection concatenation already used by useKeyboardShortcuts.ts and
// CollectionNode.tsx (`${workspacePath}/collections/${name}`); an
// external-type collection's own `path` is used directly instead, since it
// isn't necessarily under the workspace's `collections/` folder.
export function resolveCollectionPath(
  collectionName: string,
  workspacePath: string,
  collections: CollectionReference[],
): string {
  const ref = collections.find((c) => c.name === collectionName);
  if (ref?.type === 'external' && ref.path) return ref.path;
  return `${workspacePath}/collections/${collectionName}`;
}

// Resolves the active workspace's collection list and the given collection
// name into a filesystem path, or undefined while either is still loading /
// unavailable (no active workspace, or the name doesn't matter yet).
export function useCollectionPath(collectionName: string | undefined): string | undefined {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: workspaces = [] } = useWorkspaces();
  const { data: workspaceConfig } = useWorkspaceConfig(activeWorkspaceId);
  const workspace = workspaces.find((w) => w.id === activeWorkspaceId);
  if (!collectionName || !workspace) return undefined;
  return resolveCollectionPath(collectionName, workspace.path, workspaceConfig?.collections ?? []);
}
