import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';
import { LoadTestDialog } from '../LoadTestDialog';

// Mock the tauri-api module so no real IPC call is made.
vi.mock('@/lib/tauri-api', () => ({
  runLoadTest: vi.fn(),
}));

// Mock the execute-request helper so no real variable resolution runs.
vi.mock('@/lib/execute-request', () => ({
  resolveRequestFields: vi.fn(async (_tabId: string, req: RequestState) => ({
    url: req.url,
    headers: req.headers,
    queryParams: req.queryParams,
    body: (req.body as { mode: string } | null)?.mode === 'none' ? null : req.body,
    auth: req.auth,
    collection: undefined,
    environmentName: undefined,
    requestPath: undefined,
  })),
}));

import { runLoadTest } from '@/lib/tauri-api';

const noop = () => {
  // Intentional no-op for test props.
};

function makeRequest(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://example.com',
    headers: [],
    queryParams: [],
    pathParams: [],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'none' },
    settings: {
      followRedirects: true,
      timeoutMs: 30000,
      verifySsl: true,
    },
    docs: null,
  } as unknown as RequestState;
}

describe('LoadTestDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // Anchor on the label text, then walk to the sibling input inside the
  // same wrapper <div class='space-y-1.5'>. Robust to added inputs
  // elsewhere in the dialog.
  function getDelayInput(): HTMLInputElement {
    const label = screen.getByText(/delay between requests/i);
    const input = label.parentElement?.querySelector('input[type="number"]');
    if (!(input instanceof HTMLInputElement)) {
      throw new Error('delay input not found');
    }
    return input;
  }

  it('renders the delay input', () => {
    render(<LoadTestDialog open onOpenChange={noop} request={makeRequest()} tabId='t1' />);
    expect(getDelayInput()).toBeDefined();
  });

  it('forwards intervalMs = seconds * 1000 to runLoadTest', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 1,
      succeeded: 1,
      failed: 0,
      failedTransport: 0,
      failedStatus: 0,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });

    render(<LoadTestDialog open onOpenChange={noop} request={makeRequest()} tabId='t1' />);

    const delayInput = getDelayInput();
    fireEvent.change(delayInput, { target: { value: '0.5' } });

    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));

    await waitFor(() => expect(runLoadTest).toHaveBeenCalledTimes(1));
    const configArg = (runLoadTest as unknown as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(configArg.intervalMs).toBe(500);
  });

  it('shows the failure breakdown when failures exist', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 10,
      succeeded: 5,
      failed: 5,
      failedTransport: 2,
      failedStatus: 3,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });

    render(<LoadTestDialog open onOpenChange={noop} request={makeRequest()} tabId='t1' />);
    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));

    await waitFor(() => expect(screen.getByText(/3 status, 2 transport/i)).toBeDefined());
  });
});
