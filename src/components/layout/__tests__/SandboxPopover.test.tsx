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
    saveCollectionSettings: vi.fn(),
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
    vi.mocked(tauriApi.saveCollectionSettings).mockReset();
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
    const popover = await screen.findByText('JavaScript Sandbox', { selector: 'p' });
    const dot = within(popover.closest('div')!.parentElement!.parentElement!).queryByText(
      'Developer Mode',
    );
    expect(dot).toBeInTheDocument();
  });

  it('saves immediately when switching to Safe Mode, preserving the rest of the loaded settings', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'developer', docs: 'hello' }),
    );
    vi.mocked(tauriApi.saveCollectionSettings).mockResolvedValue(undefined);

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Safe Mode'));

    // Must send the whole settings object back, not a bare `{ sandboxMode }` literal —
    // saveCollectionSettings is a full replace on the backend (see this plan's Global
    // Constraints), so a partial payload would silently wipe `docs`/`headers`/etc.
    await waitFor(() =>
      expect(tauriApi.saveCollectionSettings).toHaveBeenCalledWith(
        'my-api',
        baseSettings({ sandboxMode: 'safe', docs: 'hello' }),
      ),
    );
  });
});
