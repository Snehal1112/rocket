# Plan 4 — URL Overlay + Environment Switcher

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Plan 3  
**Spec:** `docs/superpowers/specs/variables-design.md`

**Goal:** Show scope-coloured badges in `VariableAwareUrlInput` for each variable source. Split `EnvironmentSwitcher` into Global and Environment sections.

---

## File Map

| File | Change |
|---|---|
| `src/lib/url-variables.ts` | Add `VariableSource`, `VariableScopeEntry`, `buildScopedContext`, `sourceBadgeClass` |
| `src/components/request/VariableAwareUrlInput.tsx` | Accept `scopedContext`, render badges |
| `src/components/request/RequestPanel.tsx` | Build + pass `scopedContext` |
| `src/components/layout/EnvironmentSwitcher.tsx` | Two-section dropdown |

---

## Chunk 1: Scope-aware overlay

### Task 1: Update url-variables.ts

- [ ] **Step 1: Add types**

```ts
export type VariableSource = 'runtime'|'request'|'folder'|'environment'|'collection'|'global'|'process'

export interface VariableScopeEntry {
  value:  string
  source: VariableSource
  label:  string    // "Staging", "Collection", "Folder", etc.
  secret: boolean   // true → show ●●●● in tooltip
}

export interface UrlToken {
  type: 'text' | 'variable'
  value: string       // var name without braces, or raw text
  start: number
  end: number
  resolved?: string
  source?: VariableSource
  sourceLabel?: string
  secret?: boolean
}
```

- [ ] **Step 2: Add buildScopedContext**

```ts
export function buildScopedContext(params: {
  runtimeVars?:    Record<string, string>
  requestVars?:    CollectionVariable[]
  folderVars?:     CollectionVariable[]
  collectionVars?: CollectionVariable[]
  envVars?:        Record<string, string>
  envLabel?:       string
  globalVars?:     Record<string, string>
  processEnvVars?: Record<string, string>
}): Map<string, VariableScopeEntry> {
  const out = new Map<string, VariableScopeEntry>();
  const add = (k: string, v: string, source: VariableSource, label: string, secret = false) =>
    out.set(k, { value: v, source, label, secret });

  for (const [k, v] of Object.entries(params.processEnvVars ?? {}))
    add(`process.env.${k}`, v, 'process', 'Process Env');
  for (const [k, v] of Object.entries(params.globalVars ?? {}))
    add(k, v, 'global', 'Global');
  for (const [k, v] of Object.entries(params.envVars ?? {}))
    add(k, v, 'environment', params.envLabel ?? 'Environment');
  for (const v of (params.collectionVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'collection', 'Collection', v.secret);
  }
  for (const v of (params.folderVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'folder', 'Folder', v.secret);
  }
  for (const v of (params.requestVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'request', 'Request', v.secret);
  }
  for (const [k, v] of Object.entries(params.runtimeVars ?? {}))
    add(k, v, 'runtime', 'Runtime');
  return out;
}
```

- [ ] **Step 3: Add sourceBadgeClass**

```ts
export function sourceBadgeClass(source: VariableSource): string {
  const classes: Record<VariableSource, string> = {
    runtime:     'bg-orange-500/15 text-orange-700 dark:text-orange-400',
    request:     'bg-purple-500/15 text-purple-700 dark:text-purple-400',
    folder:      'bg-amber-500/15 text-amber-700 dark:text-amber-400',
    environment: 'bg-primary/15 text-primary',
    collection:  'bg-blue-500/15 text-blue-700 dark:text-blue-400',
    global:      'bg-teal-500/15 text-teal-700 dark:text-teal-400',
    process:     'bg-muted text-muted-foreground',
  };
  return classes[source];
}
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/url-variables.ts
git commit -m "feat: add VariableSource + buildScopedContext + scope badges to url-variables"
```

---

### Task 2: Update VariableAwareUrlInput + RequestPanel

- [ ] **Step 1: Add scopedContext prop to VariableAwareUrlInput**

```tsx
interface VariableAwareUrlInputProps {
  value: string
  onChange: (value: string) => void
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>
  onCurlImport?: (parsed: ParsedCurl) => void
  scopedContext?: Map<string, VariableScopeEntry>
  // legacy props kept for backward compat
  collectionVariables?: Record<string, string>
  pathParams?: Record<string, string>
  queryParams?: Record<string, string>
  placeholder?: string
  className?: string
}
```

- [ ] **Step 2: Use scopedContext in token rendering**

When rendering variable token in overlay, look up from `scopedContext`:
```tsx
const entry = scopedContext?.get(token.value);
const resolvedValue = entry?.secret ? '●●●●' : (entry?.value ?? '');
const badgeClass = entry ? sourceBadgeClass(entry.source) : 'bg-destructive/15 text-destructive';
const label = entry?.sourceLabel ?? 'Unresolved';
```

- [ ] **Step 3: Wire in RequestPanel**

```tsx
const scopedContext = useMemo(() => buildScopedContext({
  envVars, envLabel: activeEnvId ?? undefined,
  globalVars, processEnvVars, collectionVars: collectionVariables ?? [],
  folderVars, requestVars,
}), [envVars, activeEnvId, globalVars, processEnvVars, collectionVariables, folderVars, requestVars]);
```

Pass `scopedContext` to `<VariableAwareUrlInput>`.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/
git commit -m "feat: scope badges in VariableAwareUrlInput"
```

---

## Chunk 2: Environment Switcher

### Task 3: Two-section dropdown

**File:** `src/components/layout/EnvironmentSwitcher.tsx`

- [ ] **Step 1: Restructure dropdown**

```tsx
const globalEnvName = useEnvStore(s => s.globalEnvName);
const setGlobalEnv  = useEnvStore(s => s.setGlobalEnv);
const activeEnvId   = useEnvStore(s => s.activeEnvId);
const setActiveEnv  = useEnvStore(s => s.setActiveEnv);
const environments  = useEnvStore(s => s.environments);

// Dropdown structure:
// ── Global section ──────────────────────────────
//   No Global Environment  ← deselect option
//   <env> ✓  (one per env file)
// ── separator ────────────────────────────────────
// ── Environment section ──────────────────────────
//   No Environment  ← deselect option
//   <env> ✓  (one per env file)
// ── separator ────────────────────────────────────
//   Manage Environments → opens EnvironmentsDialog
```

Global section selects use `setGlobalEnv(name)`. Environment section uses `setActiveEnv(name)`. Both sections show the same list of env files.

- [ ] **Step 2: Update trigger button**

```tsx
<Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2 text-xs">
  {globalEnvName && (
    <span className="h-2 w-2 rounded-full bg-teal-500 shrink-0"
      title={`Global: ${globalEnvName}`} />
  )}
  <span className={!activeEnvId ? 'text-muted-foreground' : ''}>
    {activeEnvId ?? 'No Environment'}
  </span>
  <ChevronDown className="h-3 w-3 opacity-50" />
</Button>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/layout/EnvironmentSwitcher.tsx
git commit -m "feat: split EnvironmentSwitcher into Global + Environment sections"
```

---
