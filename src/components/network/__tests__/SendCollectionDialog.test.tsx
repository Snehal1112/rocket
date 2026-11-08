import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SendCollectionDialog } from '@/components/network/SendCollectionDialog';
import { useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeNode } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    sendNetworkCollection: vi.fn(),
    cancelNetworkTransfer: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const started = (notes: string[] = []) => ({ transferId: 't1', notes });

// The options of the waiting toast: its id, its duration and its Cancel action.
const waitingOptions = () =>
  mocks.toast.info.mock.calls[0]?.[1] as {
    id: string;
    duration: number;
    action: { label: string; onClick: () => void };
  };

const ben = makeNode({
  deviceId: 'DEV1',
  shortId: 'AAAA-1111',
  displayName: 'Ben Ortiz',
  group: 'paired',
  status: 'online',
});
const cara = makeNode({
  deviceId: 'DEV2',
  shortId: 'BBBB-2222',
  displayName: 'Cara Lund',
  group: 'paired',
  status: 'online',
});

function setup(nodes = [ben], collectionName = 'Orders') {
  useNetworkStore.setState({ nodes });
  const onClose = vi.fn();
  const view = render(<SendCollectionDialog open collectionName={collectionName} onClose={onClose} />);
  return { onClose, view };
}

describe('SendCollectionDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.sendNetworkCollection.mockResolvedValue(started());
    mocks.api.cancelNetworkTransfer.mockResolvedValue(undefined);
  });

  it('lists online paired nodes only', async () => {
    setup([
      ben,
      makeNode({ deviceId: 'OFF', displayName: 'Offline Olga', group: 'paired', status: 'offline' }),
      makeNode({ deviceId: 'NEAR', displayName: 'Nearby Nils', group: 'nearby', status: 'available' }),
      cara,
    ]);
    expect(await screen.findByRole('radio', { name: /Ben Ortiz/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Cara Lund/ })).toBeInTheDocument();
    expect(screen.queryByText(/Offline Olga/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Nearby Nils/)).not.toBeInTheDocument();
  });

  it('shows the short id next to each name', async () => {
    setup([ben, cara]);
    expect(await screen.findByText('AAAA-1111')).toBeInTheDocument();
    expect(screen.getByText('BBBB-2222')).toBeInTheDocument();
  });

  it('chooses the only online node by itself', async () => {
    setup([ben]);
    expect(await screen.findByRole('radio', { name: /Ben Ortiz/ })).toBeChecked();
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('asks the user to choose when several nodes are online', async () => {
    setup([ben, cara]);
    expect(await screen.findByRole('button', { name: 'Send' })).toBeDisabled();
    await userEvent.click(screen.getByRole('radio', { name: /Cara Lund/ }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('keeps Include environments off by default and sends false', async () => {
    setup();
    const box = await screen.findByRole('checkbox', { name: 'Include environments' });
    expect(box).not.toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', false),
    );
  });

  it('sends true when Include environments is ticked', async () => {
    setup();
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Include environments' }));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', true),
    );
  });

  it('explains the checkbox with always-visible help text', async () => {
    setup();
    const box = await screen.findByRole('checkbox', { name: 'Include environments' });
    expect(box).toHaveAccessibleDescription(/Off by default/);
  });

  it('passes a name with spaces, quotes and unicode unchanged', async () => {
    const name = '  Orders "v2" – café 日本 (copy)  ';
    setup([ben], name);
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', name, false),
    );
  });

  it('puts a double quote in the title unchanged', async () => {
    setup([ben], 'Orders "v2"');
    expect(await screen.findByRole('dialog', { name: 'Send Orders "v2"' })).toBeInTheDocument();
  });

  it('closes at once when there are no notes and tells the user to wait for the answer', async () => {
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Sent to Ben Ortiz. Waiting for them to accept.',
      expect.objectContaining({ id: 'network-transfer-waiting-t1' }),
    );
    expect(screen.queryByText('Not sent with the collection')).not.toBeInTheDocument();
  });

  it('puts a Cancel action on the waiting toast', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    expect(waitingOptions().action.label).toBe('Cancel');
    // The toast stays until the transfer ends: the bridge dismisses it.
    expect(waitingOptions().duration).toBe(Number.POSITIVE_INFINITY);
  });

  it('Cancel on the waiting toast cancels that transfer', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    waitingOptions().action.onClick();
    await waitFor(() => expect(mocks.api.cancelNetworkTransfer).toHaveBeenCalledWith('t1'));
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('remembers the send for the bridge and marks a Cancel as local', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    // The bridge turns the waiting toast into "Sending to Ben Ortiz..." from this entry.
    expect(store().advanceOutgoing('t1', 'sending')).toBe('Ben Ortiz');
    waitingOptions().action.onClick();
    expect(store().wasCancelledHere('t1')).toBe(true);
  });

  it('unmarks the local cancel when the cancel is refused', async () => {
    mocks.api.cancelNetworkTransfer.mockRejectedValue('Invalid input: Already sent.');
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    waitingOptions().action.onClick();
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
    expect(store().wasCancelledHere('t1')).toBe(false);
  });

  it('shows no waiting toast when the transfer finished before the call returned', async () => {
    const pending = createDeferred<ReturnType<typeof started>>();
    mocks.api.sendNetworkCollection.mockReturnValue(pending.promise);
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    store().finishTransfer('t1');
    await act(async () => {
      pending.resolve(started());
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('shows the name rule error inline', async () => {
    mocks.api.sendNetworkCollection.mockRejectedValue(
      'Invalid input: This collection name cannot be sent. Rename it and try again.',
    );
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This collection name cannot be sent. Rename it and try again.',
    );
  });

  it('shows the error when the cancel is refused', async () => {
    mocks.api.cancelNetworkTransfer.mockRejectedValue(
      'Invalid input: Already sent. Waiting for the result.',
    );
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    waitingOptions().action.onClick();
    await waitFor(() =>
      expect(mocks.toast.error).toHaveBeenCalledWith('Already sent. Waiting for the result.'),
    );
  });

  it('lists the notes after Send and keeps the dialog open until Done', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(
      started(['Flows: skipped', 'Bearer token: blanked']),
    );
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Not sent with the collection')).toBeInTheDocument();
    expect(screen.getByText('Flows: skipped')).toBeInTheDocument();
    expect(screen.getByText('Bearer token: blanked')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    // The waiting toast shows in both cases.
    expect(mocks.toast.info).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Send' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('shows a note with a double quote as given', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(started(['Auth "token" in Orders "v2": blanked']));
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Auth "token" in Orders "v2": blanked')).toBeInTheDocument();
  });

  it('names a node without a display name by its short id', async () => {
    const { onClose } = setup([makeNode({ ...ben, displayName: '' })]);
    expect(await screen.findByRole('radio', { name: /AAAA-1111/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Sent to Unnamed node. Waiting for them to accept.',
      expect.objectContaining({ id: 'network-transfer-waiting-t1' }),
    );
  });

  it('shows the error inline, keeps the dialog open and enables Send again', async () => {
    mocks.api.sendNetworkCollection.mockRejectedValue(
      'Invalid input: This collection is too big to send (11 MiB, limit 10).',
    );
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('This collection is too big to send (11 MiB, limit 10).');
    expect(block).toHaveAttribute('data-slot', 'state-block');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(onClose).not.toHaveBeenCalled();
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('disables Send and Cancel while the call is in flight', async () => {
    const pending = createDeferred<ReturnType<typeof started>>();
    mocks.api.sendNetworkCollection.mockReturnValue(pending.promise);
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    await userEvent.keyboard('{Escape}');
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => {
      pending.resolve(started());
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it('Cancel closes without sending', async () => {
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(mocks.api.sendNetworkCollection).not.toHaveBeenCalled();
  });

  it('shows a needs-setup state and a disabled Send when no node is online', async () => {
    setup([makeNode({ group: 'paired', status: 'offline' })]);
    expect(await screen.findByText('No paired node is online')).toBeInTheDocument();
    expect(document.querySelector('[data-slot="empty-state"]')).toHaveAttribute(
      'data-cause',
      'needs-setup',
    );
    expect(screen.queryByRole('radio')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('disables Send when the chosen node goes offline', async () => {
    setup([ben, cara]);
    await userEvent.click(await screen.findByRole('radio', { name: /Ben Ortiz/ }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }, cara] }));
    expect(screen.queryByRole('radio', { name: /Ben Ortiz/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByRole('alert')).toHaveTextContent('That node went offline. Choose another one.');
  });

  it('does not pick another node by itself when the chosen one goes offline', async () => {
    setup([ben, cara]);
    await userEvent.click(await screen.findByRole('radio', { name: /Ben Ortiz/ }));
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }, cara] }));
    // Only Cara is left, and she is still not chosen.
    expect(screen.getByRole('radio', { name: /Cara Lund/ })).not.toBeChecked();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(mocks.api.sendNetworkCollection).not.toHaveBeenCalled();
  });

  it('switches to the empty state when the last node goes offline', async () => {
    setup([ben]);
    await screen.findByRole('radio', { name: /Ben Ortiz/ });
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }] }));
    expect(screen.getByText('No paired node is online')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('starts clean when it opens again', async () => {
    const { view, onClose } = setup();
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Include environments' }));
    view.rerender(<SendCollectionDialog open={false} collectionName='Orders' onClose={onClose} />);
    view.rerender(<SendCollectionDialog open collectionName='Orders' onClose={onClose} />);
    expect(await screen.findByRole('checkbox', { name: 'Include environments' })).not.toBeChecked();
  });

  it('does not show the notes of an earlier send when it opens again', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(started(['Flows: skipped']));
    const { view, onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Flows: skipped')).toBeInTheDocument();
    view.rerender(<SendCollectionDialog open={false} collectionName='Orders' onClose={onClose} />);
    view.rerender(<SendCollectionDialog open collectionName='Orders' onClose={onClose} />);
    expect(screen.queryByText('Flows: skipped')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Send' })).toBeInTheDocument();
  });

  it('does not switch to another node when the auto-chosen one leaves and another arrives', async () => {
    setup([ben]);
    expect(await screen.findByRole('radio', { name: /Ben Ortiz/ })).toBeChecked();
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }, cara] }));
    expect(screen.getByRole('radio', { name: /Cara Lund/ })).not.toBeChecked();
    expect(screen.getByRole('alert')).toHaveTextContent('That node went offline. Choose another one.');
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(mocks.api.sendNetworkCollection).not.toHaveBeenCalled();
  });

  it('calls the backend once on a double click', async () => {
    const pending = createDeferred<ReturnType<typeof started>>();
    mocks.api.sendNetworkCollection.mockReturnValue(pending.promise);
    setup();
    const button = await screen.findByRole('button', { name: 'Send' });
    await act(async () => {
      button.click();
      button.click();
    });
    expect(mocks.api.sendNetworkCollection).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve(started());
    });
  });

  it('lists two identical notes as two items', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(started(['Flows: skipped', 'Flows: skipped']));
    // A duplicate React key only logs an error, so the test watches for it.
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findAllByText('Flows: skipped')).toHaveLength(2);
    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });
});
