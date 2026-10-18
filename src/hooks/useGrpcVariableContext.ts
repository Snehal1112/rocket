import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironmentName,
  useProcessEnvVars,
  useResolvedEnvironment,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderVariables,
  getRequestVariables,
  type ResolvedVariable,
} from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

function enabledMap(vars: ResolvedVariable[] | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  for (const v of vars ?? []) if (v.enabled) out[v.key] = v.value;
  return out;
}

export interface GrpcVariableScope {
  /** Scope-aware variables for the editors, so `{{name}}` highlights like it does for HTTP. */
  variableContext: Map<string, VariableScopeEntry>;
  /** Sent with a call so the backend resolves the same environment. */
  environmentName?: string;
  globalEnvName?: string;
}

/**
 * The variable scopes a gRPC tab sees: process, global and active environment,
 * the collection, the folder chain and the request itself.
 */
export function useGrpcVariableContext(
  source: { collection: string; path: string } | undefined,
): GrpcVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  // Flattened by the backend, so a variable inherited through `extends` is included.
  const { data: resolvedEnv } = useResolvedEnvironment(activeCollection, activeEnvId);
  const { data: resolvedGlobal } = useResolvedEnvironment(null, globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars(source?.collection);

  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);
  const [folderVars, setFolderVars] = useState<CollectionVariable[]>([]);
  const [requestVars, setRequestVars] = useState<CollectionVariable[]>([]);

  const collection = source?.collection;
  const path = source?.path;

  useEffect(() => {
    if (!collection) {
      setCollectionVars([]);
      return;
    }
    getCollectionSettings(collection)
      .then((s) => setCollectionVars(s.variables))
      .catch(() => setCollectionVars([]));
  }, [collection]);

  useEffect(() => {
    if (!collection || !path) {
      setFolderVars([]);
      setRequestVars([]);
      return;
    }
    const folderPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
    getRequestVariables(collection, path)
      .then(setRequestVars)
      .catch(() => setRequestVars([]));
    if (folderPath) {
      getFolderVariables(collection, folderPath)
        .then(setFolderVars)
        .catch(() => setFolderVars([]));
    } else {
      setFolderVars([]);
    }
  }, [collection, path]);

  const variableContext = useMemo(() => {
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    return buildScopedContext({
      envVars: enabledMap(resolvedEnv?.variables),
      envSecretKeys: secretKeysOf(resolvedEnv?.variables),
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars: enabledMap(resolvedGlobal?.variables),
      globalSecretKeys: secretKeysOf(resolvedGlobal?.variables),
      processEnvVars,
      collectionVars,
      folderVars,
      requestVars,
    });
  }, [
    activeEnvId,
    environments,
    resolvedEnv,
    resolvedGlobal,
    processEnvVars,
    collectionVars,
    folderVars,
    requestVars,
  ]);

  return {
    variableContext,
    environmentName: activeEnvId ?? undefined,
    globalEnvName: globalEnvName ?? undefined,
  };
}
