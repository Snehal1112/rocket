# Path/Query Token Highlighting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Highlight `:pathParam` tokens and `?query=value` segments in the URL input overlay alongside existing `{{variable}}` highlights, with resolved/unresolved status styling.

**Architecture:** Task 1 extends `parseUrlTokens` to recognize `:param` and query tokens within text segments. Task 2 updates the overlay rendering in `VariableAwareUrlInput` to style path/query tokens. Task 3 wires pathParams and queryParams from RequestPanel.

**Tech Stack:** React, TypeScript, Tailwind CSS (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-path-query-token-highlighting-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/lib/url-variables.ts` | Extend `UrlToken.type` and `parseUrlTokens` for path/query tokens |
| `src/components/request/VariableAwareUrlInput.tsx` | Accept pathParams/queryParams props, render new token styles |
| `src/components/request/RequestPanel.tsx` | Pass pathParams/queryParams to VariableAwareUrlInput |

---

### Task 1: Extend token parser for path and query tokens

**Files:**
- Modify: `src/lib/url-variables.ts`

- [ ] **Step 1: Update UrlToken type and parseUrlTokens**

Replace the entire contents of `src/lib/url-variables.ts` with:

```ts
// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;
// Matches :paramName between / delimiters or at end of path.
const PATH_PARAM_REGEX = /:(\w+)/g;

export interface UrlToken {
  type: 'text' | 'variable' | 'pathParam' | 'queryKey' | 'queryValue';
  value: string;
  start: number;
  end: number;
  resolved?: string;
  source?: string;
}

interface TokenContext {
  envVariables: Record<string, string>;
  envName?: string;
  collectionVariables?: Record<string, string>;
  pathParams?: Record<string, string>;
  queryParams?: Record<string, string>;
}

// Parses text segments for :pathParam tokens in the path portion (before ?).
function expandPathParams(
  text: string,
  offset: number,
  pathParams?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIdx = 0;

  for (const match of text.matchAll(PATH_PARAM_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIdx) {
      tokens.push({ type: 'text', value: text.slice(lastIdx, matchStart), start: offset + lastIdx, end: offset + matchStart });
    }
    const paramName = match[1];
    const resolved = pathParams && paramName in pathParams ? pathParams[paramName] : undefined;
    tokens.push({
      type: 'pathParam',
      value: paramName,
      start: offset + matchStart,
      end: offset + matchStart + match[0].length,
      resolved,
      source: resolved !== undefined ? 'Path Params' : undefined,
    });
    lastIdx = matchStart + match[0].length;
  }

  if (lastIdx < text.length) {
    tokens.push({ type: 'text', value: text.slice(lastIdx), start: offset + lastIdx, end: offset + text.length });
  }

  return tokens;
}

// Parses query string segments into key=value tokens.
function expandQueryTokens(
  queryString: string,
  offset: number,
  queryParams?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  // Split on & but keep the delimiters as text.
  const pairs = queryString.split(/(&)/);
  let pos = 0;

  for (const segment of pairs) {
    if (segment === '&') {
      tokens.push({ type: 'text', value: '&', start: offset + pos, end: offset + pos + 1 });
      pos += 1;
      continue;
    }
    const eqIdx = segment.indexOf('=');
    if (eqIdx > 0) {
      const key = segment.slice(0, eqIdx);
      const val = segment.slice(eqIdx + 1);
      const resolved = queryParams && key in queryParams ? queryParams[key] : undefined;
      tokens.push({
        type: 'queryKey',
        value: key,
        start: offset + pos,
        end: offset + pos + key.length,
        resolved,
        source: resolved !== undefined ? 'Query Params' : undefined,
      });
      tokens.push({ type: 'text', value: '=', start: offset + pos + key.length, end: offset + pos + key.length + 1 });
      if (val) {
        tokens.push({
          type: 'queryValue',
          value: val,
          start: offset + pos + key.length + 1,
          end: offset + pos + segment.length,
        });
      }
    } else if (segment) {
      tokens.push({ type: 'text', value: segment, start: offset + pos, end: offset + pos + segment.length });
    }
    pos += segment.length;
  }

  return tokens;
}

// Parses a URL string into tokens: {{variables}}, :pathParams, and query key=value pairs.
export function parseUrlTokens(
  url: string,
  envVariables: Record<string, string>,
  envName?: string,
  collectionVariables?: Record<string, string>,
  pathParams?: Record<string, string>,
  queryParams?: Record<string, string>,
): UrlToken[] {
  // First pass: split on {{var}} patterns.
  const varTokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIndex) {
      varTokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];
    let resolved: string | undefined;
    let source: string | undefined;
    if (varName in envVariables) {
      resolved = envVariables[varName];
      source = envName;
    } else if (collectionVariables && varName in collectionVariables) {
      resolved = collectionVariables[varName];
      source = 'Collection';
    }
    varTokens.push({ type: 'variable', value: varName, start: matchStart, end: matchStart + match[0].length, resolved, source });
    lastIndex = matchStart + match[0].length;
  }
  if (lastIndex < url.length) {
    varTokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  // Second pass: expand text segments for :pathParam and query tokens.
  const finalTokens: UrlToken[] = [];
  for (const token of varTokens) {
    if (token.type !== 'text') {
      finalTokens.push(token);
      continue;
    }

    // Check if this text segment contains the query string separator.
    const qIdx = token.value.indexOf('?');
    if (qIdx >= 0) {
      // Path portion before ?
      const pathPart = token.value.slice(0, qIdx);
      if (pathPart) {
        finalTokens.push(...expandPathParams(pathPart, token.start, pathParams));
      }
      // The ? itself.
      finalTokens.push({ type: 'text', value: '?', start: token.start + qIdx, end: token.start + qIdx + 1 });
      // Query portion after ?
      const queryPart = token.value.slice(qIdx + 1);
      if (queryPart) {
        finalTokens.push(...expandQueryTokens(queryPart, token.start + qIdx + 1, queryParams));
      }
    } else {
      // No query string — just expand path params.
      finalTokens.push(...expandPathParams(token.value, token.start, pathParams));
    }
  }

  return finalTokens;
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
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/lib/url-variables.ts
git commit -m "feat: extend URL token parser for :pathParam and query key=value highlighting"
```

---

### Task 2: Update VariableAwareUrlInput for new token types

**Files:**
- Modify: `src/components/request/VariableAwareUrlInput.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Add pathParams and queryParams to props**

Find the props interface:
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

Replace with:
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  collectionVariables?: Record<string, string>;
  pathParams?: Record<string, string>;
  queryParams?: Record<string, string>;
  placeholder?: string;
  className?: string;
}
```

- [ ] **Step 2: Destructure new props and pass to parseUrlTokens**

Find the destructuring and add `pathParams, queryParams,` after `collectionVariables,`.

Find the `parseUrlTokens` call:
```tsx
  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined, collectionVariables);
```

Replace with:
```tsx
  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined, collectionVariables, pathParams, queryParams);
```

- [ ] **Step 3: Update the token rendering to handle new types**

Find the token rendering block inside the overlay div. Currently it checks `if (token.type === 'text')` and renders all non-text tokens as `{{variable}}` with popover. Replace the entire `tokens.map` block:

Find:
```tsx
          tokens.map((token, i) => {
            if (token.type === 'text') {
              return <span key={i}>{token.value}</span>;
            }
            const isResolved = token.resolved !== undefined;
            return (
              <Popover
```

Replace the entire map function (from `tokens.map((token, i) => {` through its closing `})`) with:

```tsx
          tokens.map((token, i) => {
            if (token.type === 'text') {
              return <span key={i}>{token.value}</span>;
            }

            // Path param tokens: :paramName — no popover, just styled span.
            if (token.type === 'pathParam') {
              const isResolved = token.resolved !== undefined;
              return (
                <span
                  key={i}
                  className={cn(
                    'rounded-sm px-0.5',
                    isResolved
                      ? 'bg-violet-500/15 text-violet-500'
                      : 'bg-destructive/15 text-destructive',
                  )}
                  title={isResolved ? `${token.value} = ${token.resolved}` : `${token.value} (unresolved)`}
                >
                  :{token.value}
                </span>
              );
            }

            // Query key tokens: highlighted key name.
            if (token.type === 'queryKey') {
              const isResolved = token.resolved !== undefined;
              return (
                <span
                  key={i}
                  className={cn(
                    'rounded-sm px-0.5',
                    isResolved
                      ? 'bg-amber-500/15 text-amber-500'
                      : 'text-muted-foreground',
                  )}
                  title={isResolved ? `${token.value} = ${token.resolved}` : token.value}
                >
                  {token.value}
                </span>
              );
            }

            // Query value tokens: plain text style.
            if (token.type === 'queryValue') {
              return <span key={i} className="text-muted-foreground">{token.value}</span>;
            }

            // Variable tokens: {{name}} — with popover for editing.
            const isResolved = token.resolved !== undefined;
            return (
              <Popover
                key={i}
                open={editingToken?.start === token.start}
                onOpenChange={(open) => { if (!open) setEditingToken(null); }}
              >
                <PopoverTrigger asChild>
                  <span
                    className={cn(
                      'rounded-sm px-0.5 cursor-pointer pointer-events-auto',
                      isResolved
                        ? 'bg-primary/15 text-primary'
                        : 'bg-destructive/15 text-destructive',
                    )}
                    onMouseEnter={() => handleTokenHover(token)}
                  >
                    {`{{${token.value}}}`}
                  </span>
                </PopoverTrigger>
                <PopoverContent className="w-72 p-3 space-y-2" side="bottom" align="start">
                  <div className="text-xs font-medium">{token.value}</div>
                  {isResolved && token.source && (
                    <div className="text-2xs text-muted-foreground">
                      Source: {token.source}
                    </div>
                  )}
                  {!isResolved && !activeEnvId && (
                    <div className="text-2xs text-destructive">
                      No active environment selected.
                    </div>
                  )}
                  {!isResolved && activeEnvId && (
                    <div className="text-2xs text-destructive">
                      Not found in {activeEnvId}.
                    </div>
                  )}
                  {activeEnvId && (
                    <div className="space-y-1.5">
                      <Input
                        className="h-7 text-xs font-mono"
                        value={editValue}
                        onChange={(e) => setEditValue(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') void handleSave();
                          if (e.key === 'Escape') setEditingToken(null);
                        }}
                        placeholder="Variable value"
                      />
                      <div className="flex gap-1.5">
                        <Button size="sm" className="h-6 text-2xs" onClick={() => void handleSave()}>
                          Save
                        </Button>
                        <Button size="sm" variant="ghost" className="h-6 text-2xs" onClick={() => setEditingToken(null)}>
                          Cancel
                        </Button>
                      </div>
                    </div>
                  )}
                </PopoverContent>
              </Popover>
            );
          })
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/VariableAwareUrlInput.tsx
git commit -m "feat: render path param and query key token highlights in URL overlay"
```

---

### Task 3: Wire pathParams and queryParams from RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

**Depends on:** Task 2

- [ ] **Step 1: Build pathParams and queryParams maps**

Find the existing state/computed values area (around the `enabledParamCount` / `enabledHeaderCount` lines). Add after them:

```tsx
  const pathParamMap = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of request.pathParams) {
      if (p.enabled && p.key) map[p.key] = p.value;
    }
    return map;
  }, [request.pathParams]);

  const queryParamMap = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of request.queryParams) {
      if (p.enabled && p.key) map[p.key] = p.value;
    }
    return map;
  }, [request.queryParams]);
```

Make sure `useMemo` is imported (check the existing imports — it should be since `useState`, `useCallback`, `useRef`, `useEffect` are already imported; if not, add it).

- [ ] **Step 2: Pass to VariableAwareUrlInput**

Find the VariableAwareUrlInput usage. Add `pathParams` and `queryParams` props:

Find:
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

Replace with:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            onCurlImport={handleCurlImport}
            collectionVariables={collectionVars}
            pathParams={pathParamMap}
            queryParams={queryParamMap}
            placeholder="https://api.example.com/resource"
          />
```

- [ ] **Step 3: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/RequestPanel.tsx
git commit -m "feat: wire pathParams and queryParams into URL token highlighting"
```

---

## Done

Path/query token highlighting complete:
- `:userId` in URL path shows in violet (`bg-violet-500/15 text-violet-500`) when resolved, destructive when unresolved
- Query keys like `page` in `?page=1` show in amber (`bg-amber-500/15 text-amber-500`)
- `{{variable}}` tokens unchanged — still primary color with hover-edit popover
- Path/query tokens show resolved value on hover (title attribute) but no edit popover
- All three token types coexist: `{{host}}/api/:userId?page=1` highlights all three differently
