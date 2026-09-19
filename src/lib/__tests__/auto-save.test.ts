import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue(undefined),
}));

const markClean = vi.fn();
vi.mock('@/stores/pane-store', () => ({
  usePaneStore: {
    getState: () => ({ markClean }),
  },
}));

import { saveRequest } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';
import { cancelAutoSave, scheduleAutoSave } from '../auto-save';
import { createDefaultRequest } from '../pane-utils';

function baseRequest(overrides: Partial<RequestState> = {}): RequestState {
  return { ...createDefaultRequest(), ...overrides };
}

describe('scheduleAutoSave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // Regression test: auto-save used to filter headers by `enabled` (dropping
  // every disabled header) instead of by non-blank key, unlike every other
  // save path.
  it('preserves a disabled header instead of dropping it', () => {
    const request = baseRequest({
      headers: [{ id: 'h1', key: 'X-Debug', value: '1', enabled: false }],
    });

    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', request);
    vi.advanceTimersByTime(500);

    expect(saveRequest).toHaveBeenCalledWith(
      'my-collection',
      'req.yml',
      expect.objectContaining({
        headers: [{ key: 'X-Debug', value: '1', enabled: false }],
      }),
    );
  });

  it('drops a blank-key draft row', () => {
    const request = baseRequest({
      headers: [{ id: 'h1', key: '', value: 'unfinished', enabled: true }],
    });

    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', request);
    vi.advanceTimersByTime(500);

    expect(saveRequest).toHaveBeenCalledWith(
      'my-collection',
      'req.yml',
      expect.objectContaining({ headers: [] }),
    );
  });

  // Regression test: auto-save routed all non-OAuth2 auth through
  // execute-request.ts's toApiAuth, which unconditionally converts AWS
  // SigV4 to { authType: 'none' } (that converter is correct for the wire
  // request, but wrong for what gets written to disk).
  it('preserves AWS SigV4 auth instead of silently converting it to none', () => {
    const request = baseRequest({
      auth: {
        authType: 'aws-sig-v4',
        awsSigV4: {
          accessKey: 'AKIA...',
          secretKey: 'secret',
          region: 'us-east-1',
          service: 'execute-api',
          sessionToken: 'session-abc',
        },
      },
    });

    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', request);
    vi.advanceTimersByTime(500);

    expect(saveRequest).toHaveBeenCalledWith(
      'my-collection',
      'req.yml',
      expect.objectContaining({
        auth: expect.objectContaining({
          authType: 'aws-sig-v4',
          accessKey: 'AKIA...',
          secretKey: 'secret',
          sessionToken: 'session-abc',
        }),
      }),
    );
  });

  it('cancelAutoSave prevents a pending save from firing', () => {
    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', baseRequest());
    cancelAutoSave('tab1');
    vi.advanceTimersByTime(500);

    expect(saveRequest).not.toHaveBeenCalled();
  });
});
