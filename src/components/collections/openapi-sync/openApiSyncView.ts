import type {
  CollectionItem,
  Folder,
  OpenApiFieldChange,
  OpenApiSyncInterval,
  OpenApiSyncItem,
  OpenApiSyncPlan,
  OpenApiVariableChange,
} from '@/lib/tauri-api';

export type SyncGroupKind = 'new' | 'changed' | 'removed';

export interface SyncGroup {
  kind: SyncGroupKind;
  label: string;
  items: OpenApiSyncItem[];
}

const GROUP_LABEL: Record<SyncGroupKind, string> = {
  new: 'New',
  changed: 'Changed',
  removed: 'Removed',
};

/** Groups the plan items as New, Changed, Removed, in that order. Empty groups stay. */
export function groupItems(items: OpenApiSyncItem[]): SyncGroup[] {
  return (['new', 'changed', 'removed'] as const).map((kind) => ({
    kind,
    label: GROUP_LABEL[kind],
    items: items.filter((i) => i.kind === kind),
  }));
}

/** The id of a list entry. A key can repeat across kinds, so the kind is part of it. */
export function itemId(item: OpenApiSyncItem): string {
  return `${item.kind}:${item.key}`;
}

export function variableId(change: OpenApiVariableChange): string {
  return `variable:${change.kind}:${change.name}`;
}

/** The id of the entry shown first, or null when the plan has no entries. */
export function firstEntryId(plan: OpenApiSyncPlan): string | null {
  const first = groupItems(plan.items).find((g) => g.items.length > 0)?.items[0];
  if (first) return itemId(first);
  const variable = plan.variableChanges[0];
  return variable ? variableId(variable) : null;
}

export type SyncEntry =
  | { type: 'item'; item: OpenApiSyncItem }
  | { type: 'variable'; change: OpenApiVariableChange };

const entryCache = new WeakMap<OpenApiSyncPlan, Map<string, SyncEntry>>();

/** Looks an entry up by id. The map is built once per plan, so a big plan stays cheap. */
export function findEntry(plan: OpenApiSyncPlan, id: string | null): SyncEntry | null {
  if (!id) return null;
  let map = entryCache.get(plan);
  if (!map) {
    map = new Map();
    for (const change of plan.variableChanges) {
      map.set(variableId(change), { type: 'variable', change });
    }
    for (const item of plan.items) map.set(itemId(item), { type: 'item', item });
    entryCache.set(plan, map);
  }
  return map.get(id) ?? null;
}

const OUTCOME_LABEL: Record<OpenApiFieldChange['outcome'], string> = {
  takeSpec: 'Spec wins',
  keepYours: 'Yours kept',
  conflict: 'Conflict',
};

export function outcomeLabel(outcome: OpenApiFieldChange['outcome']): string {
  return OUTCOME_LABEL[outcome];
}

/** A hint for a rename the importer could only guess. */
export function renameHint(item: OpenApiSyncItem): string | null {
  if (item.kind === 'new' && item.possiblyRenamedFrom) {
    return `Possibly renamed from ${item.possiblyRenamedFrom}`;
  }
  if (item.kind === 'removed' && item.possiblyRenamedTo) {
    return `Possibly renamed to ${item.possiblyRenamedTo}`;
  }
  return null;
}

/** A readable name for a changed field, such as `header:Accept` or `body:$.a.b`. */
export function fieldLabel(field: string): string {
  if (field.startsWith('header:')) return `Header ${field.slice('header:'.length)}`;
  if (field.startsWith('body:')) return `Body ${field.slice('body:'.length)}`;
  if (field === 'url') return 'URL';
  return field.charAt(0).toUpperCase() + field.slice(1);
}

const RESET_SCOPE =
  'Your changes to the URL, parameters, headers, body and auth of this request will be replaced with the last synced version.';
const RESET_SCOPE_MANY =
  'Your changes to the URL, parameters, headers, body and auth of these requests will be replaced with the last synced version.';
const RESET_KEPT =
  'Your name, method, description, scripts, tests, assertions and docs are kept. A backup is kept so you can restore it.';

/** The changed-field names a reset covers. A field is the name or `name:detail`. */
const RESET_FIELDS = ['url', 'header', 'query', 'path', 'form', 'body', 'auth'];

/** True when a reset would change something: a field it covers changed, or the row has no list. */
export function isResetEligible(fields: string[]): boolean {
  if (fields.length === 0) return true;
  return fields.some((f) => RESET_FIELDS.some((r) => f === r || f.startsWith(`${r}:`)));
}

/** The confirm text for resetting one request to the last synced spec. */
export function resetConfirmText(row: { method: string; url: string; fields: string[] }): string {
  const target = `${row.method} ${row.url}`.trim();
  return `Reset ${target} to the spec? ${RESET_SCOPE} ${RESET_KEPT}`;
}

/** The confirm text for resetting several requests at once. */
export function resetAllConfirmText(count: number): string {
  return `Reset ${count} ${count === 1 ? 'request' : 'requests'} to the spec? ${count === 1 ? RESET_SCOPE : RESET_SCOPE_MANY} ${RESET_KEPT}`;
}

export function variableLabel(change: OpenApiVariableChange): string {
  return change.kind === 'baseUrl' ? `Base URL ${change.name}` : `Security ${change.name}`;
}

const INTERVAL_LABEL: Record<OpenApiSyncInterval, string> = {
  off: 'Off',
  every_5_minutes: 'Every 5 minutes',
  every_15_minutes: 'Every 15 minutes',
  every_30_minutes: 'Every 30 minutes',
  hourly: 'Every hour',
  daily: 'Every day',
  weekly: 'Every week',
  monthly: 'Every month',
};

export function intervalLabel(interval: OpenApiSyncInterval): string {
  return INTERVAL_LABEL[interval];
}

/** The hosts the backend reads as a GitHub location, the only kind that takes a token. */
const GITHUB_HOSTS = ['github.com', 'raw.githubusercontent.com', 'api.github.com'];

/** True for a GitHub file link, the only kind that takes a token. */
export function isGithubLocation(location: string): boolean {
  try {
    const url = new URL(location);
    return url.protocol === 'https:' && GITHUB_HOSTS.includes(url.host);
  } catch {
    return false;
  }
}

const FILE_PREFIX = 'file://';

/** True for the display form of a spec file on this computer. */
export function isFileLocation(location: string): boolean {
  return location.startsWith(FILE_PREFIX);
}

/** The file name and folder of a `file://` location. */
export function fileParts(location: string): { name: string; folder: string } {
  const path = location.slice(FILE_PREFIX.length);
  const cut = path.lastIndexOf('/');
  const name = path.slice(cut + 1);
  let folder = cut <= 0 ? '/' : path.slice(0, cut);
  // A Windows path has a slash before the drive letter.
  if (/^\/[A-Za-z]:/.test(folder)) folder = folder.slice(1);
  return { name, folder };
}

/** A link with a query or hash is split into a short form and the full text. A file shows its
 * name and its folder. */
export function splitLocation(location: string): { short: string; full: string | null } {
  if (isFileLocation(location)) {
    const { name, folder } = fileParts(location);
    return { short: `${name} in ${folder}`, full: null };
  }
  try {
    const url = new URL(location);
    if (
      (url.protocol === 'https:' || url.protocol === 'http:') &&
      (url.search || url.hash || url.username || url.password)
    ) {
      return { short: `${url.host}${url.pathname}`, full: location };
    }
  } catch {
    // Not a URL, so it is shown as it is.
  }
  return { short: location, full: null };
}

/** The raw text of a failed command, for matching only. It is never shown. */
function rawErrorText(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err && typeof err.message === 'string') {
    return err.message;
  }
  return 'Unknown error.';
}

/** True when the pending copy no longer matches the one that was checked. */
export function isPendingChangedError(err: unknown): boolean {
  return rawErrorText(err).includes('pending spec changed');
}

/** What a field takes when the user has not chosen: conflicts keep the user's value. */
export function defaultChoice(outcome: OpenApiFieldChange['outcome']): 'spec' | 'yours' {
  return outcome === 'takeSpec' ? 'spec' : 'yours';
}

/** Keeps the items whose name or key contains the query. An empty query keeps all. */
export function filterItems(items: OpenApiSyncItem[], query: string): OpenApiSyncItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.name.toLowerCase().includes(q) || i.key.toLowerCase().includes(q));
}

/** Keeps the variable changes whose label contains the query. */
export function filterVariables(
  changes: OpenApiVariableChange[],
  query: string,
): OpenApiVariableChange[] {
  const q = query.trim().toLowerCase();
  if (!q) return changes;
  return changes.filter((c) => variableLabel(c).toLowerCase().includes(q));
}

/** Counts the HTTP and the other protocol requests of a folder tree. Summaries without a kind are HTTP. */
function countRequests(items: CollectionItem[], counts = { http: 0, other: 0 }) {
  for (const item of items) {
    if (item.type === 'folder') countRequests(item.items, counts);
    else if (item.type === 'request') counts.http += 1;
    else if (item.type === 'summary') {
      if (item.kind === undefined || item.kind === 'http') counts.http += 1;
      else counts.other += 1;
    } else if (item.type !== 'scriptFile') counts.other += 1;
  }
  return counts;
}

/**
 * Whether connecting to a spec makes sense. A collection that is not loaded yet, one without
 * requests, and one with any HTTP request qualify. One with only GraphQL, gRPC or WebSocket
 * requests does not, since a spec only describes HTTP operations.
 */
export function offersConnect(root: Folder | undefined): boolean {
  if (!root) return true;
  const { http, other } = countRequests(root.items);
  return http > 0 || other === 0;
}
