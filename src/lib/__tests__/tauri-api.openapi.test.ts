import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { importOpenApiFile, importOpenApiUrl, type OpenApiImportResult } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const result = {
  report: {},
  collection: 'Orders',
  sourceUrl: null,
  warnings: [],
} as unknown as OpenApiImportResult;

describe('OpenAPI import API', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('importOpenApiUrl invokes the link command', async () => {
    vi.mocked(invoke).mockResolvedValue(result);
    await expect(importOpenApiUrl('https://x.test/a.yaml', 'paths', 'ws1')).resolves.toEqual(
      result,
    );
    expect(invoke).toHaveBeenCalledWith('import_openapi_url', {
      link: 'https://x.test/a.yaml',
      groupBy: 'paths',
      targetWorkspaceId: 'ws1',
    });
  });

  it('importOpenApiFile invokes the file command', async () => {
    vi.mocked(invoke).mockResolvedValue(result);
    await expect(importOpenApiFile('/tmp/a.yaml', 'paths', 'ws1')).resolves.toEqual(result);
    expect(invoke).toHaveBeenCalledWith('import_openapi_file', {
      path: '/tmp/a.yaml',
      groupBy: 'paths',
      targetWorkspaceId: 'ws1',
    });
  });
});
