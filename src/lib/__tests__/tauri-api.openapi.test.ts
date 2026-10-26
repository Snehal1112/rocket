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

  it('importOpenApiUrl passes the token when given', async () => {
    vi.mocked(invoke).mockResolvedValue(result);
    await importOpenApiUrl('https://github.com/o/r/blob/main/a.yaml', 'tags', 'ws1', 't0k');
    expect(invoke).toHaveBeenCalledWith('import_openapi_url', {
      link: 'https://github.com/o/r/blob/main/a.yaml',
      groupBy: 'tags',
      targetWorkspaceId: 'ws1',
      token: 't0k',
    });
  });

  it('importOpenApiUrl leaves the token key out when not given', async () => {
    vi.mocked(invoke).mockResolvedValue(result);
    await importOpenApiUrl('https://x.test/a.yaml', 'tags', 'ws1');
    const args = vi.mocked(invoke).mock.calls[0][1] as Record<string, unknown>;
    expect('token' in args).toBe(false);
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
