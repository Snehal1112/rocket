import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SandboxPopover } from '@/components/layout/SandboxPopover';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSettings: vi.fn(),
    setCollectionCapability: vi.fn(),
  };
});

function baseSettings(
  overrides: Partial<tauriApi.CollectionSettings> = {},
): tauriApi.CollectionSettings {
  return { headers: [], variables: [], sandboxMode: 'safe', ...overrides };
}

describe('SandboxPopover', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.getCollectionSettings).mockReset();
    vi.mocked(tauriApi.setCollectionCapability).mockReset();
  });

  it('is disabled with no active collection', () => {
    render(<SandboxPopover />);
    expect(screen.getByRole('button', { name: /JavaScript Sandbox/i })).toBeDisabled();
    expect(tauriApi.getCollectionSettings).not.toHaveBeenCalled();
  });

  it("loads and displays the active collection's sandbox mode", async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'developer' }),
    );

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalledWith('my-api'));

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
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'developer', docs: 'hello' }),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockResolvedValue(undefined);

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Safe Mode'));

    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'developerMode', false),
    );
  });

  it('requires confirmation before enabling Developer Mode, and does not save on cancel', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'safe' }),
    );

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

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
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'safe', docs: 'hello' }),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockResolvedValue(undefined);

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

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
    vi.mocked(tauriApi.getCollectionSettings).mockRejectedValue(new Error('boom'));

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));

    expect(await screen.findByText('Failed to load sandbox mode.')).toBeInTheDocument();
    expect(screen.queryByText('Safe Mode')).not.toBeInTheDocument();
  });

  it('shows an error and keeps the confirmation dialog open when saving Developer Mode fails', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'safe' }),
    );
    vi.mocked(tauriApi.setCollectionCapability).mockRejectedValue(new Error('boom'));

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /Enable/i }));

    await waitFor(() => expect(tauriApi.setCollectionCapability).toHaveBeenCalled());
    // A failed save must not silently dismiss the dialog as if it succeeded.
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
  });
});
