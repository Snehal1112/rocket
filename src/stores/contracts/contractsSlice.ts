import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type { Contract, ContractsState } from '@/types/contracts';
import { type ContractsActions, contractsActions } from './contractsActions';

// Re-export so SP5+ components can import Contract from the store layer
export type { Contract };

export interface ContractsStore extends ContractsState, ContractsActions {
  // Sync mutations
  patchContract: (id: string, patch: Partial<Contract>) => void;
  upsert: (contract: Contract) => void;
  setHovered: (id: string | null) => void;
}

export const useContractsStore = create<ContractsStore>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    byId: {},
    byCollection: {},
    hoveredId: null,
    loading: false,
    error: null,

    // Async actions (IPC-backed)
    ...contractsActions(set, get),

    // Sync mutations
    patchContract: (id, patch) =>
      set((state) => ({
        byId: {
          ...state.byId,
          [id]: { ...state.byId[id], ...patch, updatedAt: new Date().toISOString() },
        },
      })),

    upsert: (contract) => set((state) => ({ byId: { ...state.byId, [contract.id]: contract } })),

    setHovered: (id) => set({ hoveredId: id }),
  })),
);
