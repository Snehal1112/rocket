import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn(),
}));

vi.mock('@/lib/execute-request', () => ({
  resolveRequestFieldsForPath: vi.fn(async () => ({
    url: 'https://example.com/ping',
    headers: [],
    queryParams: [],
    body: undefined,
    auth: { authType: 'none' },
    collection: 'demo',
    environmentName: undefined,
    requestPath: 'ping.yml',
  })),
}));

import { executeRunnerEntry } from '@/lib/runner-execute';
import { executeRequest } from '@/lib/tauri-api';

function baseRequest(): Request {
  return {
    uid: 'r1',
    name: 'Ping',
    method: 'GET',
    url: 'https://example.com/ping',
    headers: [],
    auth: { authType: 'none' },
    fileName: 'ping.yml',
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
});
