import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncTab } from '../SpecSyncTab';
import { makePlan, makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn(),
    onOpenApiSyncChecked: vi.fn(() => Promise.resolve(() => undefined)),
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
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('c');
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

  it('never offers a check or any action', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue(makePlan());
    renderWithQuery(<SpecSyncTab collection='c' />);
    await screen.findByText('Folder: pets / read');
    await waitFor(() => expect(screen.queryByText(/check now/i)).toBeNull());
    expect(screen.queryByRole('checkbox')).toBeNull();
  });
});
