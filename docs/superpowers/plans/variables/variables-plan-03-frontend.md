# Plan 3 — Frontend Store + Resolution Pipeline

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Plan 2  
**Spec:** `docs/superpowers/specs/variables-design.md`

Covers the frontend side of all three critical gaps:
- **C1** — `getFolderChainVariables` replaces single-folder fetch
- **C2** — `initialValue?` in TypeScript type + `varsToMap` fallback
- **C3** — `resolveBody()` (form fields individually) + `resolveAuthFields()` (all auth types)

**Goal:** Wire all 7 variable scopes into `buildVariableContext`. Update `execute-request.ts` to resolve every request field correctly, including form body fields and header keys.

---

## File Map

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Add 5 new wrappers |
| `src/types/index.ts` | Add `initialValue?` to `CollectionVariable` |
| `src/store/env-store.ts` | Add global env + process env state |
| `src/lib/variable-context.ts` | New — `buildVariableContext` |
| `src/lib/__tests__/variable-context.test.ts` | New — 9 tests |
| `src/lib/execute-request.ts` | Full resolution pipeline |

---

## Chunk 1: Types + API bridge

### Task 1: Update CollectionVariable type + add wrappers

- [ ] **Step 1: Add initialValue to type**

In `src/types/index.ts`:
```ts
export interface CollectionVariable {
  key:           string
  value:         string
  initialValue?: string   // fallback when value empty; shared default in Git
  enabled:       boolean
  secret:        boolean
}
```

- [ ] **Step 2: Add new Tauri API wrappers**

```ts
// Global env (selection pointer)
export const getGlobalEnvironmentName = () =>
  invoke<string | null>('get_global_environment_name');
export const setGlobalEnvironment = (name: string | null) =>
  invoke<void>('set_global_environment', { name });

// Process env
export const getProcessEnvVars = () =>
  invoke<Record<string, string>>('get_process_env_vars');

// Folder variables — takes request_path; server walks full parent chain
export const getFolderChainVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_folder_chain_variables', { collection, requestPath });
export const saveFolderVariables = (collection: string, folderPath: string, variables: CollectionVariable[]) =>
  invoke<void>('save_folder_variables', { collection, folderPath, variables });

// Request variables
export const getRequestVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_request_variables', { collection, requestPath });
export const saveRequestVariables = (collection: string, requestPath: string, variables: CollectionVariable[]) =>
  invoke<void>('save_request_variables', { collection, requestPath, variables });
```

- [ ] **Step 3: Commit**

```bash
git add src/types/index.ts src/lib/tauri-api.ts
git commit -m "feat(api): add global env, process env, folder chain, request var wrappers"
```

---

## Chunk 2: useEnvStore global env + process env

### Task 2: Add global env + process env to store

- [ ] **Step 1: Add state + actions**

```ts
// State
globalEnvName:   string | null   // = null
globalEnv:       Environment | null  // = null
processEnvVars:  Record<string, string>  // = {}

// Actions
fetchGlobalEnv: async () => {
  const name = await getGlobalEnvironmentName();
  if (!name) { set({ globalEnvName: null, globalEnv: null }); return; }
  const { activeCollection } = get();
  if (activeCollection) {
    try {
      const env = await getEnvironment(activeCollection, name);
      set({ globalEnvName: name, globalEnv: env }); return;
    } catch {}
  }
  set({ globalEnvName: name, globalEnv: null });
},

setGlobalEnv: async (name: string | null) => {
  await setGlobalEnvironment(name);
  await useEnvStore.getState().fetchGlobalEnv();
},

loadProcessEnvVars: async () => {
  set({ processEnvVars: await getProcessEnvVars() });
},

getGlobalVariables: () => {
  const { globalEnv } = get();
  if (!globalEnv) return {};
  return Object.fromEntries(
    globalEnv.variables.filter(v => v.enabled).map(v => [v.key, v.value])
  );
},
```

- [ ] **Step 2: Tests**

```ts
it('getGlobalVariables returns enabled vars only', () => {
  useEnvStore.setState({ globalEnv: {
    name: 'shared', variables: [
      { key: 'A', value: 'a', enabled: true },
      { key: 'B', value: 'b', enabled: false },
    ]
  }});
  const vars = useEnvStore.getState().getGlobalVariables();
  expect(vars['A']).toBe('a');
  expect(vars['B']).toBeUndefined();
});
it('fetchGlobalEnv null clears state', async () => {
  vi.mocked(getGlobalEnvironmentName).mockResolvedValue(null);
  await useEnvStore.getState().fetchGlobalEnv();
  expect(useEnvStore.getState().globalEnv).toBeNull();
});
```

- [ ] **Step 3: Commit**

```bash
git add src/store/env-store.ts
git commit -m "feat(store): add global env + process env to useEnvStore"
```

---

## Chunk 3: buildVariableContext

### Task 3: Create variable-context.ts

- [ ] **Step 1: Write tests**

```ts
// src/lib/__tests__/variable-context.test.ts
import { buildVariableContext, resolveWithContext } from '../variable-context';

const cv = (key: string, value: string, initialValue?: string) =>
  ({ key, value, initialValue, enabled: true, secret: false });

describe('buildVariableContext', () => {
  it('env beats collection', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','col')], envVars: { k:'env' } })['k']).toBe('env'));
  it('folder beats env', () =>
    expect(buildVariableContext({ folderVars: [cv('k','folder')], envVars: { k:'env' } })['k']).toBe('folder'));
  it('request beats folder', () =>
    expect(buildVariableContext({ requestVars: [cv('k','req')], folderVars: [cv('k','folder')] })['k']).toBe('req'));
  it('runtime beats request', () =>
    expect(buildVariableContext({ runtimeVars: { k:'rt' }, requestVars: [cv('k','req')] })['k']).toBe('rt'));
  it('collection beats global', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','col')], globalVars: { k:'global' } })['k']).toBe('col'));
  it('env beats global', () =>
    expect(buildVariableContext({ envVars: { k:'env' }, globalVars: { k:'global' } })['k']).toBe('env'));
  it('process.env uses dotted key', () => {
    const ctx = buildVariableContext({ processEnvVars: { API:'secret' } });
    expect(ctx['API']).toBeUndefined();
    expect(ctx['process.env.API']).toBe('secret');
  });
  it('initialValue fallback when value empty', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','','default')] })['k']).toBe('default'));
  it('disabled vars excluded', () => {
    const ctx = buildVariableContext({ collectionVars: [{ key:'k', value:'v', enabled:false, secret:false }] });
    expect(ctx['k']).toBeUndefined();
  });
});

describe('resolveWithContext', () => {
  it('resolves vars', () =>
    expect(resolveWithContext('{{a}}/{{b}}', { a:'x', b:'y' })).toBe('x/y'));
  it('leaves unknown as-is', () =>
    expect(resolveWithContext('{{miss}}', {})).toBe('{{miss}}'));
  it('handles whitespace in braces', () =>
    expect(resolveWithContext('{{ key }}', { key:'val' })).toBe('val'));
  it('resolves process.env.KEY', () =>
    expect(resolveWithContext('{{process.env.K}}', { 'process.env.K':'v' })).toBe('v'));
});
```

- [ ] **Step 2: Implement variable-context.ts**

```ts
// src/lib/variable-context.ts
import type { CollectionVariable } from '@/types';

const VAR_REGEX = /\{\{\s*([\w.]+)\s*\}\}/g;

function varsToMap(vars: CollectionVariable[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const v of vars) {
    if (!v.enabled || !v.key) continue;
    const val = v.value || v.initialValue || '';  // C2: initialValue fallback
    if (val) out[v.key] = val;
  }
  return out;
}

export function buildVariableContext(params: {
  runtimeVars?:    Record<string, string>
  requestVars?:    CollectionVariable[]
  folderVars?:     CollectionVariable[]   // already chain-merged by backend
  collectionVars?: CollectionVariable[]
  envVars?:        Record<string, string>
  globalVars?:     Record<string, string>
  processEnvVars?: Record<string, string>
}): Record<string, string> {
  const ctx: Record<string, string> = {};
  // Lowest priority first — each layer overwrites on collision
  for (const [k, v] of Object.entries(params.processEnvVars ?? {}))
    ctx[`process.env.${k}`] = v;
  Object.assign(ctx, params.globalVars ?? {});
  Object.assign(ctx, varsToMap(params.collectionVars ?? []));
  Object.assign(ctx, params.envVars ?? {});           // env beats collection
  Object.assign(ctx, varsToMap(params.folderVars ?? []));
  Object.assign(ctx, varsToMap(params.requestVars ?? []));
  Object.assign(ctx, params.runtimeVars ?? {});       // runtime wins all
  return ctx;
}

export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => key in ctx ? ctx[key] : match);
}

export function resolveMapWithContext(
  map: Record<string, string>, ctx: Record<string, string>
): Record<string, string> {
  return Object.fromEntries(Object.entries(map).map(([k, v]) => [k, resolveWithContext(v, ctx)]));
}
```

- [ ] **Step 3: Run tests + commit**

```bash
npx vitest run src/lib/__tests__/variable-context.test.ts
git add src/lib/variable-context.ts src/lib/__tests__/variable-context.test.ts
git commit -m "feat: buildVariableContext — merges all 7 scopes"
```

---

## Chunk 4: execute-request.ts — full resolution

### Task 4: Replace resolveVariables with full pipeline

- [ ] **Step 1: Build context**

Replace the existing resolution setup in `sendRequest`:

```ts
import { buildVariableContext, resolveWithContext } from '@/lib/variable-context';
import { getFolderChainVariables, getRequestVariables } from '@/lib/tauri-api';

const envStore       = useEnvStore.getState();
const envVars        = envStore.getActiveVariables();
const globalVars     = envStore.getGlobalVariables();
const processEnvVars = envStore.processEnvVars;
const collectionVars = useCollectionsStore.getState().collectionVariables ?? [];
const collection     = found?.tab.source?.collection;

// C1: server walks full parent folder chain — just pass request_path
let folderVars: CollectionVariable[] = [];
if (collection && request.path) {
  try { folderVars = await getFolderChainVariables(collection, request.path); } catch {}
}

let requestVars: CollectionVariable[] = [];
if (collection && request.path) {
  try { requestVars = await getRequestVariables(collection, request.path); } catch {}
}

const ctx = buildVariableContext({ processEnvVars, globalVars, envVars, collectionVars, folderVars, requestVars });
const resolve = (text: string) => resolveWithContext(text, ctx);
```

- [ ] **Step 2: Resolve all fields**

```ts
const resolvedUrl          = resolve(request.url);
const resolvedHeaders      = request.headers
  .filter(h => h.enabled)
  .map(h => ({ ...h, key: resolve(h.key), value: resolve(h.value) }));  // both key AND value
const resolvedQueryParams  = (request.queryParams ?? []).filter(p => p.enabled)
  .map(p => ({ ...p, value: resolve(p.value) }));
const resolvedPathParams   = (request.pathParams ?? []).filter(p => p.enabled)
  .map(p => ({ ...p, value: resolve(p.value) }));
const resolvedAuth         = resolveAuthFields(request.auth, resolve);
const resolvedBody         = resolveBody(request.body, resolve);
```

- [ ] **Step 3: Add resolveAuthFields helper**

```ts
function resolveAuthFields(auth: RequestAuth | undefined, resolve: (s: string) => string) {
  if (!auth) return auth;
  switch (auth.type) {
    case 'bearer':  return { ...auth, token: resolve(auth.token ?? '') };
    case 'basic':   return { ...auth, username: resolve(auth.username ?? ''), password: resolve(auth.password ?? '') };
    case 'apikey':  return { ...auth, key: resolve(auth.key ?? ''), value: resolve(auth.value ?? '') };
    case 'oauth2':  return { ...auth,
      clientId: resolve(auth.clientId ?? ''), clientSecret: resolve(auth.clientSecret ?? ''),
      accessTokenUrl: resolve(auth.accessTokenUrl ?? ''), authorizationUrl: resolve(auth.authorizationUrl ?? ''),
      scope: resolve(auth.scope ?? '') };
    case 'awsv4':   return { ...auth,
      accessKeyId: resolve(auth.accessKeyId ?? ''), secretAccessKey: resolve(auth.secretAccessKey ?? ''),
      region: resolve(auth.region ?? ''), service: resolve(auth.service ?? '') };
    default: return auth;
  }
}
```

- [ ] **Step 4: Add resolveBody helper (C3)**

```ts
function resolveBody(body: RequestBody | undefined, resolve: (s: string) => string) {
  if (!body) return body;
  switch (body.mode) {
    case 'json': case 'xml': case 'text': case 'sparql': case 'graphql':
      return { ...body, content: resolve(body.content ?? '') };
    case 'formUrlEncoded': case 'multipart':
      // C3: resolve each field value individually — NOT as a single string
      return { ...body, params: (body.params ?? []).map(p =>
        p.enabled ? { ...p, value: resolve(p.value) } : p) };
    default: return body;  // binary/file: not interpolated
  }
}
```

- [ ] **Step 5: Tests**

```ts
it('resolves form-urlencoded field values individually', async () => {
  // setup env with TOKEN=abc
  const request = makeRequest({ body: { mode: 'formUrlEncoded',
    params: [{ key: 'token', value: '{{TOKEN}}', enabled: true }] }});
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.body.params[0].value).toBe('abc');
});

it('resolves both header key and value', async () => {
  const request = makeRequest({ headers: [{ key: '{{HNAME}}', value: '{{HVAL}}', enabled: true }] });
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.headers[0].key).toBe('X-My-Header');
  expect(sent.headers[0].value).toBe('my-value');
});

it('uses initialValue when value is empty', async () => {
  useCollectionsStore.setState({ collectionVariables: [
    { key: 'base', value: '', initialValue: 'https://api.example.com', enabled: true, secret: false }
  ]});
  const request = makeRequest({ url: '{{base}}/users' });
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.url).toBe('https://api.example.com/users');
});
```

- [ ] **Step 6: Run full suite + commit**

```bash
npx vitest run && yarn tsc --noEmit
git add src/lib/execute-request.ts
git commit -m "feat: full variable resolution pipeline in execute-request.ts"
```

---
