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

export function findEntry(plan: OpenApiSyncPlan, id: string | null): SyncEntry | null {
  if (!id) return null;
  const item = plan.items.find((i) => itemId(i) === id);
  if (item) return { type: 'item', item };
  const change = plan.variableChanges.find((v) => variableId(v) === id);
  return change ? { type: 'variable', change } : null;
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
  hourly: 'Every hour',
  daily: 'Every day',
  weekly: 'Every week',
  monthly: 'Every month',
};

export function intervalLabel(interval: OpenApiSyncInterval): string {
  return INTERVAL_LABEL[interval];
}

/** The message of a failed command, which arrives as a string or an Error. */
export function syncErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err && typeof err.message === 'string') {
    return err.message;
  }
  return 'Unknown error.';
}

/** True when the pending copy no longer matches the one that was checked. */
export function isPendingChangedError(err: unknown): boolean {
  return syncErrorMessage(err).startsWith('Conflict');
}
