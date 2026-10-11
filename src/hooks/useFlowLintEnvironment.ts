import { useMemo, useSyncExternalStore } from 'react';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { getQueryClient } from '@/lib/query-client';
import type { FlowLintEnvironment } from '@/lib/tauri-api';

// Snapshot strings, so React can compare them by value.
const UNKNOWN = '?';
const NONE = '-';
const NAMED = '+';

function readGlobalSnapshot(): string {
  try {
    const name = getQueryClient().getQueryData<string | null>(environmentKeys.globalName);
    if (name === undefined) return UNKNOWN;
    return name ? NAMED + name : NONE;
  } catch {
    return UNKNOWN;
  }
}

function subscribeGlobal(onChange: () => void): () => void {
  try {
    return getQueryClient().getQueryCache().subscribe(onChange);
  } catch {
    return () => undefined;
  }
}

/**
 * The environment names a flow lint reads. The global environment name comes
 * from the query cache and this hook follows it, so a lint runs again when it
 * loads or changes. While it is not loaded, a variable might hide in that
 * scope, so no environment is named and the variable rule stays silent.
 */
export function useFlowLintEnvironment(environmentName: string | null): FlowLintEnvironment {
  const global = useSyncExternalStore(subscribeGlobal, readGlobalSnapshot);
  return useMemo(
    () =>
      global === UNKNOWN
        ? { environmentName: null, globalEnvName: null }
        : { environmentName, globalEnvName: global === NONE ? null : global.slice(1) },
    [environmentName, global],
  );
}
