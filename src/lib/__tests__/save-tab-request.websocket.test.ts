import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import { saveTabRequest } from '@/lib/save-tab-request';
import * as tauriApi from '@/lib/tauri-api';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn().mockResolvedValue({ fileName: 'a.yml' }),
    saveGraphQlRequest: vi.fn().mockResolvedValue({ fileName: 'q.yml' }),
    saveWebSocketRequest: vi.fn().mockResolvedValue({ fileName: 'chat.yml' }),
  };
});

function tab(request: RequestTab['request']): RequestTab {
  return { id: 'u1', title: 'T', tabType: 'request', request, response: null, isDirty: true };
}

describe('saveTabRequest websocket routing', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.saveRequest).mockClear();
    vi.mocked(tauriApi.saveGraphQlRequest).mockClear();
    vi.mocked(tauriApi.saveWebSocketRequest).mockClear();
  });

  it('saves a websocket tab with the websocket command and never the http one', async () => {
    const saved = await saveTabRequest(
      'c',
      'chat',
      tab(createDefaultWebSocketRequestState('wss://x')),
    );

    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveGraphQlRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1);
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.uid).toBe('u1');
    expect(payload.url).toBe('wss://x');
    expect(saved.fileName).toBe('chat.yml');
  });

  it('still saves an http tab with the http command', async () => {
    await saveTabRequest('c', 'a', tab(createDefaultRequest()));
    expect(tauriApi.saveWebSocketRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveRequest).toHaveBeenCalledTimes(1);
  });

  it('honours the name and file name overrides used by Save to Collection', async () => {
    await saveTabRequest('c', 'chat', tab(createDefaultWebSocketRequestState()), {
      name: 'Renamed',
      fileName: 'chat',
    });
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.name).toBe('Renamed');
    expect(payload.fileName).toBe('chat');
  });
});
