import { beforeEach, describe, expect, it } from 'vitest';
import { useLayoutStore } from '../layout-store';

describe('useLayoutStore', () => {
  beforeEach(() => {
    useLayoutStore.setState({
      requestLayout: 'stacked',
      sidebarWidth: 280,
      isConsoleOpen: false,
      consoleHeight: 280,
    });
  });

  it('defaults to stacked', () => {
    expect(useLayoutStore.getState().requestLayout).toBe('stacked');
  });

  it('setRequestLayout updates to side-by-side', () => {
    useLayoutStore.getState().setRequestLayout('side-by-side');
    expect(useLayoutStore.getState().requestLayout).toBe('side-by-side');
  });

  it('setRequestLayout can toggle back to stacked', () => {
    useLayoutStore.getState().setRequestLayout('side-by-side');
    useLayoutStore.getState().setRequestLayout('stacked');
    expect(useLayoutStore.getState().requestLayout).toBe('stacked');
  });
});

describe('layout-store — sidebar and console state', () => {
  beforeEach(() => {
    useLayoutStore.setState({
      sidebarWidth: 280,
      isConsoleOpen: false,
      consoleHeight: 280,
    });
  });

  it('has correct default values', () => {
    const s = useLayoutStore.getState();
    expect(s.sidebarWidth).toBe(280);
    expect(s.isConsoleOpen).toBe(false);
    expect(s.consoleHeight).toBe(280);
  });

  it('setSidebarWidth updates sidebarWidth', () => {
    useLayoutStore.getState().setSidebarWidth(350);
    expect(useLayoutStore.getState().sidebarWidth).toBe(350);
  });

  it('setConsoleOpen toggles isConsoleOpen', () => {
    useLayoutStore.getState().setConsoleOpen(true);
    expect(useLayoutStore.getState().isConsoleOpen).toBe(true);
    useLayoutStore.getState().setConsoleOpen(false);
    expect(useLayoutStore.getState().isConsoleOpen).toBe(false);
  });

  it('setConsoleHeight updates consoleHeight', () => {
    useLayoutStore.getState().setConsoleHeight(400);
    expect(useLayoutStore.getState().consoleHeight).toBe(400);
  });
});
