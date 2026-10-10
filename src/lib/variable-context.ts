import type { CollectionVariable } from '@/lib/tauri-api';
import { generateDynamicVar } from './dynamic-vars';

const VAR_REGEX = /\{\{\s*([$\w.-]+)\s*\}\}/g;

// Convert a CollectionVariable array into a plain key→value map, skipping
// disabled entries and falling back to initialValue when value is empty.
function varsToMap(vars: CollectionVariable[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const v of vars) {
    if (!v.enabled || !v.key) continue;
    const val = v.value || v.initialValue || '';
    if (val) out[v.key] = val;
  }
  return out;
}

// Build a flat variable context by merging all 7 scopes in priority order.
// Later assignments win — highest-priority scope is applied last.
export function buildVariableContext(params: {
  runtimeVars?: Record<string, string>;
  requestVars?: CollectionVariable[];
  folderVars?: CollectionVariable[]; // chain-merged by the backend
  collectionVars?: CollectionVariable[];
  envVars?: Record<string, string>;
  globalVars?: Record<string, string>;
  processEnvVars?: Record<string, string>;
}): Record<string, string> {
  const ctx: Record<string, string> = {};

  // Lowest priority first — each layer overwrites on collision.
  for (const [k, v] of Object.entries(params.processEnvVars ?? {})) ctx[`process.env.${k}`] = v;
  Object.assign(ctx, params.globalVars ?? {});
  Object.assign(ctx, varsToMap(params.collectionVars ?? []));
  Object.assign(ctx, params.envVars ?? {}); // env beats collection
  Object.assign(ctx, varsToMap(params.folderVars ?? []));
  Object.assign(ctx, varsToMap(params.requestVars ?? []));
  Object.assign(ctx, params.runtimeVars ?? {}); // runtime wins all

  return ctx;
}

const SET_VAR_REGEX = /\brok\.setVar\(\s*(['"`])([^'"`\n]+)\1/g;

// Names a script sets with a literal rok.setVar('name', ...) call. A name built at run
// time cannot be seen here.
export function scriptRuntimeVarNames(script: string | undefined): Set<string> {
  const names = new Set<string>();
  if (!script) return names;
  for (const match of script.matchAll(SET_VAR_REGEX)) names.add(match[2].trim());
  return names;
}

// The context without the names the request's pre-request script sets. Their placeholders
// then reach the backend as written, and the backend fills them with the runtime value
// after the script ran, the same way it does for runner and Flow sends.
export function withoutScriptRuntimeVars(
  ctx: Record<string, string>,
  script: string | undefined,
): Record<string, string> {
  const names = scriptRuntimeVarNames(script);
  if (names.size === 0) return ctx;
  return Object.fromEntries(Object.entries(ctx).filter(([key]) => !names.has(key)));
}

// Replace every {{var}} placeholder in template using the provided context.
// Unknown placeholders are left unchanged.
export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => {
    if (key.startsWith('$')) {
      return generateDynamicVar(key.slice(1)) ?? match;
    }
    return key in ctx ? ctx[key] : match;
  });
}

// Resolve all values in a string→string map using the provided context.
export function resolveMapWithContext(
  map: Record<string, string>,
  ctx: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(Object.entries(map).map(([k, v]) => [k, resolveWithContext(v, ctx)]));
}
