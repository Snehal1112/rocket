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

/**
 * Client issues first, then backend issues. A client issue the backend also
 * reports, by code, node and edge, is dropped in favour of the backend one.
 */
export function mergeFlowIssues(client: FlowIssue[], backend: FlowIssue[]): FlowIssue[] {
  const backendKeys = new Set(backend.map(issueKey));
  return [...client.filter((issue) => !backendKeys.has(issueKey(issue))), ...backend];
}
