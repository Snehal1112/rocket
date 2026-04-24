import { type Extension, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';

const VAR_REGEX = /\{\{[$\w.-]+\}\}/g;

// Matches a partial {{variable opener that has no closing }}.
// This keeps the {{ prefix visible so autocomplete can activate.
const PARTIAL_VAR_REGEX = /\{\{[$\w.-]*$/;

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

  // Find all complete {{var}} ranges — these stay visible.
  const visibleRanges: Array<[number, number]> = [];
  VAR_REGEX.lastIndex = 0;
  let match = VAR_REGEX.exec(doc);
  while (match !== null) {
    visibleRanges.push([match.index, match.index + match[0].length]);
    match = VAR_REGEX.exec(doc);
  }

  // Also keep any trailing partial opener ({{ with no closing }}) visible
  // so the user can see they're typing a variable and autocomplete activates.
  const partial = PARTIAL_VAR_REGEX.exec(doc);
  if (partial) {
    visibleRanges.push([partial.index, doc.length]);
  }

  // Sort ranges by start position (partial may interleave with complete ranges).
  visibleRanges.sort((a, b) => a[0] - b[0]);

  // Replace everything outside visible ranges with ● widgets.
  let pos = 0;
  for (const [start, end] of visibleRanges) {
    if (pos < start) {
      builder.add(pos, start, Decoration.replace({ widget: new MaskWidget(start - pos) }));
    }
    pos = Math.max(pos, end);
  }
  // Mask trailing text after last visible range.
  if (pos < doc.length) {
    builder.add(pos, doc.length, Decoration.replace({ widget: new MaskWidget(doc.length - pos) }));
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
