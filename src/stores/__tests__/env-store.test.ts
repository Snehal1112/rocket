import { beforeEach, describe, expect, it } from 'vitest';
import { useEnvStore } from '../env-store';

describe('useEnvStore — UI state', () => {
  beforeEach(() => {
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
    localStorage.clear();
  });

  it('defaults to null active env and collection', () => {
    const s = useEnvStore.getState();
    expect(s.activeEnvId).toBeNull();
    expect(s.activeCollection).toBeNull();
  });

  it('setActiveCollection updates activeCollection', () => {
    useEnvStore.getState().setActiveCollection('my-collection');
    expect(useEnvStore.getState().activeCollection).toBe('my-collection');
  });

  it('setActiveEnvId updates activeEnvId', () => {
    useEnvStore.setState({ activeCollection: 'col' });
    useEnvStore.getState().setActiveEnvId('dev');
    expect(useEnvStore.getState().activeEnvId).toBe('dev');
  });

  it('setActiveEnvId persists to localStorage when collection is set', () => {
    useEnvStore.setState({ activeCollection: 'col' });
    useEnvStore.getState().setActiveEnvId('staging');
    expect(localStorage.getItem('rocket-api:active-env:col')).toBe('staging');
  });

  it('setActiveEnvId removes localStorage entry when set to null', () => {
    useEnvStore.setState({ activeCollection: 'col' });
    localStorage.setItem('rocket-api:active-env:col', 'staging');
    useEnvStore.getState().setActiveEnvId(null);
    expect(localStorage.getItem('rocket-api:active-env:col')).toBeNull();
  });
});
