import { beforeEach, describe, expect, it, vi } from 'vitest';
import { contractCheckToastId } from '@/lib/network-contract-check-utils';
import { handleNetworkContractCheck } from '@/lib/network-event-bridge';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { info: vi.fn(), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), listNetworkLinks: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

describe('handleNetworkContractCheck', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  it('stores an offer and shows a toast with a review action', () => {
    handleNetworkContractCheck({
      type: 'offerReceived',
      check_id: 'c1',
      device_id: 'DEV1',
      contract: 'Orders API',
      change_count: 1,
    });
    expect(store().contractChecksIncoming).toHaveLength(1);
    const [text, options] = mocks.toast.info.mock.calls[0];
    expect(text).toBe('"A teammate" wants to check a change to "Orders API" (1 change)');
    expect(options.id).toBe(contractCheckToastId('c1'));
    options.action.onClick();
    expect(store().dialogContractCheckId).toBe('c1');
  });

  it('uses the known device name', () => {
    useNetworkStore.setState({
      nodes: [{ deviceId: 'DEV1', displayName: 'Ben Ortiz' } as never],
    });
    handleNetworkContractCheck({
      type: 'offerReceived',
      check_id: 'c1',
      device_id: 'DEV1',
      contract: 'Orders API',
      change_count: 4,
    });
    expect(mocks.toast.info.mock.calls[0][0]).toBe(
      '"Ben Ortiz" wants to check a change to "Orders API" (4 changes)',
    );
  });

  it('stores a result', () => {
    handleNetworkContractCheck({
      type: 'resultReceived',
      check_id: 'c1',
      device_id: 'DEV1',
      outcome: 'answered',
      rows: [{ index: 0, verdict: 'break' }],
    });
    expect(store().contractCheckResults.c1.DEV1.rows).toEqual([{ index: 0, verdict: 'break' }]);
  });

  it('clears on expiry', () => {
    store().addIncomingContractCheck({
      checkId: 'c1',
      deviceId: 'DEV1',
      contract: 'x',
      changeCount: 1,
    });
    handleNetworkContractCheck({ type: 'expired', check_id: 'c1' });
    expect(store().contractChecksIncoming).toEqual([]);
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(contractCheckToastId('c1'));
  });
});
