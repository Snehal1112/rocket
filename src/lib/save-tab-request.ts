import {
  buildGraphQlSavePayload,
  buildRequestSavePayload,
  type RequestSavePayloadOverrides,
} from '@/lib/request-save-mapper';
import { saveGraphQlRequest, saveRequest } from '@/lib/tauri-api';
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
  return saveRequest(collection, path, buildRequestSavePayload(tab, overrides));
}
