import { describe, expect, it } from 'vitest';
import { getRunnerSummary } from '@/lib/runner-summary';
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
});
