import { type Extension, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { setVariableContextEffect, variableContextField } from './variable-context-facet';

const VAR_REGEX = /\{\{([$\w.-]+)\}\}/g;

/**
 * ViewPlugin that scans the document for {{variable}} patterns and applies
 * Decoration.mark() with CSS classes based on resolution status.
 *
 * Reads the variableContext facet to determine if each variable is resolved
 * and which scope it belongs to.
 */
class VariableHighlightPlugin {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildDecorations(view);
  }

  update(update: ViewUpdate) {
    // Rebuild when: document changes, viewport changes, or variable context changes.
    if (
      update.docChanged ||
      update.viewportChanged ||
      update.transactions.some((tr) => tr.effects.some((e) => e.is(setVariableContextEffect)))
    ) {
      this.decorations = buildDecorations(update.view);
    }
  }
}

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const context = view.state.field(variableContextField);
  const doc = view.state.doc.toString();

  // Reset lastIndex for global regex reuse.
  VAR_REGEX.lastIndex = 0;
  let match = VAR_REGEX.exec(doc);

  while (match !== null) {
    const from = match.index;
    const to = from + match[0].length;
    const varName = match[1];
    const entry = context.get(varName);

    const cssClass = entry ? `cm-var cm-var-${entry.source}` : 'cm-var cm-var-unresolved';

    builder.add(from, to, Decoration.mark({ class: cssClass }));
    match = VAR_REGEX.exec(doc);
  }

  return builder.finish();
}

/**
 * Extension that highlights {{variable}} tokens with scope-colored CSS classes.
 * Requires the variableContextField to be included in the editor state.
 */
export function variableHighlight(): Extension {
  return ViewPlugin.fromClass(VariableHighlightPlugin, {
    decorations: (v) => v.decorations,
  });
}
