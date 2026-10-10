import type { FlowNodeDetail, FlowRunRecord, FlowTab } from '@/types/pane-types';

export const MAX_RUN_HISTORY = 5;

// The newest runs keep the masked request, response and script logs of each
// node. Older records keep the rest of the detail only, so the history stays small.
export const FULL_DETAIL_RUNS = 2;

// Drops `exchange` and `logs` from every node detail of a record. Returns the
// same record when no detail has either, and never changes the input.
export function stripRecordDetail(record: FlowRunRecord): FlowRunRecord {
  let changed = false;
  const nodeDetail: Record<string, FlowNodeDetail> = {};
  for (const [nodeId, detail] of Object.entries(record.nodeDetail)) {
    if (detail.exchange === undefined && detail.logs === undefined) {
      nodeDetail[nodeId] = detail;
      continue;
    }
    const { exchange: _exchange, logs: _logs, ...rest } = detail;
    nodeDetail[nodeId] = rest;
    changed = true;
  }
  return changed ? { ...record, nodeDetail } : record;
}

// Adds a record, newest first. A record of a run that is already in the list
// replaces it in place and keeps the time the run first finished, because the
// finished event and the final summary can both report the same run. Records
// past the newest FULL_DETAIL_RUNS lose their exchange and logs.
export function appendRunRecord(
  history: FlowRunRecord[] | undefined,
  record: FlowRunRecord,
): FlowRunRecord[] {
  const current = history ?? [];
  const index = current.findIndex((r) => r.runId === record.runId);
  const next =
    index >= 0
      ? current.map((r, i) => (i === index ? { ...record, finishedAt: r.finishedAt } : r))
      : [record, ...current].slice(0, MAX_RUN_HISTORY);
  return next.map((r, i) => (i >= FULL_DETAIL_RUNS ? stripRecordDetail(r) : r));
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
