import { useEffect, useSyncExternalStore } from 'react';
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
  useCacheVersion();
  useEffect(() => {
    if (requestPath) loadSavedRequestPreview(collection, requestPath);
  }, [collection, requestPath]);
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
  useCacheVersion();
  const joined = paths.join('\n');
  // biome-ignore lint/correctness/useExhaustiveDependencies: `joined` stands for `paths`.
  useEffect(() => {
    if (!collection) return;
    for (const path of paths) loadSavedRequestPreview(collection, path);
  }, [collection, joined]);
  const out: Record<string, SavedRequestPreview> = {};
  if (!collection) return out;
  for (const path of paths) {
    const entry = peekSavedRequestPreview(collection, path);
    if (entry?.status === 'ready') out[path] = entry.preview;
  }
  return out;
}
