import type { OpenApiSyncStatus } from '@/lib/tauri-api';

export interface ChipState {
  kind: 'needs-approval' | 'failed' | 'updates' | 'off' | 'connected';
  label: string;
  detail?: string;
}

/** The one status the header chip shows. The first match wins. */
export function chipState(status: OpenApiSyncStatus, waiting: number): ChipState {
  if (status.consent !== 'granted') return { kind: 'needs-approval', label: 'Needs approval' };
  if (status.lastStatus?.state === 'failed') {
    return { kind: 'failed', label: 'Check failed', detail: status.lastMessage };
  }
  if (waiting > 0) {
    return { kind: 'updates', label: waiting === 1 ? '1 update' : `${waiting} updates` };
  }
  if (status.interval === 'off') return { kind: 'off', label: 'Checks off' };
  return { kind: 'connected', label: 'Connected' };
}
