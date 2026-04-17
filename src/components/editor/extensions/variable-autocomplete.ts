import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionSource,
} from '@codemirror/autocomplete';
import type { Extension } from '@codemirror/state';
import type { VariableSource } from '@/lib/url-variables';
import { variableContextFacet } from './variable-context-facet';

/**
 * Boost values for autocomplete ranking. Higher = ranked first.
 * Matches the variable resolution priority order so the variable that
 * would actually win appears first in the completion list.
 */
const SCOPE_BOOST: Record<VariableSource, number> = {
  runtime: 6,
  request: 5,
  folder: 4,
  environment: 3,
  collection: 2,
  global: 1,
  process: 0,
};

/**
 * Single-character badge for the autocomplete detail column.
 */
function scopeBadge(source: VariableSource): string {
  switch (source) {
    case 'runtime':
      return 'R';
    case 'request':
      return 'Q';
    case 'folder':
      return 'F';
    case 'environment':
      return 'E';
    case 'collection':
      return 'C';
    case 'global':
      return 'G';
    case 'process':
      return 'P';
  }
}

/**
 * Completion source for {{variable}} references.
 *
 * Activates when the cursor follows `{{` (with optional partial name typed).
 * Lists all keys from the variableContext facet with scope badges and
 * resolved values.
 *
 * Accepting a completion inserts the variable name and appends `}}`
 * if not already present after the cursor.
 */
const variableCompletionSource: CompletionSource = (context: CompletionContext) => {
  // Match `{{` optionally followed by partial variable name chars.
  const before = context.matchBefore(/\{\{[\w.-]*/);
  if (!before) return null;

  const varContext = context.state.facet(variableContextFacet);
  if (varContext.size === 0) return null;

  const options: Completion[] = [];

  for (const [key, entry] of varContext) {
    options.push({
      label: key,
      detail: scopeBadge(entry.source),
      info: entry.secret ? '●●●●' : entry.value,
      type: 'variable',
      boost: SCOPE_BOOST[entry.source] ?? 0,
      apply: (view, _completion, from, to) => {
        // `from` points to the start of `{{...`. We want to replace from `{{` onward.
        // Insert the key. If `}}` doesn't already follow, append it.
        const afterCursor = view.state.sliceDoc(to, to + 2);
        const insert = afterCursor === '}}' ? key : `${key}}}`;
        // Replace from after `{{` to current cursor position.
        const insertFrom = from + 2;
        view.dispatch({
          changes: { from: insertFrom, to, insert },
          selection: { anchor: insertFrom + insert.length },
        });
      },
    });
  }

  return {
    from: before.from,
    options,
    filter: true, // CM6 handles fuzzy filtering.
  };
};

/**
 * Extension that provides {{variable}} autocomplete.
 * Requires the variableContextFacet to be provided in the editor state.
 *
 * Ctrl+Space triggers completions manually (built into @codemirror/autocomplete).
 */
export function variableAutocomplete(): Extension {
  return autocompletion({
    override: [variableCompletionSource],
    activateOnTyping: true,
    icons: false, // We use the detail column for scope badges instead.
  });
}
