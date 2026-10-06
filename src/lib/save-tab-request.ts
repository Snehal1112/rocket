import {
  buildGraphQlSavePayload,
  buildGrpcSavePayload,
  buildRequestSavePayload,
  type RequestSavePayloadOverrides,
} from '@/lib/request-save-mapper';
import {
  saveGraphQlRequest,
  saveGrpcRequest,
  saveRequest,
  saveWebSocketRequest,
} from '@/lib/tauri-api';
import { buildWebSocketSavePayload } from '@/lib/websocket-mapper';
import type { RequestTab } from '@/types/pane-types';

// Saves a tab through the command that matches its protocol. Writing a GraphQL
// tab through saveRequest would replace the GraphQL file with an HTTP one.
export async function saveTabRequest(
  collection: string,
  path: string,
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): Promise<{ fileName?: string }> {
  if (tab.request.requestType === 'graphql') {
    return saveGraphQlRequest(collection, path, buildGraphQlSavePayload(tab, overrides));
  }
  if (tab.request.requestType === 'grpc') {
    return saveGrpcRequest(collection, path, buildGrpcSavePayload(tab, overrides));
  }
  if (tab.request.requestType === 'websocket') {
    return saveWebSocketRequest(collection, path, buildWebSocketSavePayload(tab, overrides));
  }
  return saveRequest(collection, path, buildRequestSavePayload(tab, overrides));
}
