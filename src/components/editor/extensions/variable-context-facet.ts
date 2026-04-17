import { StateEffect, StateField } from '@codemirror/state';
import type { VariableScopeEntry } from '@/lib/url-variables';

/**
 * State effect to update the variable context. Dispatch this whenever the
 * environment store changes:
 *
 *   view.dispatch({ effects: setVariableContextEffect.of(newContext) })
 */
export const setVariableContextEffect = StateEffect.define<Map<string, VariableScopeEntry>>();

/**
 * StateField holding the current variable context map.
 *
 * Replaces the old Facet-based approach. A Facet is static — its value is set
 * at editor creation and cannot be updated afterwards via a StateEffect.
 * A StateField reacts to transactions, so dispatching setVariableContextEffect
 * actually updates what highlight/autocomplete/popover extensions read.
 */
export const variableContextField = StateField.define<Map<string, VariableScopeEntry>>({
  create: () => new Map(),
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setVariableContextEffect)) return effect.value;
    }
    return value;
  },
});
