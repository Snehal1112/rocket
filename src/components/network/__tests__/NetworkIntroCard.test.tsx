import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkIntroCard } from '@/components/network/NetworkIntroCard';
import type { NetworkStatus } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { useSettingsStore } from '@/stores/settings-store';
import { makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    markNetworkIntroSeen: vi.fn(),
    setNetworkVisibility: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const INTRO = "You're visible on this network as 'Ana Lima'.";

function setup(patch: Partial<NetworkStatus> = {}) {
  useNetworkStore.setState({ status: makeStatus({ introSeen: false, ...patch }) });
  render(<NetworkIntroCard />);
}

describe('NetworkIntroCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    useSettingsStore.getState().reset();
    mocks.api.markNetworkIntroSeen.mockResolvedValue(undefined);
    mocks.api.setNetworkVisibility.mockResolvedValue(undefined);
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus({ introSeen: true }));
  });

  it('says who the user is visible as', () => {
    setup();
    expect(screen.getByText(INTRO)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Edit name' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Hide my node' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'OK' })).toBeInTheDocument();
  });

  it.each([
    ['the intro was seen', { introSeen: true }],
    ['the node is hidden', { hidden: true }],
    ['networking is off', { state: 'off' as const, offReason: 'No keychain.' }],
  ])('stays away when %s', (_label, patch) => {
    setup(patch);
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('stays away until the status is loaded', () => {
    render(<NetworkIntroCard />);
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('OK marks the intro as seen once and hides the card at once', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalled());
    expect(useNetworkStore.getState().status?.introSeen).toBe(true);
  });

  it('Edit name opens the Network section of Settings and marks the intro as seen', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Edit name' }));
    expect(useSettingsStore.getState().open).toBe(true);
    expect(useSettingsStore.getState().section).toBe('network');
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
  });

  it('Hide my node hides the node and marks the intro as seen', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.api.setNetworkVisibility).toHaveBeenCalledWith(true));
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('keeps the card and shows the error when hiding fails', async () => {
    mocks.api.setNetworkVisibility.mockRejectedValue('Internal error: settings locked');
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('settings locked'));
    expect(mocks.api.markNetworkIntroSeen).not.toHaveBeenCalled();
    expect(screen.getByText(INTRO)).toBeInTheDocument();
  });

  it('hides the card for this session even when saving the flag fails', async () => {
    mocks.api.markNetworkIntroSeen.mockRejectedValue('Internal error: disk full');
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    await waitFor(() => expect(log).toHaveBeenCalled());
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
    log.mockRestore();
  });
});
