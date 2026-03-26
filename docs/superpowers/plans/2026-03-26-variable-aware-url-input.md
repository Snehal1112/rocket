# Variable-Aware URL Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Highlight `{{variable}}` tokens inline in the request URL input with colored overlays and support hover-edit via popover to update environment variables.

**Architecture:** Task 1 creates a pure token parser. Task 2 builds the VariableAwareUrlInput component with overlay rendering and hover popover. Task 3 integrates it into RequestPanel replacing the plain Input.

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand, shadcn/ui Popover (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-variable-aware-url-input-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/lib/url-variables.ts` | Create — parse `{{var}}` tokens, resolve against variables map |
| `src/components/request/VariableAwareUrlInput.tsx` | Create — URL input with overlay highlights + hover popover |
| `src/components/request/RequestPanel.tsx` | Modify — replace plain `<Input>` with `<VariableAwareUrlInput>` |

---

### Task 1: Create URL variable parser

**Files:**
- Create: `src/lib/url-variables.ts`

- [ ] **Step 1: Create the parser**

Create `src/lib/url-variables.ts` with:

```ts
// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface UrlToken {
  type: 'text' | 'variable';
  value: string;        // raw text segment or variable name (without braces)
  start: number;        // character offset in the URL string
  end: number;          // character offset end (exclusive)
  resolved?: string;    // resolved value (only for variable tokens)
  source?: string;      // environment name (only for resolved variables)
}

// Parses a URL string into alternating text and variable tokens.
export function parseUrlTokens(
  url: string,
  variables: Record<string, string>,
  envName?: string,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    // Add preceding text segment.
    if (matchStart > lastIndex) {
      tokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];
    const resolved = varName in variables ? variables[varName] : undefined;
    tokens.push({
      type: 'variable',
      value: varName,
      start: matchStart,
      end: matchStart + match[0].length,
      resolved,
      source: resolved !== undefined ? envName : undefined,
    });
    lastIndex = matchStart + match[0].length;
  }

  // Add trailing text segment.
  if (lastIndex < url.length) {
    tokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  return tokens;
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
git commit -m "feat: add URL variable token parser with environment resolution"
```

---

### Task 2: Create VariableAwareUrlInput component

**Files:**
- Create: `src/components/request/VariableAwareUrlInput.tsx`

**Depends on:** Task 1

**Context:** This component renders a real `<input>` for keyboard interaction and an absolutely-positioned overlay `<div>` that renders the same text with colored `<span>` wrappers around `{{var}}` tokens. The overlay is `pointer-events-none` so all interactions hit the real input. Hovering a token in the overlay (enabled via `pointer-events-auto` on token spans) opens a shadcn Popover for editing.

The component reads environment data from `useEnvStore` (the Zustand store at `src/stores/env-store.ts`) to resolve variables and update them.

Key types from the codebase:
- `Environment`: `{ name: string; variables: Variable[] }` from `@/lib/tauri-api`
- `Variable`: `{ key: string; value: string; enabled: boolean; secret: boolean }` from `@/lib/tauri-api`
- `useEnvStore`: Zustand store with `getActiveVariables()`, `activeEnvId`, `environments`, `updateEnvironment(env)`

- [ ] **Step 1: Create the component**

Create `src/components/request/VariableAwareUrlInput.tsx` with:

```tsx
import { useState, useRef, useCallback } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { parseUrlTokens, type UrlToken } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  placeholder?: string;
  className?: string;
}

export function VariableAwareUrlInput({
  value,
  onChange,
  onKeyDown,
  placeholder,
  className,
}: VariableAwareUrlInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const variables = useEnvStore((s) => s.getActiveVariables());
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);

  const [editingToken, setEditingToken] = useState<UrlToken | null>(null);
  const [editValue, setEditValue] = useState('');

  const tokens = parseUrlTokens(value, variables, activeEnvId ?? undefined);

  const handleTokenHover = useCallback((token: UrlToken) => {
    setEditingToken(token);
    setEditValue(token.resolved ?? '');
  }, []);

  const handleSave = useCallback(async () => {
    if (!editingToken || !activeEnvId) return;
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return;

    const updatedVars = env.variables.map((v) =>
      v.key === editingToken.value ? { ...v, value: editValue } : v,
    );

    // If variable doesn't exist yet, add it.
    if (!env.variables.some((v) => v.key === editingToken.value)) {
      updatedVars.push({ key: editingToken.value, value: editValue, enabled: true, secret: false });
    }

    await updateEnvironment({ ...env, variables: updatedVars });
    setEditingToken(null);
  }, [editingToken, editValue, activeEnvId, environments, updateEnvironment]);

  return (
    <div className={cn('relative flex-1', className)}>
      {/* Real input for keyboard interaction. */}
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs text-transparent caret-foreground outline-none ring-ring/50 focus-visible:ring-[3px] focus-visible:border-ring"
      />

      {/* Overlay with token highlights. */}
      <div
        className="absolute inset-0 flex items-center px-3 py-1 font-mono text-xs pointer-events-none overflow-hidden whitespace-nowrap"
        aria-hidden="true"
      >
        {tokens.length > 0 ? (
          tokens.map((token, i) => {
            if (token.type === 'text') {
              return <span key={i}>{token.value}</span>;
            }
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
        ) : (
          <span className="text-muted-foreground">{placeholder}</span>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/VariableAwareUrlInput.tsx
git commit -m "feat: create VariableAwareUrlInput with token highlights and hover-edit popover"
```

---

### Task 3: Integrate VariableAwareUrlInput into RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

**Depends on:** Task 2

- [ ] **Step 1: Add import**

Find the imports section in `src/components/request/RequestPanel.tsx`. Add:

```tsx
import { VariableAwareUrlInput } from './VariableAwareUrlInput';
```

- [ ] **Step 2: Replace the URL Input with VariableAwareUrlInput**

Find the URL Input (~line 165-171):
```tsx
          <Input
            className="h-8 flex-1 font-mono text-xs"
            placeholder="https://api.example.com/resource"
            value={request.url}
            onChange={(e) => { setUrlError(''); handleUrlChange(e.target.value); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
          />
```

Replace with:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            placeholder="https://api.example.com/resource"
          />
```

- [ ] **Step 3: Remove unused Input import if no longer needed**

Check if `Input` is still used elsewhere in RequestPanel. If the URL input was the only usage, remove the import. (It's likely still used by the unsaved changes dialog or other sections — check before removing.)

- [ ] **Step 4: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/RequestPanel.tsx
git commit -m "feat: integrate VariableAwareUrlInput into RequestPanel — replaces plain Input"
```

---

## Done

The URL input now:
- Highlights `{{variable}}` tokens with color-coded overlays (primary for resolved, destructive for unresolved)
- Shows a hover popover with variable name, source environment, and editable value
- Saving from the popover updates the variable in the active environment
- Plain text typing and keyboard shortcuts (Enter to send) work exactly as before
- No changes to the send-time variable resolution logic (`useEnvStore.resolveVariables`)
