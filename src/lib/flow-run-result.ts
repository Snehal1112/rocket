import type { FlowRunFinishedEvent, FlowRunSummary } from '@/lib/tauri-api';
import type { FlowLastRun } from '@/types/pane-types';

/** What the toolbar reports. `FlowPane` adds the failed node's label. */
export type FlowRunResult = Omit<FlowLastRun, 'failedLabel'>;

const isCancelled = (stoppedReason: string) => stoppedReason === 'cancelled';

// A cancelled run reports its cancelled nodes as failed (roadmap F-11), so it
// names no failed node and carries no counts.
export function summarizeRun(summary: FlowRunSummary, totalMs: number | null): FlowRunResult {
  const base = { runId: summary.runId, stoppedReason: summary.stoppedReason, totalMs };
  if (isCancelled(summary.stoppedReason)) return { ...base, failedCount: 0, skippedCount: 0 };
  const failed = summary.steps.filter((s) => s.status === 'failed');
  const skipped = summary.steps.filter(
    (s) => s.status === 'skipped' && s.skipReason !== 'branch_not_taken',
  );
  const result: FlowRunResult = {
    ...base,
    failedCount: failed.length,
    skippedCount: skipped.length,
  };
  if (failed[0]) result.failedNodeId = failed[0].nodeId;
  return result;
}

// Used by a toolbar that only resumed the run. The event has counts, no steps
// and no timing.
export function resultFromFinishedEvent(event: FlowRunFinishedEvent): FlowRunResult {
  const base = { runId: event.run_id, stoppedReason: event.stopped_reason, totalMs: null };
  if (isCancelled(event.stopped_reason)) return { ...base, failedCount: 0, skippedCount: 0 };
  return {
    ...base,
    failedCount: event.failed_count,
    skippedCount: Math.max(0, event.skipped_count - (event.not_taken_count ?? 0)),
  };
}

// The summary and the finished event can both report the same run. The timed
// summary result is richer, so a counts-only result never replaces it.
export function mergeRunResult(prev: FlowLastRun | undefined, next: FlowLastRun): FlowLastRun {
  if (prev && prev.runId === next.runId && prev.totalMs !== null && next.totalMs === null) {
    return prev;
  }
  return next;
}

export function formatRunDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 59_950) return `${(ms / 1000).toFixed(1)} s`;
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)} m ${total % 60} s`;
}
