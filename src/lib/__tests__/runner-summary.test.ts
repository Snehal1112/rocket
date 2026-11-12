import { describe, expect, it } from 'vitest';
import { getRunnerPasses, getRunnerSummary } from '@/lib/runner-summary';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

function makeTab(requests: RunnerRequestEntry[]): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState: 'done',
    requests,
  };
}

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'x.yml',
    request: {
      uid: 'x',
      name: 'X',
      method: 'GET',
      url: '',
      headers: [],
      auth: { authType: 'none' },
    },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

describe('getRunnerSummary', () => {
  it('returns all zeros for an empty run', () => {
    expect(getRunnerSummary(makeTab([]))).toEqual({
      included: 0,
      total: 0,
      passed: 0,
      failed: 0,
      skipped: 0,
    });
  });

  it('counts total regardless of inclusion, but included only counts included entries', () => {
    const tab = makeTab([
      entry({ included: true, status: 'passed' }),
      entry({ included: false, status: 'pending' }),
    ]);
    expect(getRunnerSummary(tab)).toMatchObject({ total: 2, included: 1 });
  });

  it('counts passed, failed, and skipped independently', () => {
    const tab = makeTab([
      entry({ status: 'passed' }),
      entry({ status: 'passed' }),
      entry({ status: 'failed' }),
      entry({ status: 'skipped' }),
      entry({ status: 'running' }),
    ]);
    expect(getRunnerSummary(tab)).toMatchObject({ passed: 2, failed: 1, skipped: 1 });
  });

  describe('with a data file', () => {
    const data = {
      columns: ['id'],
      rows: [{}, {}, {}, {}],
      source: 'perRun' as const,
      fileName: 'r.csv',
    };
    const reqs = (statuses: RunnerRequestEntry['status'][], excludeLast = false) =>
      statuses.map((status, i) =>
        entry({
          requestPath: `${i}.yml`,
          status,
          included: !(excludeLast && i === statuses.length - 1),
        }),
      );

    it('counts closed passes plus the running pass, over included entries only', () => {
      const tab = {
        ...makeTab(reqs(['passed', 'running', 'pending'], true)),
        runState: 'running' as const,
        iterationData: data,
        iterationIndex: 2,
        completedPasses: [
          { iteration: 0, requests: reqs(['passed', 'passed', 'skipped'], true) },
          { iteration: 1, requests: reqs(['passed', 'failed', 'skipped'], true) },
        ],
      };
      expect(getRunnerSummary(tab)).toEqual({
        included: 8,
        total: 12,
        passed: 4,
        failed: 1,
        skipped: 0,
      });
      expect(getRunnerPasses(tab)?.map((p) => p.iteration)).toEqual([0, 1, 2]);
    });

    it('does not count the open requests again once the run has finished', () => {
      const last = reqs(['passed', 'passed']);
      const tab = {
        ...makeTab(last),
        runState: 'stopped' as const,
        iterationData: { ...data, rows: [{}, {}] },
        completedPasses: [
          { iteration: 0, requests: last },
          { iteration: 1, requests: reqs(['skipped', 'skipped']) },
        ],
      };
      const summary = getRunnerSummary(tab);
      expect(summary).toMatchObject({ included: 4, passed: 2, skipped: 2 });
      expect(summary.passed + summary.failed + summary.skipped).toBeLessThanOrEqual(
        summary.included,
      );
      expect(getRunnerPasses(tab)).toHaveLength(2);
    });

    it('returns no passes without a data file', () => {
      expect(getRunnerPasses(makeTab([]))).toBeNull();
    });
  });
});
