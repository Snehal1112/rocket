import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { errorSuffix, flowNodeName } from './flowA11y';
import { nodeStatusLabel } from './nodes/nodeStatus';

export type AnnounceRunState = 'idle' | 'running' | 'done';

export interface AnnounceSnapshot {
  runState: AnnounceRunState;
  status: Record<string, FlowNodeStatus>;
}

export interface Announcements {
  // Spoken politely, in order.
  polite: string[];
  // Failures, spoken at once.
  alerts: string[];
  // True when a new run began, so the owner can clear the last failures.
  runStarted: boolean;
}

// More node results than this in one update (for example the final summary
// landing at once) are replaced by the run summary.
export const MAX_NODE_MESSAGES = 5;

const TERMINAL: ReadonlySet<FlowNodeStatus> = new Set<FlowNodeStatus>([
  'success',
  'failed',
  'skipped',
]);

// Compares two snapshots and says what changed. A node that is only `running`
// is not announced. Failures go to `alerts`, never to `polite`.
export function diffAnnouncements(
  prev: AnnounceSnapshot,
  next: AnnounceSnapshot,
  nodes: FlowNode[],
  nodeDetail?: Record<string, FlowNodeDetail>,
): Announcements {
  const polite: string[] = [];
  const failures: string[] = [];
  const results: string[] = [];
  const runStarted = prev.runState !== 'running' && next.runState === 'running';
  if (runStarted) polite.push('Run started.');

  const byId = new Map(nodes.map((n) => [n.id, n]));
  for (const [id, status] of Object.entries(next.status)) {
    if (!TERMINAL.has(status) || prev.status[id] === status) continue;
    const node = byId.get(id);
    if (!node) continue;
    const name = flowNodeName(node.kind);
    const detail = nodeDetail?.[id];
    if (status === 'failed') {
      failures.push(`${name} failed${errorSuffix(detail?.error)}.`);
    } else {
      results.push(`${name} ${nodeStatusLabel(status, detail)}.`);
    }
  }
  if (results.length <= MAX_NODE_MESSAGES) polite.push(...results);

  if (prev.runState === 'running' && next.runState === 'done') {
    const statuses = nodes.map((n) => next.status[n.id]);
    const count = (s: FlowNodeStatus) => statuses.filter((x) => x === s).length;
    polite.push(
      `Run finished: ${count('success')} succeeded, ${count('failed')} failed, ${count('skipped')} skipped.`,
    );
  }

  const alerts = failures.slice(0, MAX_NODE_MESSAGES);
  if (failures.length > MAX_NODE_MESSAGES) {
    alerts.push(`and ${failures.length - MAX_NODE_MESSAGES} more failed.`);
  }
  return { polite, alerts, runStarted };
}
