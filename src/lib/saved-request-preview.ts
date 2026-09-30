import { isSensitiveHeader, REDACTED_VALUE } from '@/lib/sensitive-headers';
import {
  type CollectionChangedEvent,
  getRequest,
  onCollectionChanged,
  type Request,
} from '@/lib/tauri-api';

/** What the flow shows of a saved request. Secrets are already masked. */
export interface SavedRequestPreview {
  method: string;
  url: string;
  headers: { key: string; value: string; enabled: boolean }[];
  /** The auth kind, such as 'bearer'. Never a credential. */
  authType: string;
  /** The first 20 lines of the body, or null when there is none. */
  bodyPreview: string | null;
}

// A stale ready entry stays on screen while it is loaded again, so a change
// elsewhere in the collection never flashes the card back to empty.
export type PreviewEntry =
  | { status: 'loading' }
  | { status: 'ready'; preview: SavedRequestPreview; stale?: boolean }
  | { status: 'error'; error: string };

const BODY_PREVIEW_LINES = 20;

export function toSavedRequestPreview(request: Request): SavedRequestPreview {
  const body = request.body;
  let text: string | null = null;
  if (body && (body.mode === 'formdata' || body.mode === 'formurlencoded')) {
    const lines = (body.formData ?? []).filter((e) => e.enabled).map((e) => `${e.key}=${e.value}`);
    text = lines.length ? lines.join('\n') : null;
  } else if (body && body.mode !== 'none' && body.content) {
    text = body.content;
  }
  return {
    method: request.method,
    url: request.url,
    headers: request.headers.map((h) => ({
      key: h.key,
      value: isSensitiveHeader(h.key) ? REDACTED_VALUE : h.value,
      enabled: h.enabled,
    })),
    authType: request.auth.authType,
    bodyPreview: text === null ? null : text.split('\n').slice(0, BODY_PREVIEW_LINES).join('\n'),
  };
}

const cache = new Map<string, PreviewEntry>();
// The token of each load in flight. A clear drops the token, so a result that
// arrives after it is ignored.
const inflight = new Map<string, object>();
const listeners = new Set<() => void>();
let version = 0;
let subscribedToChanges = false;

const keyOf = (collection: string, path: string) => `${collection}\u0000${path}`;

function notify() {
  version += 1;
  for (const listener of listeners) listener();
}

// True when a watcher path is a flow file or the flows folder of the
// collection. Saving a flow writes there, and no saved request lives there.
function isFlowPath(collection: string, path: string): boolean {
  const normalized = path.replace(/\\/g, '/');
  const dir = `/${collection}/flows`;
  return normalized.endsWith(dir) || normalized.includes(`${dir}/`);
}

/**
 * Refreshes the previews a collection change may affect. A change to a flow
 * file is ignored. An event without a collection refreshes every preview.
 */
export function handleCollectionChanged(event: CollectionChangedEvent): void {
  const collection = event.collection ?? null;
  if (collection !== null && event.path && isFlowPath(collection, event.path)) return;
  refreshSavedRequestPreviews(collection);
}

// A saved request can change in its own tab, so a collection change refreshes
// that collection's previews. Tests that mock tauri-api without this listener
// simply skip it.
function watchCollectionChanges() {
  if (subscribedToChanges) return;
  subscribedToChanges = true;
  try {
    onCollectionChanged(handleCollectionChanged).catch(() => undefined);
  } catch {
    // No event bridge (tests); the cache is cleared explicitly there.
  }
}

export function peekSavedRequestPreview(
  collection: string,
  path: string,
): PreviewEntry | undefined {
  return cache.get(keyOf(collection, path));
}

export function loadSavedRequestPreview(collection: string, path: string): void {
  watchCollectionChanges();
  const key = keyOf(collection, path);
  const current = cache.get(key);
  const isStale = current?.status === 'ready' && current.stale === true;
  if ((current && !isStale) || inflight.has(key)) return;
  const token = {};
  inflight.set(key, token);
  // A stale preview stays visible during the load; otherwise show a placeholder.
  if (!current) {
    cache.set(key, { status: 'loading' });
    notify();
  }
  // A failed reload replaces a stale preview too, because it usually means
  // the request was deleted or moved.
  const settle = (entry: PreviewEntry) => {
    if (inflight.get(key) !== token) return;
    inflight.delete(key);
    cache.set(key, entry);
    notify();
  };
  Promise.resolve()
    .then(() => getRequest(collection, path))
    .then((request) => {
      if (!request) throw new Error('request not found');
      settle({ status: 'ready', preview: toSavedRequestPreview(request) });
    })
    .catch((err) => {
      settle({ status: 'error', error: err instanceof Error ? err.message : String(err) });
    });
}

const inCollection = (key: string, collection: string | null) =>
  collection === null || key.startsWith(`${collection}\u0000`);

/** Drops the previews of one collection, or all of them. */
export function clearSavedRequestPreviewCache(collection?: string): void {
  const scope = collection ?? null;
  for (const key of [...cache.keys()]) if (inCollection(key, scope)) cache.delete(key);
  for (const key of [...inflight.keys()]) if (inCollection(key, scope)) inflight.delete(key);
  notify();
}

/**
 * Marks the previews of one collection, or all of them when null, for a
 * reload. A ready preview stays visible until its reload lands; any other
 * entry is dropped.
 */
export function refreshSavedRequestPreviews(collection: string | null): void {
  for (const [key, entry] of [...cache.entries()]) {
    if (!inCollection(key, collection)) continue;
    inflight.delete(key);
    if (entry.status === 'ready') cache.set(key, { ...entry, stale: true });
    else cache.delete(key);
  }
  notify();
}

export function subscribeSavedRequestPreviews(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getSavedRequestPreviewVersion(): number {
  return version;
}
