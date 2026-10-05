import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn().mockResolvedValue({ status: 200 }),
  executeGraphQlRequest: vi.fn().mockResolvedValue({ status: 200 }),
}));

import { executeGraphQlRequest, executeRequest } from '@/lib/tauri-api';
import { dispatchSend } from '../dispatch-send';
import { createDefaultRequestFor } from '../pane-utils';

const baseInput = {
  method: 'POST' as const,
  url: 'https://api.example.com/graphql',
  headers: [],
  queryParams: [],
  body: { mode: 'json' as const, content: 'stale' },
  auth: { authType: 'none' as const },
  options: { followRedirects: true, timeoutMs: 1000, verifySsl: true },
};

describe('dispatchSend', () => {
  beforeEach(() => vi.clearAllMocks());

  it('sends a graphql tab through executeGraphQlRequest and drops any http body', async () => {
    const request = createDefaultRequestFor('graphql');
    request.graphql = { query: '{ a }', variables: '', operationName: 'A' };
    await dispatchSend(request, baseInput, { query: '{ a }', variables: undefined });

    expect(executeRequest).not.toHaveBeenCalled();
    expect(executeGraphQlRequest).toHaveBeenCalledWith({
      request: { ...baseInput, body: undefined },
      query: '{ a }',
      variables: undefined,
      operationName: 'A',
    });
  });

  it('sends an http tab through executeRequest', async () => {
    await dispatchSend(createDefaultRequestFor('http'), baseInput, undefined);
    expect(executeGraphQlRequest).not.toHaveBeenCalled();
    expect(executeRequest).toHaveBeenCalledWith(baseInput);
  });
});
