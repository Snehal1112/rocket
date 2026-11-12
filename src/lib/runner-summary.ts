import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

export interface RunnerSummary {
  included: number;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
}

export interface RunnerPass {
  iteration: number;
  requests: RunnerRequestEntry[];
}

// The passes of a data-file run, oldest first: the finished ones, then the one
// that is running. Null when the run has no data file. A finished pass keeps
// its object, so views can skip it.
export function getRunnerPasses(tab: RunnerTab): RunnerPass[] | null {
  if (!tab.iterationData && !tab.completedPasses) return null;
  const passes = [...(tab.completedPasses ?? [])];
  if (tab.runState === 'running') {
    passes.push({ iteration: tab.iterationIndex ?? passes.length, requests: tab.requests });
  }
  return passes;
}

interface Counts {
  passed: number;
  failed: number;
  skipped: number;
}

function countEntries(requests: RunnerRequestEntry[], includedOnly: boolean): Counts {
  const counts = { passed: 0, failed: 0, skipped: 0 };
  for (const e of requests) {
    if (includedOnly && !e.included) continue;
    if (e.status === 'passed') counts.passed += 1;
    if (e.status === 'failed') counts.failed += 1;
    if (e.status === 'skipped') counts.skipped += 1;
  }
  return counts;
}

// A closed pass never changes, so its counts are computed once.
const closedPassCounts = new WeakMap<RunnerRequestEntry[], Counts>();

function closedCounts(requests: RunnerRequestEntry[]): Counts {
  let counts = closedPassCounts.get(requests);
  if (!counts) {
    counts = countEntries(requests, true);
    closedPassCounts.set(requests, counts);
  }
  return counts;
}

// Pure aggregate derived from a RunnerTab's current requests (and its passes,
// with a data file) — no separate summary state to keep in sync with the run.
export function getRunnerSummary(tab: RunnerTab): RunnerSummary {
  const rowCount = tab.iterationData?.rows.length ?? 1;
  let included = 0;
  for (const e of tab.requests) {
    if (e.included) included += 1;
  }

  let counts: Counts;
  if (tab.iterationData || tab.completedPasses) {
    // Only the running pass is read live. Closed passes come from the cache.
    counts =
      tab.runState === 'running'
        ? countEntries(tab.requests, true)
        : { passed: 0, failed: 0, skipped: 0 };
    for (const pass of tab.completedPasses ?? []) {
      const closed = closedCounts(pass.requests);
      counts.passed += closed.passed;
      counts.failed += closed.failed;
      counts.skipped += closed.skipped;
    }
  } else {
    counts = countEntries(tab.requests, false);
  }

  return {
    included: included * rowCount,
    total: tab.requests.length * rowCount,
    passed: counts.passed,
    failed: counts.failed,
    skipped: counts.skipped,
  };
}
