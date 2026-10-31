import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const getProxySettings = vi.fn();
const saveProxySettings = vi.fn();
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getProxySettings: () => getProxySettings(),
  saveProxySettings: (...a: unknown[]) => saveProxySettings(...a),
}));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { ProxySection } from '../ProxySection';

describe('ProxySection', () => {
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
    render(<ProxySection />);
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.getByDisplayValue('bob')).toBeInTheDocument();
    expect(screen.getByLabelText('Proxy password')).toHaveValue('');
    expect(screen.getByText(/a password is saved/i)).toBeInTheDocument();
  });

  it('keeps the stored password when the field is left empty', async () => {
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    const [settings, password] = saveProxySettings.mock.calls[0];
    expect(settings.mode).toBe('custom');
    expect(password).toEqual({ action: 'keep' });
  });

  it('sends a typed password and can clear the stored one', async () => {
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'set', value: 'pw' });
    await waitFor(() => expect(getProxySettings).toHaveBeenCalledTimes(2));

    saveProxySettings.mockClear();
    fireEvent.click(await screen.findByRole('button', { name: /remove saved password/i }));
    fireEvent.click(await screen.findByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(saveProxySettings).toHaveBeenCalledTimes(1));
    expect(saveProxySettings.mock.calls[0][1]).toEqual({ action: 'clear' });
  });

  it('hides the url fields unless the mode is custom', async () => {
    getProxySettings.mockResolvedValue({ mode: 'system', hasPassword: false });
    render(<ProxySection />);
    await waitFor(() => expect(getProxySettings).toHaveBeenCalled());
    expect(screen.queryByLabelText('HTTP proxy URL')).toBeNull();
    fireEvent.click(await screen.findByRole('radio', { name: /custom/i }));
    expect(screen.getByLabelText('HTTP proxy URL')).toBeInTheDocument();
  });

  it('disables Save and shows an error when the saved setting could not be loaded', async () => {
    getProxySettings.mockRejectedValue(new Error('boom'));
    render(<ProxySection />);
    expect(await screen.findByRole('alert')).toHaveTextContent(/could not load/i);
    expect(screen.getByRole('button', { name: /^save$/i })).toBeDisabled();
  });

  it('says a single proxy URL is used for both schemes', async () => {
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    expect(screen.getByText(/used for both http and https/i)).toBeInTheDocument();
  });

  it('has a Proxy heading', async () => {
    render(<ProxySection />);
    expect(screen.getByRole('heading', { name: 'Proxy' })).toBeInTheDocument();
    await screen.findByDisplayValue('http://proxy.corp:8080');
  });

  it('stays on the section and reloads the saved setting after Save', async () => {
    const stored = { mode: 'custom', httpProxy: 'http://proxy.corp:8080', username: 'bob' };
    getProxySettings
      .mockResolvedValueOnce({ ...stored, hasPassword: false })
      .mockResolvedValueOnce({ ...stored, hasPassword: true });
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    expect(screen.queryByText(/a password is saved/i)).toBeNull();
    fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(getProxySettings).toHaveBeenCalledTimes(2));
    expect(await screen.findByText(/a password is saved/i)).toBeInTheDocument();
    expect(screen.getByLabelText('Proxy password')).toHaveValue('');
  });

  it('keeps the form and Save when the refresh after a successful Save fails', async () => {
    getProxySettings
      .mockResolvedValueOnce({
        mode: 'custom',
        httpProxy: 'http://proxy.corp:8080',
        username: 'bob',
        hasPassword: true,
      })
      .mockRejectedValueOnce(new Error('boom'));
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(getProxySettings).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(screen.getByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('button', { name: /^save$/i })).toBeEnabled();
  });

  it('drops edits on Reload', async () => {
    render(<ProxySection />);
    const field = await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(field, { target: { value: 'http://other:1' } });
    fireEvent.click(screen.getByRole('button', { name: /^reload$/i }));
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(saveProxySettings).not.toHaveBeenCalled();
  });

  it('retries a failed load on Reload', async () => {
    getProxySettings.mockRejectedValueOnce(new Error('boom'));
    render(<ProxySection />);
    expect(await screen.findByRole('alert')).toHaveTextContent(/use reload to try again/i);
    fireEvent.click(screen.getByRole('button', { name: /^reload$/i }));
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^save$/i })).toBeEnabled();
  });
});
