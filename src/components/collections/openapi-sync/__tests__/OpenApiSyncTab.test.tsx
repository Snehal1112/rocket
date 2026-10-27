import { act, configure, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearApplyState } from '@/lib/queries/openapi-sync-queries';
import { useApplySync } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncTab } from '../OpenApiSyncTab';
import type { OpenApiApplyReport, OpenApiSyncItem } from '@/lib/tauri-api';
import { changedItem, makePlan, makeStatus, newItem, removedItem } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn(),
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
  });

  it('shows a loading state first', () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockReturnValue(new Promise(() => undefined));
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(screen.getByText('Loading OpenAPI sync...')).toBeInTheDocument();
  });

  it('defaults_to_overview_otherwise', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText('Nothing to show yet')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Overview' })).toHaveAttribute('data-state', 'active');
    expect(screen.queryByRole('tab', { name: /^Spec updates/ })).toBeNull();
    expect(tauriApi.getOpenApiSyncPlan).not.toHaveBeenCalled();
  });

  it('trusts the plan when it says nothing is pending', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan({ pending: false }));
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText('Nothing to show yet')).toBeInTheDocument();
  });

  it('defaults_to_spec_updates_when_updates_wait', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [changedItem, newItem] }),
    );
    renderWithQuery(<OpenApiSyncTab collection='c' />);
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
    renderWithQuery(<OpenApiSyncTab collection='cb1' />);
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
  });

  it('inner_tabs_switch', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    await screen.findByText('Folder: pets / read');
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name: 'Your changes' }));
    expect(screen.queryByText('Folder: pets / read')).toBeNull();
    await user.click(screen.getByRole('tab', { name: 'Spec updates (3)' }));
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
  });

  it('overview_and_changes_show_the_empty_state', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasPending: false }));
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText('Nothing to show yet')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('tab', { name: 'Your changes' }));
    expect(screen.getByText('Nothing to show yet')).toBeInTheDocument();
  });

  it('after_disconnect_the_tab_shows_that_nothing_is_connected', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    renderWithQuery(<OpenApiSyncTab collection='nc1' />);
    expect(
      await screen.findByText('This collection is not connected to an OpenAPI spec.'),
    ).toBeInTheDocument();
  });

  it('shows the list with the first item selected and switches the detail', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Delete pet'));
    expect(screen.getByText('Treated as edited, will be kept.')).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('c', []);
  });

  it('shows_a_fixed_sentence_for_an_unreadable_state', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue(
      'bad yaml in /home/me/proj/.rocket/openapi-source.yml at line 3',
    );
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(
      await screen.findByText('Something went wrong with the OpenAPI sync. Try again.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/state file cannot be read/)).toBeNull();
    expect(screen.queryByText(/home\/me/)).toBeNull();
  });

  it('prefixes a known status error with the state file sentence', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('consent_required: x');
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText(/The sync state file cannot be read: /)).toBeInTheDocument();
  });

  it('shows a fixed sentence for a plan error, never the path', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockRejectedValue('cannot read /home/me/spec.json');
    renderWithQuery(<OpenApiSyncTab collection='c' />);
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
    renderWithQuery(<OpenApiSyncTab collection='c' />);
    expect(await screen.findByText('The pending spec changed, check again')).toBeInTheDocument();
  });

  it('no longer has the folded Settings section', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<OpenApiSyncTab collection='c' />);
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
    renderWithQuery(<OpenApiSyncTab collection='dis1' />);
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
    renderWithQuery(<OpenApiSyncTab collection='c' />);
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
    renderWithQuery(<OpenApiSyncTab collection='cons1' />);
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
    await screen.findByText('Folder: pets / read');
    expect(
      screen.getByText(
        'This pending spec came with the collection files. It may not have been checked on this machine.',
      ),
    ).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByRole('button', { name: /^Apply/ })).toBeEnabled();
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
    renderWithQuery(<OpenApiSyncTab collection='cons2' />);
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
    renderWithQuery(<OpenApiSyncTab collection='sel1' />);
    await screen.findByText('Folder: pets / read');
    expect(screen.getAllByRole('checkbox')).toHaveLength(3);
    expect(screen.getByText('0 selected')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByText('1 selected')).toBeInTheDocument();
  });

  it('links a rename, re-plans with the link and shows one Changed item', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation(async (_c, links = []) =>
      links.length > 0
        ? makePlan({ items: [{ ...changedItem, key: 'GET /pets', name: 'List pets' } as OpenApiSyncItem] })
        : makePlan({ items: [{ ...newItem }, ({ ...removedItem, key: 'GET /animals', possiblyRenamedTo: undefined } as OpenApiSyncItem)] }),
    );
    renderWithQuery(<OpenApiSyncTab collection='link1' />);
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

  it('skips the selected items with their fingerprints and drops them after the refetch', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan)
      .mockResolvedValueOnce(makePlan())
      .mockResolvedValue(makePlan({ items: [newItem, removedItem] }));
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({ ...report, applied: [], skipped: 1 });
    renderWithQuery(<OpenApiSyncTab collection='skip1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: /^Skip selected/ }));
    await waitFor(() =>
      expect(tauriApi.applyOpenApiSync).toHaveBeenCalledWith('skip1', {
        items: [],
        skips: [{ key: 'POST /pets', fingerprint: 'f2' }],
        variables: [],
        renameLinks: [],
      }),
    );
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
    renderWithQuery(<OpenApiSyncTab collection='apply1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Select all (3)' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
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
    renderWithQuery(<OpenApiSyncTab collection='choice1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByText('Add pet'));
    await user.click(
      within(screen.getByRole('radiogroup', { name: 'Url' })).getByRole('radio', {
        name: 'Take spec',
      }),
    );
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalled());
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].items).toEqual([
      { key: 'POST /pets', digest: 'd2', choices: { url: 'spec' } },
    ]);
  });

  it('searches every group and selects all matches, hidden rows included', async () => {
    const items = Array.from({ length: 600 }, (_, i) => ({
      ...changedItem,
      key: `GET /p${i}`,
      name: `Request ${i}`,
    })) as OpenApiSyncItem[];
    mockSync(makePlan({ items }));
    renderWithQuery(<OpenApiSyncTab collection='search1' />);
    const user = userEvent.setup();
    await screen.findByText('Changed (600)');
    expect(screen.getByRole('button', { name: 'Select all (600)' })).toBeInTheDocument();
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search changes' }), {
      target: { value: 'Request 5' },
    });
    // Request 5, 50-59 and 500-599 match, the last ones past the 500 row cap.
    expect(await screen.findByText('Changed (111)')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Select all (111)' }));
    expect(screen.getByText('111 selected')).toBeInTheDocument();
  });

  it('shows the sentence for an apply error', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue(
      'Conflict: an earlier apply is not finished yet at /home/me/x',
    );
    renderWithQuery(<OpenApiSyncTab collection='err1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    expect(
      await screen.findByText('Another apply is not finished yet. Wait a moment and try again.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/\/home\/me/)).toBeNull();
  });

  it('Check again refetches the status and the plan', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    renderWithQuery(<OpenApiSyncTab collection='again1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
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
    renderWithQuery(<OpenApiSyncTab collection='links2' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Link as rename' }));
    await screen.findByText('Changed (1)');
    // A skip with a link in place carries no links.
    await user.click(await screen.findByRole('checkbox', { name: 'Include List pets in apply' }));
    await user.click(screen.getByRole('button', { name: /^Skip selected/ }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].renameLinks).toEqual([]);
    await user.click((await screen.findAllByRole('button', { name: 'Close' }))[0] as HTMLElement);
    // An apply sends the link, and the next selection does not.
    await user.click(await screen.findByRole('checkbox', { name: 'Include List pets in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
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
    renderWithQuery(<OpenApiSyncTab collection='keep1' />);
    const link = await screen.findByRole('button', { name: 'Link as rename' });
    await userEvent.setup().click(link);
    expect(screen.queryByText('Loading the plan...')).toBeNull();
    expect(screen.getByRole('checkbox', { name: 'Include List pets in apply' })).toBeInTheDocument();
    // Apply waits for the real plan.
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
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
        <OpenApiSyncTab collection='busy1' />
        <OpenApiSyncTab collection='busy1' />
      </>,
    );
    const user = userEvent.setup();
    await screen.findAllByText('Add pet');
    for (const box of screen.getAllByRole('checkbox', { name: 'Include Add pet in apply' })) {
      await user.click(box);
    }
    await user.click(screen.getAllByRole('button', { name: 'Apply' })[0] as HTMLElement);
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    // The other tab sees the apply and can neither apply nor skip.
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'Apply' })[1]).toBeDisabled());
    expect(screen.getAllByRole('button', { name: /^Skip selected/ })[1]).toBeDisabled();
    await act(async () => finish(report));
    // Both tabs show the shared report, so Escape closes it for both.
    await screen.findAllByText('Apply finished');
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByText('Apply finished')).toBeNull());
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'Apply' })[1]).toBeEnabled());
    expect(screen.queryByText('Skipped')).toBeNull();
  });

  it('keeps a selection made outside the filter when Select all runs on a filter', async () => {
    mockSync();
    renderWithQuery(<OpenApiSyncTab collection='keep2' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.type(screen.getByRole('searchbox', { name: 'Search changes' }), 'List');
    await user.click(screen.getByRole('button', { name: 'Select all (1)' }));
    expect(screen.getByText('2 selected')).toBeInTheDocument();
  });

  it('closes the confirm dialog when the plan changes under it', async () => {
    mockSync();
    renderWithQuery(<OpenApiSyncTab collection='conf1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    expect(screen.getByRole('button', { name: 'Apply now' })).toBeInTheDocument();
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(
      makePlan({ items: [{ ...changedItem, digest: 'zz' } as OpenApiSyncItem] }),
    );
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'conf1' } as never));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Apply now' })).toBeNull());
    expect(screen.getByText('0 selected')).toBeInTheDocument();
    // Selecting again must not pop the dialog open by itself.
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByText('1 selected')).toBeInTheDocument();
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
    renderWithQuery(<OpenApiSyncTab collection='links3' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Link as rename' }));
    await screen.findByText('Changed (1)');
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'links3' } as never));
    await user.click(await screen.findByRole('checkbox', { name: 'Include List pets in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalled());
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].renameLinks).toEqual([
      ['GET /animals', 'GET /pets'],
    ]);
  });

  it('disables Apply while the plan refetches', async () => {
    mockSync();
    renderWithQuery(<OpenApiSyncTab collection='fetch1' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(new Promise(() => undefined));
    const onChecked = vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0];
    await act(async () => onChecked?.({ collection: 'fetch1' } as never));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled());
  });

  it('shows the report again when the tab is opened after leaving mid-apply', async () => {
    mockSync();
    let finish: (v: OpenApiApplyReport) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise<OpenApiApplyReport>((resolve) => {
        finish = resolve;
      }),
    );
    const first = renderWithQuery(<OpenApiSyncTab collection='away1' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    first.unmount();
    await act(async () => finish(report));
    renderWithQuery(<OpenApiSyncTab collection='away1' />);
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
    const first = renderWithQuery(<OpenApiSyncTab collection='away2' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    first.unmount();
    await act(async () => fail('Conflict: an earlier apply is not finished yet'));
    renderWithQuery(<OpenApiSyncTab collection='away2' />);
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
    renderWithQuery(<OpenApiSyncTab collection='vars1' />);
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('checkbox', { name: 'Include Base URL baseUrl in apply' }),
    );
    await user.click(screen.getByRole('button', { name: 'Apply' }));
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
        <OpenApiSyncTab collection='race1' />
        <Starter />
      </>,
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: 'Include Add pet in apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    fireEvent.click(screen.getByText('start other'));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    expect(await screen.findByText('Another apply is still running. Wait for it to finish.')).toBeInTheDocument();
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1);
  });
});
