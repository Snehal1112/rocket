import type { OpenApiSyncStatus } from '@/lib/tauri-api';

export interface ChipState {
  kind: 'needs-approval' | 'failed' | 'updates' | 'off' | 'connected';
  label: string;
  detail?: string;
}

/**
 * The one status the header chip shows. The first match wins. A null `waiting` means the count
 * is not known yet, for example while the plan loads or when it cannot be built.
 */
export function chipState(status: OpenApiSyncStatus, waiting: number | null): ChipState {
  if (status.consent !== 'granted') return { kind: 'needs-approval', label: 'Needs approval' };
  if (status.lastStatus?.state === 'failed') {
    return { kind: 'failed', label: 'Check failed', detail: status.lastMessage };
  }
  if (waiting === null && status.hasPending) return { kind: 'updates', label: 'Updates waiting' };
  if (waiting !== null && waiting > 0) {
    return { kind: 'updates', label: waiting === 1 ? '1 update' : `${waiting} updates` };
  }
  if (status.interval === 'off') return { kind: 'off', label: 'Checks off' };
  return { kind: 'connected', label: 'Connected' };
}
