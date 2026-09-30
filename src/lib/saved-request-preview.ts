import { isSensitiveHeader, REDACTED_VALUE } from '@/lib/sensitive-headers';
import { getRequest, onCollectionChanged, type Request } from '@/lib/tauri-api';

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

export type PreviewEntry =
  | { status: 'loading' }
  | { status: 'ready'; preview: SavedRequestPreview }
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
const listeners = new Set<() => void>();
let version = 0;
let subscribedToChanges = false;

const keyOf = (collection: string, path: string) => `${collection}\u0000${path}`;

function notify() {
  version += 1;
  for (const listener of listeners) listener();
}

// A saved request can change in its own tab, so a collection change drops
// that collection's previews. Tests that mock tauri-api without this
// listener simply skip it.
function watchCollectionChanges() {
  if (subscribedToChanges) return;
  subscribedToChanges = true;
  try {
    onCollectionChanged((event) => clearSavedRequestPreviewCache(event.collection)).catch(
      () => undefined,
    );
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
  if (cache.has(key)) return;
  // A clear during the load replaces or drops this placeholder. The result is
  // then stale, so it is only written while the placeholder is still there.
  const pending: PreviewEntry = { status: 'loading' };
  cache.set(key, pending);
  notify();
  const settle = (entry: PreviewEntry) => {
    if (cache.get(key) !== pending) return;
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

export function clearSavedRequestPreviewCache(collection?: string): void {
  if (collection === undefined) cache.clear();
  else
    for (const key of [...cache.keys()])
      if (key.startsWith(`${collection}\u0000`)) cache.delete(key);
  notify();
}

export function subscribeSavedRequestPreviews(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getSavedRequestPreviewVersion(): number {
  return version;
}
