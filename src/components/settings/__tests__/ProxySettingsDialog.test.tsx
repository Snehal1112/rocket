import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const getProxySettings = vi.fn();
const saveProxySettings = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  getProxySettings: () => getProxySettings(),
  saveProxySettings: (...a: unknown[]) => saveProxySettings(...a),
}));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { ProxySettingsDialog } from '../ProxySettingsDialog';

describe('ProxySettingsDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getProxySettings.mockResolvedValue({
      mode: 'custom',
      httpProxy: 'http://proxy.corp:8080',
      username: 'bob',
      hasPassword: true,
    });
    saveProxySettings.mockResolvedValue(undefined);
  });

  it('loads the saved setting and shows that a password is stored without showing it', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.getByDisplayValue('bob')).toBeInTheDocument();
    expect(screen.getByLabelText('Proxy password')).toHaveValue('');
    expect(screen.getByText(/a password is saved/i)).toBeInTheDocument();
  });

  it('keeps the stored password when the field is left empty', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    const [settings, password] = saveProxySettings.mock.calls[0];
    expect(settings.mode).toBe('custom');
    expect(password).toEqual({ action: 'keep' });
  });

  it('sends a typed password and can clear the stored one', async () => {
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'set', value: 'pw' });

    saveProxySettings.mockClear();
    fireEvent.click(screen.getByRole('button', { name: /remove saved password/i }));
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'clear' });
  });

  it('hides the url fields unless the mode is custom', async () => {
    getProxySettings.mockResolvedValue({ mode: 'system', hasPassword: false });
    render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);
    await waitFor(() => expect(getProxySettings).toHaveBeenCalled());
    expect(screen.queryByLabelText('HTTP proxy URL')).toBeNull();
    fireEvent.click(await screen.findByRole('radio', { name: /custom/i }));
    expect(screen.getByLabelText('HTTP proxy URL')).toBeInTheDocument();
  });
});
