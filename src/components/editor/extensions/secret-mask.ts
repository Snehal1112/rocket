import { type Extension, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';

const VAR_REGEX = /\{\{[\w.-]+\}\}/g;

/**
 * Widget that renders ● characters as a replacement for secret text.
 * The actual document text is preserved — only the visual rendering is masked.
 */
class MaskWidget extends WidgetType {
  constructor(readonly length: number) {
    super();
  }

  eq(other: MaskWidget) {
    return this.length === other.length;
  }

  toDOM() {
    const span = document.createElement('span');
    span.textContent = '●'.repeat(this.length);
    span.className = 'cm-secret-mask';
    return span;
  }

  ignoreEvent() {
    return false;
  }
}

/**
 * ViewPlugin that replaces all non-{{variable}} text with ● characters.
 * Variable tokens remain visible and colored. The document content is
 * unchanged — only the visual presentation is masked.
 *
 * Variable tokens remain visible; only literal content is hidden.
 */
class SecretMaskPlugin {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildMask(view);
  }

  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged) {
      this.decorations = buildMask(update.view);
    }
  }
}

function buildMask(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc.toString();
  if (!doc) return builder.finish();

  // Find all {{var}} ranges — these stay visible.
  const varRanges: Array<[number, number]> = [];
  VAR_REGEX.lastIndex = 0;
  let match = VAR_REGEX.exec(doc);
  while (match !== null) {
    varRanges.push([match.index, match.index + match[0].length]);
    match = VAR_REGEX.exec(doc);
  }

  // Replace everything outside {{var}} ranges with ● widgets.
  let pos = 0;
  for (const [start, end] of varRanges) {
    if (pos < start) {
      const len = start - pos;
      builder.add(pos, start, Decoration.replace({ widget: new MaskWidget(len) }));
    }
    pos = end;
  }
  // Mask trailing text after last variable.
  if (pos < doc.length) {
    const len = doc.length - pos;
    builder.add(pos, doc.length, Decoration.replace({ widget: new MaskWidget(len) }));
  }

  return builder.finish();
}

/**
 * Extension that masks non-variable text with ● characters.
 * Use for secret/password fields where the raw value should be hidden
 * but variable references remain visible.
 */
export function secretMask(): Extension {
  return ViewPlugin.fromClass(SecretMaskPlugin, {
    decorations: (v) => v.decorations,
  });
}
