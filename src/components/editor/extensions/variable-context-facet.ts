import { Facet, StateEffect } from '@codemirror/state';
import type { VariableScopeEntry } from '@/lib/url-variables';

/**
 * Facet providing the current variable context (Map<string, VariableScopeEntry>)
 * to all extensions. Updated reactively via setVariableContextEffect.
 *
 * When multiple inputs are provided, the last one wins (most recent update).
 */
export const variableContextFacet = Facet.define<
  Map<string, VariableScopeEntry>,
  Map<string, VariableScopeEntry>
>({
  combine: (inputs) => inputs[inputs.length - 1] ?? new Map(),
});

/**
 * State effect to update the variable context. Dispatch this when the
 * environment store changes:
 *
 *   view.dispatch({ effects: setVariableContextEffect.of(newContext) })
 */
export const setVariableContextEffect = StateEffect.define<Map<string, VariableScopeEntry>>();
