import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearApplyState } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { DisconnectSyncDialog } from '../DisconnectSyncDialog';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, disconnectOpenApiSync: vi.fn() };
});

function renderDialog() {
  const onDone = vi.fn();
  renderWithQuery(
    <DisconnectSyncDialog
      collection='c1'
      open
      onOpenChange={vi.fn()}
      onDisconnected={onDone}
    />,
  );
  return { user: userEvent.setup(), onDone };
}

describe('DisconnectSyncDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearApplyState();
  });
  afterEach(() => clearApplyState());

  it('says what disconnecting does and offers the token checkbox', () => {
    renderDialog();
    expect(
      screen.getByText('Rocket stops checking this spec. Your requests are not changed.'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('checkbox', {
        name: 'Also remove the stored token (this also removes it for any copy of this collection on this machine that uses the same link)',
      }),
    ).not.toBeChecked();
  });

  it('disconnects without removing the token by default', async () => {
    vi.mocked(tauriApi.disconnectOpenApiSync).mockResolvedValue(undefined);
    const { user, onDone } = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(tauriApi.disconnectOpenApiSync).toHaveBeenCalledWith('c1', false);
  });

  it('passes removeToken when the box is ticked', async () => {
    vi.mocked(tauriApi.disconnectOpenApiSync).mockResolvedValue(undefined);
    const { user } = renderDialog();
    await user.click(screen.getByRole('checkbox'));
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    await waitFor(() => expect(tauriApi.disconnectOpenApiSync).toHaveBeenCalledWith('c1', true));
  });

  it('calls once on a double click', async () => {
    vi.mocked(tauriApi.disconnectOpenApiSync).mockReturnValue(new Promise(() => undefined));
    const { user } = renderDialog();
    await user.dblClick(screen.getByRole('button', { name: 'Disconnect' }));
    expect(tauriApi.disconnectOpenApiSync).toHaveBeenCalledTimes(1);
  });

  it('shows the error and stays open when it fails', async () => {
    vi.mocked(tauriApi.disconnectOpenApiSync).mockRejectedValue('Conflict: an apply is running');
    const { user, onDone } = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(onDone).not.toHaveBeenCalled();
  });
});
