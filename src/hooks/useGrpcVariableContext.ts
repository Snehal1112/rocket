import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderVariables,
  getRequestVariables,
} from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

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
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();

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
    const envVars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envSecretKeys: secretKeysOf(activeEnv?.variables),
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
      processEnvVars,
      collectionVars,
      folderVars,
      requestVars,
    });
  }, [
    activeEnvId,
    environments,
    globalEnv,
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
