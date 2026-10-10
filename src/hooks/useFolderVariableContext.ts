import { useEffect, useMemo, useState } from 'react';
import { folderChainPath } from '@/lib/folder-settings-convert';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderChainVariables,
} from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

export interface FolderVariableScope {
  /** Scope-aware variables for the editors of a folder's sections. */
  variableContext: Map<string, VariableScopeEntry>;
  environmentName: string | undefined;
}

/**
 * The variable scopes a folder's sections see: process, global and active environment, the
 * collection, the saved folder chain down to this folder, and this folder's unsaved edits.
 * `ownVariables` goes last so an unsaved edit wins over the saved value of the same name.
 */
export function useFolderVariableContext(
  collectionName: string,
  folderPath: string,
  ownVariables: CollectionVariable[],
): FolderVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars(collectionName);

  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);
  const [chainVars, setChainVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    getCollectionSettings(collectionName)
      .then((s) => setCollectionVars(s.variables))
      .catch(() => setCollectionVars([]));
  }, [collectionName]);

  useEffect(() => {
    const path = folderChainPath(folderPath);
    if (!path) {
      setChainVars([]);
      return;
    }
    getFolderChainVariables(collectionName, path)
      .then(setChainVars)
      .catch(() => setChainVars([]));
  }, [collectionName, folderPath]);

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
      folderVars: [...chainVars, ...ownVariables],
    });
  }, [
    activeEnvId,
    environments,
    globalEnv,
    processEnvVars,
    collectionVars,
    chainVars,
    ownVariables,
  ]);

  return { variableContext, environmentName: activeEnvId ?? undefined };
}
