import type { OpenApiSelection, OpenApiSyncPlan } from '@/lib/tauri-api';

export interface SelectionState {
  approved: Set<string>;
  choices: Map<string, Record<string, 'spec' | 'yours'>>;
  skips: Set<string>;
  variables: Set<string>;
  links: Array<[string, string]>;
}

/** Keeps only the choices for fields that are in the item's current changes. */
function currentChoices(
  item: OpenApiSyncPlan['items'][number],
  chosen: Record<string, 'spec' | 'yours'> | undefined,
): Record<string, 'spec' | 'yours'> {
  const out: Record<string, 'spec' | 'yours'> = {};
  if (!chosen || item.kind !== 'changed') return out;
  for (const change of item.changes) {
    const choice = chosen[change.field];
    if (choice) out[change.field] = choice;
  }
  return out;
}

/**
 * Builds the apply selection from the user's choices. Digests and fingerprints come from the
 * plan items untouched, and a key that is not in the plan is dropped.
 */
export function buildSelection(plan: OpenApiSyncPlan, state: SelectionState): OpenApiSelection {
  const items: OpenApiSelection['items'] = [];
  const skips: OpenApiSelection['skips'] = [];
  for (const item of plan.items) {
    // A skip wins over an approval, so an item is never sent as both.
    if (state.skips.has(item.key)) {
      skips.push({ key: item.key, fingerprint: item.fingerprint });
    } else if (state.approved.has(item.key)) {
      items.push({
        key: item.key,
        digest: item.digest,
        choices: currentChoices(item, state.choices.get(item.key)),
      });
    }
  }
  const variables = plan.variableChanges
    .filter((v) => state.variables.has(v.name))
    .map((v) => ({ name: v.name, digest: v.digest }));
  return { items, skips, variables, renameLinks: state.links.map(([a, b]) => [a, b]) };
}
