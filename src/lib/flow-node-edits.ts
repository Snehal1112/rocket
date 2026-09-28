import type { Auth, FlowEdge, Folder, InlineRequestData, Request } from '@/lib/tauri-api';

// Body modes whose content is plain text, so an inline request can carry it.
const RAW_BODY_MODES = new Set(['json', 'xml', 'text']);

export interface InlineConversion {
  inline: InlineRequestData;
  /** Human-readable names of the parts the inline model cannot carry. */
  dropped: string[];
}

const authCarriesNothing = (auth: Auth) => auth.authType === 'none' || auth.authType === 'inherit';

const hasText = (value: string | null | undefined) => (value ?? '').trim() !== '';

/**
 * Copies a saved request into the inline model a Flow node stores. The inline
 * model holds only method, url, headers and a text body, so everything else
 * is listed in `dropped` for the confirmation the user sees first.
 */
export function savedToInline(request: Request): InlineConversion {
  const dropped: string[] = [];

  const headers = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ name: h.key, value: h.value }));
  const disabled = request.headers.length - headers.length;
  if (disabled > 0) dropped.push(`${disabled} disabled header${disabled === 1 ? '' : 's'}`);

  let body: string | null = null;
  const source = request.body;
  if (source && source.mode !== 'none') {
    if (RAW_BODY_MODES.has(source.mode)) {
      body = hasText(source.content) ? (source.content ?? null) : null;
    } else {
      dropped.push(`${source.mode} body`);
    }
  }

  if (!authCarriesNothing(request.auth)) dropped.push(`${request.auth.authType} auth`);
  if (hasText(request.preRequestScript)) dropped.push('pre-request script');
  if (hasText(request.postResponseScript)) dropped.push('post-response script');
  if (hasText(request.tests)) dropped.push('tests');
  if ((request.assertions?.length ?? 0) > 0) dropped.push('assertions');
  if (request.actions?.some((a) => !a.disabled)) dropped.push('actions');

  return {
    inline: { method: request.method.toUpperCase(), url: request.url, headers, body },
    dropped,
  };
}

/** True when discarding this inline request would lose something the user typed. */
export function inlineHasContent(request: InlineRequestData): boolean {
  return hasText(request.url) || request.headers.length > 0 || hasText(request.body);
}

const HEADER_INDEX_TARGET = /^headers\[(\d+)\]/;

/**
 * Wires into `nodeId` that target a header by position, where that position
 * no longer exists. Such a wire fails at run time with "header index out of
 * range". Wires that name a header are safe, because the run adds it.
 */
export function indexWiresOutOfRange(
  edges: FlowEdge[],
  nodeId: string,
  headerCount: number,
): FlowEdge[] {
  return edges.filter((e) => {
    if (e.targetNodeId !== nodeId) return false;
    const match = HEADER_INDEX_TARGET.exec(e.targetField);
    return match !== null && Number(match[1]) >= headerCount;
  });
}

export interface SavedRequestEntry {
  /** Path relative to the collection root, as a Saved source stores it. */
  path: string;
  name: string;
  method: string;
}

/**
 * Flattens a collection tree into its requests. Paths are built exactly like
 * the sidebar builds them: folder directory names joined by "/", then the
 * request's file name (or its name when there is no file name).
 */
export function requestEntriesOf(folder: Folder, basePath = ''): SavedRequestEntry[] {
  const entries: SavedRequestEntry[] = [];
  for (const item of folder.items) {
    if (item.type === 'folder') {
      const dir = item.dirName ?? item.name;
      entries.push(...requestEntriesOf(item, basePath ? `${basePath}/${dir}` : dir));
    } else if (item.type === 'request' || item.type === 'summary') {
      const file = item.fileName ?? item.name;
      entries.push({
        path: basePath ? `${basePath}/${file}` : file,
        name: item.name,
        method: item.method,
      });
    }
  }
  return entries;
}
