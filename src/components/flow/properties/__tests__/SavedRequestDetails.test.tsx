import { render, screen } from '@testing-library/react';
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

  it('shows a load error and keeps the card fallback', async () => {
    getRequest.mockRejectedValue('file not found');
    render(<SavedRequestDetails collection='demo' requestPath='gone.yml' />);
    expect(await screen.findByText('Could not load request: file not found')).toBeInTheDocument();
  });
});
