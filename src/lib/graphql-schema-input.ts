import {
  getActiveGlobalEnvName,
  getActiveWorkspaceRequestGuardPolicy,
  resolveRequestFields,
} from '@/lib/execute-request';
import type { ExecuteRequestInput } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

// The HTTP side of an introspection request: the tab's endpoint, headers, auth
// and settings. Scripts, assertions and the body are left out on purpose; the
// backend adds the introspection query and ignores them anyway.
export async function buildSchemaRequestInput(
  tabId: string,
  request: RequestState,
): Promise<ExecuteRequestInput> {
  const resolved = await resolveRequestFields(tabId, request);
  return {
    method: request.method,
    url: resolved.url,
    headers: resolved.headers,
    queryParams: resolved.queryParams,
    auth: resolved.auth,
    options: {
      followRedirects: request.settings?.followRedirects ?? true,
      timeoutMs: request.settings?.timeoutMs ?? 30000,
      verifySsl: request.settings?.verifySsl ?? true,
    },
    collection: resolved.collection,
    environmentName: resolved.environmentName,
    requestPath: resolved.requestPath,
    globalEnvName: getActiveGlobalEnvName(),
    requestName: 'GraphQL introspection',
    requestGuardPolicy: await getActiveWorkspaceRequestGuardPolicy(),
  };
}
