import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ContractStatusChip } from './ContractStatusChip';

function getDot(container: HTMLElement): HTMLElement | null {
  return container.querySelector('[aria-hidden="true"].rounded-full');
}

describe('ContractStatusChip', () => {
  it('renders Active with a dot', () => {
    const { container } = render(<ContractStatusChip status='active' />);
    expect(screen.getByRole('status')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(getDot(container)).not.toBeNull();
  });

  it('renders Drift with count and no warning emoji', () => {
    render(<ContractStatusChip status='drift' count={3} />);
    expect(screen.getByText('Drift · 3')).toBeInTheDocument();
  });

  it('renders Drift with a dot', () => {
    const { container } = render(<ContractStatusChip status='drift' count={2} />);
    expect(getDot(container)).not.toBeNull();
  });

  it('renders Breaching with a dot', () => {
    const { container } = render(<ContractStatusChip status='breach' />);
    expect(screen.getByText('Breaching')).toBeInTheDocument();
    expect(getDot(container)).not.toBeNull();
  });

  it('renders In review with a dot', () => {
    const { container } = render(<ContractStatusChip status='in_review' />);
    expect(screen.getByText('In review')).toBeInTheDocument();
    expect(getDot(container)).not.toBeNull();
  });

  it('renders Paused with a dot', () => {
    const { container } = render(<ContractStatusChip status='paused' />);
    expect(screen.getByText('Paused')).toBeInTheDocument();
    expect(getDot(container)).not.toBeNull();
  });

  it('renders Expired with a dot', () => {
    const { container } = render(<ContractStatusChip status='expired' />);
    expect(screen.getByText('Expired')).toBeInTheDocument();
    expect(getDot(container)).not.toBeNull();
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
