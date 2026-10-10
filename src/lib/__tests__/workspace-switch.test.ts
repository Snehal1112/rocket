import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';

const clearPreviews = vi.fn();
vi.mock('@/lib/saved-request-preview', () => ({
  clearSavedRequestPreviewCache: () => clearPreviews(),
}));
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

  it('clears the saved-request previews of flow nodes', () => {
    clearWorkspaceScopedCaches(new QueryClient());
    expect(clearPreviews).toHaveBeenCalled();
  });

  it('removes collection-keyed queries but leaves global env queries to their reload', () => {
    const qc = new QueryClient();
    qc.setQueryData(['environments', 'api'], []);
    qc.setQueryData(['flows', 'api'], []);
    qc.setQueryData(['assistant', 'reference-tree', 'api'], {});
    qc.setQueryData(['environments', 'global', 'list'], []);

    clearWorkspaceScopedCaches(qc);

    expect(qc.getQueryData(['environments', 'api'])).toBeUndefined();
    expect(qc.getQueryData(['flows', 'api'])).toBeUndefined();
    expect(qc.getQueryData(['assistant', 'reference-tree', 'api'])).toBeUndefined();
    expect(qc.getQueryData(['environments', 'global', 'list'])).toEqual([]);
  });
});
