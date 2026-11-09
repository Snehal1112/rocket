import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SyncReviewDialog } from '@/components/network/SyncReviewDialog';
import { handleNetworkSync } from '@/lib/network-event-bridge';
import type { ConflictView } from '@/lib/tauri-api';
import { type SyncReviewEntry, useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeSyncReview, makeSyncSummary } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn(), dismiss: vi.fn() },
  api: {
    reviewNetworkSync: vi.fn(),
    confirmNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    getNetworkSyncConflictFiles: vi.fn(),
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: { diffState: { filePath: string; oldContent: string; newContent: string } }) => (
    <div data-testid='diff' data-path={props.diffState.filePath}>
      <pre data-testid='diff-old'>{props.diffState.oldContent}</pre>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const store = () => useNetworkStore.getState();

const both: ConflictView = { path: 'a.yml', kind: 'both_changed', mineHash: 'h1', theirsHash: 'h2' };
const gone: ConflictView = {
  path: 'b.yml',
  kind: 'delete_vs_edit',
  mineHash: null,
  theirsHash: 'h3',
};

function openEntry(patch: Partial<SyncReviewEntry> = {}) {
  const entry = makeSyncReview(patch);
  store().addSyncReview(entry);
  useNetworkStore.setState({ dialogSyncId: entry.syncId });
  return render(<SyncReviewDialog />);
}

const group = (path: string) => screen.getByRole('radiogroup', { name: `Resolution for ${path}` });
const choose = (path: string, label: string) =>
  userEvent.click(within(group(path)).getByRole('radio', { name: label }));

describe('SyncReviewDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.reviewNetworkSync.mockResolvedValue(undefined);
    mocks.api.confirmNetworkSync.mockResolvedValue(undefined);
    mocks.api.cancelNetworkSync.mockResolvedValue(undefined);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'mine text', theirs: 'their text' });
  });

  it('renders nothing without an open sync', () => {
    const { container } = render(<SyncReviewDialog />);
    expect(container).toBeEmptyDOMElement();
  });

  describe('review', () => {
    it('shows who, what and the counts on both sides', () => {
      openEntry({
        summary: makeSyncSummary({
          addedHere: 1,
          changedHere: 2,
          deletedHere: 0,
          sentAdded: 0,
          sentChanged: 3,
          sentDeleted: 1,
        }),
      });
      expect(
        screen.getByRole('dialog', { name: 'Ben Ortiz wants to sync a collection' }),
      ).toBeInTheDocument();
      expect(screen.getByText('Orders')).toBeInTheDocument();
      expect(screen.getByText('Device ID AAAA-1111')).toBeInTheDocument();
      expect(screen.getByText('Coming to you: 1 added, 2 changed')).toBeInTheDocument();
      expect(screen.getByText('Going to Ben Ortiz: 3 changed, 1 deleted')).toBeInTheDocument();
    });

    it('shows a collection name with a double quote as given', () => {
      openEntry({ collectionName: 'Orders "v2"' });
      expect(screen.getByText('Orders "v2"')).toBeInTheDocument();
    });

    it('names a node without a display name', () => {
      openEntry({ displayName: '  ' });
      expect(
        screen.getByRole('dialog', { name: 'Unnamed node wants to sync a collection' }),
      ).toBeInTheDocument();
    });

    it('marks the changed scripts and auth files by path', () => {
      openEntry({
        summary: makeSyncSummary({
          scriptsChanged: ['Auth/login.yml'],
          authChanged: ['Auth/login.yml', 'Billing/pay.yml'],
        }),
      });
      expect(screen.getByText('Scripts changed (1)')).toBeInTheDocument();
      expect(screen.getByText('Auth changed (2)')).toBeInTheDocument();
      expect(screen.getAllByText('Auth/login.yml')).toHaveLength(2);
      expect(screen.getByText('Billing/pay.yml')).toBeInTheDocument();
    });

    it('shows no mark section when nothing changed in scripts or auth', () => {
      openEntry();
      expect(screen.queryByText(/Scripts changed/)).toBeNull();
      expect(screen.queryByText(/Auth changed/)).toBeNull();
    });

    it('says changed scripts keep the current script access', () => {
      openEntry();
      expect(
        screen.getByText(
          "Changed scripts run with this collection's current script access. A sync never changes trust settings.",
        ),
      ).toBeInTheDocument();
    });

    it('accepts at once when there is no conflict and sends no decision', async () => {
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() => expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, []));
    });

    it('keeps Accept off until every conflict has a choice', async () => {
      openEntry({ conflicts: [both, gone] });
      const accept = screen.getByRole('button', { name: 'Accept' });
      expect(accept).toBeDisabled();
      await choose('a.yml', 'Keep mine');
      expect(accept).toBeDisabled();
      await choose('b.yml', 'Keep theirs');
      expect(accept).toBeEnabled();
    });

    it('starts every conflict with no choice', () => {
      openEntry({ conflicts: [both] });
      for (const radio of within(group('a.yml')).getAllByRole('radio')) {
        expect(radio).not.toBeChecked();
      }
    });

    it('sends one decision per conflict with the wire resolution', async () => {
      openEntry({ conflicts: [both, gone] });
      await choose('a.yml', 'Keep both');
      await choose('b.yml', 'Keep theirs');
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, [
          { path: 'a.yml', resolution: 'keep_both' },
          { path: 'b.yml', resolution: 'keep_theirs' },
        ]),
      );
    });

    it('lets the user change a choice', async () => {
      openEntry({ conflicts: [both] });
      await choose('a.yml', 'Keep mine');
      await choose('a.yml', 'Keep theirs');
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, [
          { path: 'a.yml', resolution: 'keep_theirs' },
        ]),
      );
    });

    it('hides keep both when one side has no file', () => {
      openEntry({ conflicts: [gone] });
      expect(within(group('b.yml')).queryByRole('radio', { name: 'Keep both' })).toBeNull();
      expect(within(group('b.yml')).getAllByRole('radio')).toHaveLength(2);
    });

    it('hides keep both for collection and folder settings', () => {
      openEntry({
        conflicts: [
          { ...both, path: 'opencollection.yml' },
          { ...both, path: 'Auth/folder.yml' },
        ],
      });
      expect(within(group('opencollection.yml')).queryByRole('radio', { name: 'Keep both' })).toBeNull();
      expect(within(group('Auth/folder.yml')).queryByRole('radio', { name: 'Keep both' })).toBeNull();
    });

    it('lists each conflict with its kind', () => {
      openEntry({ conflicts: [both, gone] });
      expect(screen.getByText('Conflicts (2)')).toBeInTheDocument();
      expect(screen.getByText('Changed on both nodes')).toBeInTheDocument();
      expect(screen.getByText('Deleted on one node, changed on the other')).toBeInTheDocument();
    });

    it('opens the diff for one conflict and closes it again', async () => {
      openEntry({ conflicts: [both] });
      expect(screen.queryByTestId('diff')).toBeNull();
      await userEvent.click(screen.getByRole('button', { name: 'Compare a.yml' }));
      expect(await screen.findByTestId('diff-old')).toHaveTextContent('mine text');
      expect(screen.getByTestId('diff-new')).toHaveTextContent('their text');
      await userEvent.click(screen.getByRole('button', { name: 'Hide a.yml' }));
      expect(screen.queryByTestId('diff')).toBeNull();
    });

    it('still lets the user choose when the files cannot be loaded', async () => {
      mocks.api.getNetworkSyncConflictFiles.mockRejectedValue('unavailable');
      openEntry({ conflicts: [both] });
      await userEvent.click(screen.getByRole('button', { name: 'Compare a.yml' }));
      expect(
        await screen.findByText('The files could not be loaded. You can still choose.'),
      ).toBeInTheDocument();
      await choose('a.yml', 'Keep mine');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    });

    it('declines with no decisions, removes the entry and closes', async () => {
      openEntry({ conflicts: [both] });
      await choose('a.yml', 'Keep mine');
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      await waitFor(() => expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', false, []));
      await waitFor(() => expect(store().dialogSyncId).toBeNull());
      expect(store().syncReviews).toHaveLength(0);
      expect(store().wasSyncDeclinedHere('s1')).toBe(true);
    });

    it('closes without an answer and leaves the review pending', async () => {
      openEntry();
      await userEvent.keyboard('{Escape}');
      expect(store().dialogSyncId).toBeNull();
      expect(store().syncReviews).toHaveLength(1);
      expect(mocks.api.reviewNetworkSync).not.toHaveBeenCalled();
    });

    it('shows the error in a state block and keeps the buttons when the answer fails', async () => {
      mocks.api.reviewNetworkSync.mockRejectedValue('The sync is no longer running.');
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      const block = await screen.findByText('The sync is no longer running.');
      expect(block.closest('[data-slot="state-block"]')).toHaveAttribute('data-tone', 'error');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
    });
  });

  describe('after Accept', () => {
    async function accepted(patch: Partial<SyncReviewEntry> = {}) {
      openEntry(patch);
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await screen.findByText('Syncing with Ben Ortiz...');
    }

    it('shows that the sync is running, with Done and no Cancel', async () => {
      await accepted();
      expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();
      expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    });

    it('shows the progress of this sync', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 's1', doneBytes: 50, totalBytes: 100 }));
      expect(screen.getByText('50 bytes of 100 bytes')).toBeInTheDocument();
    });

    it('Done closes the dialog and the sync goes on', async () => {
      await accepted();
      await userEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(store().dialogSyncId).toBeNull();
      expect(store().syncReviews).toHaveLength(1);
    });

    it('does not show the progress of another sync', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 'other', doneBytes: 100, totalBytes: 100 }));
      expect(screen.queryByText('100 bytes of 100 bytes')).toBeNull();
    });
  });

  describe('late events', () => {
    it('keeps the dialog of one sync when another sync finishes', () => {
      store().addSyncReview(makeSyncReview({ syncId: 's2', displayName: 'Cara Lund' }));
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's2',
          link_id: 'L2',
          display_name: 'Cara Lund',
          collection_name: 'Other',
          outcome: 'applied',
          message: null,
        }),
      );
      expect(
        screen.getByRole('dialog', { name: 'Ben Ortiz wants to sync a collection' }),
      ).toBeInTheDocument();
    });

    it('a late Accept answer does not bring a finished sync back', async () => {
      const answer = createDeferred<undefined>();
      mocks.api.reviewNetworkSync.mockReturnValue(answer.promise);
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'cancelled',
          message: null,
        }),
      );
      await act(async () => answer.resolve(undefined));
      expect(store().syncReviews).toHaveLength(0);
      expect(store().dialogSyncId).toBeNull();
    });

    it('a late Decline answer does not close the dialog of another sync', async () => {
      const answer = createDeferred<undefined>();
      mocks.api.reviewNetworkSync.mockReturnValue(answer.promise);
      store().addSyncReview(makeSyncReview({ syncId: 's2' }));
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      act(() => store().openSyncDialog('s2'));
      await act(async () => answer.resolve(undefined));
      expect(store().dialogSyncId).toBe('s2');
    });

    it('shows why an unanswered review ended', () => {
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'failed',
          message: 'The other node went offline.',
        }),
      );
      expect(screen.getByText('The other node went offline.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
      expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
    });

    it('switches to the expired view when the review expires', () => {
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'expired',
          message: null,
        }),
      );
      expect(screen.getByText('The request expired.')).toBeInTheDocument();
    });
  });

  describe('confirm', () => {
    const confirm = (patch: Partial<SyncReviewEntry> = {}) =>
      openEntry({
        kind: 'confirm',
        summary: makeSyncSummary({ addedHere: 1, changedHere: 1, deletedHere: 1 }),
        ...patch,
      });

    it('asks to apply the changes of the other node', () => {
      confirm();
      expect(screen.getByRole('dialog', { name: 'Apply 3 changes from Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByText('Coming to you: 1 added, 1 changed, 1 deleted')).toBeInTheDocument();
    });

    it('says one change in the singular', () => {
      confirm({ summary: makeSyncSummary({ addedHere: 1, changedHere: 0, deletedHere: 0 }) });
      expect(screen.getByRole('dialog', { name: 'Apply 1 change from Ben Ortiz' })).toBeInTheDocument();
    });

    it('shows the same marks and the sandbox warning', () => {
      confirm({ summary: makeSyncSummary({ scriptsChanged: ['s.yml'], authChanged: ['t.yml'] }) });
      expect(screen.getByText('Scripts changed (1)')).toBeInTheDocument();
      expect(screen.getByText('Auth changed (1)')).toBeInTheDocument();
      expect(screen.getByText(/Changed scripts run with this collection's current script access/)).toBeInTheDocument();
    });

    it('shows no conflict choices in the confirm dialog', () => {
      confirm({ conflicts: [both] });
      expect(screen.queryByText(/Conflicts/)).toBeNull();
      expect(screen.queryByRole('radio')).toBeNull();
    });

    it('applies', async () => {
      confirm();
      await userEvent.click(screen.getByRole('button', { name: 'Apply' }));
      await waitFor(() => expect(mocks.api.confirmNetworkSync).toHaveBeenCalledWith('s1', true));
      await screen.findByText('Syncing with Ben Ortiz...');
    });

    it('declines', async () => {
      confirm();
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      await waitFor(() => expect(mocks.api.confirmNetworkSync).toHaveBeenCalledWith('s1', false));
      await waitFor(() => expect(store().dialogSyncId).toBeNull());
    });
  });
});
