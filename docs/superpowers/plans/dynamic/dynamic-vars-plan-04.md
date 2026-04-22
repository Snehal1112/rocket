# Dynamic Variables Plan 04: URL Variables — Dynamic Source Type

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `'dynamic'` as a new variable source type in `url-variables.ts` so `{{$randomEmail}}` tokens in the URL bar show the correct source badge and preview value.

**Architecture:** Add `'dynamic'` to the `VariableSource` union type, update `parseUrlTokens` to detect `$`-prefixed variables, add badge colour for the new source, and update `buildScopedContext` to recognise dynamic vars.

**Tech Stack:** TypeScript, Vitest

**Spec:** Before starting, read `docs/superpowers/specs/2026-04-21-dynamic-variables-design.md`.

**Depends on:** Plan 02 (`src/lib/dynamic-vars.ts` must exist)

---

### Task 1: Add `'dynamic'` source type and badge colour

**Files:**
- Modify: `src/lib/url-variables.ts`

- [ ] **Step 1: Add `'dynamic'` to `VariableSource`**

Find the `VariableSource` type definition:

```typescript
export type VariableSource = 'environment' | 'collection' | 'global' | 'folder' | 'request' | 'process' | 'runtime';
```

Replace with:

```typescript
export type VariableSource = 'environment' | 'collection' | 'global' | 'folder' | 'request' | 'process' | 'runtime' | 'dynamic';
```

- [ ] **Step 2: Add badge colour for `'dynamic'`**

Find the `sourceBadgeClass` function (or the equivalent colour mapping). Add a case for `'dynamic'`:

```typescript
case 'dynamic': return 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-400';
```

And add the badge letter mapping (wherever the badge letters like `E`, `C`, `G`, `F`, `R`, `P` are defined):

```typescript
case 'dynamic': return 'D';
```

- [ ] **Step 3: Update `VAR_REGEX` in url-variables.ts**

Find the `VAR_REGEX` in `url-variables.ts`:

```typescript
const VAR_REGEX = /\{\{\s*([\w.-]+)\s*\}\}/g;
```

Replace with:

```typescript
const VAR_REGEX = /\{\{\s*([\$\w.-]+)\s*\}\}/g;
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/url-variables.ts
git commit -m "feat: add 'dynamic' source type to url-variables with badge colour"
```

---

### Task 2: Update parseUrlTokens for `$`-prefixed variables

**Files:**
- Modify: `src/lib/url-variables.ts`

- [ ] **Step 1: Add import**

At the top of `url-variables.ts`, add:

```typescript
import { generateDynamicVar, isDynamicVar } from './dynamic-vars';
```

- [ ] **Step 2: Update variable token creation in parseUrlTokens**

Find the section inside `parseUrlTokens` where variable tokens are created — the block that sets `resolved` and `source` for a matched `{{varName}}`. It looks something like:

```typescript
if (varName in envVariables) {
  resolved = envVariables[varName];
  source = envName;
} else if (collectionVariables && varName in collectionVariables) {
  resolved = collectionVariables[varName];
  source = 'Collection';
}
```

Add a `$` prefix check **before** the existing checks:

```typescript
if (varName.startsWith('$')) {
  const stripped = varName.slice(1);
  if (isDynamicVar(stripped)) {
    resolved = generateDynamicVar(stripped);
    source = 'Dynamic';
  }
  // else: unknown $var — resolved stays undefined, shown as unresolved
} else if (varName in envVariables) {
  resolved = envVariables[varName];
  source = envName;
} else if (collectionVariables && varName in collectionVariables) {
  resolved = collectionVariables[varName];
  source = 'Collection';
}
```

- [ ] **Step 3: Update buildResolver for `$`-prefix**

Find the `buildResolver` function. Its returned closure does:

```typescript
return (text: string) =>
  text.replace(VAR_REGEX, (match, key) => {
    if (key in envVariables) return envVariables[key];
    if (collectionVariables && key in collectionVariables) return collectionVariables[key];
    return match;
  });
```

Update to check for `$` prefix first:

```typescript
return (text: string) =>
  text.replace(VAR_REGEX, (match, key) => {
    if (key.startsWith('$')) {
      const stripped = key.slice(1);
      return isDynamicVar(stripped) ? (generateDynamicVar(stripped) ?? match) : match;
    }
    if (key in envVariables) return envVariables[key];
    if (collectionVariables && key in collectionVariables) return collectionVariables[key];
    return match;
  });
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/url-variables.ts
git commit -m "feat: resolve dynamic variables in URL bar token parsing"
```

---

### Task 3: Add tests for dynamic vars in url-variables

**Files:**
- Modify: `src/lib/__tests__/url-variables.test.ts` (or create if it doesn't exist)

- [ ] **Step 1: Add tests**

Add a `describe('dynamic variables in URL', ...)` block:

```typescript
describe('dynamic variables in URL', () => {
  it('parses {{$randomEmail}} as a dynamic source token', () => {
    const tokens = parseUrlTokens(
      'https://api.test/{{$randomEmail}}',
      {},       // envVariables
      'test',   // envName
    );
    const varToken = tokens.find(t => t.type === 'variable' && t.value === '$randomEmail');
    expect(varToken).toBeDefined();
    expect(varToken!.source).toBe('Dynamic');
    expect(varToken!.resolved).toBeDefined();
    expect(varToken!.resolved).toContain('@');
  });

  it('parses {{$guid}} as dynamic and resolves to UUID', () => {
    const tokens = parseUrlTokens('{{$guid}}', {});
    const varToken = tokens.find(t => t.type === 'variable');
    expect(varToken).toBeDefined();
    expect(varToken!.source).toBe('Dynamic');
    expect(varToken!.resolved).toMatch(/^[0-9a-f]{8}-/i);
  });

  it('unknown $var has no source and no resolved value', () => {
    const tokens = parseUrlTokens('{{$doesNotExist}}', {});
    const varToken = tokens.find(t => t.type === 'variable');
    expect(varToken).toBeDefined();
    expect(varToken!.source).toBeUndefined();
    expect(varToken!.resolved).toBeUndefined();
  });

  it('buildResolver resolves $guid in URL text', () => {
    const resolver = buildResolver({});
    const result = resolver('https://api.test/{{$guid}}');
    expect(result).toMatch(/^https:\/\/api\.test\/[0-9a-f]{8}-/i);
  });
});
```

- [ ] **Step 2: Run tests**

```bash
npx vitest run src/lib/__tests__/url-variables.test.ts
```

Expected: all tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src/lib/__tests__/url-variables.test.ts
git commit -m "test: add dynamic variable tests for URL bar token parsing"
```
