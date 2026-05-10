import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  AttachContractInput,
  Contract,
  ContractChangelog,
  ContractScope,
} from '@/lib/tauri-api';
import { useContractStore } from '../contract-store';

// Mock tauri-api so tests don't touch the Tauri IPC layer.
vi.mock('@/lib/tauri-api', () => ({
  attachContract: vi.fn(),
  listContracts: vi.fn(),
  getContract: vi.fn(),
  deleteContract: vi.fn(),
  getContractChangelog: vi.fn(),
}));

import {
  attachContract as apiAttach,
  deleteContract as apiDelete,
  getContractChangelog as apiGetChangelog,
  listContracts as apiList,
} from '@/lib/tauri-api';

const mockList = vi.mocked(apiList);
const mockAttach = vi.mocked(apiAttach);
const mockDelete = vi.mocked(apiDelete);
const mockGetChangelog = vi.mocked(apiGetChangelog);

const ROOT = '/tmp/workspace/collections/payments';

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

function makeInput(overrides: Partial<AttachContractInput> = {}): AttachContractInput {
  return {
    title: 'Payments API v1',
    provider: { id: 'p1', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'c1', name: 'Platform Team', kind: 'team' }],
    version: 'v1',
    effectiveDate: '2026-01-01',
    expiryDate: null,
    documentPaths: [],
    scope: collectionScope,
    policy: { breakingChangePolicy: 'strict', noticeDays: 30 },
    initialSnapshots: [],
    publishImmediately: false,
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

describe('contract-store', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe('loadContracts', () => {
    it('populates contractsByRoot on success and clears loading', async () => {
      const contract = makeContract();
      mockList.mockResolvedValueOnce([contract]);

      await useContractStore.getState().loadContracts(ROOT);

      expect(mockList).toHaveBeenCalledWith(ROOT);
      expect(useContractStore.getState().contractsByRoot[ROOT]).toEqual([contract]);
      expect(useContractStore.getState().loading).toBe(false);
      expect(useContractStore.getState().error).toBeNull();
    });

    it('sets error and clears loading when the IPC call rejects', async () => {
      mockList.mockRejectedValueOnce(new Error('boom'));

      await useContractStore.getState().loadContracts(ROOT);

      expect(useContractStore.getState().error).toContain('boom');
      expect(useContractStore.getState().loading).toBe(false);
      // contractsByRoot is not touched on failure — no entry written for the root.
      expect(useContractStore.getState().contractsByRoot[ROOT]).toBeUndefined();
    });

    it('sets loading=true while the call is in flight', async () => {
      let resolveList!: (c: Contract[]) => void;
      mockList.mockImplementationOnce(
        () =>
          new Promise((r) => {
            resolveList = r;
          }),
      );

      const pending = useContractStore.getState().loadContracts(ROOT);
      expect(useContractStore.getState().loading).toBe(true);

      resolveList([]);
      await pending;
      expect(useContractStore.getState().loading).toBe(false);
    });
  });

  describe('attachContract', () => {
    it('appends to an existing list for the same root', async () => {
      const existing = makeContract({ id: 'c1' });
      const created = makeContract({ id: 'c2', title: 'New contract' });
      useContractStore.setState({ contractsByRoot: { [ROOT]: [existing] } });
      mockAttach.mockResolvedValueOnce(created);

      const returned = await useContractStore.getState().attachContract(ROOT, makeInput());

      expect(returned).toEqual(created);
      expect(mockAttach).toHaveBeenCalledWith(ROOT, makeInput());
      expect(useContractStore.getState().contractsByRoot[ROOT]).toEqual([existing, created]);
    });

    it('initialises the list when the root has no entries yet', async () => {
      const created = makeContract({ id: 'c1' });
      mockAttach.mockResolvedValueOnce(created);

      await useContractStore.getState().attachContract(ROOT, makeInput());

      expect(useContractStore.getState().contractsByRoot[ROOT]).toEqual([created]);
    });

    it('does not swallow IPC errors', async () => {
      mockAttach.mockRejectedValueOnce(new Error('write failed'));

      await expect(useContractStore.getState().attachContract(ROOT, makeInput())).rejects.toThrow(
        'write failed',
      );
      expect(useContractStore.getState().contractsByRoot[ROOT]).toBeUndefined();
    });
  });

  describe('removeContract', () => {
    it('filters out the contract and drops its changelog entry', async () => {
      const c1 = makeContract({ id: 'c1' });
      const c2 = makeContract({ id: 'c2' });
      const log: ContractChangelog = { contractId: 'c1', entries: [] };
      useContractStore.setState({
        contractsByRoot: { [ROOT]: [c1, c2] },
        changelogs: { c1: log },
      });
      mockDelete.mockResolvedValueOnce(undefined);

      await useContractStore.getState().removeContract(ROOT, 'c1');

      expect(mockDelete).toHaveBeenCalledWith(ROOT, 'c1');
      expect(useContractStore.getState().contractsByRoot[ROOT]).toEqual([c2]);
      expect(useContractStore.getState().changelogs.c1).toBeUndefined();
    });

    it('leaves unrelated changelog entries intact', async () => {
      const c1 = makeContract({ id: 'c1' });
      const c2 = makeContract({ id: 'c2' });
      const otherLog: ContractChangelog = { contractId: 'c2', entries: [] };
      useContractStore.setState({
        contractsByRoot: { [ROOT]: [c1, c2] },
        changelogs: { c1: { contractId: 'c1', entries: [] }, c2: otherLog },
      });
      mockDelete.mockResolvedValueOnce(undefined);

      await useContractStore.getState().removeContract(ROOT, 'c1');

      expect(useContractStore.getState().changelogs.c2).toEqual(otherLog);
    });
  });

  describe('loadChangelog', () => {
    it('stores the changelog keyed by contract id', async () => {
      const log: ContractChangelog = {
        contractId: 'c1',
        entries: [
          {
            timestamp: '2026-04-10T12:00:00Z',
            requestPath: 'auth/login',
            field: 'headers.X-Trace',
            changeType: 'added',
            oldValue: null,
            newValue: 'on',
            isBreaking: false,
          },
        ],
      };
      mockGetChangelog.mockResolvedValueOnce(log);

      await useContractStore.getState().loadChangelog(ROOT, 'c1');

      expect(mockGetChangelog).toHaveBeenCalledWith(ROOT, 'c1');
      expect(useContractStore.getState().changelogs.c1).toEqual(log);
    });
  });

  describe('contractsFor / contractsForScope', () => {
    it('contractsFor returns empty array for an unknown root', () => {
      expect(useContractStore.getState().contractsFor('/nowhere')).toEqual([]);
    });

    it('contractsForScope filters by discriminant and rel_path', () => {
      const all: Contract[] = [
        makeContract({ id: 'col', scope: { type: 'collection' } }),
        makeContract({
          id: 'folder-a',
          scope: { type: 'folder', rel_path: 'auth' },
        }),
        makeContract({
          id: 'folder-b',
          scope: { type: 'folder', rel_path: 'billing' },
        }),
        makeContract({
          id: 'req',
          scope: { type: 'request', rel_path: 'auth/login' },
        }),
      ];
      useContractStore.setState({ contractsByRoot: { [ROOT]: all } });

      const state = useContractStore.getState();
      expect(state.contractsForScope(ROOT, 'collection').map((c) => c.id)).toEqual(['col']);
      expect(state.contractsForScope(ROOT, 'folder', 'auth').map((c) => c.id)).toEqual([
        'folder-a',
      ]);
      expect(state.contractsForScope(ROOT, 'folder', 'billing').map((c) => c.id)).toEqual([
        'folder-b',
      ]);
      expect(state.contractsForScope(ROOT, 'request', 'auth/login').map((c) => c.id)).toEqual([
        'req',
      ]);
      // Unknown rel_path does not match anything.
      expect(state.contractsForScope(ROOT, 'folder', 'missing')).toEqual([]);
    });
  });

  describe('contractStatus', () => {
    // Pin "today" so the 30-day window is deterministic across runs.
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date('2026-04-11T12:00:00Z'));
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('returns active when expiryDate is null', () => {
      const { contractStatus } = useContractStore.getState();
      expect(contractStatus(makeContract({ expiryDate: null }))).toBe('active');
    });

    it('returns expired when expiryDate is before today', () => {
      const { contractStatus } = useContractStore.getState();
      expect(contractStatus(makeContract({ expiryDate: '2026-04-10' }))).toBe('expired');
    });

    it('returns expiring when expiryDate is within 30 days', () => {
      const { contractStatus } = useContractStore.getState();
      // Today + 10 days.
      expect(contractStatus(makeContract({ expiryDate: '2026-04-21' }))).toBe('expiring');
      // Today + 30 days exactly — still inside the window.
      expect(contractStatus(makeContract({ expiryDate: '2026-05-11' }))).toBe('expiring');
    });

    it('returns active when expiryDate is beyond 30 days', () => {
      const { contractStatus } = useContractStore.getState();
      // Today + 31 days.
      expect(contractStatus(makeContract({ expiryDate: '2026-05-12' }))).toBe('active');
    });
  });
});
