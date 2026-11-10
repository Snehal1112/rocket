import { create } from 'zustand';
import type { ContractChange } from '@/lib/tauri-api';

/** One teammate asked in a run. The name is kept so the row survives the node going offline. */
export interface ContractCheckTarget {
  checkId: string;
  deviceId: string;
  /** Peer text. Render it as plain text only. */
  name: string;
}

/** The last check the provider started for one contract. */
export interface ContractCheckRun {
  changes: ContractChange[];
  targets: ContractCheckTarget[];
}

interface ContractCheckRunsState {
  runs: Record<string, ContractCheckRun>;
  setRun: (key: string, run: ContractCheckRun) => void;
  reset: () => void;
}

export const contractCheckRunKey = (collectionId: string, contractId: string): string =>
  `${collectionId}::${contractId}`;

/** Keeps the provider's rows outside the view, so closing and reopening it loses nothing. */
export const useContractCheckRunsStore = create<ContractCheckRunsState>((set) => ({
  runs: {},
  setRun: (key, run) => set((s) => ({ runs: { ...s.runs, [key]: run } })),
  reset: () => set({ runs: {} }),
}));
