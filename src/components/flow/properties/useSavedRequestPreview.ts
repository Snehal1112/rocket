import { useEffect, useMemo, useSyncExternalStore } from 'react';
import {
  getSavedRequestPreviewVersion,
  loadSavedRequestPreview,
  peekSavedRequestPreview,
  type SavedRequestPreview,
  subscribeSavedRequestPreviews,
} from '@/lib/saved-request-preview';

export { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';

const useCacheVersion = () =>
  useSyncExternalStore(subscribeSavedRequestPreviews, getSavedRequestPreviewVersion);

/** One saved request's preview, loaded on first use. */
export function useSavedRequestPreview(collection: string, requestPath: string | null) {
  const version = useCacheVersion();
  // A clear bumps the version, so the effect loads the request again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` triggers the reload.
  useEffect(() => {
    if (requestPath) loadSavedRequestPreview(collection, requestPath);
  }, [collection, requestPath, version]);
  const entry = requestPath ? peekSavedRequestPreview(collection, requestPath) : undefined;
  return {
    preview: entry?.status === 'ready' ? entry.preview : null,
    error: entry?.status === 'error' ? entry.error : null,
    loading: requestPath !== null && (!entry || entry.status === 'loading'),
  };
}

/** The ready previews of many saved requests, keyed by path. */
export function useSavedRequestPreviews(
  collection: string | null,
  paths: string[],
): Record<string, SavedRequestPreview> {
  const version = useCacheVersion();
  const joined = paths.join('\n');
  // A clear bumps the version, so the effect loads the requests again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `joined` stands for `paths` and `version` triggers the reload.
  useEffect(() => {
    if (!collection) return;
    for (const path of joined ? joined.split('\n') : []) loadSavedRequestPreview(collection, path);
  }, [collection, joined, version]);
  // The same record until the cache changes, so callers can memoize on it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` marks a cache change.
  return useMemo(() => {
    const out: Record<string, SavedRequestPreview> = {};
    if (!collection) return out;
    for (const path of joined ? joined.split('\n') : []) {
      const entry = peekSavedRequestPreview(collection, path);
      if (entry?.status === 'ready') out[path] = entry.preview;
    }
    return out;
  }, [collection, joined, version]);
}
