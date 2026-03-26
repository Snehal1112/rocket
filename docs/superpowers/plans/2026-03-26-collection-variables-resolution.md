# Collection Variables Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire collection-level variables into `{{var}}` resolution so they resolve at send time alongside environment variables (env wins on conflict), and show in the URL input overlay.

**Architecture:** Task 1 updates `parseUrlTokens` to accept a second variables map with a different source label. Task 2 updates `execute-request.ts` to fetch collection variables and build a merged resolution function. Task 3 updates `VariableAwareUrlInput` and `RequestPanel` to show collection variables in the overlay.

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand, Tauri (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-collection-variables-resolution-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/lib/url-variables.ts` | Update `parseUrlTokens` to accept optional collection variables |
| `src/lib/execute-request.ts` | Fetch collection vars, build merged resolution function |
| `src/components/request/VariableAwareUrlInput.tsx` | Accept `collectionVariables` prop, merge into resolution |
| `src/components/request/RequestPanel.tsx` | Fetch collection variables, pass to VariableAwareUrlInput |

---

### Task 1: Update parseUrlTokens to support two variable sources

**Files:**
- Modify: `src/lib/url-variables.ts`

- [ ] **Step 1: Update the function signature and resolution logic**

Replace the entire contents of `src/lib/url-variables.ts` with:

```ts
// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface UrlToken {
  type: 'text' | 'variable';
  value: string;        // raw text segment or variable name (without braces)
  start: number;        // character offset in the URL string
  end: number;          // character offset end (exclusive)
  resolved?: string;    // resolved value (only for variable tokens)
  source?: string;      // source label: env name or "Collection"
}

// Parses a URL string into alternating text and variable tokens.
// Environment variables take precedence over collection variables.
export function parseUrlTokens(
  url: string,
  envVariables: Record<string, string>,
  envName?: string,
  collectionVariables?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIndex) {
      tokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];

    // Env wins over collection.
    let resolved: string | undefined;
    let source: string | undefined;
    if (varName in envVariables) {
      resolved = envVariables[varName];
      source = envName;
    } else if (collectionVariables && varName in collectionVariables) {
      resolved = collectionVariables[varName];
      source = 'Collection';
    }

    tokens.push({
      type: 'variable',
      value: varName,
      start: matchStart,
      end: matchStart + match[0].length,
      resolved,
      source,
    });
    lastIndex = matchStart + match[0].length;
  }

  if (lastIndex < url.length) {
    tokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  return tokens;
}

// Builds a resolve function that substitutes {{var}} with merged variables.
// Environment variables take precedence over collection variables.
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
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/lib/url-variables.ts
git commit -m "feat: update parseUrlTokens to support env + collection variable sources"
```

---

### Task 2: Fetch collection variables in execute-request.ts

**Files:**
- Modify: `src/lib/execute-request.ts`

**Depends on:** Task 1

**Context:** Currently `sendRequest` uses `useEnvStore.getState().resolveVariables` which only knows about environment variables. Replace this with the new `buildResolver` function that merges env + collection variables.

The tab's collection is found via `findTabInTree(root, tabId)` → `tab.source?.collection`. If the request belongs to a collection, fetch its settings via `getCollectionSettings(name)`.

- [ ] **Step 1: Add imports**

Find the imports at the top. Add:

```ts
import { getCollectionSettings } from '@/lib/tauri-api';
import { findTabInTree } from '@/lib/pane-utils';
import { buildResolver } from '@/lib/url-variables';
```

- [ ] **Step 2: Replace the resolve function construction**

Find (~line 76-77):
```ts
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  const resolve = useEnvStore.getState().resolveVariables;
```

Replace with:
```ts
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  // Build merged variable resolution: env vars (high priority) + collection vars (fallback).
  const envVars = useEnvStore.getState().getActiveVariables();

  let collectionVars: Record<string, string> = {};
  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  if (found?.tab.source?.collection) {
    try {
      const settings = await getCollectionSettings(found.tab.source.collection);
      for (const v of settings.variables) {
        if (v.enabled) collectionVars[v.key] = v.value;
      }
    } catch {
      // Collection settings unavailable — proceed with env vars only.
    }
  }

  const resolve = buildResolver(envVars, collectionVars);
```

- [ ] **Step 3: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/lib/execute-request.ts
git commit -m "feat: resolve collection variables alongside env variables at send time"
```

---

### Task 3: Show collection variables in VariableAwareUrlInput

**Files:**
- Modify: `src/components/request/VariableAwareUrlInput.tsx`
- Modify: `src/components/request/RequestPanel.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Add collectionVariables prop to VariableAwareUrlInput**

In `src/components/request/VariableAwareUrlInput.tsx`, find the props interface:
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  placeholder?: string;
  className?: string;
}
```

Replace with:
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  collectionVariables?: Record<string, string>;
  placeholder?: string;
  className?: string;
}
```

- [ ] **Step 2: Destructure the new prop and pass to parseUrlTokens**

Find the destructuring (add `collectionVariables` after `onCurlImport`):
```tsx
  onCurlImport,
  collectionVariables,
  placeholder,
```

Find the `parseUrlTokens` call:
```tsx
  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined);
```

Replace with:
```tsx
  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined, collectionVariables);
```

- [ ] **Step 3: Fetch collection variables in RequestPanel and pass down**

In `src/components/request/RequestPanel.tsx`, add imports:
```tsx
import { getCollectionSettings, type CollectionVariable } from '@/lib/tauri-api';
```

Find the state declarations (~line 70-72). Add after:
```tsx
  const [collectionVars, setCollectionVars] = useState<Record<string, string>>({});
```

Add a useEffect to fetch collection variables when the tab's collection changes. Add after the existing useEffect blocks:
```tsx
  // Fetch collection variables for the URL input overlay.
  useEffect(() => {
    if (!tab.source?.collection) { setCollectionVars({}); return; }
    getCollectionSettings(tab.source.collection)
      .then((s) => {
        const vars: Record<string, string> = {};
        for (const v of s.variables) {
          if (v.enabled) vars[v.key] = v.value;
        }
        setCollectionVars(vars);
      })
      .catch(() => setCollectionVars({}));
  }, [tab.source?.collection]);
```

Find the VariableAwareUrlInput usage:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            onCurlImport={handleCurlImport}
            placeholder="https://api.example.com/resource"
          />
```

Replace with:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            onCurlImport={handleCurlImport}
            collectionVariables={collectionVars}
            placeholder="https://api.example.com/resource"
          />
```

- [ ] **Step 4: Remove unused CollectionVariable import if needed**

Check if `CollectionVariable` type is actually used. If the import was only for the type and it's unused after the loop approach, remove it. The `getCollectionSettings` import is still needed.

- [ ] **Step 5: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/VariableAwareUrlInput.tsx src/components/request/RequestPanel.tsx
git commit -m "feat: show collection variables in URL input overlay with 'Collection' source label"
```

---

## Done

Collection variables are now fully wired:
- `{{var}}` in requests resolves against both environment vars (priority) and collection vars (fallback)
- URL input overlay shows collection-resolved tokens with "Source: Collection" in the popover
- Environment variables always win on conflict (same key in both)
- Only `enabled: true` variables participate in resolution
- Collection variables fetched from `collection.json` via existing Tauri API (no backend changes)
