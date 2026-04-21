import { type Extension, StateEffect, StateField } from '@codemirror/state';
import {
  type EditorView,
  showTooltip,
  type Tooltip,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { variableContextField } from './variable-context-facet';

const VAR_REGEX = /\{\{([$\w.-]+)\}\}/g;

// ── State management ───────────────────────────────────────────

export interface PopoverState {
  /** Variable name (without {{ }}). */
  varName: string;
  /** Document offset of the token start. */
  from: number;
  /** Document offset of the token end. */
  to: number;
  /** Token type. */
  tokenType: 'variable' | 'pathParam';
  /** Resolved scope entry (or undefined if unresolved). */
  entry: VariableScopeEntry | undefined;
}

/** Effect to open a popover for a specific token. */
export const openPopoverEffect = StateEffect.define<PopoverState>();

/** Effect to close the active popover. */
export const closePopoverEffect = StateEffect.define<null>();

/**
 * State field that holds the currently active popover (or null).
 * Drives the showTooltip facet.
 */
const activePopoverField = StateField.define<PopoverState | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(openPopoverEffect)) return effect.value;
      if (effect.is(closePopoverEffect)) return null;
    }
    // Close popover if the document changed (user typed something).
    if (tr.docChanged && value !== null) return null;
    return value;
  },
});

/**
 * Tooltip provider — reads the active popover state field and returns
 * a Tooltip positioned at the token start. The tooltip's `create`
 * method returns a DOM container; the React wrapper will portal into it.
 */
const popoverTooltip = showTooltip.computeN([activePopoverField], (state) => {
  const popover = state.field(activePopoverField);
  if (!popover) return [];

  const tooltip: Tooltip = {
    pos: popover.from,
    above: false, // Show below the token.
    strictSide: true,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'cm-variable-popover-container';
      // Reserve space so CM6 can measure the tooltip height before React
      // paints the portal content. Without this, height=0 causes wrong placement.
      dom.style.minWidth = '320px';
      dom.style.minHeight = '72px';
      // The React wrapper will find this element and portal into it.
      // We store metadata as data attributes so the wrapper can read them.
      dom.dataset.varName = popover.varName;
      dom.dataset.tokenType = popover.tokenType;
      dom.dataset.from = String(popover.from);
      dom.dataset.to = String(popover.to);
      return { dom };
    },
  };

  return [tooltip];
});

// ── Click handler plugin ───────────────────────────────────────

/**
 * Finds the {{variable}} token at the given document position.
 * Returns the variable name, start, and end offsets, or null.
 */
function findVarTokenAt(
  doc: string,
  pos: number,
): { varName: string; from: number; to: number } | null {
  VAR_REGEX.lastIndex = 0;
  let match = VAR_REGEX.exec(doc);
  while (match !== null) {
    const from = match.index;
    const to = from + match[0].length;
    if (pos >= from && pos <= to) {
      return { varName: match[1], from, to };
    }
    match = VAR_REGEX.exec(doc);
  }
  return null;
}

/**
 * Returns true if the mouse event coordinates fall within the rendered pixel
 * bounds of the token at [from, to). Uses coordsAtPos to get the actual glyph
 * rect rather than relying on posAtCoords snapping, which can map clicks in
 * the editor padding to position 0 and falsely hit a token that starts there.
 */
function clickIsInsideToken(
  view: EditorView,
  event: MouseEvent,
  from: number,
  to: number,
): boolean {
  const start = view.coordsAtPos(from);
  const end = view.coordsAtPos(to);
  if (!start || !end) return false;
  const x = event.clientX;
  const y = event.clientY;
  return x >= start.left && x <= end.right && y >= start.top && y <= end.bottom;
}

/**
 * ViewPlugin that handles click events on {{variable}} tokens.
 * When a click lands inside a variable token, it opens the popover.
 */
class PopoverClickPlugin {
  // biome-ignore lint/suspicious/noEmptyBlockStatements: required interface stub for ViewPlugin.
  update(_update: ViewUpdate) {}
}

/**
 * Extension that provides the popover system:
 * - State field for active popover
 * - Tooltip provider that positions the popover DOM
 * - Click handler that opens/closes the popover
 */
export function variablePopoverExtension(): Extension {
  return [
    activePopoverField,
    popoverTooltip,
    ViewPlugin.fromClass(PopoverClickPlugin, {
      eventHandlers: {
        mousedown: (event: MouseEvent, view: EditorView) => {
          const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
          if (pos === null) return false;

          const doc = view.state.doc.toString();
          const token = findVarTokenAt(doc, pos);

          if (token && clickIsInsideToken(view, event, token.from, token.to)) {
            const context = view.state.field(variableContextField);
            const entry = context.get(token.varName);

            // Prevent default so the click doesn't reposition the cursor
            // and immediately close the popover via the docChanged check.
            event.preventDefault();

            view.dispatch({
              effects: openPopoverEffect.of({
                varName: token.varName,
                from: token.from,
                to: token.to,
                tokenType: 'variable',
                entry,
              }),
            });
            return true; // Handled.
          }

          // Click outside any variable token — close active popover.
          const current = view.state.field(activePopoverField);
          if (current) {
            view.dispatch({ effects: closePopoverEffect.of(null) });
          }
          return false;
        },
      },
    }),
  ];
}

/**
 * Helper to programmatically close the popover from outside CM6
 * (e.g., from the React wrapper after a commit).
 */
export function closePopover(view: EditorView) {
  view.dispatch({ effects: closePopoverEffect.of(null) });
}

/**
 * Helper to read the active popover state from outside CM6.
 */
export function getActivePopover(view: EditorView): PopoverState | null {
  return view.state.field(activePopoverField, false) ?? null;
}
