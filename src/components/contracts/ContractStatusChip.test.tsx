import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ContractStatusChip } from './ContractStatusChip';

describe('ContractStatusChip', () => {
  it('renders Active with dot', () => {
    render(<ContractStatusChip status='active' />);
    expect(screen.getByRole('status')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  it('renders Drift with count', () => {
    render(<ContractStatusChip status='drift' count={3} />);
    expect(screen.getByText('⚠ Drift · 3')).toBeInTheDocument();
  });

  it('renders Breaching', () => {
    render(<ContractStatusChip status='breach' />);
    expect(screen.getByText('Breaching')).toBeInTheDocument();
  });

  it('has sr-only text for each status', () => {
    const statuses = [
      'active',
      'drift',
      'breach',
      'in_review',
      'draft',
      'paused',
      'expired',
    ] as const;
    for (const status of statuses) {
      const { unmount } = render(<ContractStatusChip status={status} />);
      const srOnly = document.querySelector('.sr-only');
      expect(srOnly?.textContent).toMatch(/Status:/);
      unmount();
    }
  });
});
