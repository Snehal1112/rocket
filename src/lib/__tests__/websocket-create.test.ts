import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { createWebSocketItem } from '@/lib/websocket-create';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveWebSocketRequest: vi.fn(),
    saveRequest: vi.fn(),
  };
});

describe('createWebSocketItem', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.saveWebSocketRequest).mockReset();
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveWebSocketRequest).mockImplementation(async (_c, path, req) => ({
      ...req,
      fileName: path,
    }));
  });

  it('saves a real websocket file, never an http one', async () => {
    await createWebSocketItem('my-api', undefined, 'Chat', 'wss://x/ws');

    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [collection, path, payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(collection).toBe('my-api');
    expect(path).toBe('Chat.yml');
    expect(payload.url).toBe('wss://x/ws');
    expect(payload.name).toBe('Chat');
    expect(payload.messages).toHaveLength(1);
    expect(payload.uid).toBeTruthy();
  });

  it('places the file under the folder and opens a websocket tab on the saved path', async () => {
    const tab = await createWebSocketItem('my-api', 'realtime', 'Chat', '');

    expect(vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0][1]).toBe('realtime/Chat.yml');
    expect(tab.request.requestType).toBe('websocket');
    expect(tab.source).toEqual({ collection: 'my-api', path: 'realtime/Chat.yml' });
    expect(tab.isDirty).toBe(false);
  });
});
