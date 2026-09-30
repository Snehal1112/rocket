import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => undefined)),
}));

import { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
import { SavedRequestDetails } from '../SavedRequestDetails';

describe('SavedRequestDetails', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('shows the method, URL, masked headers, auth type and body', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'POST',
      url: '{{base}}/login',
      headers: [{ key: 'Authorization', value: 'Bearer abc', enabled: true }],
      body: { mode: 'json', content: '{"a":1}' },
      auth: { authType: 'bearer', token: 't' },
    });
    render(<SavedRequestDetails collection='demo' requestPath='auth/login.yml' />);
    expect(await screen.findByText('POST')).toBeInTheDocument();
    expect(screen.getByTestId('saved-request-url')).toHaveTextContent('{{base}}/login');
    expect(screen.getByTestId('saved-request-headers')).toHaveTextContent('Authorization');
    expect(screen.getByTestId('saved-request-headers')).toHaveTextContent('••••••');
    expect(screen.getByTestId('saved-request-auth')).toHaveTextContent('Bearer');
    expect(screen.getByTestId('saved-request-body')).toHaveTextContent('{"a":1}');
  });

  it('shows a load error', async () => {
    getRequest.mockRejectedValue('file not found');
    render(<SavedRequestDetails collection='demo' requestPath='gone.yml' />);
    expect(await screen.findByText('Could not load request: file not found')).toBeInTheDocument();
  });

  it('reloads after the collection cache is cleared', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'POST',
      url: 'https://x.test/login',
      headers: [],
      auth: { authType: 'none' },
    });
    render(<SavedRequestDetails collection='demo' requestPath='auth/login.yml' />);
    expect(await screen.findByText('POST')).toBeInTheDocument();
    act(() => clearSavedRequestPreviewCache('demo'));
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('POST')).toBeInTheDocument();
  });

  it('lists repeated header names without key clashes', async () => {
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'GET',
      url: 'https://x.test',
      headers: [
        { key: 'X-A', value: '1', enabled: true },
        { key: 'X-A', value: '2', enabled: true },
      ],
      auth: { authType: 'none' },
    });
    render(<SavedRequestDetails collection='demo' requestPath='dup.yml' />);
    expect(await screen.findByText('X-A: 2')).toBeInTheDocument();
    expect(errors.mock.calls.some((c) => String(c[0]).includes('same key'))).toBe(false);
    errors.mockRestore();
  });
});
