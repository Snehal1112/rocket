import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncTab } from '../SpecSyncTab';
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
  };
});

describe('SpecSyncTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows a loading state first', () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockReturnValue(new Promise(() => undefined));
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(screen.getByText('Loading spec sync...')).toBeInTheDocument();
  });

  it('says no changes were found when nothing is pending, without asking for a plan', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: false, lastStatus: { state: 'unchanged' } }),
    );
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(await screen.findByText('No changes found')).toBeInTheDocument();
    expect(screen.getByText('Last checked 2h ago.')).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).not.toHaveBeenCalled();
  });

  it('trusts the plan when it says nothing is pending', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan({ pending: false }));
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(await screen.findByText('No changes found')).toBeInTheDocument();
  });

  it('shows the list with the first item selected and switches the detail', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(await screen.findByText('Folder: pets / read')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Delete pet'));
    expect(screen.getByText('Treated as edited, will be kept')).toBeInTheDocument();
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('c', []);
  });

  it('shows the state file error with its message', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('bad yaml at line 3');
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(
      await screen.findByText(/The sync state file cannot be read: bad yaml at line 3/),
    ).toBeInTheDocument();
  });

  it('asks for a new check when the pending spec changed', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockRejectedValue('Conflict: the pending spec changed, check again');
    renderWithQuery(<SpecSyncTab collection='c' />);
    expect(await screen.findByText('The pending spec changed, check again')).toBeInTheDocument();
  });

  it('never offers a check now button', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<SpecSyncTab collection='c' />);
    await screen.findByText('Folder: pets / read');
    await waitFor(() => expect(screen.queryByText(/check now/i)).toBeNull());
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

describe('SpecSyncTab actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
  });

  it('puts a checkbox on every row and counts the selection', async () => {
    mockSync();
    renderWithQuery(<SpecSyncTab collection='sel1' />);
    await screen.findByText('Folder: pets / read');
    expect(screen.getAllByRole('checkbox')).toHaveLength(3);
    expect(screen.getByText('0 selected')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
    expect(screen.getByText('1 selected')).toBeInTheDocument();
  });

  it('links a rename, re-plans with the link and shows one Changed item', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockImplementation(async (_c, links = []) =>
      links.length > 0
        ? makePlan({ items: [{ ...changedItem, key: 'GET /pets', name: 'List pets' } as OpenApiSyncItem] })
        : makePlan({ items: [{ ...newItem }, ({ ...removedItem, key: 'GET /animals', possiblyRenamedTo: undefined } as OpenApiSyncItem)] }),
    );
    renderWithQuery(<SpecSyncTab collection='link1' />);
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
    renderWithQuery(<SpecSyncTab collection='skip1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
    await user.click(screen.getByRole('button', { name: 'Skip selected' }));
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
    renderWithQuery(<SpecSyncTab collection='apply1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('button', { name: 'Select all (3)' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    expect(screen.getByText('1 new request will be added.')).toBeInTheDocument();
    expect(
      screen.getByText('1 removed request will be deleted, you did not edit it.'),
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
    renderWithQuery(<SpecSyncTab collection='choice1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByText('Add pet'));
    await user.click(
      within(screen.getByRole('radiogroup', { name: 'Url' })).getByRole('radio', {
        name: 'Take spec',
      }),
    );
    await user.click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Apply now' }));
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalled());
    expect(vi.mocked(tauriApi.applyOpenApiSync).mock.calls[0]?.[1].items).toEqual([
      { key: 'POST /pets', digest: 'd2', choices: { url: 'spec' } },
    ]);
  });

  it('searches every group and selects all matches, hidden rows included', async () => {
    const items = Array.from({ length: 1200 }, (_, i) => ({
      ...changedItem,
      key: `GET /p${i}`,
      name: `Request ${i}`,
    })) as OpenApiSyncItem[];
    mockSync(makePlan({ items }));
    renderWithQuery(<SpecSyncTab collection='search1' />);
    const user = userEvent.setup();
    await screen.findByText('Changed (1200)');
    expect(screen.getByRole('button', { name: 'Select all (1200)' })).toBeInTheDocument();
    await user.type(screen.getByRole('searchbox', { name: 'Search changes' }), 'Request 11');
    // Request 11, 110-119 and 1100-1199 match.
    expect(await screen.findByText('Changed (111)')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Select all (111)' }));
    expect(screen.getByText('111 selected')).toBeInTheDocument();
  });

  it('shows the sentence for an apply error', async () => {
    mockSync();
    vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue(
      'Conflict: an earlier apply is not finished yet at /home/me/x',
    );
    renderWithQuery(<SpecSyncTab collection='err1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
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
    renderWithQuery(<SpecSyncTab collection='again1' />);
    const user = userEvent.setup();
    await screen.findByText('Add pet');
    await user.click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
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
});
