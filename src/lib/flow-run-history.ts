import type { FlowRunRecord, FlowTab } from '@/types/pane-types';

export const MAX_RUN_HISTORY = 5;

// Adds a record, newest first. A record of a run that is already in the list
// replaces it in place, because the finished event and the final summary can
// both report the same run.
export function appendRunRecord(
  history: FlowRunRecord[] | undefined,
  record: FlowRunRecord,
): FlowRunRecord[] {
  const current = history ?? [];
  const index = current.findIndex((r) => r.runId === record.runId);
  if (index >= 0) return current.map((r, i) => (i === index ? record : r));
  return [record, ...current].slice(0, MAX_RUN_HISTORY);
}

// Snapshots the tab's results for one finished run. The store never mutates
// these maps, it replaces them, so sharing the references is a safe copy.
// Returns null unless the tab's result is that run's and the tab has not moved
// on to another run.
export function buildRunRecord(
  tab: FlowTab | null,
  runId: string,
  finishedAt: number,
): FlowRunRecord | null {
  if (!tab?.lastRun || tab.lastRun.runId !== runId) return null;
  if (tab.runId !== undefined && tab.runId !== runId) return null;
  return {
    runId,
    finishedAt,
    environmentName: tab.lastRun.environmentName ?? null,
    result: tab.lastRun,
    nodeStatus: tab.nodeStatus,
    nodeDetail: tab.nodeDetail ?? {},
  };
}

const pad = (n: number) => String(n).padStart(2, '0');

// Local time as HH:MM:SS. Written by hand so the format never depends on the locale.
export function formatClock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function outcomeLabel(record: FlowRunRecord): string {
  const { stoppedReason, failedCount } = record.result;
  if (stoppedReason !== 'completed') return stoppedReason;
  return failedCount > 0 ? `${failedCount} failed` : 'completed';
}

// Text of a selector entry, such as "14:02:11 · 2 failed · staging".
export function runRecordLabel(record: FlowRunRecord): string {
  const parts = [formatClock(record.finishedAt), outcomeLabel(record)];
  if (record.environmentName) parts.push(record.environmentName);
  return parts.join(' · ');
}
