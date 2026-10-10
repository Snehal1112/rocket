// Canonical request-tab -> persisted-payload mapper, shared by every save
// path (normal save, save-to-collection). Building this in one place means
// a request saved for the first time carries the same auth/tags/settings/
// scripts/assertions as a normal save, instead of a hand-picked subset.
import { toApiBody } from '@/lib/execute-request';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Request as ApiRequest, GraphQlRequest, GrpcRequest } from '@/lib/tauri-api';
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

// Builds the persisted HTTP payload from request state. Every save path goes through
// this one function, so autosave and the Save button write the same fields.
export function toApiRequest(uid: string, name: string, request: RequestState): ApiRequest {
  const s = request.settings;
  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    pathParams: toPersistedPathParams(request.pathParams),
    body: toApiBody(request.body),
    auth: toPersistedAuth(request.auth),
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

export function buildRequestSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): ApiRequest {
  const payload = toApiRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
  );
  return overrides?.fileName !== undefined ? { ...payload, fileName: overrides.fileName } : payload;
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

// Builds the persisted gRPC payload from tab state. Shared by the Save button,
// save-to-collection and auto-save, so all three write the same fields. Request
// variables are not sent: they are saved on their own path and an empty list keeps them.
export function toApiGrpcRequest(uid: string, name: string, request: RequestState): GrpcRequest {
  const g = request.grpc;
  return {
    uid,
    name,
    url: request.url,
    method: g?.method ? g.method : undefined,
    methodType: g?.methodType ?? 'unary',
    protoFilePath: g?.protoFilePath.trim() ? g.protoFilePath.trim() : undefined,
    metadata: toPersistedHeaders(request.headers),
    messages: (g?.messages ?? []).map((m, i) => ({
      title: m.title,
      selected: i === g?.activeMessage,
      content: m.content,
    })),
    auth: toPersistedAuth(request.auth),
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    docs: request.docs ?? null,
    assertions: request.assertions ?? [],
    seq: g?.passthrough.seq,
    description: g?.passthrough.description,
    scripts: g?.passthrough.scripts,
  };
}

export function buildGrpcSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): GrpcRequest {
  const payload = toApiGrpcRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
  );
  return overrides?.fileName !== undefined ? { ...payload, fileName: overrides.fileName } : payload;
}
