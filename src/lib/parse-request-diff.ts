import type { FieldChange, RowChange, RequestDiff } from '@/types/visual-diff-types';

interface KVRow {
  key: string;
  value: string;
  enabled: boolean;
}

interface RequestJson {
  method?: string;
  url?: string;
  headers?: KVRow[];
  queryParams?: KVRow[];
  pathParams?: KVRow[];
  body?: { mode?: string; content?: string } | null;
  auth?: { type?: string } | null;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
}

/** Tries to parse content as a JSON request object. Returns null if unparseable or not a request. */
function tryParseRequest(content: string | undefined): RequestJson | null {
  if (!content) return null;
  try {
    const obj = JSON.parse(content) as unknown;
    if (typeof obj !== 'object' || obj === null || Array.isArray(obj)) return null;
    const r = obj as Record<string, unknown>;
    // Must have at least one of method or url to qualify as a request file.
    if (!r.method && !r.url) return null;
    return r as RequestJson;
  } catch {
    return null;
  }
}

/** Converts null to undefined so null/undefined fields compare as equal. */
function norm<T>(val: T | null | undefined): T | undefined {
  return val === null ? undefined : val;
}

/** Builds a FieldChange for a single scalar field. */
function field<T>(label: string, oldVal: T | undefined, newVal: T | undefined): FieldChange<T> {
  const changed = JSON.stringify(oldVal) !== JSON.stringify(newVal);
  return { label, oldValue: oldVal, newValue: newVal, changed };
}

/** Diffs two KV-row arrays by key, preserving new order and appending removed keys at end. */
function diffRows(oldRows: KVRow[], newRows: KVRow[]): RowChange[] {
  const result: RowChange[] = [];
  const oldMap = new Map(oldRows.map((r) => [r.key, r]));
  const newMap = new Map(newRows.map((r) => [r.key, r]));

  // Walk new rows first, then append keys only in old.
  const allKeys = [
    ...newRows.map((r) => r.key),
    ...oldRows.filter((r) => !newMap.has(r.key)).map((r) => r.key),
  ];
  const seen = new Set<string>();

  for (const key of allKeys) {
    if (seen.has(key)) continue;
    seen.add(key);
    const o = oldMap.get(key);
    const n = newMap.get(key);
    if (!o) {
      result.push({ key, oldRow: undefined, newRow: { value: n!.value, enabled: n!.enabled }, status: 'added' });
    } else if (!n) {
      result.push({ key, oldRow: { value: o.value, enabled: o.enabled }, newRow: undefined, status: 'removed' });
    } else {
      const changed = o.value !== n.value || o.enabled !== n.enabled;
      result.push({
        key,
        oldRow: { value: o.value, enabled: o.enabled },
        newRow: { value: n.value, enabled: n.enabled },
        status: changed ? 'modified' : 'unchanged',
      });
    }
  }
  return result;
}

/**
 * Parses old and new JSON request file strings into a structured diff.
 * Returns null if neither parses as a valid request object.
 */
export function parseRequestDiff(
  oldContent: string | undefined,
  newContent: string | undefined,
): RequestDiff | null {
  const old = tryParseRequest(oldContent);
  const nw = tryParseRequest(newContent);
  if (!old && !nw) return null;

  const method = field('Method', norm(old?.method), norm(nw?.method));
  const url = field('URL', norm(old?.url), norm(nw?.url));
  const headers = diffRows(old?.headers ?? [], nw?.headers ?? []);
  const queryParams = diffRows(old?.queryParams ?? [], nw?.queryParams ?? []);
  const pathParams = diffRows(old?.pathParams ?? [], nw?.pathParams ?? []);

  const oldBody = old?.body
    ? { mode: old.body.mode ?? 'none', content: norm(old.body.content) }
    : { mode: 'none', content: undefined };
  const newBody = nw?.body
    ? { mode: nw.body.mode ?? 'none', content: norm(nw.body.content) }
    : { mode: 'none', content: undefined };
  const body = field('Body', oldBody, newBody);

  const auth = field('Auth', old?.auth?.type ?? 'none', nw?.auth?.type ?? 'none');
  const preRequestScript = field('Pre-request Script', norm(old?.preRequestScript), norm(nw?.preRequestScript));
  const postResponseScript = field('Post-response Script', norm(old?.postResponseScript), norm(nw?.postResponseScript));

  const rowsChanged = [...headers, ...queryParams, ...pathParams].some(
    (r) => r.status !== 'unchanged',
  );
  const hasChanges =
    method.changed ||
    url.changed ||
    rowsChanged ||
    body.changed ||
    auth.changed ||
    preRequestScript.changed ||
    postResponseScript.changed;

  return {
    method,
    url,
    headers,
    queryParams,
    pathParams,
    body,
    auth,
    preRequestScript,
    postResponseScript,
    hasChanges,
  };
}
