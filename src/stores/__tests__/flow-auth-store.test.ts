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

  it('clears one key without touching the others', () => {
    const { setAuth, clearAuth, getAuth } = useFlowAuthStore.getState();
    setAuth('k1', { authType: 'none' });
    setAuth('k2', { authType: 'none' });
    clearAuth('k1');
    expect(getAuth('k1')).toBeUndefined();
    expect(getAuth('k2')).toEqual({ authType: 'none' });
  });

  it('is never persisted to browser storage', () => {
    useFlowAuthStore
      .getState()
      .setAuth('k1', { authType: 'bearer', bearer: { token: 'secret-1' } });
    expect(JSON.stringify({ ...localStorage })).not.toContain('secret-1');
    expect(JSON.stringify({ ...sessionStorage })).not.toContain('secret-1');
  });
});
