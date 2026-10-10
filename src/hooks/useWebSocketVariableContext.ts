import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import { type CollectionVariable, getCollectionSettings } from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

/**
 * Variable highlighting for the WebSocket panel: environment, global, process and collection
 * variables. Folder and request-level variables are resolved by the backend at connect time but
 * are not highlighted here yet (the HTTP panel loads them per request).
 */
export function useWebSocketVariableContext(
  collection: string | undefined,
): Map<string, VariableScopeEntry> {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars(collection);
  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    if (!collection) {
      setCollectionVars([]);
      return;
    }
    let cancelled = false;
    getCollectionSettings(collection)
      .then((settings) => {
        if (!cancelled) setCollectionVars(settings.variables);
      })
      .catch(() => {
        if (!cancelled) setCollectionVars([]);
      });
    return () => {
      cancelled = true;
    };
  }, [collection]);

  return useMemo(() => {
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    const envVars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars = globalEnv
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
    });
  }, [activeEnvId, environments, globalEnv, processEnvVars, collectionVars]);
}
