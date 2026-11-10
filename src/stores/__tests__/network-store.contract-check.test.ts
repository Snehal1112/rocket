import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useNetworkStore } from '@/stores/network-store';

vi.mock('@/lib/tauri-api', () => ({
  getNetworkStatus: vi.fn(),
  listNetworkNodes: vi.fn(),
  listNetworkLinks: vi.fn(),
}));

const store = () => useNetworkStore.getState();
const check = (checkId = 'c1') => ({
  checkId,
  deviceId: 'DEV1',
  contract: 'Orders API',
  changeCount: 2,
});

describe('network store contract check slice', () => {
  beforeEach(() => store().reset());

  it('adds and removes an incoming check', () => {
    store().addIncomingContractCheck(check());
    store().addIncomingContractCheck(check());
    expect(store().contractChecksIncoming).toHaveLength(1);
    store().removeIncomingContractCheck('c1');
    expect(store().contractChecksIncoming).toEqual([]);
  });

  it('keeps two offers side by side', () => {
    store().addIncomingContractCheck(check('c1'));
    store().addIncomingContractCheck(check('c2'));
    expect(store().contractChecksIncoming.map((c) => c.checkId)).toEqual(['c1', 'c2']);
  });

  it('has no result until one is set', () => {
    expect(store().contractCheckResults.c1?.DEV1).toBeUndefined();
    store().setContractCheckResult('c1', 'DEV1', {
      outcome: 'answered',
      rows: [{ index: 0, verdict: 'pass' }],
    });
    expect(store().contractCheckResults.c1.DEV1.outcome).toBe('answered');
  });

  it('expiry removes the incoming check and marks an unanswered result', () => {
    store().addIncomingContractCheck(check());
    store().trackContractCheck('c1', 'DEV1');
    store().expireContractCheck('c1');
    expect(store().contractChecksIncoming).toEqual([]);
    expect(store().contractCheckResults.c1.DEV1.outcome).toBe('expired');
  });

  it('expiry keeps an answer', () => {
    store().trackContractCheck('c1', 'DEV1');
    store().setContractCheckResult('c1', 'DEV1', { outcome: 'declined', rows: [] });
    store().expireContractCheck('c1');
    expect(store().contractCheckResults.c1.DEV1.outcome).toBe('declined');
  });

  it('opens and closes the dialog', () => {
    store().openContractCheckDialog('c1');
    expect(store().dialogContractCheckId).toBe('c1');
    store().closeContractCheckDialog();
    expect(store().dialogContractCheckId).toBeNull();
  });
});
