import { type QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  deleteEnvironment,
  deleteGlobalEnvironment,
  type Environment,
  getGlobalEnvironment,
  getGlobalEnvironmentName,
  getProcessEnvVars,
  listEnvironments,
  listGlobalEnvironments,
  saveEnvironment,
  saveGlobalEnvironment,
  setGlobalEnvironment,
} from '@/lib/tauri-api';

export const environmentKeys = {
  collection: (collectionName: string) => ['environments', collectionName] as const,
  /** Prefix of every global environment query, to invalidate them all. */
  globalAll: ['environments', 'global'] as const,
  globalName: ['environments', 'global', 'name'] as const,
  // The extra segment keeps an environment named "name" or "list" off the other keys.
  global: (name: string) => ['environments', 'global', 'env', name] as const,
  globalList: ['environments', 'global', 'list'] as const,
  /** Prefix of every process env query, to invalidate them all. */
  processAll: ['environments', 'process'] as const,
  process: (collection: string | null) => ['environments', 'process', collection] as const,
};

export function useEnvironments(collectionName: string | null) {
  return useQuery({
    queryKey: environmentKeys.collection(collectionName ?? ''),
    queryFn: () => listEnvironments(collectionName ?? ''),
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
    queryFn: () => getGlobalEnvironment(name ?? ''),
    enabled: !!name,
  });
}

export function useGlobalEnvironments() {
  return useQuery({
    queryKey: environmentKeys.globalList,
    queryFn: listGlobalEnvironments,
  });
}

/** The host environment for `{{process.env.*}}`. Empty for a collection that is not allowed it. */
export function useProcessEnvVars(collection: string | null | undefined = null) {
  const scope = collection ?? null;
  return useQuery({
    queryKey: environmentKeys.process(scope),
    queryFn: () => getProcessEnvVars(scope),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

export function useSaveEnvironment(collectionName: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveEnvironment(collectionName ?? '', env),
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
    mutationFn: (name: string) => deleteEnvironment(collectionName ?? '', name),
    onSuccess: () => {
      if (collectionName) {
        qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) });
      }
    },
  });
}

// Brings every global environment cache entry up to date. The send paths read the
// cache synchronously, so the entry for the active name is awaited here.
async function refreshGlobalEnvironments(qc: QueryClient, activeName: string | null) {
  await qc.invalidateQueries({ queryKey: environmentKeys.globalAll });
  await fetchActiveGlobalEnvironment(qc, activeName);
}

// A failed refetch must not turn an already persisted change into an error.
async function fetchActiveGlobalEnvironment(qc: QueryClient, activeName: string | null) {
  if (!activeName) return;
  try {
    await qc.fetchQuery({
      queryKey: environmentKeys.global(activeName),
      queryFn: () => getGlobalEnvironment(activeName),
      staleTime: 0,
    });
  } catch (error) {
    console.warn('Could not refresh the global environment', error);
  }
}

// Drops every cached global environment and reads the active one again. A
// workspace switch can keep the same environment name with other values.
export async function reloadGlobalEnvironments(qc: QueryClient) {
  qc.removeQueries({ queryKey: environmentKeys.globalAll });
  let name: string | null = null;
  try {
    name = await qc.fetchQuery({
      queryKey: environmentKeys.globalName,
      queryFn: getGlobalEnvironmentName,
      staleTime: 0,
    });
  } catch (error) {
    console.warn('Could not read the active global environment', error);
  }
  await fetchActiveGlobalEnvironment(qc, name);
}

export function useSetGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string | null) => setGlobalEnvironment(name),
    onSuccess: async (_data, name) => {
      qc.setQueryData(environmentKeys.globalName, name);
      await refreshGlobalEnvironments(qc, name);
    },
  });
}

export function useSaveGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveGlobalEnvironment(env),
    onSuccess: async (_data, env) => {
      // Write the saved value first so a send right after the save never sees the old one.
      qc.setQueryData(environmentKeys.global(env.name), env);
      await refreshGlobalEnvironments(
        qc,
        qc.getQueryData<string | null>(environmentKeys.globalName) ?? null,
      );
    },
  });
}

export function useDeleteGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => deleteGlobalEnvironment(name),
    onSuccess: async (_data, name) => {
      qc.removeQueries({ queryKey: environmentKeys.global(name) });
      // The backend may have cleared the active name, so read it again.
      await qc.invalidateQueries({ queryKey: environmentKeys.globalName });
      const active = qc.getQueryData<string | null>(environmentKeys.globalName) ?? null;
      await refreshGlobalEnvironments(qc, active === name ? null : active);
    },
  });
}
