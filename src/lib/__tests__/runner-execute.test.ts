import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn(),
  executeGraphQlRequest: vi.fn(),
}));

vi.mock('@/lib/execute-request', () => ({
  resolveRequestFieldsForPath: vi.fn(async () => ({
    url: 'https://example.com/ping',
    headers: [],
    queryParams: [],
    pathParams: [],
    body: undefined,
    auth: { authType: 'none' },
    collection: 'demo',
    environmentName: undefined,
    requestPath: 'ping.yml',
  })),
  getActiveGlobalEnvName: vi.fn(() => 'global-prod'),
  toApiOptions: vi.fn(() => ({ followRedirects: true, timeoutMs: 0, verifySsl: true })),
  getActiveWorkspaceRequestGuardPolicy: vi.fn(async () => ({
    blockScriptRedirectsToInternalHosts: true,
    alsoBlockPrivateRanges: true,
  })),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { executeRunnerEntry } from '@/lib/runner-execute';
import { executeGraphQlRequest, executeRequest } from '@/lib/tauri-api';

function baseRequest(): Request {
  return {
    uid: 'r1',
    name: 'Ping',
    method: 'GET',
    url: 'https://example.com/ping',
    headers: [],
    auth: { authType: 'none' },
    fileName: 'ping.yml',
    tests: 'pm.test("ok", () => {})',
  };
}

describe('executeRunnerEntry', () => {
  beforeEach(() => vi.clearAllMocks());

  it('reports passed for a 2xx response with no failing tests', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [{ name: 'ok', status: 'passed', error: null }],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('passed');
    expect(outcome.result?.status).toBe(200);

    // Regression: assert testsScript field mapping (request.tests → ExecuteRequestInput.testsScript)
    expect(executeRequest).toHaveBeenCalledWith(
      expect.objectContaining({ testsScript: 'pm.test("ok", () => {})' }),
    );

    // Regression: assert auth inheritance (mapApiRequestToState with fromCollection=true)
    expect(resolveRequestFieldsForPath).toHaveBeenCalledWith(
      'demo',
      'ping.yml',
      expect.objectContaining({ auth: { authType: 'inherit' } }),
      true,
    );
  });

  it('reports failed for a non-2xx response', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 500,
      statusText: 'Error',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('reports failed when a test assertion fails', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [{ name: 'bad', status: 'failed', error: 'nope' }],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('reports failed when a script error is present', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: 'ReferenceError: x is not defined',
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('catches a thrown error from executeRequest and reports it', async () => {
    vi.mocked(executeRequest).mockRejectedValue(new Error('network down'));
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
    expect(outcome.error).toBe('network down');
  });

  it('passes globalEnvName, pathParams, and requestGuardPolicy through to executeRequest', async () => {
    // Regression test for a real gap found by final review: runner-executed
    // requests built ExecuteRequestInput without these three fields, so a
    // workspace that opted into the request-mutation host guard was
    // protected on single Send but not when the same request ran through
    // the Collection Runner -- a silent security-control bypass.
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    });
    await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);

    expect(executeRequest).toHaveBeenCalledWith(
      expect.objectContaining({
        globalEnvName: 'global-prod',
        pathParams: [],
        requestGuardPolicy: {
          blockScriptRedirectsToInternalHosts: true,
          alsoBlockPrivateRanges: true,
        },
      }),
    );
  });

  it('sends a graphql entry through executeGraphQlRequest and fails it on errors[]', async () => {
    vi.mocked(executeGraphQlRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '{"data":null,"errors":[{"message":"boom"}]}',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 40,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    });
    const graphql = {
      uid: 'g1',
      name: 'Search',
      method: 'POST' as const,
      url: 'https://example.com/graphql',
      headers: [],
      auth: { authType: 'none' as const },
      body: { query: '{ a }' },
    };
    const outcome = await executeRunnerEntry(
      'demo',
      'search.yml',
      { ...baseRequest(), name: 'Search' },
      undefined,
      graphql,
    );

    expect(executeRequest).not.toHaveBeenCalled();
    expect(executeGraphQlRequest).toHaveBeenCalledWith(
      expect.objectContaining({
        query: '{ a }',
        fallbackFirst: true,
        request: expect.objectContaining({ body: undefined }),
      }),
    );
    expect(outcome.status).toBe('failed');
  });
});
