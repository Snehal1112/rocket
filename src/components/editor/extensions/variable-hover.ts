import type { Extension, Transaction } from '@codemirror/state';
import { type EditorView, hoverTooltip, type Tooltip } from '@codemirror/view';
import { isDynamicVar } from '@/lib/dynamic-vars';
import { sourceBadgeClass, type VariableScopeEntry } from '@/lib/url-variables';
import { variableContextField } from './variable-context-facet';
import { findVarTokenAt, getActivePopover, openPopoverEffect } from './variable-popover';

/** How long the pointer rests on a token before the value shows. */
const HOVER_DELAY_MS = 300;
/** Longest value shown. A longer one is cut and ends with an ellipsis. */
const HOVER_VALUE_LIMIT = 200;
const MASK = '●●●●';

export function truncateForHover(value: string): string {
  return value.length > HOVER_VALUE_LIMIT ? `${value.slice(0, HOVER_VALUE_LIMIT)}…` : value;
}

// Every node is built with textContent, so a value can never become markup,
// and no attribute carries the value.
function node(tag: 'div' | 'span', className: string, text?: string): HTMLElement {
  const el = document.createElement(tag);
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

/** The tooltip body for one variable token. `entry` is undefined when it does not resolve. */
export function buildVariableHoverDom(
  varName: string,
  entry: VariableScopeEntry | undefined,
): HTMLElement {
  const card = node(
    'div',
    'cm-variable-hover max-w-sm rounded-sm border border-border bg-card px-2 py-1.5 text-xs text-popover-foreground shadow-md',
  );

  if (varName.startsWith('$') && isDynamicVar(varName.slice(1))) {
    card.append(node('div', 'font-mono', `Dynamic (${varName}), generated at send`));
    return card;
  }
  if (!entry) {
    card.append(node('div', 'text-muted-foreground', 'Unresolved'));
    return card;
  }

  const header = node('div', 'mb-1 flex items-center gap-1.5');
  header.append(
    node(
      'span',
      `rounded-full px-1.5 py-0.5 text-2xs font-medium ${sourceBadgeClass(entry.source)}`,
      entry.label,
    ),
  );
  const shown = entry.secret
    ? MASK
    : entry.value === ''
      ? '(empty)'
      : truncateForHover(entry.value);
  card.append(header, node('div', 'font-mono [overflow-wrap:anywhere]', shown));
  return card;
}

/**
 * The hover source: a tooltip for the `{{name}}` token under the pointer, or null.
 * Exported for tests; the extension passes it to `hoverTooltip`.
 */
export function variableHoverSource(view: EditorView, pos: number, side: -1 | 1): Tooltip | null {
  // The click popover already shows the value, so the hover stays out of its way.
  if (getActivePopover(view)) return null;
  const token = findVarTokenAt(view.state.doc.toString(), pos);
  if (!token) return null;
  // At a token's edge the pointer must be on the token side of the edge.
  if ((pos === token.from && side < 0) || (pos === token.to && side > 0)) return null;
  const entry = view.state.field(variableContextField, false)?.get(token.varName);
  return {
    pos: token.from,
    end: token.to,
    above: true,
    create: () => ({ dom: buildVariableHoverDom(token.varName, entry) }),
  };
}

/** True when a transaction opens the click popover, so the hover card must close. */
export function hideOnPopoverOpen(tr: Transaction): boolean {
  return tr.effects.some((e) => e.is(openPopoverEffect));
}

/** Shows a variable's scope and value when the pointer rests on its `{{name}}` token. */
export function variableHover(): Extension {
  return hoverTooltip(variableHoverSource, {
    hoverTime: HOVER_DELAY_MS,
    hideOn: hideOnPopoverOpen,
  });
}
