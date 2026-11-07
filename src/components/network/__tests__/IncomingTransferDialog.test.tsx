import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingTransferDialog } from '@/components/network/IncomingTransferDialog';
import {
  INCOMING_TTL_MS,
  type IncomingTransfer,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeIncomingTransfer } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    respondNetworkTransfer: vi.fn(),
    cancelNetworkTransfer: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

function openOffer(patch: Partial<IncomingTransfer> = {}) {
  const offer = makeIncomingTransfer(patch);
  store().addIncomingTransfer(offer);
  useNetworkStore.setState({ dialogTransferId: offer.transferId });
  return render(<IncomingTransferDialog />);
}

describe('IncomingTransferDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.respondNetworkTransfer.mockResolvedValue(undefined);
    mocks.api.cancelNetworkTransfer.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const SCRIPT_WARNING =
    "This collection's scripts will run in the Safe sandbox when you send its requests. Developer mode, host environment and vault access stay off until you allow them.";

  it('renders nothing while no dialog is open', () => {
    render(<IncomingTransferDialog />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('shows the sender, the short id, the collection, the size and both buttons', async () => {
    openOffer();
    expect(
      await screen.findByRole('dialog', { name: 'Dana Roy wants to send you a collection' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Orders')).toBeInTheDocument();
    expect(screen.getByText('2 KiB')).toBeInTheDocument();
    expect(screen.getByText('Device ID ZZZZ-9999')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
  });

  it('warns that scripts run in the Safe sandbox before Accept', async () => {
    openOffer();
    const note = await screen.findByRole('note');
    expect(note).toHaveTextContent(SCRIPT_WARNING);
    expect(screen.getByText('Nothing is saved until you accept.')).toBeInTheDocument();
  });

  it('drops the script warning once the offer is accepted', async () => {
    openOffer({ accepted: true });
    await screen.findByRole('dialog');
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });

  it('shows a collection name with a double quote as given', async () => {
    openOffer({ collectionName: 'My "Quoted" API' });
    expect(await screen.findByText('My "Quoted" API')).toBeInTheDocument();
  });

  it('names a sender without a display name', async () => {
    openOffer({ displayName: '' });
    expect(
      await screen.findByRole('dialog', { name: 'Unnamed node wants to send you a collection' }),
    ).toBeInTheDocument();
  });

  it('shows 0 bytes', async () => {
    openOffer({ sizeBytes: 0 });
    expect(await screen.findByText('0 bytes')).toBeInTheDocument();
  });

  it('shows a huge size in MiB', async () => {
    openOffer({ sizeBytes: 5 * 1024 * 1024 * 1024 });
    expect(await screen.findByText('5120 MiB')).toBeInTheDocument();
  });

  it('shows the offer whose id the dialog holds', async () => {
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 'a', collectionName: 'First' }));
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 'b', collectionName: 'Second' }));
    useNetworkStore.setState({ dialogTransferId: 'b' });
    render(<IncomingTransferDialog />);
    expect(await screen.findByText('Second')).toBeInTheDocument();
    expect(screen.queryByText('First')).not.toBeInTheDocument();
  });

  it('Accept sends the answer and waits for the bytes', async () => {
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', true));
    expect(await screen.findByText('Receiving from Dana Roy...')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(store().incomingTransfers[0]?.accepted).toBe(true);
  });

  it('Cancel while receiving cancels the transfer and remembers it was local', async () => {
    openOffer({ accepted: true });
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(mocks.api.cancelNetworkTransfer).toHaveBeenCalledWith('t1'));
    expect(store().wasCancelledHere('t1')).toBe(true);
    expect(mocks.api.respondNetworkTransfer).not.toHaveBeenCalled();
  });

  it('shows the error in a state block when the receiving cancel fails', async () => {
    mocks.api.cancelNetworkTransfer.mockRejectedValue('Internal error: link down');
    openOffer({ accepted: true });
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('link down');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });

  it('Decline sends the answer, removes the offer and closes the dialog', async () => {
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
    await waitFor(() => expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', false));
    await waitFor(() => expect(store().dialogTransferId).toBeNull());
    expect(store().incomingTransfers).toEqual([]);
  });

  it('shows the error in a state block and keeps the buttons when the answer fails', async () => {
    mocks.api.respondNetworkTransfer.mockRejectedValue('Internal error: link down');
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('link down');
    expect(block).toHaveAttribute('data-slot', 'state-block');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    expect(store().incomingTransfers[0]?.accepted).toBe(false);
  });

  it('closing the dialog with Escape neither accepts nor declines', async () => {
    openOffer();
    await screen.findByRole('dialog');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(store().dialogTransferId).toBeNull());
    expect(mocks.api.respondNetworkTransfer).not.toHaveBeenCalled();
    expect(store().incomingTransfers).toHaveLength(1);
  });

  it('a late Accept answer does not bring a finished offer back', async () => {
    const pending = createDeferred<void>();
    mocks.api.respondNetworkTransfer.mockReturnValue(pending.promise);
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    act(() => {
      store().finishTransfer('t1');
      store().closeTransferDialog();
    });
    await act(async () => {
      pending.resolve();
    });
    expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', true);
    expect(store().incomingTransfers).toEqual([]);
  });

  it('a late Decline answer does not close the dialog of another offer', async () => {
    const pending = createDeferred<void>();
    mocks.api.respondNetworkTransfer.mockReturnValue(pending.promise);
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 't2', collectionName: 'Billing' }));
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
    act(() => store().openTransferDialog('t2'));
    await act(async () => {
      pending.resolve();
    });
    expect(store().dialogTransferId).toBe('t2');
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['t2']);
  });

  it('closes with the transfer that went away', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
      store().closeTransferDialog();
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('switches to the expired view when the offer expires', () => {
    vi.useFakeTimers();
    openOffer();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(INCOMING_TTL_MS);
    });
    expect(screen.getByRole('dialog', { name: 'Collection offer' })).toBeInTheDocument();
    expect(screen.getByText('The request expired.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(store().dialogTransferId).toBeNull();
  });

  it('keeps the dialog open on an expired finish', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
    });
    expect(await screen.findByText('The request expired.')).toBeInTheDocument();
  });

  it('shows why an unanswered offer ended', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
      store().endTransferDialog('The other node went offline.');
    });
    expect(await screen.findByText('The other node went offline.')).toBeInTheDocument();
    expect(screen.queryByText('The request expired.')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
  });

  it('shows the progress line only after 300 ms', () => {
    vi.useFakeTimers();
    openOffer({ accepted: true });
    act(() => {
      store().setTransferProgress({
        transferId: 't1',
        direction: 'receive',
        doneBytes: 1024,
        totalBytes: 2048,
      });
    });
    expect(screen.getByText('Receiving from Dana Roy...')).toBeInTheDocument();
    expect(screen.queryByText('1 KiB of 2 KiB')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(screen.queryByText('1 KiB of 2 KiB')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.getByText('1 KiB of 2 KiB')).toBeInTheDocument();
  });

  it('never shows the progress line for a transfer that ends within 300 ms', () => {
    vi.useFakeTimers();
    openOffer({ accepted: true });
    act(() => {
      store().setTransferProgress({
        transferId: 't1',
        direction: 'receive',
        doneBytes: 2048,
        totalBytes: 2048,
      });
      vi.advanceTimersByTime(200);
      store().finishTransfer('t1');
      store().closeTransferDialog();
      vi.advanceTimersByTime(500);
    });
    expect(screen.queryByText('2 KiB of 2 KiB')).not.toBeInTheDocument();
  });
});
