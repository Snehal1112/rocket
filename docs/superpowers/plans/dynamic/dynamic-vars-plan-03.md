# Dynamic Variables Plan 03: TypeScript Resolver Integration

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `variable-context.ts` to resolve `$`-prefixed dynamic variables using the generator registry, and update the regex to allow `$` in variable names.

**Architecture:** Two changes: (1) update `VAR_REGEX` to allow `$` as first character, (2) add `$` prefix check in `resolveWithContext` before user variable lookup.

**Tech Stack:** TypeScript, Vitest

**Spec:** Before starting, read `docs/superpowers/specs/2026-04-21-dynamic-variables-design.md`.

**Depends on:** Plan 02 (`src/lib/dynamic-vars.ts` must exist)

---

### Task 1: Update VAR_REGEX and resolveWithContext

**Files:**
- Modify: `src/lib/variable-context.ts`

- [ ] **Step 1: Update the regex**

In `src/lib/variable-context.ts`, find:

```typescript
const VAR_REGEX = /\{\{\s*([\w.-]+)\s*\}\}/g;
```

Replace with:

```typescript
const VAR_REGEX = /\{\{\s*([\$\w.-]+)\s*\}\}/g;
```

This allows `$` as the first character of a variable name inside `{{...}}`.

- [ ] **Step 2: Update `resolveWithContext`**

Add the import at the top of the file:

```typescript
import { generateDynamicVar } from './dynamic-vars';
```

Find:

```typescript
export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => (key in ctx ? ctx[key] : match));
}
```

Replace with:

```typescript
export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => {
    if (key.startsWith('$')) {
      return generateDynamicVar(key.slice(1)) ?? match;
    }
    return key in ctx ? ctx[key] : match;
  });
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/variable-context.ts
git commit -m "feat: integrate dynamic variable resolution into variable-context.ts"
```

---

### Task 2: Write integration tests

**Files:**
- Modify: `src/lib/__tests__/variable-context.test.ts`

- [ ] **Step 1: Add dynamic variable tests**

Add these tests to the existing test file, inside a new `describe('dynamic variables', ...)` block:

```typescript
describe('dynamic variables', () => {
  it('resolves {{$guid}} to a valid UUID', () => {
    const result = resolveWithContext('{{$guid}}', {});
    expect(result).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('resolves {{$randomInt}} to a number string', () => {
    const result = resolveWithContext('{{$randomInt}}', {});
    const num = parseInt(result, 10);
    expect(num).toBeGreaterThanOrEqual(0);
    expect(num).toBeLessThanOrEqual(1000);
  });

  it('does not shadow dynamic vars with user vars', () => {
    const result = resolveWithContext('{{$guid}}', { '$guid': 'user-override' });
    expect(result).not.toBe('user-override');
    expect(result).toMatch(/^[0-9a-f]{8}-/i); // starts like a UUID
  });

  it('leaves unknown $vars unresolved', () => {
    const result = resolveWithContext('{{$doesNotExist}}', {});
    expect(result).toBe('{{$doesNotExist}}');
  });

  it('resolves mixed dynamic and regular vars', () => {
    const result = resolveWithContext('{{baseUrl}}/users/{{$randomUUID}}', {
      baseUrl: 'https://api.test',
    });
    expect(result).toMatch(/^https:\/\/api\.test\/users\/[0-9a-f]{8}-/i);
  });

  it('resolves {{$timestamp}} to a unix epoch', () => {
    const result = resolveWithContext('{{$timestamp}}', {});
    const num = parseInt(result, 10);
    expect(num).toBeGreaterThan(1000000000);
  });

  it('resolves with whitespace: {{ $guid }}', () => {
    const result = resolveWithContext('{{ $guid }}', {});
    expect(result).toMatch(/^[0-9a-f]{8}-/i);
  });

  it('two $guid in same template produce different values', () => {
    const result = resolveWithContext('{{$guid}}|{{$guid}}', {});
    const [a, b] = result.split('|');
    // They should both be valid UUIDs but different
    expect(a).toMatch(/^[0-9a-f]{8}-/i);
    expect(b).toMatch(/^[0-9a-f]{8}-/i);
    // Probabilistic — extremely unlikely to be the same
    expect(a).not.toBe(b);
  });
});
```

- [ ] **Step 2: Run all variable-context tests**

```bash
npx vitest run src/lib/__tests__/variable-context.test.ts
```

Expected: all tests PASS (both existing and new).

- [ ] **Step 3: Commit**

```bash
git add src/lib/__tests__/variable-context.test.ts
git commit -m "test: add dynamic variable integration tests for variable-context.ts"
```
