import type { OpenApiSyncItem, OpenApiSyncPlan } from '@/lib/tauri-api';

type Choice = 'spec' | 'yours';

export interface DefaultSelection {
  approved: Set<string>;
  choices: Map<string, Record<string, Choice>>;
  variables: Set<string>;
}

/** The conflict fields of a changed item, which are the fields the user edited too. */
function conflictFields(item: OpenApiSyncItem): string[] {
  if (item.kind !== 'changed') return [];
  return item.changes.filter((c) => c.outcome === 'conflict').map((c) => c.field);
}

/** True when the item is a removed request that is not known to be unedited. */
export function isEditedRemoved(item: OpenApiSyncItem): boolean {
  return item.kind === 'removed' && item.edited !== false;
}

/**
 * The selection a fresh plan starts with. New items and changed items without a conflict are
 * selected. A changed item with a conflict starts unselected and keeps the user's version.
 * A removed item is never selected, so nothing is deleted without a deliberate click.
 * Variable changes are selected too. A value the user changed is kept by the apply.
 */
export function defaultSelection(plan: OpenApiSyncPlan): DefaultSelection {
  const approved = new Set<string>();
  const choices = new Map<string, Record<string, Choice>>();
  for (const item of plan.items) {
    const conflicts = conflictFields(item);
    if (conflicts.length > 0) {
      choices.set(item.key, Object.fromEntries(conflicts.map((f) => [f, 'yours' as const])));
    } else if (item.kind !== 'removed') {
      approved.add(item.key);
    }
  }
  return { approved, choices, variables: new Set(plan.variableChanges.map((v) => v.name)) };
}

/** How many of the user's edits Accept all would overwrite with the spec. */
export function countOverwrites(plan: OpenApiSyncPlan): number {
  return plan.items.reduce((sum, item) => sum + conflictFields(item).length, 0);
}

/**
 * Applies Accept all to the current selection. Every new and changed item is selected and every
 * conflict field takes the spec. A removed item the user edited is dropped from the selection.
 */
export function acceptAllSelection(
  plan: OpenApiSyncPlan,
  approved: Set<string>,
  choices: Map<string, Record<string, Choice>>,
): Omit<DefaultSelection, 'variables'> {
  const nextApproved = new Set(approved);
  const nextChoices = new Map(choices);
  for (const item of plan.items) {
    if (item.kind === 'removed') {
      if (isEditedRemoved(item)) nextApproved.delete(item.key);
      continue;
    }
    nextApproved.add(item.key);
    const conflicts = conflictFields(item);
    if (conflicts.length > 0) {
      nextChoices.set(item.key, {
        ...nextChoices.get(item.key),
        ...Object.fromEntries(conflicts.map((f) => [f, 'spec' as const])),
      });
    }
  }
  return { approved: nextApproved, choices: nextChoices };
}

/**
 * The fields where Keep my values must pin the user's value: the conflicts. Any other field the
 * user edited is kept by the backend already, and pinning a field the spec also changes in an
 * untouched part (a header the user disabled while the spec changed its value) would drop that
 * update.
 */
function editedFields(item: OpenApiSyncItem): string[] {
  return conflictFields(item);
}

/**
 * Applies Accept all with Keep my values on. Every new and changed item is selected and every
 * conflict keeps the user's value, so the spec never replaces a field both sides changed. A removed
 * item the user edited is dropped from the selection, as in `acceptAllSelection`.
 */
export function acceptAllKeepingMine(
  plan: OpenApiSyncPlan,
  approved: Set<string>,
  choices: Map<string, Record<string, Choice>>,
): Omit<DefaultSelection, 'variables'> {
  const nextApproved = new Set(approved);
  const nextChoices = new Map(choices);
  for (const item of plan.items) {
    if (item.kind === 'removed') {
      if (isEditedRemoved(item)) nextApproved.delete(item.key);
      continue;
    }
    nextApproved.add(item.key);
    const edited = editedFields(item);
    if (edited.length > 0) {
      nextChoices.set(item.key, {
        ...nextChoices.get(item.key),
        ...Object.fromEntries(edited.map((f) => [f, 'yours' as const])),
      });
    }
  }
  return { approved: nextApproved, choices: nextChoices };
}
