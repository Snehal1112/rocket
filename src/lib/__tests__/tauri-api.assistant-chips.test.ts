import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type AssistantResponseChip,
  buildAssistantChipResource,
  maskAssistantResponse,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('assistant chip commands', () => {
  it('builds a chip resource with a null path when none is given', async () => {
    await buildAssistantChipResource('collection', 'shop');
    expect(invoke).toHaveBeenCalledWith('build_assistant_chip_resource', {
      kind: 'collection',
      collection: 'shop',
      path: null,
    });
  });

  it('passes the request path through', async () => {
    await buildAssistantChipResource('request', 'shop', 'orders/list.yml');
    expect(invoke).toHaveBeenCalledWith('build_assistant_chip_resource', {
      kind: 'request',
      collection: 'shop',
      path: 'orders/list.yml',
    });
  });

  it('sends a response to be masked', async () => {
    const response: AssistantResponseChip = {
      method: 'GET',
      url: 'https://api.test',
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 2,
      headers: [],
      body: '{}',
      isBinary: false,
      tests: [],
    };
    await maskAssistantResponse('shop', 'list.yml', response);
    expect(invoke).toHaveBeenCalledWith('mask_assistant_response', {
      collection: 'shop',
      requestPath: 'list.yml',
      response,
    });
  });
});
