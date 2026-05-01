import { useCallback } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useSaveEnvironment,
  useSaveGlobalEnvironment,
} from '@/lib/queries/environment-queries';
import type { VariableSource } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

/**
 * Shared hook that saves a variable value edit to the correct store
 * based on the variable's scope. Consolidates the duplicated commit
 * logic from VariableAwareInput and VariableAwareUrlInput.
 *
 * Editable scopes: environment, global, and unresolved (scope === null → env).
 * Read-only scopes: collection, folder, request, process, runtime.
 *
 * Path param edits are handled separately via onPathParamChange prop —
 * this hook does not handle them.
 */
export function useVariableCommit() {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const saveEnvMutation = useSaveEnvironment(activeCollection);
  const saveGlobalMutation = useSaveGlobalEnvironment();

  const commit = useCallback(
    async (varName: string, newValue: string, scope: VariableSource | null) => {
      if (scope === 'global' && globalEnv) {
        const vars = globalEnv.variables.map((v) =>
          v.key === varName ? { ...v, value: newValue } : v,
        );
        if (!globalEnv.variables.some((v) => v.key === varName)) {
          vars.push({ key: varName, value: newValue, enabled: true, secret: false });
        }
        await saveGlobalMutation.mutateAsync({ ...globalEnv, variables: vars });
      } else if ((scope === 'environment' || scope === null) && activeEnvId) {
        const env = environments.find((e) => e.name === activeEnvId);
        if (env) {
          const vars = env.variables.map((v) =>
            v.key === varName ? { ...v, value: newValue } : v,
          );
          if (!env.variables.some((v) => v.key === varName)) {
            vars.push({ key: varName, value: newValue, enabled: true, secret: false });
          }
          await saveEnvMutation.mutateAsync({ ...env, variables: vars });
        }
      }
      // Collection, folder, request, process, runtime: read-only — no-op.
    },
    [activeEnvId, environments, saveEnvMutation, globalEnv, saveGlobalMutation],
  );

  return commit;
}
