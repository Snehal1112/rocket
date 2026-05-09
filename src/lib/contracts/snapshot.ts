import type { ContractScope, RequestShapeMap, RequestShape } from '@/types/contracts';

/**
 * Builds a RequestShapeMap from a collection's live requests, filtered to scope.
 *
 * Option B note: this function is used ONLY in NewContractModal for the live
 * "preview" panel showing which endpoints will be snapshotted. The authoritative
 * snapshot is taken by Rust when `publish_contract` is called.
 */
export function computeSnapshot(
  requests: CollectionRequest[],
  scope: ContractScope,
): RequestShapeMap {
  const inScope = filterRequestsByScope(requests, scope);
  return Object.fromEntries(inScope.map((req) => [req.id, buildShape(req)]));
}

function buildShape(req: CollectionRequest): RequestShape {
  return {
    method: req.method,
    path: req.path,
    params: (req.params ?? []).map((p) => ({
      key: p.key,
      required: p.required ?? false,
      type: p.type,
    })),
    headers: (req.headers ?? []).map((h) => ({ key: h.key, required: h.required ?? false })),
    bodySchema: req.body?.schema ?? undefined,
  };
}

function filterRequestsByScope(
  requests: CollectionRequest[],
  scope: ContractScope,
): CollectionRequest[] {
  if (scope.type === 'collection') return requests;
  if (scope.type === 'folder') return requests.filter((r) => r.folderId === scope.folderId);
  return requests.filter((r) => scope.requestIds.includes(r.id));
}

/** Loose type for any request object from the collection store.
 *  The actual type comes from rocket-collection's IPC shape — adjust
 *  field names if the real type differs (e.g. `query` vs `params`). */
export interface CollectionRequest {
  id: string;
  method: string;
  path: string;
  folderId?: string;
  params?: Array<{ key: string; required?: boolean; type?: string }>;
  headers?: Array<{ key: string; required?: boolean }>;
  body?: { schema?: string } | null;
}
