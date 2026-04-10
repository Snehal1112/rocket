import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChangelogEntry, Contract, ContractChangelog, ContractScope } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import { ContractPanel } from '../ContractPanel';

vi.mock('@/lib/tauri-api', () => ({
  attachContract: vi.fn(),
  listContracts: vi.fn(),
  getContract: vi.fn(),
  deleteContract: vi.fn(),
  getContractChangelog: vi.fn(),
}));

import {
  deleteContract as apiDelete,
  getContractChangelog as apiGetChangelog,
} from '@/lib/tauri-api';

const mockDelete = vi.mocked(apiDelete);
const mockGetChangelog = vi.mocked(apiGetChangelog);

const ROOT = '/tmp/workspace/collections/payments';
const collectionScope: ContractScope = { type: 'collection' };

function makeContract(overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c1',
    title: 'Payments API v1',
    provider: 'Billing Team',
    consumer: 'Platform Team',
    project: 'Checkout',
    version: 'v1',
    effectiveDate: '2026-01-01',
    expiryDate: null,
    documentPath: null,
    enforcementMode: 'warn',
    scope: collectionScope,
    ...overrides,
  };
}

function makeEntry(overrides: Partial<ChangelogEntry> = {}): ChangelogEntry {
  return {
    timestamp: '2026-04-10T12:00:00Z',
    requestPath: 'auth/login',
    field: 'headers.X-Trace',
    changeType: 'changed',
    oldValue: 'off',
    newValue: 'on',
    ...overrides,
  };
}

function resetStore() {
  useContractStore.setState({
    contractsByRoot: {},
    changelogs: {},
    error: null,
    loading: false,
  });
}

// Build an ISO date (YYYY-MM-DD) offset from real "today" so status-based
// tests stay deterministic without faking timers. Fake timers would freeze
// setTimeout and make @testing-library's waitFor hang.
function daysFromToday(offset: number): string {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() + offset);
  return d.toISOString().slice(0, 10);
}

describe('ContractPanel', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
    // Default: no changelog entries returned from the backend.
    mockGetChangelog.mockResolvedValue({ contractId: 'c1', entries: [] });
  });

  it('renders contract metadata, scope, and the Active status badge', async () => {
    render(
      <ContractPanel open onOpenChange={vi.fn()} contract={makeContract()} collectionRoot={ROOT} />,
    );

    expect(screen.getByText('Payments API v1')).toBeDefined();
    expect(screen.getByText(/provider: billing team/i)).toBeDefined();
    expect(screen.getByText(/consumer: platform team/i)).toBeDefined();
    expect(screen.getByText('Checkout')).toBeDefined();
    expect(screen.getByText('v1')).toBeDefined();
    expect(screen.getByText('2026-01-01')).toBeDefined();
    expect(screen.getByText(/entire collection/i)).toBeDefined();
    expect(screen.getByText(/^active$/i)).toBeDefined();

    // Changelog fetch is fired as a side effect on open.
    await waitFor(() => expect(mockGetChangelog).toHaveBeenCalledWith(ROOT, 'c1'));
  });

  it('shows "Expiring soon" badge when the contract is inside the warning window', async () => {
    const expiry = daysFromToday(10);
    render(
      <ContractPanel
        open
        onOpenChange={vi.fn()}
        contract={makeContract({ expiryDate: expiry })}
        collectionRoot={ROOT}
      />,
    );
    expect(screen.getByText(/expiring soon/i)).toBeDefined();
    expect(screen.getByText(expiry)).toBeDefined();
    await waitFor(() => expect(mockGetChangelog).toHaveBeenCalled());
  });

  it('shows "Expired" badge when the contract is past its expiry', async () => {
    render(
      <ContractPanel
        open
        onOpenChange={vi.fn()}
        contract={makeContract({ expiryDate: daysFromToday(-1) })}
        collectionRoot={ROOT}
      />,
    );
    expect(screen.getByText(/^expired$/i)).toBeDefined();
    await waitFor(() => expect(mockGetChangelog).toHaveBeenCalled());
  });

  it('shows the empty changelog placeholder when there are no entries', async () => {
    render(
      <ContractPanel open onOpenChange={vi.fn()} contract={makeContract()} collectionRoot={ROOT} />,
    );
    await waitFor(() => expect(mockGetChangelog).toHaveBeenCalled());
    expect(screen.getByText(/no changes recorded since contract was signed/i)).toBeDefined();
  });

  it('renders changelog rows when entries are present in the store', async () => {
    const log: ContractChangelog = {
      contractId: 'c1',
      entries: [
        makeEntry({
          field: 'headers.X-Trace',
          changeType: 'changed',
          oldValue: 'off',
          newValue: 'on',
        }),
        makeEntry({
          field: 'query.limit',
          changeType: 'added',
          oldValue: null,
          newValue: '100',
        }),
      ],
    };
    // Seed the store *and* make the on-open refetch return the same value so
    // state after the effect is still populated.
    useContractStore.setState({ changelogs: { c1: log } });
    mockGetChangelog.mockResolvedValueOnce(log);

    render(
      <ContractPanel open onOpenChange={vi.fn()} contract={makeContract()} collectionRoot={ROOT} />,
    );

    expect(screen.getByText('headers.X-Trace')).toBeDefined();
    expect(screen.getByText('query.limit')).toBeDefined();
    expect(screen.getByText(/^changed$/i)).toBeDefined();
    expect(screen.getByText(/^added$/i)).toBeDefined();
    // Table is rendered — no empty placeholder.
    expect(screen.queryByText(/no changes recorded/i)).toBeNull();
  });

  it('delete button calls deleteContract and closes the panel', async () => {
    mockDelete.mockResolvedValueOnce(undefined);
    // Seed the store so removeContract can filter an existing entry.
    useContractStore.setState({
      contractsByRoot: { [ROOT]: [makeContract()] },
    });
    const onOpenChange = vi.fn();

    render(
      <ContractPanel
        open
        onOpenChange={onOpenChange}
        contract={makeContract()}
        collectionRoot={ROOT}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /remove contract/i }));

    await waitFor(() => expect(mockDelete).toHaveBeenCalledWith(ROOT, 'c1'));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    // Store state reflects the removal.
    expect(useContractStore.getState().contractsByRoot[ROOT]).toEqual([]);
  });

  it('does not fetch the changelog when the panel is closed', () => {
    render(
      <ContractPanel
        open={false}
        onOpenChange={vi.fn()}
        contract={makeContract()}
        collectionRoot={ROOT}
      />,
    );
    expect(mockGetChangelog).not.toHaveBeenCalled();
  });
});
