import { describe, it, expect, beforeEach } from 'vitest';
import { useLayoutStore } from '../layout-store';

describe('useLayoutStore', () => {
  beforeEach(() => {
    useLayoutStore.setState({ requestLayout: 'stacked' });
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
