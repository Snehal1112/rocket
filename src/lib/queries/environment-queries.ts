import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  deleteEnvironment,
  deleteGlobalEnvironment,
  getGlobalEnvironment,
  getGlobalEnvironmentName,
  getProcessEnvVars,
  listEnvironments,
  listGlobalEnvironments,
  saveEnvironment,
  saveGlobalEnvironment,
  setGlobalEnvironment,
  type Environment,
} from '@/lib/tauri-api';

export const environmentKeys = {
  collection: (collectionName: string) => ['environments', collectionName] as const,
  globalName: ['environments', 'global', 'name'] as const,
  global: (name: string) => ['environments', 'global', name] as const,
  globalList: ['environments', 'global', 'list'] as const,
  process: ['environments', 'process'] as const,
};

export function useEnvironments(collectionName: string | null) {
  return useQuery({
    queryKey: environmentKeys.collection(collectionName ?? ''),
    queryFn: () => listEnvironments(collectionName!),
    enabled: !!collectionName,
  });
}

export function useGlobalEnvironmentName() {
  return useQuery({
    queryKey: environmentKeys.globalName,
    queryFn: getGlobalEnvironmentName,
  });
}

export function useGlobalEnvironment(name: string | null) {
  return useQuery({
    queryKey: environmentKeys.global(name ?? ''),
    queryFn: () => getGlobalEnvironment(name!),
    enabled: !!name,
  });
}

export function useGlobalEnvironments() {
  return useQuery({
    queryKey: environmentKeys.globalList,
    queryFn: listGlobalEnvironments,
  });
}

export function useProcessEnvVars() {
  return useQuery({
    queryKey: environmentKeys.process,
    queryFn: getProcessEnvVars,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

export function useSaveEnvironment(collectionName: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveEnvironment(collectionName!, env),
    onSuccess: () => {
      if (collectionName) {
        qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) });
      }
    },
  });
}

export function useDeleteEnvironment(collectionName: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => deleteEnvironment(collectionName!, name),
    onSuccess: () => {
      if (collectionName) {
        qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) });
      }
    },
  });
}

export function useSetGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string | null) => setGlobalEnvironment(name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalName });
    },
  });
}

export function useSaveGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveGlobalEnvironment(env),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalList });
      qc.invalidateQueries({ queryKey: environmentKeys.globalName });
    },
  });
}

export function useDeleteGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => deleteGlobalEnvironment(name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalList });
    },
  });
}
