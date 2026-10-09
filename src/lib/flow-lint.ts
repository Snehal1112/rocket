import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowLint } from '@/lib/tauri-api';

/** Wait this long after the last edit before linting in the backend. */
export const LINT_DEBOUNCE_MS = 500;

/** Rules only the backend reports. The client rule set must not repeat them. */
export const BACKEND_LINT_CODES = [
  'invalid_graph',
  'dangling_saved_request',
  'unknown_variable',
  'exit_without_edge',
  'switch_without_default',
  'no_path_to_output',
  'auth_no_effect',
  'auth_wire_overrides_auth',
  'callback_not_wired',
] as const;

// A rejected save names the same structural rule as an invalid_graph lint.
const CODE_ALIASES: Record<string, string> = { save: 'invalid_graph' };

const issueKey = (issue: FlowIssue) =>
  `${CODE_ALIASES[issue.code] ?? issue.code}|${issue.nodeId ?? ''}|${issue.edgeId ?? ''}`;

/** Maps one backend lint to the client issue shape, leaving out absent keys. */
export function toFlowIssue(lint: FlowLint): FlowIssue {
  return {
    code: lint.code,
    severity: lint.severity,
    ...(lint.nodeId ? { nodeId: lint.nodeId } : {}),
    ...(lint.edgeId ? { edgeId: lint.edgeId } : {}),
    message: lint.message,
    ...(lint.hint ? { hint: lint.hint } : {}),
  };
}

/** The ids the current graph holds, so lints on deleted items can be dropped. */
export interface GraphIds {
  nodeIds: ReadonlySet<string>;
  edgeIds: ReadonlySet<string>;
}

/**
 * Errors first, then warnings; inside one severity the client issues come
 * before the backend ones. A client issue the backend also reports, by code,
 * node and edge, is dropped in favour of the backend one. A backend
 * invalid_graph on an item the client already flags as an error is dropped,
 * because the client copy of the rule is more specific. Backend issues on
 * items missing from `present` are dropped.
 */
export function mergeFlowIssues(
  client: FlowIssue[],
  backend: FlowIssue[],
  present?: GraphIds,
): FlowIssue[] {
  const live = present
    ? backend.filter(
        (i) =>
          (!i.nodeId || present.nodeIds.has(i.nodeId)) &&
          (!i.edgeId || present.edgeIds.has(i.edgeId)),
      )
    : backend;
  // A rejected save is not a client rule, so it does not hide the backend lint.
  const clientErrorSpots = new Set(
    client
      .filter((i) => i.severity === 'error' && i.code !== 'save')
      .map((i) => `${i.nodeId ?? ''}|${i.edgeId ?? ''}`),
  );
  const kept = live.filter(
    (i) =>
      i.code !== 'invalid_graph' || !clientErrorSpots.has(`${i.nodeId ?? ''}|${i.edgeId ?? ''}`),
  );
  const keptKeys = new Set(kept.map(issueKey));
  const merged = [...client.filter((i) => !keptKeys.has(issueKey(i))), ...kept];
  const rank = (i: FlowIssue) => (i.severity === 'error' ? 0 : 1);
  return merged.sort((a, b) => rank(a) - rank(b));
}
