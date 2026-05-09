import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Contract, ContractScope } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import { ContractBadge } from '../ContractBadge';

vi.mock('@/lib/tauri-api', () => ({
  attachContract: vi.fn(),
  listContracts: vi.fn(),
  getContract: vi.fn(),
  deleteContract: vi.fn(),
  getContractChangelog: vi.fn().mockResolvedValue({ contractId: '', entries: [] }),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: (selector: (s: { openContractTab: ReturnType<typeof vi.fn> }) => unknown) =>
    selector({ openContractTab: vi.fn() }),
}));

const ROOT = '/tmp/workspace/collections/payments';
const NAME = 'payments';
const collectionScope: ContractScope = { type: 'collection' };

function makeContract(overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c1',
    title: 'Payments API v1',
    provider: { id: 'p1', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'c1', name: 'Platform Team', kind: 'team' }],
    project: 'Checkout',
    version: 'v1',
    status: 'active',
    effectiveDate: '2026-01-01',
    expiryDate: null,
    documentPaths: [],
    enforcementMode: 'warn',
    scope: collectionScope,
    policy: { breakingChangePolicy: 'strict', noticeDays: 30 },
    driftCount: 0,
    breachCount: 0,
    endpointCount: 0,
    createdBy: null,
    createdAt: null,
    updatedAt: null,
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

describe('ContractBadge', () => {
  // Pin "today" so expiring/expired status windows are deterministic.
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-11T12:00:00Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders nothing when the contracts array is empty', () => {
    const { container } = render(
      <ContractBadge contracts={[]} collectionName={NAME} collectionRoot={ROOT} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders a button with aria-label Manage contracts', () => {
    render(
      <ContractBadge
        contracts={[makeContract({ title: 'Payments API v2.3' })]}
        collectionName={NAME}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /manage contracts/i });
    expect(button).toBeDefined();
  });

  it('uses the muted foreground colour for an active contract', () => {
    render(
      <ContractBadge
        contracts={[makeContract({ expiryDate: null })]}
        collectionName={NAME}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /manage contracts/i });
    expect(button.className).toContain('text-muted-foreground');
    expect(button.className).not.toContain('text-warning');
    expect(button.className).not.toContain('text-destructive');
  });

  it('uses the warning colour when the primary contract is expiring', () => {
    render(
      <ContractBadge
        // Today + 10 days — inside the 30-day window.
        contracts={[makeContract({ expiryDate: '2026-04-21' })]}
        collectionName={NAME}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /manage contracts/i });
    expect(button.className).toContain('text-warning');
  });

  it('uses the destructive colour when the primary contract is expired', () => {
    render(
      <ContractBadge
        contracts={[makeContract({ expiryDate: '2026-04-10' })]}
        collectionName={NAME}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /manage contracts/i });
    expect(button.className).toContain('text-destructive');
  });

  it('uses the first contract as the primary when multiple are attached', () => {
    const first = makeContract({ id: 'a', title: 'First', expiryDate: null });
    const second = makeContract({ id: 'b', title: 'Second', expiryDate: '2026-04-10' });
    render(
      <ContractBadge contracts={[first, second]} collectionName={NAME} collectionRoot={ROOT} />,
    );
    const button = screen.getByRole('button', { name: /manage contracts/i });
    // Status derives from the first contract, not the expired second.
    expect(button.className).toContain('text-muted-foreground');
    expect(button.className).not.toContain('text-destructive');
  });
});
