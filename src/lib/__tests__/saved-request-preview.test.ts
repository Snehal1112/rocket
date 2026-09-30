import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => undefined)),
}));

import {
  clearSavedRequestPreviewCache,
  handleCollectionChanged,
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

  it('ignores a load that finishes after the cache was cleared', async () => {
    let resolveFirst: (r: Request) => void = () => undefined;
    getRequest.mockImplementationOnce(
      () =>
        new Promise<Request>((resolve) => {
          resolveFirst = resolve;
        }),
    );
    getRequest.mockResolvedValueOnce(request({ method: 'PUT' }));
    loadSavedRequestPreview('demo', 'a.yml');
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalledTimes(1));
    clearSavedRequestPreviewCache('demo');
    loadSavedRequestPreview('demo', 'a.yml');
    await vi.waitFor(() => expect(peekSavedRequestPreview('demo', 'a.yml')?.status).toBe('ready'));
    resolveFirst(request({ method: 'GET' }));
    await new Promise((r) => setTimeout(r, 0));
    const entry = peekSavedRequestPreview('demo', 'a.yml');
    expect(entry?.status === 'ready' && entry.preview.method).toBe('PUT');
  });
});

describe('collection changes', () => {
  const ready = async (collection: string, path: string) =>
    vi.waitFor(() => expect(peekSavedRequestPreview(collection, path)?.status).toBe('ready'));
  const methodOf = (collection: string, path: string) => {
    const entry = peekSavedRequestPreview(collection, path);
    return entry?.status === 'ready' ? entry.preview.method : undefined;
  };

  beforeEach(async () => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
    getRequest.mockResolvedValue(request({ method: 'GET' }));
    loadSavedRequestPreview('demo', 'a.yml');
    await ready('demo', 'a.yml');
    getRequest.mockClear();
  });

  it('ignores a change to a flow file', async () => {
    handleCollectionChanged({
      type: 'fileChanged',
      collection: 'demo',
      path: '/home/u/.rocket-api/collections/demo/flows/login.yml',
      eventType: 'modify',
    });
    loadSavedRequestPreview('demo', 'a.yml');
    expect(methodOf('demo', 'a.yml')).toBe('GET');
    await new Promise((r) => setTimeout(r, 0));
    expect(getRequest).not.toHaveBeenCalled();
  });

  it('ignores a change to the flows folder itself, with Windows separators too', async () => {
    handleCollectionChanged({
      type: 'fileChanged',
      collection: 'demo',
      path: 'C:\\Users\\u\\.rocket-api\\collections\\demo\\flows',
    });
    loadSavedRequestPreview('demo', 'a.yml');
    await new Promise((r) => setTimeout(r, 0));
    expect(getRequest).not.toHaveBeenCalled();
  });

  it('refreshes on a request change and keeps the old preview until the new one lands', async () => {
    let resolveNext: (r: Request) => void = () => undefined;
    getRequest.mockImplementationOnce(
      () =>
        new Promise<Request>((resolve) => {
          resolveNext = resolve;
        }),
    );
    handleCollectionChanged({
      type: 'fileChanged',
      collection: 'demo',
      path: '/home/u/.rocket-api/collections/demo/a.yml',
    });
    expect(methodOf('demo', 'a.yml')).toBe('GET');
    loadSavedRequestPreview('demo', 'a.yml');
    loadSavedRequestPreview('demo', 'a.yml');
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalledTimes(1));
    expect(methodOf('demo', 'a.yml')).toBe('GET');
    resolveNext(request({ method: 'PUT' }));
    await vi.waitFor(() => expect(methodOf('demo', 'a.yml')).toBe('PUT'));
  });

  it('shows the error when the refresh fails', async () => {
    getRequest.mockResolvedValueOnce(undefined);
    handleCollectionChanged({ type: 'requestDeleted', collection: 'demo', path: 'a.yml' });
    loadSavedRequestPreview('demo', 'a.yml');
    await vi.waitFor(() => expect(peekSavedRequestPreview('demo', 'a.yml')?.status).toBe('error'));
  });

  it('refreshes every collection when the event names none', async () => {
    loadSavedRequestPreview('other', 'b.yml');
    await ready('other', 'b.yml');
    getRequest.mockClear();
    handleCollectionChanged({ type: 'fileChanged', collection: null });
    loadSavedRequestPreview('demo', 'a.yml');
    loadSavedRequestPreview('other', 'b.yml');
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalledTimes(2));
  });
});
