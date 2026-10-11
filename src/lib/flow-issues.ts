import {
  isValidCallbackName,
  MAX_CALLBACK_TIMEOUT_MS,
  MIN_CALLBACK_TIMEOUT_MS,
} from '@/lib/flow-callback';
import { TRIGGER_HANDLE, takesSingleInput } from '@/lib/flow-handles';
import { parseGraphProblems } from '@/lib/flow-wiring';
import type { FlowEdge, FlowNode, FlowNodeKind, RepeatUntil } from '@/lib/tauri-api';

export type IssueSeverity = 'error' | 'warning';

// The same shape the backend lint feed (plan P21) will use, so the two merge by
// `(code, nodeId)` later.
export interface FlowIssue {
  code: string;
  severity: IssueSeverity;
  nodeId?: string;
  edgeId?: string;
  message: string;
  hint?: string;
}

// What the last rejected save named. `message` is the full error text.
export interface SaveErrorInfo {
  nodeIds: string[];
  edgeIds: string[];
  message: string | null;
  /** Where the error came from. A refused run reads differently from a refused save. */
  kind?: 'save' | 'run';
}

export interface FlowIssueContext {
  save?: SaveErrorInfo;
}

// These limits must match rocket_flow::node (RepeatUntil) and flow-callback.ts.
const REPEAT_MIN_INTERVAL_MS = 100;
const REPEAT_MAX_ATTEMPTS = 1000;
const REPEAT_MAX_TIMEOUT_MS = 3_600_000;

const KIND_NAMES: Record<FlowNodeKind['kind'], string> = {
  Request: 'Request',
  Input: 'Input',
  Output: 'Output',
  If: 'If',
  Switch: 'Switch',
  WaitForCallback: 'Wait for callback',
  Transform: 'Transform',
  Auth: 'Auth',
};

function issue(
  code: string,
  severity: IssueSeverity,
  node: FlowNode,
  message: string,
  hint?: string,
): FlowIssue {
  return { code, severity, nodeId: node.id, message, ...(hint ? { hint } : {}) };
}

const hasWire = (edges: FlowEdge[], nodeId: string, field: string) =>
  edges.some((e) => e.targetNodeId === nodeId && e.targetField === field);

function expressionIssues(node: FlowNode): FlowIssue[] {
  const { kind } = node;
  let text: string | null = null;
  let field = '';
  if (kind.kind === 'If') {
    text = kind.condition;
    field = 'condition';
  } else if (kind.kind === 'Switch') {
    text = kind.value;
    field = 'value';
  } else if (kind.kind === 'Transform') {
    text = kind.script;
    field = 'script';
  }
  if (text === null || text.trim() !== '') return [];
  return [
    issue(
      'expr-blank',
      'error',
      node,
      `The ${KIND_NAMES[kind.kind]} node's ${field} is empty.`,
      'Open the node and enter an expression.',
    ),
  ];
}

function inputIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  if (!takesSingleInput(node.kind)) return [];
  if (edges.some((e) => e.targetNodeId === node.id)) return [];
  return [
    issue(
      'input-missing',
      'error',
      node,
      `The ${KIND_NAMES[node.kind.kind]} node needs an input wire.`,
      'Drag a wire from another node into its input.',
    ),
  ];
}

// Wires into an Output that carry a value. A "Run when" trigger wire carries none.
const valueWireCount = (edges: FlowEdge[], nodeId: string) =>
  edges.filter((e) => e.targetNodeId === nodeId && e.targetField !== TRIGGER_HANDLE).length;

// Mirrors rule V17 of rocket_flow::validate, with the same wording as its
// reason, so a rejected save does not add a second line to the node.
function outputIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'Output') return [];
  const found = valueWireCount(edges, node.id);
  if (found === 1) return [];
  const message = `The Output node '${kind.label}' needs exactly one value wire (Run when wires do not count), found ${found}.`;
  return found === 0
    ? [
        issue(
          'output-no-value',
          'error',
          node,
          message,
          'Wire a node into the value field to show a result.',
        ),
      ]
    : [
        issue(
          'output-many-values',
          'error',
          node,
          message,
          'Remove the extra wires so only one feeds the value field.',
        ),
      ];
}

function repeatReason(r: RepeatUntil): string | null {
  if (!r.condition.trim()) return 'The repeat-until condition is empty.';
  if (r.intervalMs < REPEAT_MIN_INTERVAL_MS) {
    return `The repeat-until interval must be at least ${REPEAT_MIN_INTERVAL_MS} ms.`;
  }
  if (r.maxAttempts < 1 || r.maxAttempts > REPEAT_MAX_ATTEMPTS) {
    return `Repeat-until max attempts must be between 1 and ${REPEAT_MAX_ATTEMPTS}.`;
  }
  if (r.timeoutMs > REPEAT_MAX_TIMEOUT_MS) {
    return `The repeat-until timeout must be at most ${REPEAT_MAX_TIMEOUT_MS} ms.`;
  }
  if (r.timeoutMs < r.intervalMs) {
    return 'The repeat-until timeout must not be shorter than the interval.';
  }
  return null;
}

function requestIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'Request') return [];
  const out: FlowIssue[] = [];
  if (kind.source.type === 'Saved') {
    if (!kind.source.requestPath.trim()) {
      out.push(
        issue(
          'request-path-empty',
          'error',
          node,
          'No saved request is chosen.',
          'Pick a request in the node settings.',
        ),
      );
    }
  } else if (!kind.source.request.url.trim() && !hasWire(edges, node.id, 'url')) {
    out.push(
      issue(
        'request-url-empty',
        'error',
        node,
        'The request URL is empty.',
        'Type a URL or wire one into the url field.',
      ),
    );
  }
  const reason = kind.repeatUntil ? repeatReason(kind.repeatUntil) : null;
  if (reason) out.push(issue('repeat-limits', 'error', node, reason));
  return out;
}

function switchIssues(node: FlowNode): FlowIssue[] {
  if (node.kind.kind !== 'Switch') return [];
  const seen = new Set<string>();
  for (const c of node.kind.cases) {
    if (seen.has(c.matches)) {
      return [
        issue(
          'switch-duplicate-match',
          'error',
          node,
          'More than one case has the same match value.',
          'Give each case a different match value.',
        ),
      ];
    }
    seen.add(c.matches);
  }
  return [];
}

function waitIssues(node: FlowNode, nodes: FlowNode[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'WaitForCallback') return [];
  const out: FlowIssue[] = [];
  if (!isValidCallbackName(kind.name)) {
    out.push(
      issue(
        'wait-name-invalid',
        'error',
        node,
        'The Wait for callback name must use only letters, digits and _.',
      ),
    );
  } else if (
    nodes.some(
      (n) => n.id !== node.id && n.kind.kind === 'WaitForCallback' && n.kind.name === kind.name,
    )
  ) {
    out.push(
      issue(
        'wait-name-duplicate',
        'error',
        node,
        `More than one Wait for callback node is named '${kind.name}'.`,
        'Give each Wait node its own name.',
      ),
    );
  }
  if (kind.timeoutMs < MIN_CALLBACK_TIMEOUT_MS || kind.timeoutMs > MAX_CALLBACK_TIMEOUT_MS) {
    out.push(
      issue(
        'wait-timeout-range',
        'error',
        node,
        `The Wait for callback timeout must be between ${MIN_CALLBACK_TIMEOUT_MS} and ${MAX_CALLBACK_TIMEOUT_MS} ms.`,
      ),
    );
  }
  if (kind.acceptWhen != null && kind.acceptWhen.trim() === '') {
    out.push(
      issue(
        'wait-accept-empty',
        'error',
        node,
        "The Wait for callback node's accept condition is empty.",
        'Clear the field to accept the first call, or enter a condition.',
      ),
    );
  }
  return out;
}

// Turns a save_flow error such as
// "Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1"
// into a sentence a person can read, without ids.
// Quoted text after match, matches, case or value is a user's Switch case value, which may be
// a secret. The first branch takes a quote at the end of the text, so a value that contains a
// quote is hidden whole. The second branch takes the next closing quote.
const QUOTED_CASE_VALUE = /(\b(?:matches|match|case|value)\s+)(['"])(?:.*\2(?=[.!?]?$)|[^'"]*\2)/gi;

// Most problems the toast spells out. The canvas shows every one on its node or wire.
const MAX_TOAST_PROBLEMS = 3;

// Hides quoted case values, then makes the text one sentence.
function tidySentence(text: string): string {
  const masked = text.replace(QUOTED_CASE_VALUE, '$1value hidden').trim();
  if (!masked) return '';
  const sentence = masked.charAt(0).toUpperCase() + masked.slice(1);
  return /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
}

export function cleanSaveMessage(message: string): string {
  const multi = parseGraphProblems(message);
  if (multi) {
    const shown = multi.problems
      .slice(0, MAX_TOAST_PROBLEMS)
      .map((p) => tidySentence(p.text))
      .filter(Boolean);
    const more = multi.total - shown.length;
    return [
      `Flow has ${multi.total} problems.`,
      ...shown,
      ...(more > 0 ? [`And ${more} more.`] : []),
    ].join(' ');
  }
  const cleaned = message
    .replace(/^Invalid input:\s*/, '')
    .replace(/^flow is invalid:\s*/, '')
    .replace(/\s*(?:—|-)?\s*node\(s\):[^;]*(?:;\s*edge\(s\):.*)?$/, '')
    .replace(/\s+through\s*$/, '');
  return tidySentence(cleaned) || 'The last save was rejected.';
}

// The text the properties panel shows for one flagged node. A multi-problem
// message gives only the problems that name this node.
export function saveMessageForNode(message: string, nodeId: string): string {
  const own = parseGraphProblems(message)?.problems.filter((p) => p.nodeIds.includes(nodeId));
  if (!own || own.length === 0) return cleanSaveMessage(message);
  return own.map((p) => tidySentence(p.text)).join(' ');
}

function saveIssues(save: SaveErrorInfo, nodeIds: Set<string>, edgeIds: Set<string>): FlowIssue[] {
  const hint =
    save.kind === 'run'
      ? 'Run the full flow, or Run from the named node.'
      : 'Fix this, then save again.';
  const multi = parseGraphProblems(save.message ?? '');
  if (multi) {
    return multi.problems.flatMap((problem) => {
      const message = tidySentence(problem.text);
      return [
        ...[...new Set(problem.nodeIds)]
          .filter((id) => nodeIds.has(id))
          .map((nodeId): FlowIssue => ({ code: 'save', severity: 'error', nodeId, message, hint })),
        ...[...new Set(problem.edgeIds)]
          .filter((id) => edgeIds.has(id))
          .map((edgeId): FlowIssue => ({ code: 'save', severity: 'error', edgeId, message, hint })),
      ];
    });
  }
  const message = cleanSaveMessage(save.message ?? '');
  return [
    ...[...new Set(save.nodeIds)]
      .filter((id) => nodeIds.has(id))
      .map((nodeId): FlowIssue => ({ code: 'save', severity: 'error', nodeId, message, hint })),
    ...[...new Set(save.edgeIds)]
      .filter((id) => edgeIds.has(id))
      .map((edgeId): FlowIssue => ({ code: 'save', severity: 'error', edgeId, message, hint })),
  ];
}

// The same sentence with a different case or a missing full stop is the same message.
const normalizedMessage = (message: string) =>
  message
    .trim()
    .replace(/[.!?]+$/, '')
    .toLowerCase();

// The save text leads with the node label or the wire ends. The client issue has neither.
const SAVE_PREFIX = /^(?:'[^']*'|wire from '[^']*' to '[^']*'):\s+/;

const issueSpot = (i: FlowIssue) => {
  const text = i.code === 'save' ? i.message.replace(SAVE_PREFIX, '') : i.message;
  return `${i.nodeId ?? ''}|${i.edgeId ?? ''}|${normalizedMessage(text)}`;
};

// A save issue that says what another issue on the same node or wire already says adds nothing.
function dropRepeatedSaveIssues(issues: FlowIssue[]): FlowIssue[] {
  const seen = new Set(issues.filter((i) => i.code !== 'save').map(issueSpot));
  return issues.filter((i) => {
    if (i.code !== 'save') return true;
    const spot = issueSpot(i);
    if (seen.has(spot)) return false;
    seen.add(spot);
    return true;
  });
}

// Rules in BACKEND_LINT_CODES (src/lib/flow-lint.ts) come from lint_flow only.
// Do not add client copies of them.
// Pure and I/O free. Issues never block a run. Errors come first, then
// warnings, each group in node order.
export function computeFlowIssues(
  nodes: FlowNode[],
  edges: FlowEdge[],
  ctx: FlowIssueContext = {},
): FlowIssue[] {
  const issues: FlowIssue[] = [];
  for (const node of nodes) {
    issues.push(
      ...expressionIssues(node),
      ...inputIssues(node, edges),
      ...requestIssues(node, edges),
      ...switchIssues(node),
      ...waitIssues(node, nodes),
      ...outputIssues(node, edges),
    );
  }
  if (ctx.save) {
    issues.push(
      ...saveIssues(ctx.save, new Set(nodes.map((n) => n.id)), new Set(edges.map((e) => e.id))),
    );
  }
  const rank = (i: FlowIssue) => (i.severity === 'error' ? 0 : 1);
  return dropRepeatedSaveIssues(issues).sort((a, b) => rank(a) - rank(b));
}

export function groupIssuesByNode(issues: FlowIssue[]): Map<string, FlowIssue[]> {
  const grouped = new Map<string, FlowIssue[]>();
  for (const i of issues) {
    if (!i.nodeId) continue;
    grouped.set(i.nodeId, [...(grouped.get(i.nodeId) ?? []), i]);
  }
  return grouped;
}

export function worstSeverity(issues: FlowIssue[]): IssueSeverity | null {
  if (issues.some((i) => i.severity === 'error')) return 'error';
  return issues.length > 0 ? 'warning' : null;
}

const severityWord = (i: FlowIssue) => (i.severity === 'error' ? 'Error' : 'Warning');

// The accessible name of a node's badge.
export function summarizeIssues(issues: FlowIssue[]): string {
  const lines = issues.map((i) => `${severityWord(i)}: ${i.message}`);
  return issues.length === 1 ? lines[0] : `${issues.length} issues: ${lines.join(' ')}`;
}

// The accessible name of the count button, such as "2 errors, 1 warning".
export function issueCountLabel(issues: FlowIssue[]): string {
  const errors = issues.filter((i) => i.severity === 'error').length;
  const warnings = issues.length - errors;
  const parts = [
    errors > 0 ? `${errors} error${errors === 1 ? '' : 's'}` : null,
    warnings > 0 ? `${warnings} warning${warnings === 1 ? '' : 's'}` : null,
  ].filter((p): p is string => p !== null);
  return parts.length > 0 ? parts.join(', ') : 'No issues';
}
