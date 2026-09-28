import { describe, expect, it } from 'vitest';
import { exitLabel } from '../flowExits';

const ifKind = { kind: 'If' as const, label: 'Ok?', condition: 'response.status === 200' };
const switchKind = {
  kind: 'Switch' as const,
  label: 'Plan',
  value: 'response.body.plan',
  cases: [{ id: 'c1', label: 'Pro plan', matches: 'pro' }],
};

describe('exitLabel', () => {
  it('labels If exits', () => {
    expect(exitLabel(ifKind, 'true')).toBe('true');
    expect(exitLabel(ifKind, 'false')).toBe('false');
    expect(exitLabel(ifKind, 'result')).toBeUndefined();
  });

  it('labels Switch exits by the current case label', () => {
    expect(exitLabel(switchKind, 'case:c1')).toBe('Pro plan');
    expect(exitLabel(switchKind, 'default')).toBe('default');
    expect(exitLabel(switchKind, 'case:gone')).toBeUndefined();
  });

  it('has no label for plain node exits', () => {
    expect(exitLabel({ kind: 'Output', label: 'Out' }, 'result')).toBeUndefined();
  });
});
