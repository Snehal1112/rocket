import type { Extension } from '@codemirror/state';
import { type EditorView, hoverTooltip, type Tooltip } from '@codemirror/view';
import { variableContextFacet } from './variable-context-facet';

const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

function findVarAt(
  doc: string,
  pos: number,
): {
  varName: string;
  from: number;
  to: number;
} | null {
  VAR_REGEX.lastIndex = 0;
  let match = VAR_REGEX.exec(doc);
  while (match !== null) {
    const from = match.index;
    const to = from + match[0].length;
    if (pos >= from && pos < to) {
      return { varName: match[1], from, to };
    }
    match = VAR_REGEX.exec(doc);
  }
  return null;
}

export function variableHoverTooltip(): Extension {
  return hoverTooltip((view, pos) => {
    const doc = view.state.doc.toString();
    const token = findVarAt(doc, pos);
    if (!token) return null;

    const context = view.state.facet(variableContextFacet);
    const entry = context.get(token.varName);

    const sourceLabel = entry
      ? entry.source.charAt(0).toUpperCase() + entry.source.slice(1)
      : 'Unresolved';
    const displayValue = entry ? (entry.secret ? '●●●●' : entry.value || '(not set)') : '(not set)';

    const tooltip: Tooltip = {
      pos: token.from,
      end: token.to,
      above: true,
      create: (_view: EditorView) => {
        const dom = document.createElement('div');
        dom.className = 'cm-var-hover';

        const nameLine = document.createElement('div');
        nameLine.className = 'cm-var-hover-name';
        nameLine.textContent = `{{${token.varName}}}`;
        dom.appendChild(nameLine);

        const metaLine = document.createElement('div');
        metaLine.className = 'cm-var-hover-meta';
        metaLine.textContent = `${sourceLabel}: ${displayValue}`;
        dom.appendChild(metaLine);

        return { dom };
      },
    };

    return tooltip;
  });
}
