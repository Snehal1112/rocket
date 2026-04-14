import { create } from 'zustand';
import {
  type AttachContractInput,
  attachContract as apiAttach,
  deleteContract as apiDelete,
  getContractChangelog as apiGetChangelog,
  listContracts as apiList,
  updateContract as apiUpdate,
  type Contract,
  type ContractChangelog,
  type UpdateContractInput,
} from '@/lib/tauri-api';

export type ContractStatus = 'active' | 'expiring' | 'expired';

interface ContractState {
  /** All contracts for the currently loaded collection, keyed by collection root. */
  contractsByRoot: Record<string, Contract[]>;
  /** Changelogs keyed by contract id. */
  changelogs: Record<string, ContractChangelog>;
  /** Last error surfaced by a store action. */
  error: string | null;
  /** True while a load or mutation is in flight. */
  loading: boolean;

  loadContracts: (collectionRoot: string) => Promise<void>;
  attachContract: (collectionRoot: string, input: AttachContractInput) => Promise<Contract>;
  updateContract: (collectionRoot: string, input: UpdateContractInput) => Promise<Contract>;
  removeContract: (collectionRoot: string, contractId: string) => Promise<void>;
  loadChangelog: (collectionRoot: string, contractId: string) => Promise<void>;

  /** Contracts for a specific collection root (stable identity when unchanged). */
  contractsFor: (collectionRoot: string) => Contract[];
  /**
   * Contracts applicable to a given scope under a collection root.
   * Today only `collection` scope is supported in the UI; folder/request
   * scopes are wire-compatible but not yet surfaced.
   */
  contractsForScope: (
    collectionRoot: string,
    scopeType: 'collection' | 'folder' | 'request',
    relPath?: string,
  ) => Contract[];
  /** Derive status from expiry date relative to today. */
  contractStatus: (contract: Contract) => ContractStatus;
}

function computeStatus(contract: Contract): ContractStatus {
  if (!contract.expiryDate) return 'active';
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const exp = new Date(contract.expiryDate);
  exp.setHours(0, 0, 0, 0);
  if (exp.getTime() < today.getTime()) return 'expired';
  const msPerDay = 1000 * 60 * 60 * 24;
  const diffDays = (exp.getTime() - today.getTime()) / msPerDay;
  if (diffDays <= 30) return 'expiring';
  return 'active';
}

export const useContractStore = create<ContractState>((set, get) => ({
  contractsByRoot: {},
  changelogs: {},
  error: null,
  loading: false,

  loadContracts: async (collectionRoot) => {
    set({ loading: true, error: null });
    try {
      const contracts = await apiList(collectionRoot);
      set((s) => ({
        contractsByRoot: { ...s.contractsByRoot, [collectionRoot]: contracts },
        loading: false,
      }));
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  attachContract: async (collectionRoot, input) => {
    const contract = await apiAttach(collectionRoot, input);
    set((s) => {
      const existing = s.contractsByRoot[collectionRoot] ?? [];
      return {
        contractsByRoot: {
          ...s.contractsByRoot,
          [collectionRoot]: [...existing, contract],
        },
      };
    });
    return contract;
  },

  updateContract: async (collectionRoot, input) => {
    const contract = await apiUpdate(collectionRoot, input);
    set((s) => {
      const existing = s.contractsByRoot[collectionRoot] ?? [];
      return {
        contractsByRoot: {
          ...s.contractsByRoot,
          [collectionRoot]: existing.map((c) => (c.id === contract.id ? contract : c)),
        },
      };
    });
    return contract;
  },

  removeContract: async (collectionRoot, contractId) => {
    await apiDelete(collectionRoot, contractId);
    set((s) => {
      const existing = s.contractsByRoot[collectionRoot] ?? [];
      const { [contractId]: _removed, ...remainingChangelogs } = s.changelogs;
      return {
        contractsByRoot: {
          ...s.contractsByRoot,
          [collectionRoot]: existing.filter((c) => c.id !== contractId),
        },
        changelogs: remainingChangelogs,
      };
    });
  },

  loadChangelog: async (collectionRoot, contractId) => {
    const log = await apiGetChangelog(collectionRoot, contractId);
    set((s) => ({
      changelogs: { ...s.changelogs, [contractId]: log },
    }));
  },

  contractsFor: (collectionRoot) => get().contractsByRoot[collectionRoot] ?? [],

  contractsForScope: (collectionRoot, scopeType, relPath) => {
    const all = get().contractsByRoot[collectionRoot] ?? [];
    return all.filter((c) => {
      if (scopeType === 'collection') return c.scope.type === 'collection';
      if (scopeType === 'folder') {
        return c.scope.type === 'folder' && c.scope.rel_path === relPath;
      }
      if (scopeType === 'request') {
        return c.scope.type === 'request' && c.scope.rel_path === relPath;
      }
      return false;
    });
  },

  contractStatus: computeStatus,
}));
