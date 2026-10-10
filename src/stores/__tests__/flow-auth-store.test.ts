import { beforeEach, describe, expect, it } from 'vitest';
import { useFlowAuthStore } from '@/stores/flow-auth-store';

describe('flow-auth-store', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
  });

  it('stores and returns an auth state by key', () => {
    useFlowAuthStore.getState().setAuth('k1', { authType: 'bearer', bearer: { token: 't' } });
    expect(useFlowAuthStore.getState().getAuth('k1')).toEqual({
      authType: 'bearer',
      bearer: { token: 't' },
    });
    expect(useFlowAuthStore.getState().getAuth('missing')).toBeUndefined();
  });

  it('keeps the fingerprint of the configuration a token was fetched for next to the state', () => {
    const { setAuth, getEntry } = useFlowAuthStore.getState();
    setAuth('k1', { authType: 'none' }, 'fp-1');
    expect(getEntry('k1')).toEqual({ auth: { authType: 'none' }, fingerprint: 'fp-1' });
    setAuth('k1', { authType: 'none' });
    expect(getEntry('k1')?.fingerprint).toBeUndefined();
    expect(getEntry('missing')).toBeUndefined();
  });

  it('clears one key without touching the others', () => {
    const { setAuth, clearAuth, getAuth } = useFlowAuthStore.getState();
    setAuth('k1', { authType: 'none' });
    setAuth('k2', { authType: 'none' });
    clearAuth('k1');
    expect(getAuth('k1')).toBeUndefined();
    expect(getAuth('k2')).toEqual({ authType: 'none' });
  });

  it('raises the epoch of the flow on every clear, even when nothing was removed', () => {
    const s = useFlowAuthStore.getState();
    const before = s.epochOf('x', 'y');
    s.clearFlow('x', 'y');
    s.clearNode('x', 'y', 'n');
    s.clearAuth(JSON.stringify(['x', 'y', '', '', 'n']));
    expect(useFlowAuthStore.getState().epochOf('x', 'y')).toBe(before + 3);
  });

  it('leaves the epoch of other flows alone', () => {
    const s = useFlowAuthStore.getState();
    const other = s.epochOf('x', 'other');
    s.clearFlow('x', 'y');
    expect(useFlowAuthStore.getState().epochOf('x', 'other')).toBe(other);
    expect(useFlowAuthStore.getState().epochOf('never', 'seen')).toBe(0);
  });

  it('ignores the epoch bump for a key that does not parse', () => {
    const before = useFlowAuthStore.getState().clearEpochs;
    useFlowAuthStore.getState().clearAuth('missing');
    expect(useFlowAuthStore.getState().clearEpochs).toEqual(before);
  });

  it('clearAll empties the entries and changes the epoch of every flow', () => {
    const s = useFlowAuthStore.getState();
    s.setAuth('k1', { authType: 'none' });
    const before = s.epochOf('never', 'seen');
    s.clearAll();
    expect(useFlowAuthStore.getState().auths).toEqual({});
    expect(useFlowAuthStore.getState().epochOf('never', 'seen')).not.toBe(before);
  });

  it('is never persisted to browser storage', () => {
    useFlowAuthStore
      .getState()
      .setAuth('k1', { authType: 'bearer', bearer: { token: 'secret-1' } });
    expect(JSON.stringify({ ...localStorage })).not.toContain('secret-1');
    expect(JSON.stringify({ ...sessionStorage })).not.toContain('secret-1');
  });
});
