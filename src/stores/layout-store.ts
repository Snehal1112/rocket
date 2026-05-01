import { create } from 'zustand';

type RequestLayout = 'stacked' | 'side-by-side';

interface LayoutStore {
  requestLayout: RequestLayout;
  sidebarWidth: number;
  isConsoleOpen: boolean;
  consoleHeight: number;

  setRequestLayout: (dir: RequestLayout) => void;
  setSidebarWidth: (w: number) => void;
  setConsoleOpen: (open: boolean) => void;
  setConsoleHeight: (h: number) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
  requestLayout: 'stacked',
  sidebarWidth: 280,
  isConsoleOpen: false,
  consoleHeight: 280,

  setRequestLayout: (dir) => set({ requestLayout: dir }),
  setSidebarWidth: (w) => set({ sidebarWidth: w }),
  setConsoleOpen: (open) => set({ isConsoleOpen: open }),
  setConsoleHeight: (h) => set({ consoleHeight: h }),
}));
