import { describe, expect, it } from 'vitest';
import type { FlowLastRun, FlowRunRecord, FlowTab } from '@/types/pane-types';
import { flowPayloadFromTab } from '../flow-save';
import {
  appendRunRecord,
  buildRunRecord,
  formatClock,
  MAX_RUN_HISTORY,
  runRecordLabel,
} from '../flow-run-history';

const result = (over: Partial<FlowLastRun> = {}): FlowLastRun => ({
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 1000,
  failedCount: 0,
  skippedCount: 0,
  ...over,
});

const record = (runId: string, over: Partial<FlowRunRecord> = {}): FlowRunRecord => ({
  runId,
  finishedAt: new Date(2026, 9, 8, 14, 2, 11).getTime(),
  environmentName: null,
  result: result({ runId }),
  nodeStatus: {},
  nodeDetail: {},
  ...over,
});

const tab = (over: Partial<FlowTab> = {}): FlowTab => ({
  id: 't1',
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: { a: 'success' },
  nodeDetail: { a: { durationMs: 5 } },
  runState: 'running',
  runId: 'run-1',
  lastRun: result({ environmentName: 'dev' }),
  ...over,
});

describe('appendRunRecord', () => {
  it('puts the newest run first and starts from no history', () => {
    expect(appendRunRecord(undefined, record('r1')).map((r) => r.runId)).toEqual(['r1']);
    const next = appendRunRecord([record('r1')], record('r2'));
    expect(next.map((r) => r.runId)).toEqual(['r2', 'r1']);
  });

  it('keeps only the newest MAX_RUN_HISTORY runs', () => {
    let history: FlowRunRecord[] | undefined;
    for (let i = 1; i <= MAX_RUN_HISTORY + 2; i += 1) {
      history = appendRunRecord(history, record(`r${i}`));
    }
    expect(history).toHaveLength(MAX_RUN_HISTORY);
    expect(history?.[0].runId).toBe(`r${MAX_RUN_HISTORY + 2}`);
    expect(history?.some((r) => r.runId === 'r1')).toBe(false);
  });

  it('replaces a record of the same run in place instead of adding a second one', () => {
    const history = [record('r3'), record('r2'), record('r1')];
    const replaced = record('r2', { environmentName: 'prod' });
    const next = appendRunRecord(history, replaced);
    expect(next.map((r) => r.runId)).toEqual(['r3', 'r2', 'r1']);
    expect(next[1].environmentName).toBe('prod');
  });
});

describe('buildRunRecord', () => {
  it('snapshots the tab maps and takes the environment from the result', () => {
    const t = tab();
    const built = buildRunRecord(t, 'run-1', 123);
    expect(built).toEqual({
      runId: 'run-1',
      finishedAt: 123,
      environmentName: 'dev',
      result: t.lastRun,
      nodeStatus: { a: 'success' },
      nodeDetail: { a: { durationMs: 5 } },
    });
  });

  it('uses empty maps and a null environment when the tab has none', () => {
    const built = buildRunRecord(tab({ nodeDetail: undefined, lastRun: result() }), 'run-1', 1);
    expect(built?.nodeDetail).toEqual({});
    expect(built?.environmentName).toBeNull();
  });

  it('returns null without a tab, without a result, or when the run ids do not match', () => {
    expect(buildRunRecord(null, 'run-1', 1)).toBeNull();
    expect(buildRunRecord(tab({ lastRun: undefined }), 'run-1', 1)).toBeNull();
    // A late result of an older run must not capture a newer run's maps.
    expect(buildRunRecord(tab({ runId: 'run-2' }), 'run-1', 1)).toBeNull();
    expect(buildRunRecord(tab({ lastRun: result({ runId: 'run-0' }) }), 'run-1', 1)).toBeNull();
  });

  it('still records when the tab never learned the run id', () => {
    expect(buildRunRecord(tab({ runId: undefined }), 'run-1', 1)?.runId).toBe('run-1');
  });
});

describe('formatClock', () => {
  it('formats local time as HH:MM:SS with zero padding', () => {
    expect(formatClock(new Date(2026, 9, 8, 14, 2, 11).getTime())).toBe('14:02:11');
    expect(formatClock(new Date(2026, 9, 8, 3, 4, 5).getTime())).toBe('03:04:05');
  });
});

describe('runRecordLabel', () => {
  const clock = formatClock(record('x').finishedAt);

  it('says completed for a clean run and counts failures otherwise', () => {
    expect(runRecordLabel(record('r1'))).toBe(`${clock} · completed`);
    expect(runRecordLabel(record('r1', { result: result({ failedCount: 2 }) }))).toBe(
      `${clock} · 2 failed`,
    );
  });

  it('labels cancelled, error and unknown stop reasons', () => {
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'cancelled' }) }))).toBe(
      `${clock} · cancelled`,
    );
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'error' }) }))).toBe(
      `${clock} · error`,
    );
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'timeout' }) }))).toBe(
      `${clock} · timeout`,
    );
  });

  it('appends the environment when there is one', () => {
    expect(runRecordLabel(record('r1', { environmentName: 'staging' }))).toBe(
      `${clock} · completed · staging`,
    );
  });
});

describe('what Save writes', () => {
  it('never includes history, the viewed run or the last result', () => {
    const t = tab({
      runHistory: [record('r1')],
      viewedRunId: 'r1',
      nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
    });
    expect(flowPayloadFromTab(t)).toEqual({
      collection: 'demo',
      flow: { name: 'my-flow', nodes: t.nodes, edges: [] },
    });
  });
});
