import { describe, expect, it } from 'vitest';
import type { DriftReport } from '@/types/contracts';
import { transitionStatus } from './statusMachine';

function report(driftCount: number, breachCount: number): DriftReport {
  return { contractId: 'c1', computedAt: '', diffs: [], driftCount, breachCount };
}

describe('transitionStatus', () => {
  it('active + no drift → stays active', () => {
    expect(transitionStatus('active', report(0, 0))).toBe('active');
  });
  it('active + drift → drift', () => {
    expect(transitionStatus('active', report(3, 0))).toBe('drift');
  });
  it('active + breach → breach', () => {
    expect(transitionStatus('active', report(2, 1))).toBe('breach');
  });
  it('drift + no changes → reverts to active (changes reverted)', () => {
    expect(transitionStatus('drift', report(0, 0))).toBe('active');
  });
  it('breach + no changes → reverts to active', () => {
    expect(transitionStatus('breach', report(0, 0))).toBe('active');
  });
  it('paused + breach → stays paused (not monitored)', () => {
    expect(transitionStatus('paused', report(5, 2))).toBe('paused');
  });
  it('draft + drift → stays draft (not published)', () => {
    expect(transitionStatus('draft', report(3, 1))).toBe('draft');
  });
});
