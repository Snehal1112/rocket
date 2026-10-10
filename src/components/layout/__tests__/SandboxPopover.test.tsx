import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SandboxPopover } from '@/components/layout/SandboxPopover';
import * as tauriApi from '@/lib/tauri-api';
import { allowed, makeTrust, renderWithQuery, requestedOnly } from '@/test/trust-fixtures';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionTrust: vi.fn(),
    setCollectionCapability: vi.fn(),
  };
});

describe('SandboxPopover', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.getCollectionTrust).mockReset();
    vi.mocked(tauriApi.setCollectionCapability).mockReset();
  });

  it('is disabled with no active collection', () => {
    renderWithQuery(<SandboxPopover />);
    expect(screen.getByRole('button', { name: /JavaScript Sandbox/i })).toBeDisabled();
    expect(tauriApi.getCollectionTrust).not.toHaveBeenCalled();
  });

  it("loads and displays the active collection's sandbox mode", async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ developerMode: allowed }),
    );

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalledWith('my-api'));

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));

    // The "Developer Mode" option label always renders regardless of the active
    // mode, so it can't distinguish the two — assert on the warning footer text,
    // which only renders when mode === 'developer'.
    expect(
      await screen.findByText('Only enable for collections from trusted authors.'),
    ).toBeInTheDocument();
  });

  it('records the capability immediately when switching to Safe Mode', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ developerMode: allowed }),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockResolvedValue(makeTrust());

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Safe Mode'));

    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'developerMode', false),
    );
  });

  it('requires confirmation before enabling Developer Mode, and does not save on cancel', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust(),
    );

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    expect(tauriApi.setCollectionCapability).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: /Cancel/i }));
    expect(tauriApi.setCollectionCapability).not.toHaveBeenCalled();
  });

  it('records Developer Mode only after the confirmation dialog is accepted', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust(),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockResolvedValue(makeTrust());

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /Enable/i }));

    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'developerMode', true),
    );
  });

  it('shows an error instead of a confident mode when loading settings fails', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockRejectedValue(new Error('boom'));

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));

    expect(await screen.findByText('Failed to load sandbox mode.')).toBeInTheDocument();
    expect(screen.queryByText('Safe Mode')).not.toBeInTheDocument();
  });

  it('shows an error and keeps the confirmation dialog open when saving Developer Mode fails', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust(),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockRejectedValue(new Error('boom'));

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /Enable/i }));

    await waitFor(() => expect(tauriApi.setCollectionCapability).toHaveBeenCalled());
    // A failed save must not silently dismiss the dialog as if it succeeded.
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
  });

  it('shows Safe mode and the requested state when the file asks for Developer mode', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ developerMode: requestedOnly, pending: true }),
    );

    renderWithQuery(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));

    expect(await screen.findByText('Requested by this collection')).toBeInTheDocument();
    expect(
      screen.queryByText('Only enable for collections from trusted authors.'),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Allow on this computer/i }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /Enable/i }));
    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'developerMode', true),
    );
  });
});
