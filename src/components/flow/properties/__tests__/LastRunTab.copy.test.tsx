import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowDebugRequest, FlowNode } from '@/lib/tauri-api';

const clip = vi.hoisted(() => ({ copyTextAsync: vi.fn(async (_text: Promise<string>) => undefined) }));
vi.mock('@/lib/clipboard', () => clip);
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

import { LastRunTab } from '../LastRunTab';

const request: FlowNode = {
  id: 'n1',
  position: { x: 0, y: 0 },
  kind: { kind: 'Request', label: 'Login', source: { type: 'Saved', requestPath: 'a.yml' } },
};

const exchange: FlowDebugRequest = {
  method: 'GET',
  url: 'https://api.test/x',
  headers: [],
  response: {
    status: 200,
    statusText: 'OK',
    durationMs: 5,
    sizeBytes: 5,
    headers: [],
    body: '{"ok":true}',
  },
};

describe('LastRunTab copy', () => {
  it('copies through the shared clipboard helper, starting inside the click', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy response body' }));

    expect(clip.copyTextAsync).toHaveBeenCalledTimes(1);
    const arg = clip.copyTextAsync.mock.calls[0][0] as Promise<string>;
    expect(arg).toBeInstanceOf(Promise);
    expect(await arg).toBe('{"ok":true}');
  });
});
