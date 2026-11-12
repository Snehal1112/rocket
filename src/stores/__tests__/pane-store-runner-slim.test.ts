import { describe, expect, it } from 'vitest';
import { getRunnerSummary } from '@/lib/runner-summary';
import type { ExecuteRequestResponse, Request } from '@/lib/tauri-api';
import { FULL_RESULT_PASSES, slimOldPasses, slimRunnerEntry } from '@/stores/pane-store';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

const result: ExecuteRequestResponse = {
  status: 200,
  statusText: 'OK',
  headers: [{ key: 'a', value: 'b', enabled: true }],
  body: 'x'.repeat(1000),
  durationMs: 12,
  ttfbMs: 5,
  sizeBytes: 1000,
  testResults: [{ name: 't', status: 'failed', error: 'no' }],
  consoleEntries: [{ level: 'log', message: 'hi' }],
  scriptError: null,
};

type Pass = { iteration: number; requests: RunnerRequestEntry[] };

function entry(status: RunnerRequestEntry['status']): RunnerRequestEntry {
  return {
    requestPath: 'r',
    request: { name: 'r', method: 'GET' } as Request,
    included: true,
    status,
    result,
  };
}

describe('runner pass slimming', () => {
  it('slimming drops heavy fields and keeps what the list reads', () => {
    const slim = slimRunnerEntry(entry('failed')).result;
    expect(slim?.body).toBe('');
    expect(slim?.headers).toEqual([]);
    expect(slim?.consoleEntries).toEqual([]);
    expect(slim?.sentRequest).toBeUndefined();
    expect(slim?.status).toBe(200);
    expect(slim?.durationMs).toBe(12);
    expect(slim?.testResults).toEqual(result.testResults);
  });

  it('an entry without a result is returned as is', () => {
    const e = { ...entry('skipped'), result: undefined };
    expect(slimRunnerEntry(e)).toBe(e);
  });

  it('keeps the latest passes full and slims the ones that fall out', () => {
    let passes: Pass[] = [];
    for (let i = 0; i < FULL_RESULT_PASSES + 2; i++) {
      passes = slimOldPasses([...passes, { iteration: i, requests: [entry('passed')] }]);
    }
    const bodies = passes.map((p) => p.requests[0].result?.body);
    expect(bodies.slice(0, 2)).toEqual(['', '']);
    for (const b of bodies.slice(2)) expect(b).not.toBe('');
  });

  it('already slim passes keep their identity', () => {
    let passes: Pass[] = [];
    for (let i = 0; i <= FULL_RESULT_PASSES; i++) {
      passes = slimOldPasses([...passes, { iteration: i, requests: [entry('passed')] }]);
    }
    const first = passes[0];
    passes = slimOldPasses([...passes, { iteration: 9, requests: [entry('passed')] }]);
    expect(passes[0]).toBe(first);
  });

  it('summary counts are the same after slimming', () => {
    const full: Pass[] = [{ iteration: 0, requests: [entry('passed'), entry('failed')] }];
    const slim: Pass[] = [{ iteration: 0, requests: full[0].requests.map(slimRunnerEntry) }];
    const tab = (completedPasses: Pass[]) =>
      ({
        tabType: 'runner',
        runState: 'done',
        requests: [entry('pending')],
        iterationData: { columns: [], rows: [{}], source: 'perRun', fileName: 'a.csv' },
        completedPasses,
      }) as unknown as RunnerTab;
    expect(getRunnerSummary(tab(slim))).toEqual(getRunnerSummary(tab(full)));
  });
});
