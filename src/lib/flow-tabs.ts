import { collectAllTabs } from '@/lib/pane-utils';
import type { FlowTab, PaneNode, Tab } from '@/types/pane-types';
import { isFlowTab } from '@/types/pane-types';

// The shape of the pane store's `collectionTabState`: tabs parked when the user switched collection.
export type TabSnapshots = Record<string, { tabs: Tab[] }>;

/**
 * Every open tab of one flow, in the pane tree and in the collection snapshots.
 * A tab that appears in both is returned once.
 */
export function findFlowTabs(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): FlowTab[] {
  const seen = new Set<string>();
  const found: FlowTab[] = [];
  const candidates = [...collectAllTabs(root), ...Object.values(snapshots).flatMap((e) => e.tabs)];
  for (const tab of candidates) {
    if (!isFlowTab(tab) || seen.has(tab.id)) continue;
    if (tab.collectionName !== collection || tab.flowName !== flowName) continue;
    seen.add(tab.id);
    found.push(tab);
  }
  return found;
}

// True when any tab of the flow is running.
export function isFlowRunning(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): boolean {
  return findFlowTabs(root, snapshots, collection, flowName).some((t) => t.runState === 'running');
}

// True when any tab of the flow has unsaved edits.
export function hasDirtyFlow(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): boolean {
  return findFlowTabs(root, snapshots, collection, flowName).some((t) => t.isDirty);
}
