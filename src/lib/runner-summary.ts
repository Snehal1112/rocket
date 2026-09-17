import type { RunnerTab } from '@/types/pane-types';

export interface RunnerSummary {
  included: number;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
}

// Pure aggregate derived from a RunnerTab's current requests — no
// separate summary state to keep in sync with the run.
export function getRunnerSummary(tab: RunnerTab): RunnerSummary {
  let included = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  for (const e of tab.requests) {
    if (e.included) included += 1;
    if (e.status === 'passed') passed += 1;
    if (e.status === 'failed') failed += 1;
    if (e.status === 'skipped') skipped += 1;
  }
  return { included, total: tab.requests.length, passed, failed, skipped };
}
