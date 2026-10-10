import { describe, expect, it } from 'vitest';
import type { FlowLastRun, FlowRunRecord, FlowTab } from '@/types/pane-types';
import { flowPayloadFromTab } from '../flow-save';
import {
  appendRunRecord,
  buildRunRecord,
  FULL_DETAIL_RUNS,
  formatClock,
  MAX_RUN_HISTORY,
  runRecordLabel,
  stripRecordDetail,
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

const heavy = (runId: string): FlowRunRecord =>
  record(runId, {
    nodeDetail: {
      a: {
        durationMs: 3,
        error: 'boom',
        exchange: { method: 'GET', url: 'http://x.test', headers: [] },
        logs: [{ level: 'log', message: 'hello' }],
      },
      b: { durationMs: 1 },
    },
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

describe('stripRecordDetail', () => {
  it('returns the same record when nothing needs stripping', () => {
    const plain = record('r1', { nodeDetail: { a: { durationMs: 2 } } });
    expect(stripRecordDetail(plain)).toBe(plain);
    const empty = record('r2');
    expect(stripRecordDetail(empty)).toBe(empty);
  });

  it('drops exchange and logs without touching the input', () => {
    const full = heavy('r1');
    const stripped = stripRecordDetail(full);
    expect(stripped).not.toBe(full);
    expect(stripped.nodeDetail.a).toEqual({ durationMs: 3, error: 'boom' });
    expect(full.nodeDetail.a.exchange).toBeDefined();
    expect(full.nodeDetail.a.logs).toHaveLength(1);
  });
});

describe('appendRunRecord', () => {
  it('keeps exchange and logs only on the two newest runs and keeps the other detail', () => {
    expect(FULL_DETAIL_RUNS).toBe(2);
    let history: FlowRunRecord[] | undefined;
    for (const id of ['r1', 'r2', 'r3', 'r4']) history = appendRunRecord(history, heavy(id));
    expect(history?.map((r) => r.runId)).toEqual(['r4', 'r3', 'r2', 'r1']);
    expect(history?.[0].nodeDetail.a.exchange).toBeDefined();
    expect(history?.[1].nodeDetail.a.logs).toHaveLength(1);
    for (const old of history?.slice(2) ?? []) {
      expect(old.nodeDetail.a.exchange).toBeUndefined();
      expect(old.nodeDetail.a.logs).toBeUndefined();
      expect(old.nodeDetail.a.durationMs).toBe(3);
      expect(old.nodeDetail.a.error).toBe('boom');
      expect(old.nodeDetail.b).toEqual({ durationMs: 1 });
    }
  });

  it('keeps the first finish time when a run is recorded again', () => {
    const first = record('r1', { finishedAt: 1000 });
    const next = appendRunRecord(
      [first],
      record('r1', { finishedAt: 2000, environmentName: 'prod' }),
    );
    expect(next).toHaveLength(1);
    expect(next[0].finishedAt).toBe(1000);
    expect(next[0].environmentName).toBe('prod');
  });

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

  it('never copies the callback URLs, which carry a bearer token', () => {
    const t = tab({ callbackUrls: { w: 'http://h/cb/SECRET' } });
    const built = buildRunRecord(t, 'run-1', 1);
    expect(built).not.toBeNull();
    expect(JSON.stringify(built)).not.toContain('SECRET');
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
