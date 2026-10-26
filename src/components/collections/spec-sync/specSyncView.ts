import type {
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
  return field.charAt(0).toUpperCase() + field.slice(1);
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

/** True for a GitHub file link, the only kind that takes a token. */
export function isGithubLocation(location: string): boolean {
  try {
    const url = new URL(location);
    return url.protocol === 'https:' && url.host === 'github.com';
  } catch {
    return false;
  }
}

/** A link with a query or hash is split into a short form and the full text. */
export function splitLocation(location: string): { short: string; full: string | null } {
  try {
    const url = new URL(location);
    if ((url.protocol === 'https:' || url.protocol === 'http:') && (url.search || url.hash)) {
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
