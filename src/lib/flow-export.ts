import { isVariableReference, redactPlaintextSecrets } from '@/lib/flow-secrets';
import type {
  Flow,
  FlowDebugHeader,
  FlowDebugRequest,
  FlowNode,
  FlowNodeKind,
  FlowNodeStatus,
} from '@/lib/tauri-api';
import type { FlowNodeDetail, FlowTab } from '@/types/pane-types';

/** What replaces a credential in an export. */
export const REDACTED = '<redacted>';

/** Longest body or value kept in a report, in characters. */
export const EXPORT_BODY_LIMIT = 65_536;

// A key or parameter name that holds a credential: it ends in one of these words.
// The short words pass and pwd must stand alone, so "compass" is left alone.
const KEY_NAME = String.raw`(?:[\w.-]*(?:token|secret|password|passwd|api[_-]?key|apikey|credential|authorization|cookie|private[_-]?key|client[_-]?assertion|secret[_-]?access[_-]?key|signature)|(?:[\w.-]*[_.-])?(?:pass|pwd))`;
const HEADER_LINE =
  /\b(authorization|proxy-authorization|cookie|set-cookie|x-api-key|x-auth-token)(\s*[:=]\s*)([^\r\n]*)/gi;
// A Bearer or Basic credential. It must look like a token (a digit or . = + /), so
// prose such as "Basic authentication" is left alone.
const BEARER = /\b(Bearer|Basic)(\s+)((?=[A-Za-z0-9._~+/=-]*[\d.=+/])[A-Za-z0-9._~+/=-]{8,})/g;
const JSON_PAIR = new RegExp(String.raw`("${KEY_NAME}"\s*:\s*")((?:[^"\\]|\\.)*)(")`, 'gi');
const PARAM = new RegExp(String.raw`(^|[?&;\s])(${KEY_NAME})(=)([^&\s"'#]*)`, 'gi');

const SENSITIVE_HEADER = new RegExp(
  String.raw`^(?:authorization|proxy-authorization|cookie|set-cookie|x-api-key|x-auth-token|${KEY_NAME})$`,
  'i',
);
const SENSITIVE_LABEL = /secret|password|passwd|token|api[\s_-]?key|credential/i;

const SCHEME_WORD = /^\s*(?:Bearer|Basic|Token|Digest|NTLM|OAuth)(?:\s+|$)/i;
// The password part of `://user:password@host`.
const URL_PASSWORD = /(:\/\/[^\s/:@?#]*:)([^\s/@?#]*)(@)/g;

// Empty, already redacted, or only variable references: nothing to hide. A leading
// scheme word such as Bearer does not count, so `Bearer {{token}}` is safe.
const nothingToHide = (value: string): boolean => {
  const rest = value.replace(SCHEME_WORD, '');
  return rest.trim() === '' || rest === REDACTED || isVariableReference(rest);
};

/**
 * Replaces credentials in free text: authorization-style header lines, Bearer and
 * Basic tokens, JSON string values and `name=value` pairs whose name holds a
 * credential. Values that are empty or only `{{variable}}` references are kept.
 * Idempotent. A defensive pass: the backend masks most of this already, but not
 * the step error (roadmap F-02).
 */
export function redactKnownSecrets(text: string): string {
  return text
    .replace(HEADER_LINE, (match, name: string, sep: string, value: string) =>
      nothingToHide(value.trim()) ? match : `${name}${sep}${REDACTED}`,
    )
    .replace(URL_PASSWORD, (match, lead: string, pw: string, at: string) =>
      nothingToHide(pw) ? match : `${lead}${REDACTED}${at}`,
    )
    .replace(BEARER, (_match, scheme: string, space: string) => `${scheme}${space}${REDACTED}`)
    .replace(JSON_PAIR, (match, open: string, value: string, close: string) =>
      nothingToHide(value) ? match : `${open}${REDACTED}${close}`,
    )
    .replace(PARAM, (match, lead: string, name: string, eq: string, value: string) =>
      nothingToHide(value) ? match : `${lead}${name}${eq}${REDACTED}`,
    );
}

/** The flow a tab holds, as `save_flow` takes it. Null while the tab is still a picker. */
export function exportableFlow(tab: FlowTab): Flow | null {
  if (!tab.flowName) return null;
  return {
    name: tab.flowName,
    nodes: tab.nodes,
    edges: tab.edges,
    ...(tab.callbackHost ? { callbackHost: tab.callbackHost } : {}),
  };
}

function maskNode(node: FlowNode): { node: FlowNode; masked: number } {
  const kind = node.kind;
  switch (kind.kind) {
    case 'Auth': {
      const out = redactPlaintextSecrets(kind.auth, REDACTED);
      if (out.fields.length === 0) return { node, masked: 0 };
      return { node: { ...node, kind: { ...kind, auth: out.auth } }, masked: out.fields.length };
    }
    case 'Input': {
      // There is no secret flag on an Input value, so the label decides.
      if (
        typeof kind.value === 'string' &&
        SENSITIVE_LABEL.test(kind.label) &&
        !nothingToHide(kind.value)
      ) {
        return { node: { ...node, kind: { ...kind, value: REDACTED } }, masked: 1 };
      }
      return { node, masked: 0 };
    }
    case 'Request': {
      if (kind.source.type !== 'Inline') return { node, masked: 0 };
      const request = kind.source.request;
      let masked = 0;
      const headers = request.headers.map((header) => {
        if (SENSITIVE_HEADER.test(header.name) && !nothingToHide(header.value)) {
          masked++;
          return { ...header, value: REDACTED };
        }
        return header;
      });
      const url = redactKnownSecrets(request.url);
      if (url !== request.url) masked++;
      const body = request.body ? redactKnownSecrets(request.body) : request.body;
      if (body !== request.body) masked++;
      if (masked === 0) return { node, masked: 0 };
      const next: FlowNodeKind = {
        ...kind,
        source: { type: 'Inline', request: { ...request, headers, url, body } },
      };
      return { node: { ...node, kind: next }, masked };
    }
    default:
      return { node, masked: 0 };
  }
}

/**
 * A copy of `flow` with literal credentials replaced by `<redacted>`: Auth node
 * fields, sensitive inline request headers, credential parameters in an inline URL
 * or body, and Input values whose label names a credential. `{{variable}}`
 * references stay. The count is the number of places changed.
 */
export function maskFlowSecrets(flow: Flow): { flow: Flow; maskedCount: number } {
  let maskedCount = 0;
  const nodes = flow.nodes.map((node) => {
    const out = maskNode(node);
    maskedCount += out.masked;
    return out.node;
  });
  return { flow: { ...flow, nodes }, maskedCount };
}

// ---------------------------------------------------------------------------
// Run report
// ---------------------------------------------------------------------------

export interface RunReportOptions {
  /** Include request and response bodies. Off by default in the UI. */
  includeBodies: boolean;
  /** The clock, for tests. */
  now?: () => Date;
}

interface ReportHeader {
  key: string;
  value: string;
}

interface ReportExchange {
  method: string;
  url: string;
  headers: ReportHeader[];
  body?: string;
  bodyTruncated?: boolean;
  response?: {
    status: number;
    statusText: string;
    durationMs: number;
    sizeBytes: number;
    headers: ReportHeader[];
    body?: string;
    truncated?: boolean;
  };
  error?: string;
}

interface ReportNode {
  id: string;
  label: string;
  kind: FlowNodeKind['kind'];
  status: FlowNodeStatus;
  statusCode?: number;
  durationMs?: number;
  attempts?: number;
  branch?: string;
  skipReason?: string;
  error?: string;
  value?: string;
  exchange?: ReportExchange;
  logs?: { level: string; message: string }[];
}

interface ReportData {
  flow: string;
  collection: string;
  runId?: string;
  generatedAt: string;
  includeBodies: boolean;
  summary: { total: number; success: number; failed: number; skipped: number; notRun: number };
  nodes: ReportNode[];
}

const cap = (text: string): string =>
  text.length > EXPORT_BODY_LIMIT
    ? `${text.slice(0, EXPORT_BODY_LIMIT)}\n... cut at ${EXPORT_BODY_LIMIT} characters in this export`
    : text;

function cleanHeaders(headers: FlowDebugHeader[]): ReportHeader[] {
  return headers.map((h) => ({
    key: h.key,
    value: SENSITIVE_HEADER.test(h.key) ? REDACTED : redactKnownSecrets(h.value),
  }));
}

function cleanExchange(ex: FlowDebugRequest, includeBodies: boolean): ReportExchange {
  const response = ex.response;
  return {
    method: ex.method,
    url: redactKnownSecrets(ex.url),
    headers: cleanHeaders(ex.headers),
    ...(includeBodies && ex.body ? { body: cap(redactKnownSecrets(ex.body)) } : {}),
    ...(includeBodies && ex.bodyTruncated ? { bodyTruncated: true } : {}),
    ...(response
      ? {
          response: {
            status: response.status,
            statusText: response.statusText,
            durationMs: response.durationMs,
            sizeBytes: response.sizeBytes,
            headers: cleanHeaders(response.headers),
            ...(includeBodies && response.body
              ? { body: cap(redactKnownSecrets(response.body)) }
              : {}),
            ...(includeBodies && response.truncated ? { truncated: true } : {}),
          },
        }
      : {}),
    ...(ex.error ? { error: redactKnownSecrets(ex.error) } : {}),
  };
}

// An Input value has no secret flag, so its label decides, as in maskNode.
function reportValue(node: FlowNode, value: string): string {
  const kind = node.kind;
  if (kind.kind === 'Input' && SENSITIVE_LABEL.test(kind.label) && !nothingToHide(value)) {
    return REDACTED;
  }
  return redactKnownSecrets(value);
}

function reportNode(
  node: FlowNode,
  status: FlowNodeStatus,
  detail: FlowNodeDetail | undefined,
  includeBodies: boolean,
): ReportNode {
  return {
    id: node.id,
    label: node.kind.label,
    kind: node.kind.kind,
    status,
    ...(detail?.statusCode !== undefined ? { statusCode: detail.statusCode } : {}),
    ...(detail?.durationMs !== undefined ? { durationMs: detail.durationMs } : {}),
    ...(detail?.attempts !== undefined ? { attempts: detail.attempts } : {}),
    ...(detail?.branch ? { branch: detail.branch } : {}),
    ...(detail?.skipReason ? { skipReason: detail.skipReason } : {}),
    ...(detail?.error ? { error: redactKnownSecrets(detail.error) } : {}),
    ...(detail?.value !== undefined ? { value: cap(reportValue(node, detail.value)) } : {}),
    ...(detail?.exchange ? { exchange: cleanExchange(detail.exchange, includeBodies) } : {}),
    ...(detail?.logs?.length
      ? {
          logs: detail.logs.map((l) => ({
            level: l.level,
            message: redactKnownSecrets(l.message),
          })),
        }
      : {}),
  };
}

// A code fence longer than any run of backticks in the text, so the text cannot close it.
function fence(text: string): string {
  const longest = Math.max(0, ...(text.match(/`+/g) ?? []).map((run) => run.length));
  const ticks = '`'.repeat(Math.max(3, longest + 1));
  return `${ticks}\n${text}\n${ticks}`;
}

const oneLine = (text: string): string => text.replace(/\s+/g, ' ').trim();

const STATUS_TEXT: Record<FlowNodeStatus, string> = {
  success: 'Success',
  failed: 'Failed',
  skipped: 'Skipped',
  running: 'Running',
  idle: 'Not run',
};

function exchangeText(ex: ReportExchange): string {
  const lines = [`${ex.method} ${ex.url}`, ...ex.headers.map((h) => `${h.key}: ${h.value}`)];
  if (ex.body) lines.push('', ex.body);
  if (ex.response) {
    const r = ex.response;
    lines.push('', `HTTP ${r.status} ${r.statusText} (${r.durationMs} ms, ${r.sizeBytes} bytes)`);
    lines.push(...r.headers.map((h) => `${h.key}: ${h.value}`));
    if (r.body) lines.push('', r.body);
  }
  if (ex.error) lines.push('', `Error: ${ex.error}`);
  return lines.join('\n');
}

function markdownReport(data: ReportData): string {
  const s = data.summary;
  const lines: string[] = [
    `# Run report: ${oneLine(data.flow)}`,
    '',
    `- Collection: ${oneLine(data.collection)}`,
    ...(data.runId ? [`- Run id: ${oneLine(data.runId)}`] : []),
    `- Generated: ${data.generatedAt}`,
    `- Result: ${s.success} succeeded, ${s.failed} failed, ${s.skipped} skipped, ${s.notRun} not run`,
    `- Request and response bodies: ${data.includeBodies ? 'included' : 'omitted'}`,
    '',
  ];
  data.nodes.forEach((n, i) => {
    lines.push(`## ${i + 1}. ${oneLine(n.label)} (${n.kind}) - ${STATUS_TEXT[n.status]}`, '');
    const facts: string[] = [];
    if (n.statusCode !== undefined) facts.push(`- Status code: ${n.statusCode}`);
    if (n.durationMs !== undefined) facts.push(`- Duration: ${n.durationMs} ms`);
    if (n.attempts !== undefined) facts.push(`- Attempts: ${n.attempts}`);
    if (n.branch) facts.push(`- Branch taken: ${oneLine(n.branch)}`);
    if (n.skipReason) facts.push(`- Skipped: ${n.skipReason.replace(/_/g, ' ')}`);
    if (facts.length > 0) lines.push(...facts, '');
    if (n.error) lines.push('Error:', '', fence(n.error), '');
    if (n.value !== undefined) lines.push('Value:', '', fence(n.value), '');
    if (n.exchange) lines.push('Request and response:', '', fence(exchangeText(n.exchange)), '');
    if (n.logs) {
      lines.push('Logs:', '', fence(n.logs.map((l) => `[${l.level}] ${l.message}`).join('\n')), '');
    }
  });
  return `${lines.join('\n')}\n`;
}

/**
 * The report of the tab's last run as JSON and as Markdown. Reads only the tab,
 * never the in-memory auth store. Every free-text field goes through
 * `redactKnownSecrets`, header values are masked by name, and bodies are left out
 * unless `includeBodies` is set (and then cut at `EXPORT_BODY_LIMIT`).
 */
export function buildRunReport(
  tab: FlowTab,
  options: RunReportOptions,
): { json: string; markdown: string } {
  const now = options.now ?? (() => new Date());
  const nodes = tab.nodes.map((node) =>
    reportNode(
      node,
      tab.nodeStatus[node.id] ?? 'idle',
      tab.nodeDetail?.[node.id],
      options.includeBodies,
    ),
  );
  const count = (...statuses: FlowNodeStatus[]) =>
    nodes.filter((n) => statuses.includes(n.status)).length;
  const data: ReportData = {
    flow: tab.flowName ?? '',
    collection: tab.collectionName ?? '',
    ...(tab.runId ? { runId: tab.runId } : {}),
    generatedAt: now().toISOString(),
    includeBodies: options.includeBodies,
    summary: {
      total: nodes.length,
      success: count('success'),
      failed: count('failed'),
      skipped: count('skipped'),
      notRun: count('idle', 'running'),
    },
    nodes,
  };
  return { json: JSON.stringify(data, null, 2), markdown: markdownReport(data) };
}

/** A safe file name for a report, such as `my-flow-run-report.json`. */
export function reportFileName(flowName: string, extension: 'json' | 'md'): string {
  const base = flowName.replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '') || 'flow';
  return `${base}-run-report.${extension}`;
}
