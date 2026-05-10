import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Contract } from '@/types/contracts';
import { ContractCard } from './ContractCard';

function wrap(ui: React.ReactElement) {
  return render(<TooltipProvider>{ui}</TooltipProvider>);
}

function makeContract(status: Contract['status'], overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c-test',
    collectionId: 'col1',
    name: 'Payments API',
    version: '1.2.0',
    status,
    provider: { id: 'billing', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'platform', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-15',
    expiresAt: null,
    signedSnapshot: null,
    driftCount: 0,
    breachCount: 0,
    endpointCount: 5,
    changelog: [],
    createdBy: 'user1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('ContractCard', () => {
  it('renders contract name', () => {
    wrap(
      <ContractCard
        contract={makeContract('active')}
        collectionRoot='/ws/col'
        onAction={vi.fn()}
      />,
    );
    expect(screen.getByText('Payments API')).toBeInTheDocument();
  });

  it('renders version tag', () => {
    wrap(
      <ContractCard
        contract={makeContract('active')}
        collectionRoot='/ws/col'
        onAction={vi.fn()}
      />,
    );
    expect(screen.getByText('1.2.0')).toBeInTheDocument();
  });

  it('renders all 7 status variants without throwing', () => {
    const statuses: Contract['status'][] = [
      'active',
      'drift',
      'breach',
      'in_review',
      'draft',
      'paused',
      'expired',
    ];
    for (const status of statuses) {
      const { unmount } = wrap(
        <ContractCard
          contract={makeContract(status)}
          collectionRoot='/ws/col'
          onAction={vi.fn()}
        />,
      );
      unmount();
    }
  });

  it('shows "Drift detected" StatusSubline for drift status', () => {
    const c = makeContract('drift', { driftCount: 3, breachCount: 1 });
    wrap(<ContractCard contract={c} collectionRoot='/ws/col' onAction={vi.fn()} />);
    expect(screen.getByText(/Drift detected/)).toBeInTheDocument();
  });

  it('shows "Monitoring paused" for paused status', () => {
    wrap(
      <ContractCard
        contract={makeContract('paused')}
        collectionRoot='/ws/col'
        onAction={vi.fn()}
      />,
    );
    expect(screen.getByText('Monitoring paused')).toBeInTheDocument();
  });

  it('calls onOpen when article is clicked', () => {
    const onOpen = vi.fn();
    wrap(
      <ContractCard
        contract={makeContract('active')}
        collectionRoot='/ws/col'
        onAction={vi.fn()}
        onOpen={onOpen}
      />,
    );
    fireEvent.click(screen.getByRole('article'));
    expect(onOpen).toHaveBeenCalledWith('c-test');
  });

  it('has aria-labelledby pointing to contract name span', () => {
    wrap(
      <ContractCard
        contract={makeContract('active')}
        collectionRoot='/ws/col'
        onAction={vi.fn()}
      />,
    );
    const article = screen.getByRole('article');
    expect(article).toHaveAttribute('aria-labelledby', 'cc-name-c-test');
    expect(document.getElementById('cc-name-c-test')?.textContent).toBe('Payments API');
  });

  it('shows +N more tooltip when >1 consumer', () => {
    const c = makeContract('active', {
      consumers: [
        { id: 'c1', name: 'Platform', kind: 'team' },
        { id: 'c2', name: 'Mobile', kind: 'team' },
        { id: 'c3', name: 'Web', kind: 'team' },
      ],
    });
    wrap(<ContractCard contract={c} collectionRoot='/ws/col' onAction={vi.fn()} />);
    expect(screen.getByText('+2 more')).toBeInTheDocument();
  });
});
