import { useWorkspaceStore } from '@/stores/workspace-store';

/**
 * Records the active workspace now. The returned check is true while that workspace is
 * still active. A callback that can run late (an OAuth2 browser flow finishing after the
 * user switched) uses it to drop data that belongs to the old workspace.
 */
export function captureWorkspace(): () => boolean {
  const id = useWorkspaceStore.getState().activeWorkspaceId;
  return () => useWorkspaceStore.getState().activeWorkspaceId === id;
}
