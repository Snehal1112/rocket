import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  closeWorkspace,
  createWorkspace,
  deleteWorkspace,
  getActiveWorkspace,
  getMultiWorkspaceMode,
  getWorkspaceConfig,
  listWorkspaces,
  openWorkspaceFromDisk,
  pinWorkspace,
  type RequestGuardPolicy,
  renameWorkspace,
  setMultiWorkspaceMode,
  switchWorkspace,
  unpinWorkspace,
  updateRequestGuardPolicy,
  updateWorkspaceDescription,
} from '@/lib/tauri-api';

export const workspaceKeys = {
  all: ['workspaces'] as const,
  active: ['workspaces', 'active'] as const,
  multiMode: ['workspaces', 'multiMode'] as const,
  config: (id: string) => ['workspaces', id, 'config'] as const,
};

export function useWorkspaces() {
  return useQuery({
    queryKey: workspaceKeys.all,
    queryFn: listWorkspaces,
  });
}

export function useActiveWorkspace() {
  return useQuery({
    queryKey: workspaceKeys.active,
    queryFn: getActiveWorkspace,
  });
}

export function useMultiWorkspaceMode() {
  return useQuery({
    queryKey: workspaceKeys.multiMode,
    queryFn: getMultiWorkspaceMode,
  });
}

export function useWorkspaceConfig(workspaceId: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.config(workspaceId ?? ''),
    queryFn: () => getWorkspaceConfig(workspaceId as string),
    enabled: Boolean(workspaceId),
  });
}

export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, path }: { name: string; path: string }) => createWorkspace(name, path),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useSwitchWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => switchWorkspace(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: workspaceKeys.all });
      qc.invalidateQueries({ queryKey: workspaceKeys.active });
    },
  });
}

export function useRenameWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, newName }: { id: string; newName: string }) => renameWorkspace(id, newName),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useCloseWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => closeWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useDeleteWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function usePinWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => pinWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useUpdateRequestGuardPolicy(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (policy: RequestGuardPolicy) =>
      updateRequestGuardPolicy(workspaceId as string, policy),
    onSuccess: () => {
      if (workspaceId) qc.invalidateQueries({ queryKey: workspaceKeys.config(workspaceId) });
    },
  });
}

export function useUnpinWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unpinWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useUpdateWorkspaceDescription() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, description }: { id: string; description: string | null }) =>
      updateWorkspaceDescription(id, description),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useOpenWorkspaceFromDisk() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => openWorkspaceFromDisk(path),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useSetMultiWorkspaceMode() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => setMultiWorkspaceMode(enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.multiMode }),
  });
}
