import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import { clearWorkspaceScopedCaches } from '@/lib/workspace-switch';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import type { AuthState } from '@/types/pane-types';

const auth = { authType: 'bearer' } as AuthState;

describe('clearWorkspaceScopedCaches', () => {
  it('drops in-memory auth of the old workspace', () => {
    useCollectionAuthStore.getState().setCollectionAuth('api', auth);
    useFolderAuthStore.getState().setFolderAuth('api', 'users', auth);
    useFlowAuthStore.getState().setAuth('k', auth);

    clearWorkspaceScopedCaches(new QueryClient());

    expect(useCollectionAuthStore.getState().getCollectionAuth('api')).toBeUndefined();
    expect(useFolderAuthStore.getState().getFolderAuth('api', 'users')).toBeUndefined();
    expect(useFlowAuthStore.getState().getAuth('k')).toBeUndefined();
  });

  it('invalidates collection-keyed queries but leaves global env queries to their reload', () => {
    const qc = new QueryClient();
    qc.setQueryData(['environments', 'api'], []);
    qc.setQueryData(['flows', 'api'], []);
    qc.setQueryData(['assistant', 'reference-tree', 'api'], {});
    qc.setQueryData(['environments', 'global', 'list'], []);

    clearWorkspaceScopedCaches(qc);

    const invalid = (key: readonly unknown[]) => qc.getQueryState(key)?.isInvalidated;
    expect(invalid(['environments', 'api'])).toBe(true);
    expect(invalid(['flows', 'api'])).toBe(true);
    expect(invalid(['assistant', 'reference-tree', 'api'])).toBe(true);
    expect(invalid(['environments', 'global', 'list'])).toBe(false);
  });
});
