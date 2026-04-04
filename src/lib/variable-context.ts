import type { CollectionVariable } from '@/lib/tauri-api';

const VAR_REGEX = /\{\{\s*([\w.]+)\s*\}\}/g;

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

// Replace every {{var}} placeholder in template using the provided context.
// Unknown placeholders are left unchanged.
export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => (key in ctx ? ctx[key] : match));
}

// Resolve all values in a string→string map using the provided context.
export function resolveMapWithContext(
  map: Record<string, string>,
  ctx: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(Object.entries(map).map(([k, v]) => [k, resolveWithContext(v, ctx)]));
}
