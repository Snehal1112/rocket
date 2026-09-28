import { createContext, useContext } from 'react';

// Moves focus to the properties panel root. A panel action that removes the
// focused element calls it, because focus would otherwise fall to the body,
// where React Flow's delete key removes the selected node.
const PanelFocusContext = createContext<() => void>(() => undefined);

export const PanelFocusProvider = PanelFocusContext.Provider;

export function usePanelRefocus(): () => void {
  return useContext(PanelFocusContext);
}

// True when focus has fallen out of the page content, which happens when the
// focused element was removed.
export function focusIsLost(): boolean {
  const active = document.activeElement;
  return !active || active === document.body || !active.isConnected;
}
