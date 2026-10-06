import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cancelAutoSave, scheduleAutoSave } from '@/lib/auto-save';
import * as tauriApi from '@/lib/tauri-api';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn().mockResolvedValue({}),
    saveGraphQlRequest: vi.fn().mockResolvedValue({}),
    saveWebSocketRequest: vi.fn().mockResolvedValue({}),
  };
});

describe('scheduleAutoSave for a websocket tab', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(tauriApi.saveRequest).mockClear();
    vi.mocked(tauriApi.saveWebSocketRequest).mockClear();
  });
  afterEach(() => {
    cancelAutoSave('u1');
    vi.useRealTimers();
  });

  it('auto-saves with the websocket command, never the http one', async () => {
    scheduleAutoSave('u1', 'c', 'chat.yml', 'Chat', createDefaultWebSocketRequestState('wss://x'));
    await vi.advanceTimersByTimeAsync(600);

    expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1);
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
  });
});
