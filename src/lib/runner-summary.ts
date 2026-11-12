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
// that is running. Null when the run has no data file.
export function getRunnerPasses(tab: RunnerTab): RunnerPass[] | null {
  if (!tab.iterationData && !tab.completedPasses) return null;
  const passes = [...(tab.completedPasses ?? [])];
  if (tab.runState === 'running') {
    passes.push({ iteration: tab.iterationIndex ?? passes.length, requests: tab.requests });
  }
  return passes;
}

// Pure aggregate derived from a RunnerTab's current requests (and its passes,
// with a data file) — no separate summary state to keep in sync with the run.
export function getRunnerSummary(tab: RunnerTab): RunnerSummary {
  const passes = getRunnerPasses(tab);
  const rowCount = tab.iterationData?.rows.length ?? 1;
  let included = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  for (const e of tab.requests) {
    if (e.included) included += 1;
  }
  const counted = passes ? passes.flatMap((p) => p.requests) : tab.requests;
  for (const e of counted) {
    if (e.status === 'passed') passed += 1;
    if (e.status === 'failed') failed += 1;
    if (e.status === 'skipped') skipped += 1;
  }
  return {
    included: included * rowCount,
    total: tab.requests.length * rowCount,
    passed,
    failed,
    skipped,
  };
}
