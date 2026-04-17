import type { CollectionVariable } from '@/lib/tauri-api';

// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export type VariableSource =
  | 'runtime'
  | 'request'
  | 'folder'
  | 'environment'
  | 'collection'
  | 'global'
  | 'process';

export interface VariableScopeEntry {
  value: string;
  source: VariableSource;
  label: string; // Human-readable label, e.g. "Staging", "Collection".
  secret: boolean; // True means show ●●●● in tooltip instead of value.
}

// Builds a resolve function that substitutes {{var}} with merged variables.
export function buildResolver(
  envVariables: Record<string, string>,
  collectionVariables?: Record<string, string>,
): (text: string) => string {
  return (text: string) =>
    text.replace(VAR_REGEX, (match, key) => {
      if (key in envVariables) return envVariables[key];
      if (collectionVariables && key in collectionVariables) return collectionVariables[key];
      return match;
    });
}

// Builds a scope-aware variable map for the overlay UI.
// Lower-priority scopes are written first; higher-priority scopes overwrite them.
// Priority (lowest → highest): process → global → collection → env → folder → request → runtime.
export function buildScopedContext(params: {
  runtimeVars?: Record<string, string>;
  requestVars?: CollectionVariable[];
  folderVars?: CollectionVariable[];
  collectionVars?: CollectionVariable[];
  envVars?: Record<string, string>;
  envLabel?: string;
  globalVars?: Record<string, string>;
  processEnvVars?: Record<string, string>;
}): Map<string, VariableScopeEntry> {
  const out = new Map<string, VariableScopeEntry>();
  const add = (k: string, v: string, source: VariableSource, label: string, secret = false) =>
    out.set(k, { value: v, source, label, secret });

  for (const [k, v] of Object.entries(params.processEnvVars ?? {}))
    add(`process.env.${k}`, v, 'process', 'Process Env');
  for (const [k, v] of Object.entries(params.globalVars ?? {})) add(k, v, 'global', 'Global');
  for (const v of (params.collectionVars ?? []).filter((v) => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'collection', 'Collection', v.secret);
  }
  for (const [k, v] of Object.entries(params.envVars ?? {}))
    add(k, v, 'environment', params.envLabel ?? 'Environment');
  for (const v of (params.folderVars ?? []).filter((v) => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'folder', 'Folder', v.secret);
  }
  for (const v of (params.requestVars ?? []).filter((v) => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'request', 'Request', v.secret);
  }
  for (const [k, v] of Object.entries(params.runtimeVars ?? {})) add(k, v, 'runtime', 'Runtime');
  return out;
}

// Returns the Tailwind class string for a variable source badge.
export function sourceBadgeClass(source: VariableSource): string {
  const classes: Record<VariableSource, string> = {
    runtime: 'bg-orange-500/15 text-orange-700 dark:text-orange-400',
    request: 'bg-purple-500/15 text-purple-700 dark:text-purple-400',
    folder: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
    environment: 'bg-primary/15 text-primary',
    collection: 'bg-blue-500/15 text-blue-700 dark:text-blue-400',
    global: 'bg-teal-500/15 text-teal-700 dark:text-teal-400',
    process: 'bg-muted text-muted-foreground',
  };
  return classes[source];
}
