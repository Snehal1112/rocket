import { create } from 'zustand';
import type { ChangeKind, DrawerState } from '@/types/contracts';
import { DEFAULT_FILTERS } from '@/types/contracts';

interface DrawerStore extends DrawerState {
  open: (contractId: string) => void;
  close: () => void;
  clearContract: () => void;
  setSearch: (search: string) => void;
  toggleKind: (kind: ChangeKind) => void;
  toggleBreakingOnly: () => void;
  toggleSinceSigned: () => void;
  resetFilters: () => void;
}

export const useDrawerStore = create<DrawerStore>((set) => ({
  isOpen: false,
  contractId: null,
  filters: { ...DEFAULT_FILTERS },

  open: (contractId) => set({ isOpen: true, contractId, filters: { ...DEFAULT_FILTERS } }),
  close: () => set({ isOpen: false }),
  clearContract: () => set({ contractId: null }),

  setSearch: (search) => set((s) => ({ filters: { ...s.filters, search } })),
  toggleKind: (kind) =>
    set((s) => ({
      filters: {
        ...s.filters,
        kinds: s.filters.kinds.includes(kind)
          ? s.filters.kinds.filter((k) => k !== kind)
          : [...s.filters.kinds, kind],
      },
    })),
  toggleBreakingOnly: () =>
    set((s) => ({ filters: { ...s.filters, breakingOnly: !s.filters.breakingOnly } })),
  toggleSinceSigned: () =>
    set((s) => ({ filters: { ...s.filters, sinceSigned: !s.filters.sinceSigned } })),
  resetFilters: () => set({ filters: { ...DEFAULT_FILTERS } }),
}));
