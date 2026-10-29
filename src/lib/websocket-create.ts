import { sanitizeFilename } from '@/lib/filename-utils';
import { createWebSocketRequest } from '@/lib/tauri-api';
import {
  createDefaultWebSocketRequestState,
  toApiWebSocketRequest,
  webSocketToTab,
} from '@/lib/websocket-mapper';
import type { RequestTab } from '@/types/pane-types';

/** Creates a new WebSocket request file, and returns the tab to open for it. */
export async function createWebSocketItem(
  collectionName: string,
  folderPath: string | undefined,
  name: string,
  url: string,
): Promise<RequestTab> {
  const fsName = sanitizeFilename(name);
  const filePath = folderPath ? `${folderPath}/${fsName}` : fsName;
  const payload = toApiWebSocketRequest(
    crypto.randomUUID(),
    name,
    createDefaultWebSocketRequestState(url),
    filePath,
  );
  const saved = await createWebSocketRequest(collectionName, filePath, payload);
  return webSocketToTab(saved, collectionName, saved.fileName ?? filePath);
}
