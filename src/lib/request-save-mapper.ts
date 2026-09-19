// Canonical request-tab -> persisted-payload mapper, shared by every save
// path (normal save, save-to-collection). Building this in one place means
// a request saved for the first time carries the same auth/tags/settings/
// scripts/assertions as a normal save, instead of a hand-picked subset.
import { toApiBody } from '@/lib/execute-request';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Request as ApiRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';

export interface RequestSavePayloadOverrides {
  /** Overrides tab.title — used when saving under a name chosen at save time. */
  name?: string;
  /** Set only for a brand-new file (save-to-collection); omit for an existing one. */
  fileName?: string;
}

export function buildRequestSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): ApiRequest {
  const body = tab.request.body;
  const s = tab.request.settings;
  return {
    uid: tab.id || crypto.randomUUID(),
    name: overrides?.name ?? tab.title,
    ...(overrides?.fileName !== undefined ? { fileName: overrides.fileName } : {}),
    method: tab.request.method,
    url: tab.request.url,
    headers: toPersistedHeaders(tab.request.headers),
    body: toApiBody(body),
    auth: toPersistedAuth(tab.request.auth),
    tags: tab.request.tags && tab.request.tags.length > 0 ? tab.request.tags : undefined,
    settings: s
      ? {
          timeout: s.timeoutMs,
          followRedirects: s.followRedirects,
          verifySsl: s.verifySsl,
          maxRedirects: s.maxRedirects,
          encodeUrl: s.encodeUrl,
        }
      : undefined,
    docs: tab.request.docs ?? null,
    preRequestScript: tab.request.preRequestScript ?? null,
    postResponseScript: tab.request.postResponseScript ?? null,
    tests: tab.request.testsScript ?? null,
    assertions: tab.request.assertions ?? [],
  };
}
