// Canonical request-tab -> persisted-payload mapper, shared by every save
// path (normal save, save-to-collection). Building this in one place means
// a request saved for the first time carries the same auth/tags/settings/
// scripts/assertions as a normal save, instead of a hand-picked subset.
import { toApiBody } from '@/lib/execute-request';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Request as ApiRequest, GraphQlRequest } from '@/lib/tauri-api';
import type { KeyValueEntry, RequestState, RequestTab } from '@/types/pane-types';

// Only enabled, named path params are persisted.
export function toPersistedPathParams(params: KeyValueEntry[]): { name: string; value: string }[] {
  return params.filter((p) => p.enabled && p.key).map((p) => ({ name: p.key, value: p.value }));
}

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
    pathParams: toPersistedPathParams(tab.request.pathParams),
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

// Builds the persisted GraphQL payload from tab state. Shared by the Save button,
// save-to-collection and auto-save, so all three write the same fields.
export function toApiGraphQlRequest(
  uid: string,
  name: string,
  request: RequestState,
): GraphQlRequest {
  const s = request.settings;
  const gql = request.graphql ?? { query: '', variables: '' };
  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    body: {
      query: gql.query,
      variables: gql.variables.trim() === '' ? undefined : gql.variables,
    },
    bodyVariants: gql.bodyVariants,
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    settings: s
      ? {
          timeout: s.timeoutMs,
          followRedirects: s.followRedirects,
          verifySsl: s.verifySsl,
          maxRedirects: s.maxRedirects,
          encodeUrl: s.encodeUrl,
        }
      : undefined,
    docs: request.docs ?? null,
    preRequestScript: request.preRequestScript ?? null,
    postResponseScript: request.postResponseScript ?? null,
    tests: request.testsScript ?? null,
    assertions: request.assertions ?? [],
    actions: request.actions ?? [],
  };
}

export function buildGraphQlSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): GraphQlRequest {
  const payload = toApiGraphQlRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
  );
  return overrides?.fileName !== undefined ? { ...payload, fileName: overrides.fileName } : payload;
}
