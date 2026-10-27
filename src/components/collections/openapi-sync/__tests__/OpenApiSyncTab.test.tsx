import { act, configure, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearApplyState } from '@/lib/queries/openapi-sync-queries';
import { useApplySync } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncNotifier } from '@/components/layout/OpenApiSyncNotifier';
import { OpenApiSyncTab } from '../OpenApiSyncTab';

/** The app mounts the global listeners once, so the tests do the same. */
function TabWithListeners({ collection }: { collection: string }) {
  return (
    <>
      <OpenApiSyncNotifier />
      <OpenApiSyncTab collection={collection} />
    </>
  );
}
import type { OpenApiApplyReport, OpenApiSyncItem } from '@/lib/tauri-api';
import { changedItem, makePlan, makeStatus, newItem, removedItem } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn(),
    getOpenApiSyncOverview: vi.fn(),
    getOpenApiSyncChanges: vi.fn(),
    getOpenApiSyncSpecView: vi.fn(),
    onOpenApiSyncChecked: vi.fn(() => Promise.resolve(() => undefined)),
    onOpenApiSyncApplied: vi.fn(() => Promise.resolve(() => undefined)),
    applyOpenApiSync: vi.fn(),
    disconnectOpenApiSync: vi.fn(),
  };
});

// Module-level apply state must not leak from one test to the next, even after a timeout.
beforeEach(() => clearApplyState());
afterEach(() => clearApplyState());

// A busy machine can make a render-heavy test slow, so the limit is generous.
vi.setConfig({ testTimeout: 30000 });
configure({ asyncUtilTimeout: 10000 });

describe('OpenApiSyncTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getOpenApiSyncChanges).mockResolvedValue({
      baselineTrusted: true,
      modified: [],
      added: [],
      deleted: [],
      backups: [],
      resetAvailable: true,
    });
    vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
      endpoints: 42,
      inSync: 36,
      changedByYou: 3,
      updatesWaiting: 0,
      specVersion: null,
      lastSyncedAt: null,
      lastSyncedUpdated: 0,
    });
  });

  it('shows a loading state first', () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockReturnValue(new Promise(() => undefined));
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(screen.getByText('Loading OpenAPI sync...')).toBeInTheDocument();
  });

  it('defaults_to_overview_otherwise', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('Spec updates waiting')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Overview' })).toHaveAttribute('data-state', 'active');
    expect(screen.getByRole('tab', { name: 'Spec updates', hidden: true })).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).not.toHaveBeenCalled();
  });

  it('view_spec_opens_the_viewer_and_back_returns_to_the_same_tab', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    vi.mocked(tauriApi.getOpenApiSyncSpecView).mockResolvedValue({
      title: 'Pets API',
      version: '1',
      fetchedAt: null,
      state: 'in_sync',
      groups: [
        {
          tag: 'pets',
          operations: [
            {
              key: 'listPets',
              method: 'GET',
              path: '/pets',
              summary: null,
              parameters: [],
              responses: [],
              isNew: false,
            },
          ],
        },
      ],
    });
    renderWithQuery(<TabWithListeners collection='c' />);
    const user = userEvent.setup();
    await screen.findByText('Spec updates waiting');
    await user.click(screen.getByRole('tab', { name: 'Your changes' }));
    await user.click(screen.getByRole('button', { name: 'View spec' }));
    expect(await screen.findByText('pets (1)')).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Overview' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(await screen.findByRole('tab', { name: 'Your changes' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('trusts the plan when it says nothing is pending', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan({ pending: false }));
    renderWithQuery(<TabWithListeners collection='c' />);
    await screen.findByText('Spec updates waiting');
    await userEvent.setup().click(screen.getByRole('tab', { name: 'Spec updates' }));
    expect(await screen.findByText('No spec updates waiting')).toBeInTheDocument();
    expect(screen.queryByText('Add pet')).toBeNull();
  });

  it('keeps_the_notes_reachable_after_everything_is_skipped', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan)
      .mockResolvedValueOnce(makePlan())
      .mockResolvedValue(
        makePlan({ items: [], skippedHidden: 3, warnings: ['One path was ignored.'] }),
      );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({
      applied: [],
      stale: [],
      notRewritable: [],
      deleted: [],
      detached: [],
      skipped: 3,
      remaining: 0,
    });
    renderWithQuery(<TabWithListeners collection='skipnotes' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Skip all' }));
    expect(await screen.findByText('No spec updates waiting')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Spec updates', hidden: true })).toBeInTheDocument();
    expect(screen.getByText('3 skipped, hidden from this list.')).toBeInTheDocument();
    expect(screen.getByText('One path was ignored.')).toBeInTheDocument();
  });

  it('disconnects_from_the_menu_to_the_end', async () => {
    let connected = true;
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockImplementation(async () =>
      connected ? makeStatus({ hasPending: false }) : null,
    );
    vi.mocked(tauriApi.disconnectOpenApiSync).mockImplementation(async () => {
      connected = false;
    });
    renderWithQuery(<TabWithListeners collection='dis2' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'OpenAPI sync actions' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Disconnect' }));
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Disconnect' }),
    );
    expect(
      await screen.findByText('This collection is not connected to an OpenAPI spec.'),
    ).toBeInTheDocument();
  });

  it('defaults_to_spec_updates_when_updates_wait', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [changedItem, newItem] }),
    );
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Spec updates (2)' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('shows_the_consent_banner_when_not_granted', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ consent: 'missing', hasPending: false }),
    );
    renderWithQuery(<TabWithListeners collection='cb1' />);
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
  });

  it('inner_tabs_switch', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<TabWithListeners collection='c' />);
    await screen.findByText('Folder: pets / read');
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: 'Your changes' }));
    expect(screen.queryByText('Folder: pets / read')).toBeNull();
    await user.click(screen.getByRole('tab', { name: 'Spec updates (3)' }));
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
  });

  it('changes_shows_the_empty_state', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasPending: false }));
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('Spec updates waiting')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('tab', { name: 'Your changes' }));
    expect(await screen.findByText('You have not changed anything.')).toBeInTheDocument();
  });

  it('shows_the_change_count_on_the_tab_label_only_when_above_zero', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasPending: false }));
    const r = (uid: string) => ({
      key: null,
      uid,
      name: uid,
      method: 'GET',
      url: '/x',
      requestPath: null,
      fields: [],
    });
    vi.mocked(tauriApi.getOpenApiSyncChanges).mockResolvedValue({
      baselineTrusted: true,
      modified: [r('a')],
      added: [r('b')],
      deleted: [r('c')],
      backups: [],
      resetAvailable: true,
    });
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByRole('tab', { name: 'Your changes (3)' })).toBeInTheDocument();
  });

  it('the_tab_mounts_the_overview_and_review_switches_to_spec_updates', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [changedItem, newItem] }),
    );
    vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
      endpoints: 42,
      inSync: 36,
      changedByYou: 3,
      updatesWaiting: 2,
      specVersion: null,
      lastSyncedAt: null,
      lastSyncedUpdated: 0,
    });
    const user = userEvent.setup();
    renderWithQuery(<TabWithListeners collection='c' />);
    await user.click(await screen.findByRole('tab', { name: 'Overview' }));
    await user.click(await screen.findByRole('button', { name: 'Review updates' }));
    expect(screen.getByRole('tab', { name: 'Spec updates (2)' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('waits_for_the_plan_before_choosing_the_default_tab', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    let resolvePlan: (p: ReturnType<typeof makePlan>) => void = () => undefined;
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(
      new Promise((r) => {
        resolvePlan = r;
      }),
    );
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('Loading the plan...')).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Overview', selected: true })).toBeNull();
    expect(screen.queryByText('Spec updates waiting')).toBeNull();
    act(() => resolvePlan(makePlan({ items: [changedItem, newItem] })));
    expect(await screen.findByRole('tab', { name: 'Spec updates (2)' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('a_chosen_tab_survives_the_plan_arriving', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    renderWithQuery(<TabWithListeners collection='c' />);
    await userEvent.setup().click(await screen.findByRole('tab', { name: 'Your changes' }));
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [changedItem, newItem] }),
    );
    const handler = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    act(() => handler?.({ collection: 'c' } as never));
    await screen.findByRole('tab', { name: 'Spec updates (2)' });
    expect(screen.getByRole('tab', { name: 'Your changes' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('a_later_pending_copy_keeps_the_overview_and_its_dialog_mounted', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    const user = userEvent.setup();
    renderWithQuery(<TabWithListeners collection='c' />);
    await user.click(await screen.findByRole('button', { name: 'Edit connection settings' }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(new Promise(() => undefined));
    // A check finds a spec: the status query is refetched.
    const handler = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    expect(handler).toBeDefined();
    act(() => handler?.({ collection: 'c' } as never));
    await waitFor(() =>
      expect(vi.mocked(tauriApi.getOpenApiSyncPlan).mock.calls.length).toBeGreaterThan(0),
    );
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Overview', hidden: true })).toHaveAttribute('data-state', 'active');
    expect(screen.queryByText('Loading the plan...')).toBeNull();
  });

  it('after_disconnect_the_tab_shows_that_nothing_is_connected', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    renderWithQuery(<TabWithListeners collection='nc1' />);
    expect(
      await screen.findByText('This collection is not connected to an OpenAPI spec.'),
    ).toBeInTheDocument();
  });

  it('shows the list with the first item selected and switches the detail', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Delete pet'));
    expect(screen.getByText('Treated as edited, will be kept.')).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('c', []);
  });

  it('shows_a_fixed_sentence_for_an_unreadable_state', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue(
      'bad yaml in /home/me/proj/.rocket/openapi-source.yml at line 3',
    );
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(
      await screen.findByText('Something went wrong with the OpenAPI sync. Try again.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/state file cannot be read/)).toBeNull();
    expect(screen.queryByText(/home\/me/)).toBeNull();
  });

  it('prefixes a known status error with the state file sentence', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('consent_required: x');
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText(/The sync state file cannot be read: /)).toBeInTheDocument();
  });

  it('shows a fixed sentence for a plan error, never the path', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockRejectedValue('cannot read /home/me/spec.json');
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(
      await screen.findByText(
        'The plan cannot be built: Something went wrong with the OpenAPI sync. Try again.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/home\/me/)).toBeNull();
  });

  it('asks for a new check when the pending spec changed', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockRejectedValue('Conflict: the pending spec changed, check again');
    renderWithQuery(<TabWithListeners collection='c' />);
    expect(await screen.findByText('The pending spec changed, check again')).toBeInTheDocument();
  });

  it('no longer has the folded Settings section', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<TabWithListeners collection='c' />);
    await screen.findByText('Folder: pets / read');
    expect(screen.queryByRole('button', { name: /^Settings/ })).toBeNull();
  });

  it('goes away after a disconnect', async () => {
    let connected = true;
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockImplementation(async () =>
      connected ? makeStatus() : null,
    );
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    vi.mocked(tauriApi.disconnectOpenApiSync).mockImplementation(async () => {
      connected = false;
    });
    renderWithQuery(<TabWithListeners collection='dis1' />);
    await screen.findByText('Folder: pets / read');
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'OpenAPI sync actions' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Edit connection settings' }));
    await user.click(await screen.findByRole('button', { name: 'Disconnect' }));
    const confirm = await screen.findByRole('alertdialog');
    await user.click(within(confirm).getByRole('button', { name: 'Disconnect' }));
    expect(
      await screen.findByText('This collection is not connected to an OpenAPI spec.'),
    ).toBeInTheDocument();
    expect(tauriApi.disconnectOpenApiSync).toHaveBeenCalledTimes(1);
  });

  it('offers a Check now button next to the plan', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<TabWithListeners collection='c' />);
    await screen.findByText('Folder: pets / read');
    expect(screen.getByRole('button', { name: 'Check now' })).toBeEnabled();
    expect(screen.queryByText('Trust this spec link')).toBeNull();
    expect(screen.queryByText(/may not have been checked/)).toBeNull();
  });

  it('asks for approval first, keeps Check now off and leaves apply and skip alone', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ consent: 'missing', interval: 'off' }),
    );
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<TabWithListeners collection='cons1' />);
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
    await screen.findByText('Folder: pets / read');
    expect(
      screen.getByText(
        'This pending spec came with the collection files. It may not have been checked on this machine.',
      ),
    ).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByRole('button', { name: /^Confirm and sync/ })).toBeEnabled();
  });

  it('explains that the link must be approved when there is no plan', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({
        consent: 'locationChanged',
        hasPending: false,
        lastCheckedAt: null,
        lastStatus: null,
      }),
    );
    renderWithQuery(<TabWithListeners collection='cons2' />);
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
  });
});

const report: OpenApiApplyReport = {
  applied: [{ key: 'POST /pets', kind: 'updated', requestPath: 'pets/add-pet.yml' }],
  stale: [],
  notRewritable: [],
  deleted: [],
  detached: [],
  skipped: 0,
  remaining: 0,
};

function mockSync(plan = makePlan()) {
  vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
  vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(plan);
}

describe('OpenApiSyncTab actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
  });

  it('puts a checkbox on every row and counts the selection', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='sel1' />);
    await screen.findByText('Folder: pets / read');
    expect(screen.getAllByRole('checkbox')).toHaveLength(3);
    // The new request starts selected, the one with a conflict does not.
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument();
  });

  it('links a rename, re-plans with the link and shows one Changed item', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation(async (_c, links = []) =>
      links.length > 0
        ? makePlan({ items: [{ ...changedItem, key: 'GET /pets', name: 'List pets' } as OpenApiSyncItem] })
        : makePlan({ items: [{ ...newItem }, ({ ...removedItem, key: 'GET /animals', possiblyRenamedTo: undefined } as OpenApiSyncItem)] }),
    );
    renderWithQuery(<TabWithListeners collection='link1' />);
    await screen.findByText('New (1)');
    expect(screen.getByText('Removed (1)')).toBeInTheDocument();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Link as rename' }));
    expect(await screen.findByText('Changed (1)')).toBeInTheDocument();
    expect(screen.getByText('New (0)')).toBeInTheDocument();
    expect(screen.getByText('Removed (0)')).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenLastCalledWith('link1', [
      ['GET /animals', 'GET /pets'],
    ]);
  });

  it('skip_all_asks_first_and_then_sends_skips_with_fingerprints', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan)
      .mockResolvedValueOnce(makePlan())
      .mockResolvedValue(makePlan({ items: [newItem, removedItem] }));
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({ ...report, applied: [], skipped: 1 });
    renderWithQuery(<TabWithListeners collection='skip1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    // Nothing is sent before the user confirms.
    expect(tauriApi.applyOpenApiSync).not.toHaveBeenCalled();
    const dialog = screen.getByRole('alertdialog');
    expect(within(dialog).getByText(/This skips 3 requests/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Skip all' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledWith('skip1', {
      items: [],
      skips: [
        { key: 'DELETE /pets', fingerprint: 'f3' },
        { key: 'POST /pets', fingerprint: 'f2' },
        { key: 'GET /pets', fingerprint: 'f1' },
      ],
      variables: [],
      renameLinks: [],
    });
    expect(await screen.findByText('1 skipped.')).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText('Add pet')).toBeNull());
  });

  it('confirms with counts, applies once on a double click and shows the result', async () => {
    mockSync(
      makePlan({
        items: [newItem, changedItem, ({ ...removedItem, edited: false } as OpenApiSyncItem)],
      }),
    );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='apply1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    // The new request is selected already. The changed and the removed one are added by hand.
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('checkbox', { name: 'Include Delete pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Confirm and sync (3)' }));
    expect(screen.getByText('1 new request will be added.')).toBeInTheDocument();
    expect(
      screen.getByText('1 removed request will be deleted if Rocket can rewrite it, because you did not edit it.'),
    ).toBeInTheDocument();
    await user.dblClick(screen.getByRole('button', { name: 'Apply now' }));
    expect(await screen.findByText('1 applied: 0 created, 1 updated.')).toBeInTheDocument();
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1);
    const selection = vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1];
    expect(selection?.items.map((i) => i.key).sort()).toEqual(
      ['DELETE /pets', 'GET /pets', 'POST /pets'].sort(),
    );
    expect(selection?.items.find((i) => i.key === 'POST /pets')?.digest).toBe('d2');
  });

  it('sends the field choices the user made', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='choice1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByText('Add pet'));
    await user.click(
      within(screen.getByRole('radiogroup', { name: 'URL' })).getByRole('radio', {
        name: 'Take spec',
      }),
    );
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalled());
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].items).toContainEqual({
      key: 'POST /pets',
      digest: 'd2',
      choices: { url: 'spec' },
    });
  });

  it('searches every group, rows past the cap included', async () => {
    const items = Array.from({ length: 600 }, (_, i) => ({
      ...changedItem,
      key: `GET /p${i}`,
      name: `Request ${i}`,
    })) as OpenApiSyncItem[];
    mockSync(makePlan({ items }));
    renderWithQuery(<TabWithListeners collection='search1' />);
    await screen.findByText('Changed (600)');
    expect(screen.getByText('600 updates from the spec · 0 selected')).toBeInTheDocument();
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search changes' }), {
      target: { value: 'Request 5' },
    });
    // Request 5, 50-59 and 500-599 match, the last ones past the 500 row cap.
    expect(await screen.findByText('Changed (111)')).toBeInTheDocument();
    // The summary counts the whole plan, not the filtered rows.
    expect(screen.getByText('600 updates from the spec · 0 selected')).toBeInTheDocument();
  });

  it('shows the sentence for an apply error', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue(
      'Conflict: an earlier apply is not finished yet at /home/me/x',
    );
    renderWithQuery(<TabWithListeners collection='err1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    expect(
      await screen.findByText('Another apply is not finished yet. Wait a moment and try again.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/\/home\/me/)).toBeNull();
  });

  it('Check again refetches the status and the plan', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='again1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await screen.findByText('1 applied: 0 created, 1 updated.');
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledTimes(2));
    const statusCalls = vi.mocked(tauriApi.getOpenApiSyncStatus).mock.calls.length;
    const planCalls = vi.mocked(tauriApi.getOpenApiSyncPlan).mock.calls.length;
    await user.click(screen.getByRole('button', { name: 'Check again' }));
    await waitFor(() =>
      expect(vi.mocked(tauriApi.getOpenApiSyncStatus).mock.calls.length).toBeGreaterThan(statusCalls),
    );
    expect(vi.mocked(tauriApi.getOpenApiSyncPlan).mock.calls.length).toBeGreaterThan(planCalls);
  });

  it('sends renameLinks once, then not again, and a skip never sends them', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    const linked = makePlan({
      items: [{ ...changedItem, key: 'GET /pets', name: 'List pets' } as OpenApiSyncItem],
    });
    const unlinked = makePlan({
      items: [
        { ...newItem },
        { ...removedItem, key: 'GET /animals', possiblyRenamedTo: undefined } as OpenApiSyncItem,
        { ...changedItem },
      ],
    });
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation(async (_c, links = []) =>
      links.length > 0 ? linked : unlinked,
    );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='links2' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Link as rename' }));
    await screen.findByText('Changed (1)');
    // A skip with a link in place carries no links.
    await user.click(await screen.findByRole('button', { name: 'Skip all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Skip all' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].renameLinks).toEqual([]);
    await user.click((await screen.findAllByRole('button', { name: 'Close' }))[0] as HTMLElement);
    // An apply sends the link, and the next selection does not.
    await user.click(await screen.findByRole('checkbox', { name: 'Include List pets in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(2));
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[1]?.[1].renameLinks).toEqual([
      ['GET /animals', 'GET /pets'],
    ]);
    await user.click((await screen.findAllByRole('button', { name: 'Close' }))[0] as HTMLElement);
    await waitFor(() =>
      expect(tauriApi.getOpenApiSyncPlan).toHaveBeenLastCalledWith('links2', []),
    );
  });

  it('keeps the list on screen while a linked plan loads', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation((_c, links = []) =>
      links.length > 0 ? new Promise(() => undefined) : Promise.resolve(makePlan()),
    );
    renderWithQuery(<TabWithListeners collection='keep1' />);
    const link = await screen.findByRole('button', { name: 'Link as rename' });
    await userEvent.setup().click(link);
    expect(screen.queryByText('Loading the plan...')).toBeNull();
    expect(screen.getByRole('checkbox', { name: 'Include List pets in apply' })).toBeInTheDocument();
    // Apply waits for the real plan.
    expect(screen.getByRole('button', { name: /^Confirm and sync/ })).toBeDisabled();
  });

  it('does not mark rows as skipped when another apply is already running', async () => {
    mockSync();
    let finish: (v: OpenApiApplyReport) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise<OpenApiApplyReport>((resolve) => {
        finish = resolve;
      }),
    );
    renderWithQuery(
      <>
        <TabWithListeners collection='busy1' />
        <TabWithListeners collection='busy1' />
      </>,
    );
    const user = userEvent.setup();
    await screen.findAllByText('Add pet');
    for (const box of screen.getAllByRole('checkbox', { name: 'Include Add pet in apply' })) {
      await user.click(box);
    }
    await user.click(screen.getAllByRole('button', { name: /^Confirm and sync/ })[0] as HTMLElement);
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    // The other tab sees the apply and can neither apply nor skip.
    await waitFor(() => expect(screen.getAllByRole('button', { name: /^Confirm and sync/ })[1]).toBeDisabled());
    expect(screen.getAllByRole('button', { name: 'Skip all' })[1]).toBeDisabled();
    await act(async () => finish(report));
    // Both tabs show the shared report, so Escape closes it for both.
    await screen.findAllByText('Apply finished');
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByText('Apply finished')).toBeNull());
    await waitFor(() => expect(screen.getAllByRole('button', { name: /^Confirm and sync/ })[1]).toBeEnabled());
    expect(screen.queryByText('Skipped')).toBeNull();
  });

  it('closes the confirm dialog when the plan changes under it', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='conf1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    expect(screen.getByRole('button', { name: 'Apply now' })).toBeInTheDocument();
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [{ ...changedItem, digest: 'zz' } as OpenApiSyncItem] }),
    );
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'conf1' } as never));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Apply now' })).toBeNull());
    expect(screen.getByText(/· 0 selected$/)).toBeInTheDocument();
    // Selecting again must not pop the dialog open by itself.
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Apply now' })).toBeNull();
  });

  it('keeps the rename links through a background refresh of the plan', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    const linked = makePlan({
      items: [{ ...changedItem, key: 'GET /pets', name: 'List pets' } as OpenApiSyncItem],
    });
    const unlinked = makePlan({
      items: [
        { ...newItem },
        { ...removedItem, key: 'GET /animals', possiblyRenamedTo: undefined } as OpenApiSyncItem,
      ],
    });
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation(async (_c, links = []) =>
      links.length > 0 ? { ...linked, warnings: [`w${Math.random()}`] } : unlinked,
    );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='links3' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Link as rename' }));
    await screen.findByText('Changed (1)');
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'links3' } as never));
    await user.click(await screen.findByRole('checkbox', { name: 'Include List pets in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalled());
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].renameLinks).toEqual([
      ['GET /animals', 'GET /pets'],
    ]);
  });

  it('disables Apply while the plan refetches', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='fetch1' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByRole('button', { name: /^Confirm and sync/ })).toBeEnabled();
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(new Promise(() => undefined));
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'fetch1' } as never));
    await waitFor(() => expect(screen.getByRole('button', { name: /^Confirm and sync/ })).toBeDisabled());
  });

  it('shows the report again when the tab is opened after leaving mid-apply', async () => {
    mockSync();
    let finish: (v: OpenApiApplyReport) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise<OpenApiApplyReport>((resolve) => {
        finish = resolve;
      }),
    );
    const first = renderWithQuery(<TabWithListeners collection='away1' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    first.unmount();
    await act(async () => finish(report));
    renderWithQuery(<TabWithListeners collection='away1' />);
    expect(await screen.findByText('1 applied: 0 created, 1 updated.')).toBeInTheDocument();
    await user.click((await screen.findAllByRole('button', { name: 'Close' }))[0] as HTMLElement);
    await waitFor(() => expect(screen.queryByText('Apply finished')).toBeNull());
  });

  it('shows the apply error again after leaving mid-apply', async () => {
    mockSync();
    let fail: (e: unknown) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise<OpenApiApplyReport>((_resolve, reject) => {
        fail = reject;
      }),
    );
    const first = renderWithQuery(<TabWithListeners collection='away2' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    first.unmount();
    await act(async () => fail('Conflict: an earlier apply is not finished yet'));
    renderWithQuery(<TabWithListeners collection='away2' />);
    expect(
      await screen.findByText('Another apply is not finished yet. Wait a moment and try again.'),
    ).toBeInTheDocument();
  });

  it('describes a variables-only apply without saying nothing was applied', async () => {
    mockSync(
      makePlan({
        items: [],
        variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv', spec: 'https://x' }],
      }),
    );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({ ...report, applied: [] });
    renderWithQuery(<TabWithListeners collection='vars1' />);
    const user = userEvent.setup();
    // The variable change is selected by default.
    await screen.findByRole('checkbox', { name: 'Include Base URL baseUrl in apply' });
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    expect(
      screen.getByText('1 variable change will be applied. A value you changed yourself is kept.'),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    expect(await screen.findByText(/1 variable change was checked/)).toBeInTheDocument();
    expect(screen.queryByText('Nothing was applied.')).toBeNull();
  });

  it('tells the user when Confirm meets an apply that is already running', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(new Promise(() => undefined));
    function Starter() {
      const apply = useApplySync('race1');
      return (
        <button
          type='button'
          onClick={() => apply.mutate({ items: [], skips: [], variables: [], renameLinks: [] })}
        >
          start other
        </button>
      );
    }
    renderWithQuery(
      <>
        <TabWithListeners collection='race1' />
        <Starter />
      </>,
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Confirm and sync/ }));
    fireEvent.click(screen.getByText('start other'));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    expect(await screen.findByText('Another apply is still running. Wait for it to finish.')).toBeInTheDocument();
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1);
  });

  it('confirm_and_sync_opens_the_confirm_dialog_with_counts', async () => {
    mockSync(makePlan({ items: [newItem, { ...newItem, key: 'GET /cats', name: 'List cats', digest: 'dc', fingerprint: 'fc' }] }));
    renderWithQuery(<TabWithListeners collection='bulk1' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Confirm and sync (2)' }));
    expect(screen.getByText('2 new requests will be added.')).toBeInTheDocument();
    expect(tauriApi.applyOpenApiSync).not.toHaveBeenCalled();
  });

  it('accept_all_warns_before_overwriting_edits', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='bulk2' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    const dialog = screen.getByRole('alertdialog');
    expect(within(dialog).getByText('1 of your edits will be overwritten.')).toBeInTheDocument();
    // Cancel changes nothing.
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
    // Confirm selects the changed item and takes the spec on its conflict.
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Accept all' }));
    expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Confirm and sync (2)' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    const items = vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].items ?? [];
    expect(items.find((i) => i.key === 'POST /pets')?.choices).toEqual({ url: 'spec' });
    // The removed request was never part of it.
    expect(items.map((i) => i.key)).not.toContain('DELETE /pets');
  });

  it('accept_all_without_conflicts_needs_no_warning', async () => {
    const plain = {
      ...changedItem,
      changes: [{ field: 'header:Accept', outcome: 'takeSpec', baseline: 'a', yours: 'a', spec: 'b' }],
    } as OpenApiSyncItem;
    mockSync(makePlan({ items: [plain, { ...removedItem, edited: true } as OpenApiSyncItem] }));
    renderWithQuery(<TabWithListeners collection='bulk3' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByText(/· 0 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
  });

  it('skip_all_does_nothing_when_cancelled', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='bulk4' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Cancel' }));
    expect(tauriApi.applyOpenApiSync).not.toHaveBeenCalled();
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
  });

  it('keep_current_sends_nothing', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='bulk5' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    expect(screen.getByText(/· 0 selected$/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Confirm and sync (0)' })).toBeDisabled();
    expect(tauriApi.applyOpenApiSync).not.toHaveBeenCalled();
  });

  it('keep_current_resets_take_spec_choices', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='bulk6' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Accept all' }));
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Confirm and sync (1)' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    const items = vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].items ?? [];
    expect(items.find((i) => i.key === 'POST /pets')?.choices).toEqual({ url: 'yours' });
  });

  it('accept_all_dialog_counts_new_and_changed_only', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='bulk7' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    expect(within(screen.getByRole('alertdialog')).getByText(/selects 2 requests/)).toBeInTheDocument();
  });

  it('a_variables_only_plan_is_accepted_but_cannot_be_skipped', async () => {
    mockSync(
      makePlan({
        items: [],
        variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
      }),
    );
    renderWithQuery(<TabWithListeners collection='bulk8' />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: 'Accept all' });
    expect(screen.getByRole('button', { name: 'Skip all' })).toBeDisabled();
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    expect(screen.getByText(/· 0 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
  });

  it('accept_all_on_a_mixed_plan_includes_variables', async () => {
    mockSync(
      makePlan({
        items: [newItem],
        variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
      }),
    );
    renderWithQuery(<TabWithListeners collection='bulk9' />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: 'Accept all' });
    expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument();
  });

  it('confirm_with_the_defaults_sends_the_variable_change_too', async () => {
    mockSync(
      makePlan({
        items: [newItem],
        variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
      }),
    );
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<TabWithListeners collection='bulk14' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Confirm and sync (2)' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    const sent = vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1];
    expect(sent?.variables).toEqual([{ name: 'baseUrl', digest: 'dv' }]);
    expect(sent?.items.map((i) => i.key)).toEqual(['GET /pets']);
  });

  it('skip_all_dialog_says_variable_changes_stay_waiting', async () => {
    mockSync(
      makePlan({
        items: [newItem],
        variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
      }),
    );
    renderWithQuery(<TabWithListeners collection='bulk15' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Skip all' }));
    expect(
      within(screen.getByRole('alertdialog')).getByText(/Variable changes cannot be skipped and stay waiting/),
    ).toBeInTheDocument();
  });

  it('a_late_skip_all_failure_does_not_restore_onto_a_new_plan', async () => {
    mockSync();
    let fail: (e: unknown) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise((_, reject) => {
        fail = reject;
      }),
    );
    renderWithQuery(<TabWithListeners collection='bulk16' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Skip all' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    // The plan changes while the skip is still running.
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [{ ...newItem, digest: 'zz' }, { ...newItem, key: 'GET /cats', name: 'List cats', digest: 'dc', fingerprint: 'fc' }] }),
    );
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'bulk16' } as never));
    await screen.findByText('List cats');
    await act(async () => fail('Conflict: an earlier apply is not finished yet'));
    // Only the defaults of the new plan are selected, not the old approvals.
    await waitFor(() => expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument());
    expect(screen.getByRole('checkbox', { name: 'Include List cats in apply' })).toBeChecked();
  });

  it('skip_selected_skips_only_the_selected_requests', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({ ...report, applied: [], skipped: 1 });
    renderWithQuery(<TabWithListeners collection='bulk10' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip selected (1)' }));
    await waitFor(() =>
      expect(tauriApi.applyOpenApiSync).toHaveBeenCalledWith('bulk10', {
        items: [],
        skips: [{ key: 'GET /pets', fingerprint: 'f1' }],
        variables: [],
        renameLinks: [],
      }),
    );
  });

  it('select_matches_never_selects_a_removed_item_and_keeps_other_selections', async () => {
    const cleanRemoved = { ...removedItem, edited: false } as OpenApiSyncItem;
    const editedRemoved = { ...removedItem, key: 'DELETE /e', name: 'Delete edited', digest: 'de', fingerprint: 'fe', edited: true } as OpenApiSyncItem;
    mockSync(makePlan({ items: [cleanRemoved, editedRemoved, changedItem, newItem] }));
    renderWithQuery(<TabWithListeners collection='bulk11' />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: /^Select matches/ });
    // The new request is selected already and stays selected through a narrower search.
    await user.type(screen.getByRole('searchbox', { name: 'Search changes' }), 'Delete');
    expect(screen.getByRole('button', { name: 'Select matches (0)' })).toBeDisabled();
    await user.clear(screen.getByRole('searchbox', { name: 'Search changes' }));
    await user.type(screen.getByRole('searchbox', { name: 'Search changes' }), 'e');
    await user.click(screen.getByRole('button', { name: 'Select matches (2)' }));
    // New and changed are selected, neither removed request is.
    expect(screen.getByText(/· 2 selected$/)).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Include Delete pet in apply' })).not.toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Include Delete edited in apply' })).not.toBeChecked();
  });

  it('the_bulk_dialog_closes_when_the_plan_changes', async () => {
    mockSync();
    renderWithQuery(<TabWithListeners collection='bulk12' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [{ ...newItem, digest: 'zz' }] }),
    );
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'bulk12' } as never));
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    expect(tauriApi.applyOpenApiSync).not.toHaveBeenCalled();
  });

  it('a_failed_skip_all_restores_the_selection', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue('Conflict: an earlier apply is not finished yet');
    renderWithQuery(<TabWithListeners collection='bulk13' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Skip all' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText(/· 1 selected$/)).toBeInTheDocument());
  });
});
