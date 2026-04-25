import { create } from 'zustand';

type RequestLayout = 'stacked' | 'side-by-side';

interface LayoutStore {
  requestLayout: RequestLayout;
  setRequestLayout: (dir: RequestLayout) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
  requestLayout: 'stacked',
  setRequestLayout: (dir) => set({ requestLayout: dir }),
}));
