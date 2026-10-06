import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
  saveGrpcRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
}));

import { saveGrpcRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';
import { createDefaultRequestFor } from '../pane-utils';
import { saveTabRequest } from '../save-tab-request';

function tabOf(kind: 'grpc' | 'http'): RequestTab {
  return {
    id: 'tab-1',
    title: 'Call',
    tabType: 'request',
    request: createDefaultRequestFor(kind),
    response: null,
    isDirty: true,
  };
}

describe('saveTabRequest for grpc', () => {
  beforeEach(() => vi.clearAllMocks());

  it('routes a grpc tab to saveGrpcRequest and never to saveRequest', async () => {
    await saveTabRequest('api', 'call.yml', tabOf('grpc'));
    expect(saveGrpcRequest).toHaveBeenCalledTimes(1);
    expect(saveRequest).not.toHaveBeenCalled();
    const [collection, path, payload] = vi.mocked(saveGrpcRequest).mock.calls[0];
    expect([collection, path]).toEqual(['api', 'call.yml']);
    expect(payload.uid).toBe('tab-1');
    expect(payload.methodType).toBe('unary');
  });

  it('keeps routing an http tab to saveRequest', async () => {
    await saveTabRequest('api', 'call.yml', tabOf('http'));
    expect(saveRequest).toHaveBeenCalledTimes(1);
    expect(saveGrpcRequest).not.toHaveBeenCalled();
  });
});
