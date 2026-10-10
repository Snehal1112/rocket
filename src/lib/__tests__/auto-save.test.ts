import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue(undefined),
  saveGraphQlRequest: vi.fn().mockResolvedValue(undefined),
  saveGrpcRequest: vi.fn().mockResolvedValue(undefined),
}));

const markRequestSaved = vi.fn();
vi.mock('@/stores/pane-store', () => ({
  usePaneStore: {
    getState: () => ({ markRequestSaved }),
  },
}));

import { saveGraphQlRequest, saveGrpcRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';
import { cancelAutoSave, scheduleAutoSave } from '../auto-save';
import { createDefaultRequest, createDefaultRequestFor } from '../pane-utils';

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

  it('saves a grpc tab through saveGrpcRequest, never saveRequest', () => {
    const request = createDefaultRequestFor('grpc');
    request.url = 'localhost:50051';

    scheduleAutoSave('tab1', 'my-collection', 'call.yml', 'Call', request);
    vi.advanceTimersByTime(500);

    expect(saveGrpcRequest).toHaveBeenCalledWith(
      'my-collection',
      'call.yml',
      expect.objectContaining({ uid: 'tab1', url: 'localhost:50051', methodType: 'unary' }),
    );
    expect(saveRequest).not.toHaveBeenCalled();
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

  it('saves enabled path params', () => {
    const request = baseRequest({
      pathParams: [{ id: 'p1', key: 'id', value: '7', enabled: true }],
    });

    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', request);
    vi.advanceTimersByTime(500);

    expect(saveRequest).toHaveBeenCalledWith(
      'my-collection',
      'req.yml',
      expect.objectContaining({ pathParams: [{ name: 'id', value: '7' }] }),
    );
  });

  it('saves an empty path param list when there are none', () => {
    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', baseRequest());
    vi.advanceTimersByTime(500);

    expect(saveRequest).toHaveBeenCalledWith(
      'my-collection',
      'req.yml',
      expect.objectContaining({ pathParams: [] }),
    );
  });

  it('cancelAutoSave prevents a pending save from firing', () => {
    scheduleAutoSave('tab1', 'my-collection', 'req.yml', 'My Request', baseRequest());
    cancelAutoSave('tab1');
    vi.advanceTimersByTime(500);

    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('saves a graphql tab through saveGraphQlRequest, never saveRequest', () => {
    const request = baseRequest({
      requestType: 'graphql',
      method: 'POST',
      graphql: { query: '{ a }', variables: '' },
    });
    scheduleAutoSave('tab1', 'my-collection', 'q.yml', 'Q', request);
    vi.advanceTimersByTime(500);
    expect(saveGraphQlRequest).toHaveBeenCalledWith(
      'my-collection',
      'q.yml',
      expect.objectContaining({ body: { query: '{ a }', variables: undefined } }),
    );
    expect(saveRequest).not.toHaveBeenCalled();
  });
});
