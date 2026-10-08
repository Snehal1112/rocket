import { describe, expect, it } from 'vitest';
import type { FlowRunFinishedEvent, FlowRunSummary, FlowStepResult } from '@/lib/tauri-api';
import type { FlowLastRun } from '@/types/pane-types';
import {
  formatRunDuration,
  mergeRunResult,
  resultFromFinishedEvent,
  summarizeRun,
} from '../flow-run-result';

const step = (
  nodeId: string,
  status: FlowStepResult['status'],
  over: Partial<FlowStepResult> = {},
): FlowStepResult => ({
  nodeId,
  status,
  statusCode: null,
  durationMs: null,
  error: null,
  value: null,
  ...over,
});

const summary = (
  steps: FlowStepResult[],
  stoppedReason = 'completed',
): FlowRunSummary => ({ runId: 'run-1', steps, stoppedReason });

describe('summarizeRun', () => {
  it('reports a clean run with no failed node', () => {
    const result = summarizeRun(summary([step('a', 'success'), step('b', 'success')]), 1200);
    expect(result).toEqual({
      runId: 'run-1',
      stoppedReason: 'completed',
      totalMs: 1200,
      failedCount: 0,
      skippedCount: 0,
    });
    expect(result).not.toHaveProperty('failedNodeId');
  });

  it('names the first failed step and counts every failure', () => {
    const result = summarizeRun(
      summary([
        step('a', 'success'),
        step('b', 'failed', { error: 'boom' }),
        step('c', 'failed', { error: 'later' }),
      ]),
      10,
    );
    expect(result.failedNodeId).toBe('b');
    expect(result.failedCount).toBe(2);
  });

  it('counts upstream skips, leaves out not-taken branches, and treats a missing reason as upstream', () => {
    const result = summarizeRun(
      summary([
        step('a', 'failed', { error: 'x' }),
        step('b', 'skipped', { skipReason: 'upstream_failed' }),
        step('c', 'skipped'),
        step('d', 'skipped', { skipReason: 'branch_not_taken' }),
      ]),
      null,
    );
    expect(result.skippedCount).toBe(2);
    expect(result.totalMs).toBeNull();
  });

  it('blames no node and counts nothing for a cancelled run', () => {
    const result = summarizeRun(
      summary(
        [step('a', 'success'), step('b', 'failed', { error: 'cancelled' }), step('c', 'skipped')],
        'cancelled',
      ),
      500,
    );
    expect(result).toEqual({
      runId: 'run-1',
      stoppedReason: 'cancelled',
      totalMs: 500,
      failedCount: 0,
      skippedCount: 0,
    });
  });
});

const finished = (over: Partial<FlowRunFinishedEvent> = {}): FlowRunFinishedEvent => ({
  type: 'flowRunFinished',
  run_id: 'run-9',
  stopped_reason: 'completed',
  node_count: 6,
  failed_count: 1,
  skipped_count: 3,
  not_taken_count: 2,
  ...over,
});

describe('resultFromFinishedEvent', () => {
  it('uses the counts only, with no timing and no failed node', () => {
    expect(resultFromFinishedEvent(finished())).toEqual({
      runId: 'run-9',
      stoppedReason: 'completed',
      totalMs: null,
      failedCount: 1,
      skippedCount: 1,
    });
  });

  it('treats a missing not_taken_count as zero and never goes negative', () => {
    expect(resultFromFinishedEvent(finished({ not_taken_count: undefined })).skippedCount).toBe(3);
    expect(resultFromFinishedEvent(finished({ skipped_count: 1, not_taken_count: 4 })).skippedCount).toBe(0);
  });

  it('zeroes the counts of a cancelled run', () => {
    const result = resultFromFinishedEvent(finished({ stopped_reason: 'cancelled' }));
    expect(result.failedCount).toBe(0);
    expect(result.skippedCount).toBe(0);
  });
});

describe('mergeRunResult', () => {
  const timed: FlowLastRun = {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: 900,
    failedNodeId: 'b',
    failedLabel: 'Login',
    failedCount: 1,
    skippedCount: 0,
  };
  const countsOnly: FlowLastRun = {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: null,
    failedCount: 1,
    skippedCount: 0,
  };

  it('keeps the timed result when a counts-only one arrives for the same run', () => {
    expect(mergeRunResult(timed, countsOnly)).toBe(timed);
  });

  it('replaces a counts-only result with the timed one for the same run', () => {
    expect(mergeRunResult(countsOnly, timed)).toBe(timed);
  });

  it('takes the new result for a different run, or when there is none yet', () => {
    expect(mergeRunResult(timed, { ...countsOnly, runId: 'run-2' }).runId).toBe('run-2');
    expect(mergeRunResult(undefined, countsOnly)).toBe(countsOnly);
  });
});

describe('formatRunDuration', () => {
  it('formats milliseconds, seconds and minutes', () => {
    expect(formatRunDuration(0)).toBe('0 ms');
    expect(formatRunDuration(850)).toBe('850 ms');
    expect(formatRunDuration(2300)).toBe('2.3 s');
    expect(formatRunDuration(125_000)).toBe('2 m 5 s');
  });

  it('switches to minutes where seconds would round to 60.0 s', () => {
    expect(formatRunDuration(999)).toBe('999 ms');
    expect(formatRunDuration(1000)).toBe('1.0 s');
    expect(formatRunDuration(59_949)).toBe('59.9 s');
    expect(formatRunDuration(59_950)).toBe('1 m 0 s');
    expect(formatRunDuration(59_999)).toBe('1 m 0 s');
    expect(formatRunDuration(60_000)).toBe('1 m 0 s');
    for (const ms of [999, 1000, 59_949, 59_950, 59_999, 60_000]) {
      expect(formatRunDuration(ms)).not.toContain('60.0 s');
    }
  });
});
