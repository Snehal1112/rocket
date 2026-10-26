import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncSettings } from '../SpecSyncSettings';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    setOpenApiSyncInterval: vi.fn(),
    setOpenApiSyncToken: vi.fn(),
    clearOpenApiSyncToken: vi.fn(),
    revokeOpenApiSyncConsent: vi.fn(),
  };
});

async function open() {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /Settings/ }));
  return user;
}

describe('SpecSyncSettings', () => {
  // jsdom lacks the pointer and scroll APIs the Radix Select relies on.
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: false }));
  });

  it('asks for hasToken only while the section is open', async () => {
    renderWithQuery(<SpecSyncSettings collection='s1' status={makeStatus()} />);
    expect(tauriApi.getOpenApiSyncStatus).not.toHaveBeenCalled();
    await open();
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('s1', true));
  });

  it('disables the interval until the link is approved', async () => {
    renderWithQuery(
      <SpecSyncSettings collection='s2' status={makeStatus({ consent: 'missing' })} />,
    );
    await open();
    expect(screen.getByRole('combobox', { name: 'Check interval' })).toBeDisabled();
  });

  it('sets the interval', async () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s3' status={makeStatus()} />);
    const user = await open();
    await user.click(screen.getByRole('combobox', { name: 'Check interval' }));
    await user.click(await screen.findByRole('option', { name: 'Every week' }));
    expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('s3', 'weekly');
  });

  it('keeps the token field write-only, empty and cleared after save', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s4' status={makeStatus()} />);
    const user = await open();
    const input = await screen.findByLabelText('Access token');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveValue('');
    await user.type(input, 'ghp_secret');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalledWith('s4', 'ghp_secret');
    await waitFor(() => expect(screen.getByLabelText('Access token')).toHaveValue(''));
  });

  it('shows the consent sentence when saving without approval', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockRejectedValue('consent_required: no grant');
    renderWithQuery(
      <SpecSyncSettings collection='s5' status={makeStatus({ consent: 'missing' })} />,
    );
    const user = await open();
    await user.type(await screen.findByLabelText('Access token'), 'tok');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(
      await screen.findByText(/Approve this spec link on this machine first/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/consent_required/)).toBeNull();
    expect(screen.getByLabelText('Access token')).toHaveValue('');
  });

  it('shows that a token is stored and clears it', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: true }));
    vi.mocked(tauriApi.clearOpenApiSyncToken).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s6' status={makeStatus()} />);
    const user = await open();
    expect(await screen.findByText('A token is stored')).toBeInTheDocument();
    expect(screen.getByLabelText('Access token')).toHaveValue('');
    await user.click(screen.getByRole('button', { name: 'Clear' }));
    expect(tauriApi.clearOpenApiSyncToken).toHaveBeenCalledWith('s6');
  });

  it('stops trusting the link', async () => {
    vi.mocked(tauriApi.revokeOpenApiSyncConsent).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s7' status={makeStatus()} />);
    const user = await open();
    await user.click(screen.getByRole('button', { name: 'Stop trusting this link' }));
    expect(tauriApi.revokeOpenApiSyncConsent).toHaveBeenCalledWith('s7');
  });
});
