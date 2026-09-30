import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => undefined)),
}));

import {
  clearSavedRequestPreviewCache,
  loadSavedRequestPreview,
  peekSavedRequestPreview,
  toSavedRequestPreview,
} from '../saved-request-preview';

const request = (overrides: Partial<Request> = {}): Request => ({
  uid: 'u',
  name: 'Login',
  method: 'POST',
  url: '{{base}}/login',
  headers: [
    { key: 'Authorization', value: 'Bearer abc', enabled: true },
    { key: 'X-Trace', value: '1', enabled: true },
    { key: 'X-Off', value: 'no', enabled: false },
  ],
  body: { mode: 'json', content: Array.from({ length: 25 }, (_, i) => `line ${i + 1}`).join('\n') },
  auth: { authType: 'bearer', token: 'secret-token' },
  ...overrides,
});

describe('toSavedRequestPreview', () => {
  it('masks sensitive headers and shows only the auth type', () => {
    const p = toSavedRequestPreview(request());
    expect(p.method).toBe('POST');
    expect(p.url).toBe('{{base}}/login');
    expect(p.headers).toEqual([
      { key: 'Authorization', value: '••••••', enabled: true },
      { key: 'X-Trace', value: '1', enabled: true },
      { key: 'X-Off', value: 'no', enabled: false },
    ]);
    expect(p.authType).toBe('bearer');
    expect(JSON.stringify(p)).not.toContain('secret-token');
  });

  it('keeps the first 20 lines of the body', () => {
    const lines = toSavedRequestPreview(request()).bodyPreview?.split('\n') ?? [];
    expect(lines).toHaveLength(20);
    expect(lines[19]).toBe('line 20');
  });

  it('shows form fields as key=value and no body as null', () => {
    const form = toSavedRequestPreview(
      request({
        body: {
          mode: 'formurlencoded',
          formData: [
            { key: 'a', value: '1', entryType: 'text', enabled: true },
            { key: 'b', value: '2', entryType: 'text', enabled: false },
          ],
        },
      }),
    );
    expect(form.bodyPreview).toBe('a=1');
    expect(toSavedRequestPreview(request({ body: { mode: 'none' } })).bodyPreview).toBeNull();
  });
});

describe('the preview cache', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('loads a request once and serves it from the cache', async () => {
    getRequest.mockResolvedValue(request());
    loadSavedRequestPreview('demo', 'auth/login.yml');
    loadSavedRequestPreview('demo', 'auth/login.yml');
    await vi.waitFor(() =>
      expect(peekSavedRequestPreview('demo', 'auth/login.yml')?.status).toBe('ready'),
    );
    expect(getRequest).toHaveBeenCalledTimes(1);
  });

  it('records a load failure, including a missing request', async () => {
    getRequest.mockResolvedValue(undefined);
    loadSavedRequestPreview('demo', 'gone.yml');
    await vi.waitFor(() =>
      expect(peekSavedRequestPreview('demo', 'gone.yml')?.status).toBe('error'),
    );
  });

  it('clears one collection only', async () => {
    getRequest.mockResolvedValue(request());
    loadSavedRequestPreview('demo', 'a.yml');
    loadSavedRequestPreview('other', 'a.yml');
    await vi.waitFor(() => expect(peekSavedRequestPreview('other', 'a.yml')?.status).toBe('ready'));
    clearSavedRequestPreviewCache('demo');
    expect(peekSavedRequestPreview('demo', 'a.yml')).toBeUndefined();
    expect(peekSavedRequestPreview('other', 'a.yml')?.status).toBe('ready');
  });
});
