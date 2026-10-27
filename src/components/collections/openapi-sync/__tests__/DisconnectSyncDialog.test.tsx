import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useEffect } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { clearApplyState, useApplySync } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { DisconnectSyncDialog } from '../DisconnectSyncDialog';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, disconnectOpenApiSync: vi.fn(), applyOpenApiSync: vi.fn() };
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

  it('starts unticked again after Cancel and reopen', async () => {
    const queryClient = new QueryClient();
    const tree = (open: boolean) => (
      <QueryClientProvider client={queryClient}>
        <DisconnectSyncDialog collection='c1' open={open} onOpenChange={vi.fn()} />
      </QueryClientProvider>
    );
    const view = render(tree(true));
    const user = userEvent.setup();
    await user.click(screen.getByRole('checkbox'));
    expect(screen.getByRole('checkbox')).toBeChecked();
    view.rerender(tree(false));
    view.rerender(tree(true));
    expect(screen.getByRole('checkbox')).not.toBeChecked();
  });

  it('is disabled with a sentence while an apply runs', async () => {
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(new Promise(() => undefined));
    function Starter() {
      const apply = useApplySync('c1');
      useEffect(() => {
        apply.mutate({ approved: [], choices: [], skips: [], variables: [], renameLinks: [] } as never);
        // biome-ignore lint/correctness/useExhaustiveDependencies: start once.
      }, []);
      return null;
    }
    renderWithQuery(
      <>
        <Starter />
        <DisconnectSyncDialog collection='c1' open onOpenChange={vi.fn()} />
      </>,
    );
    expect(screen.getByRole('button', { name: 'Disconnect' })).toBeDisabled();
    expect(
      screen.getByText('An apply is running on this collection. Try again when it finishes.', {
        selector: 'p[role=alert]',
      }),
    ).toBeInTheDocument();
  });
});
