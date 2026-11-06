import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PairingDialog } from '@/components/network/PairingDialog';
import {
  type ActivePairing,
  type PairingIncoming,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeIncoming } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    respondNetworkPairing: vi.fn(),
    cancelNetworkPairing: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const started = (patch: Partial<ActivePairing> = {}): ActivePairing => ({
  pairingId: 'p1',
  deviceId: 'DEV1',
  shortId: 'AAAA-1111',
  displayName: 'Ben Ortiz',
  code: '482913',
  phase: 'waiting',
  ...patch,
});

const request = (patch: Partial<PairingIncoming> = {}) =>
  makeIncoming({ deviceId: 'DEV2', shortId: 'DDDD-4444', code: '048213', ...patch });

function openStarted(patch: Partial<ActivePairing> = {}) {
  useNetworkStore.setState({ activePairing: started(patch), dialogPairingId: 'p1' });
  render(<PairingDialog />);
}

function openRequest(patch: Partial<PairingIncoming> = {}) {
  store().addIncoming(request(patch));
  useNetworkStore.setState({ dialogPairingId: 'p1' });
  render(<PairingDialog />);
}

describe('PairingDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.respondNetworkPairing.mockResolvedValue(undefined);
    mocks.api.cancelNetworkPairing.mockResolvedValue(undefined);
  });

  it('renders nothing while no dialog is open', () => {
    render(<PairingDialog />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  describe('started by this node', () => {
    it('shows the name, the code with a space, the short id and the buttons', async () => {
      openStarted();
      expect(await screen.findByRole('dialog', { name: 'Pair with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByText('482 913')).toBeInTheDocument();
      expect(screen.getByText('Device ID AAAA-1111')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });

    it('Accept sends the decision and waits for the peer', async () => {
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'accept'),
      );
      expect(await screen.findByText('Waiting for Ben Ortiz...')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
      expect(store().activePairing?.phase).toBe('done');
    });

    it('Cancel cancels the pairing and closes the dialog', async () => {
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
      await waitFor(() => expect(mocks.api.cancelNetworkPairing).toHaveBeenCalledWith('p1'));
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(store().activePairing).toBeNull();
    });

    it('closing a dialog that still waits for the user cancels the pairing', async () => {
      openStarted();
      await screen.findByRole('dialog');
      await userEvent.keyboard('{Escape}');
      await waitFor(() => expect(mocks.api.cancelNetworkPairing).toHaveBeenCalledWith('p1'));
    });

    it('closing after Accept only hides the dialog', async () => {
      openStarted({ phase: 'done' });
      await screen.findByRole('dialog');
      await userEvent.keyboard('{Escape}');
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(mocks.api.cancelNetworkPairing).not.toHaveBeenCalled();
      expect(store().activePairing?.phase).toBe('done');
    });

    it('does not bring a pairing back when it finished during the call', async () => {
      const pending = createDeferred<void>();
      mocks.api.respondNetworkPairing.mockReturnValue(pending.promise);
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      act(() => {
        store().setActivePairing(null);
        store().closePairingDialog();
      });
      await act(async () => {
        pending.resolve();
      });
      expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'accept');
      expect(store().activePairing).toBeNull();
    });

    it('shows the error and keeps the buttons when the decision fails', async () => {
      mocks.api.respondNetworkPairing.mockRejectedValue('Internal error: link down');
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      expect(await screen.findByRole('alert')).toHaveTextContent('link down');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(store().activePairing?.phase).toBe('waiting');
    });
  });

  describe('request from another node', () => {
    it('shows the name, the code with its leading zero and three buttons', async () => {
      openRequest();
      expect(await screen.findByRole('dialog', { name: 'Dana Roy wants to pair' })).toBeInTheDocument();
      expect(screen.getByText('048 213')).toBeInTheDocument();
      expect(screen.getByText('Device ID DDDD-4444')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Block' })).toBeEnabled();
    });

    it('Accept waits for the peer and keeps the request', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'accept'),
      );
      expect(await screen.findByText('Waiting for Dana Roy...')).toBeInTheDocument();
      expect(store().incoming[0]?.accepted).toBe(true);
    });

    it('Decline answers, removes the request and closes the dialog', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'decline'),
      );
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(store().incoming).toEqual([]);
    });

    it('Block answers with block and says so', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Block' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'block'),
      );
      expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy is blocked.');
      expect(store().incoming).toEqual([]);
    });

    it('switches to "The request expired." when the request goes while open', async () => {
      openRequest();
      await screen.findByRole('dialog', { name: 'Dana Roy wants to pair' });
      act(() => store().removeIncoming('p1'));
      expect(await screen.findByText('The request expired.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
      expect(screen.queryByText('048 213')).not.toBeInTheDocument();
      await userEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(store().dialogPairingId).toBeNull();
    });
  });

  describe('late responses', () => {
    it('a late cancel does not close the dialog of another pairing', async () => {
      const cancel = createDeferred<void>();
      mocks.api.cancelNetworkPairing.mockReturnValue(cancel.promise);
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
      // The pairing finishes, then the user opens another request.
      act(() => {
        store().setActivePairing(null);
        store().addIncoming(request({ pairingId: 'p2' }));
        store().openPairingDialog('p2');
      });
      await act(async () => {
        cancel.resolve();
      });
      expect(store().dialogPairingId).toBe('p2');
    });

    it('a late decline does not close the dialog of another pairing', async () => {
      const respond = createDeferred<void>();
      mocks.api.respondNetworkPairing.mockReturnValue(respond.promise);
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
      act(() => {
        store().removeIncoming('p1');
        store().addIncoming(request({ pairingId: 'p2' }));
        store().openPairingDialog('p2');
      });
      await act(async () => {
        respond.resolve();
      });
      expect(store().dialogPairingId).toBe('p2');
      expect(store().incoming.map((i) => i.pairingId)).toEqual(['p2']);
    });

    it('a late accept does not bring a finished request back', async () => {
      const respond = createDeferred<void>();
      mocks.api.respondNetworkPairing.mockReturnValue(respond.promise);
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      act(() => {
        store().removeIncoming('p1');
        store().closePairingDialog();
      });
      await act(async () => {
        respond.resolve();
      });
      expect(store().incoming).toEqual([]);
    });
  });
});
