import { peerName } from '@/lib/network-utils';
import type { ConflictKind, LinkState } from '@/lib/tauri-api';

/** One line about the state of a link, for menus and lists. */
export function linkStatusText(state: LinkState, displayName: string): string {
  const name = peerName(displayName);
  switch (state) {
    case 'in_sync':
      return `In sync with ${name}`;
    case 'changes_waiting':
      return `Changes waiting to sync with ${name}`;
    case 'out_of_date':
      return `Out of date with ${name}. Sync again.`;
    case 'broken':
      return `Link to ${name} is broken. Unlink and send the collection again.`;
    default:
      // A state added later gets a neutral text.
      return `Linked with ${name}`;
  }
}

/** "2 added, 1 deleted". Zero counts are left out. */
export function syncCountsText(added: number, changed: number, deleted: number): string {
  const parts: string[] = [];
  if (added > 0) parts.push(`${added} added`);
  if (changed > 0) parts.push(`${changed} changed`);
  if (deleted > 0) parts.push(`${deleted} deleted`);
  return parts.length > 0 ? parts.join(', ') : 'no changes';
}

export function conflictKindText(kind: ConflictKind): string {
  switch (kind) {
    case 'delete_vs_edit':
      return 'Deleted on one node, changed on the other';
    case 'both_added':
      return 'Added on both nodes with different content';
    case 'rename_vs_edit':
      return 'Renamed on one node, changed on the other';
    default:
      return 'Changed on both nodes';
  }
}

/**
 * Keep both is refused by the backend for collection and folder settings. The rule matches
 * `keep_both_allowed` in `rocket-app`.
 */
export function keepBothAllowed(path: string): boolean {
  const lower = path.toLowerCase();
  const last = lower.split('/').pop() ?? lower;
  return lower !== 'opencollection.yml' && last !== 'folder.yml';
}

// Separate ids keep the result toast from inheriting the Cancel action of the waiting toast.
export const syncResultToastId = (syncId: string): string => `network-sync-result-${syncId}`;
export const syncWaitingToastId = (syncId: string): string => `network-sync-waiting-${syncId}`;

export const SAFE_SANDBOX_NOTE =
  "Changed scripts run with this collection's current script access. A sync never changes trust settings.";
