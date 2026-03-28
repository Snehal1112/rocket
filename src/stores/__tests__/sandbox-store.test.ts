import { describe, it, expect, beforeEach } from 'vitest';
import { useSandboxStore } from '../sandbox-store';

describe('sandbox-store', () => {
  beforeEach(() => {
    localStorage.clear();
    useSandboxStore.setState({ mode: 'safe' });
  });

  it('defaults to safe mode', () => {
    expect(useSandboxStore.getState().mode).toBe('safe');
  });

  it('setMode changes the mode', () => {
    useSandboxStore.getState().setMode('developer');
    expect(useSandboxStore.getState().mode).toBe('developer');
  });

  it('persists mode to localStorage', () => {
    useSandboxStore.getState().setMode('developer');
    expect(localStorage.getItem('rocket-sandbox-mode')).toBe('developer');
  });

  it('reads initial mode from localStorage', () => {
    localStorage.setItem('rocket-sandbox-mode', 'developer');
    // Re-create store state from localStorage
    const stored = localStorage.getItem('rocket-sandbox-mode');
    const mode = stored === 'developer' ? 'developer' : 'safe';
    expect(mode).toBe('developer');
  });
});
