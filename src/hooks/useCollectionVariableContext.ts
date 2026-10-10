import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import { type CollectionVariable, type Environment, getCollectionSettings } from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

// Stable defaults while a query has no data, so the memoized values below are
// not rebuilt on every render.
const NO_ENVIRONMENTS: Environment[] = [];
const NO_PROCESS_ENV: Record<string, string> = {};

/** The variable scope of one collection, as the flow editors need it. */
export interface CollectionVariableScope {
  /** Scope-aware map for highlighting, autocomplete, popovers and hover. */
  variableContext: Map<string, VariableScopeEntry>;
  /** Enabled variables of the active environment, key to real value. For resolution only. */
  envVars: Record<string, string>;
  /** Enabled variables of the active global environment, key to real value. */
  globalVars: Record<string, string>;
  collectionVars: CollectionVariable[];
  processEnvVars: Record<string, string>;
  /** The active environment's name, or null. */
  activeEnvId: string | null;
  /** The active global environment's name, or null. */
  globalEnvName: string | null;
}

/**
 * The variables a flow sees for `collection`: process, global, the collection's own,
 * and the active environment looked up in that collection (not in the env store's
 * active collection, which can be a different one). This is the same layering the
 * pre-run step uses, so the editor and the run agree.
 */
export function useCollectionVariableContext(collection: string): CollectionVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: environments = NO_ENVIRONMENTS } = useEnvironments(collection);
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = NO_PROCESS_ENV } = useProcessEnvVars(collection);
  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    let cancelled = false;
    getCollectionSettings(collection)
      .then((s) => {
        if (!cancelled) setCollectionVars(s.variables);
      })
      .catch(() => {
        if (!cancelled) setCollectionVars([]);
      });
    return () => {
      cancelled = true;
    };
  }, [collection]);

  const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
  const envVars = useMemo(() => {
    const vars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) vars[v.key] = v.value;
    return vars;
  }, [activeEnv]);
  const globalVars = useMemo<Record<string, string>>(
    () =>
      globalEnv
        ? Object.fromEntries(
            globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
          )
        : {},
    [globalEnv],
  );

  const variableContext = useMemo(
    () =>
      buildScopedContext({
        envVars,
        envSecretKeys: secretKeysOf(activeEnv?.variables),
        envLabel: activeEnvId ?? undefined,
        externalSecrets: activeEnv?.externalSecrets,
        globalVars,
        globalSecretKeys: secretKeysOf(globalEnv?.variables),
        processEnvVars,
        collectionVars,
      }),
    [activeEnvId, activeEnv, envVars, globalEnv, globalVars, processEnvVars, collectionVars],
  );

  return {
    variableContext,
    envVars,
    globalVars,
    collectionVars,
    processEnvVars,
    activeEnvId,
    globalEnvName,
  };
}
