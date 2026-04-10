import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Contract, ContractScope } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import { ContractBadge } from '../ContractBadge';

// Mock tauri-api — ContractPanel (rendered alongside the badge) would otherwise
// try to talk to the backend when mounted.
vi.mock('@/lib/tauri-api', () => ({
  attachContract: vi.fn(),
  listContracts: vi.fn(),
  getContract: vi.fn(),
  deleteContract: vi.fn(),
  getContractChangelog: vi.fn().mockResolvedValue({ contractId: '', entries: [] }),
}));

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
    const { container } = render(<ContractBadge contracts={[]} collectionRoot={ROOT} />);
    // No button, no tooltip trigger — completely empty subtree.
    expect(container.firstChild).toBeNull();
  });

  it('renders a button labelled with the primary contract title', () => {
    render(
      <ContractBadge
        contracts={[makeContract({ title: 'Payments API v2.3' })]}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /view contract: payments api v2\.3/i });
    expect(button).toBeDefined();
  });

  it('uses the muted foreground colour for an active contract', () => {
    render(
      <ContractBadge contracts={[makeContract({ expiryDate: null })]} collectionRoot={ROOT} />,
    );
    const button = screen.getByRole('button', { name: /view contract/i });
    expect(button.className).toContain('text-muted-foreground');
    expect(button.className).not.toContain('text-warning');
    expect(button.className).not.toContain('text-destructive');
  });

  it('uses the warning colour when the primary contract is expiring', () => {
    render(
      <ContractBadge
        // Today + 10 days — inside the 30-day window.
        contracts={[makeContract({ expiryDate: '2026-04-21' })]}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /view contract/i });
    expect(button.className).toContain('text-warning');
  });

  it('uses the destructive colour when the primary contract is expired', () => {
    render(
      <ContractBadge
        contracts={[makeContract({ expiryDate: '2026-04-10' })]}
        collectionRoot={ROOT}
      />,
    );
    const button = screen.getByRole('button', { name: /view contract/i });
    expect(button.className).toContain('text-destructive');
  });

  it('uses the first contract as the primary when multiple are attached', () => {
    const first = makeContract({ id: 'a', title: 'First', expiryDate: null });
    const second = makeContract({ id: 'b', title: 'Second', expiryDate: '2026-04-10' });
    render(<ContractBadge contracts={[first, second]} collectionRoot={ROOT} />);

    const button = screen.getByRole('button', { name: /view contract: first/i });
    // Status derives from the *first* contract, not the expired second.
    expect(button.className).toContain('text-muted-foreground');
    expect(button.className).not.toContain('text-destructive');
  });
});
