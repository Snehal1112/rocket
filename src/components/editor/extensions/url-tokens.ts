import { type Extension, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { type ParsedCurl, parseCurl } from '@/lib/curl-parser';

const VAR_REGEX = /\{\{[\w.-]+\}\}/g;
const PATH_PARAM_REGEX = /:(\w+)/g;

interface UrlTokensConfig {
  /** Path param values for resolution. */
  pathParams?: Record<string, string>;
  /** Query param values for resolution. */
  queryParams?: Record<string, string>;
  /** Called when a curl command is pasted. */
  onCurlImport?: (parsed: ParsedCurl) => void;
}

/**
 * Builds the decoration set for a single pass over the URL document.
 * Highlights :pathParam tokens in the path portion and query keys after `?`.
 * Skips any position that falls inside a {{var}} range.
 */
function buildUrlTokenDecorations(view: EditorView, config: UrlTokensConfig): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc.toString();

  // Find all {{var}} ranges to skip.
  const varRanges: Array<[number, number]> = [];
  VAR_REGEX.lastIndex = 0;
  let vm = VAR_REGEX.exec(doc);
  while (vm !== null) {
    varRanges.push([vm.index, vm.index + vm[0].length]);
    vm = VAR_REGEX.exec(doc);
  }

  const isInsideVar = (pos: number) => varRanges.some(([s, e]) => pos >= s && pos < e);

  // Split on the first `?`.
  const qIdx = doc.indexOf('?');
  const pathPart = qIdx >= 0 ? doc.slice(0, qIdx) : doc;

  // Path params: :paramName in the path portion.
  PATH_PARAM_REGEX.lastIndex = 0;
  let pm = PATH_PARAM_REGEX.exec(pathPart);
  while (pm !== null) {
    const from = pm.index;
    const to = from + pm[0].length;
    if (!isInsideVar(from)) {
      const paramName = pm[1];
      const resolved = config.pathParams?.[paramName] !== undefined;
      const cssClass = resolved ? 'cm-pathparam' : 'cm-pathparam-unresolved';
      builder.add(from, to, Decoration.mark({ class: cssClass }));
    }
    pm = PATH_PARAM_REGEX.exec(pathPart);
  }

  // Query params: key=value segments after `?`.
  if (qIdx >= 0) {
    const queryStr = doc.slice(qIdx + 1);
    const pairs = queryStr.split('&');
    let offset = qIdx + 1;

    for (const pair of pairs) {
      if (!pair) {
        offset += 1; // Skip empty segment after &.
        continue;
      }
      const eqIdx = pair.indexOf('=');
      if (eqIdx > 0 && !isInsideVar(offset)) {
        const keyFrom = offset;
        const keyTo = offset + eqIdx;
        builder.add(keyFrom, keyTo, Decoration.mark({ class: 'cm-querykey' }));
      }
      offset += pair.length + 1; // +1 for the & separator.
    }
  }

  return builder.finish();
}

/**
 * Extension for URL bar: highlights :pathParam and query key=value tokens,
 * and optionally detects curl paste.
 *
 * The plugin class is defined inside the factory so it closes over `config`.
 * When config changes, the consumer rebuilds the extensions list and CM6
 * recreates the plugin with the new config.
 */
export function urlTokens(config: UrlTokensConfig): Extension {
  // Define the plugin class inside the factory to close over `config`.
  class UrlTokensPlugin {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = buildUrlTokenDecorations(view, config);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = buildUrlTokenDecorations(update.view, config);
      }
    }
  }

  const exts: Extension[] = [
    ViewPlugin.fromClass(UrlTokensPlugin, {
      decorations: (v) => v.decorations,
    }),
  ];

  // Curl paste detection — only wired when a handler is provided.
  if (config.onCurlImport) {
    const onCurlImport = config.onCurlImport;
    exts.push(
      EditorView.domEventHandlers({
        paste: (event: ClipboardEvent, _view: EditorView) => {
          const text = event.clipboardData?.getData('text/plain')?.trim();
          if (!text || !/^curl\s/i.test(text)) return false;
          event.preventDefault();
          const parsed = parseCurl(text);
          if (parsed) {
            onCurlImport(parsed);
          }
          return true;
        },
      }),
    );
  }

  return exts;
}
