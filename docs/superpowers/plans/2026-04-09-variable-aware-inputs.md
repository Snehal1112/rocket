# Variable-Aware Inputs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend variable highlighting and popover editing to every text value input in the request panel, plus read-only highlighting and hover tooltips in the Monaco body editor.

**Architecture:** New `VariableAwareInput` component using the same dual-layer overlay pattern as `VariableAwareUrlInput`; new `parseTextTokens` utility for plain-text tokenization; `scopedContext` (already built in `RequestPanel`) threaded down to all child editors; Monaco gets inline CSS decorations and a hover provider.

**Tech Stack:** React 19, TypeScript, Tailwind CSS, Radix UI Popover, `@monaco-editor/react` (deltaDecorations, registerHoverProvider), Vitest + @testing-library/react

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/lib/text-variables.ts` | Create | `TextToken` type + `parseTextTokens()` utility |
| `src/lib/__tests__/text-variables.test.ts` | Create | Unit tests for `parseTextTokens` |
| `src/components/request/VariableAwareInput.tsx` | Create | Dual-layer input with variable highlighting + popover |
| `src/components/request/__tests__/VariableAwareInput.test.tsx` | Create | Unit tests for `VariableAwareInput` |
| `src/components/editor/MonacoWrapper.tsx` | Modify | Add variable decorations + hover provider |
| `src/components/request/KeyValueEditor.tsx` | Modify | Add optional `variableContext` prop; swap value `<Input>` → `<VariableAwareInput>` |
| `src/components/request/HeadersEditor.tsx` | Modify | Pass-through `variableContext` and `onNavigateToSource` to `KeyValueEditor` |
| `src/components/request/QueryParamsEditor.tsx` | Modify | Same pass-through as `HeadersEditor` |
| `src/components/request/PathParamsPanel.tsx` | Modify | Swap value `<Input>` → `<VariableAwareInput>` |
| `src/components/request/AuthEditor.tsx` | Modify | Swap all text value `<Input>` fields → `<VariableAwareInput>` |
| `src/components/request/BodyEditor.tsx` | Modify | Pass `variableContext` to `KeyValueEditor` (formdata) and `MonacoWrapper` |
| `src/components/request/RequestPanel.tsx` | Modify | Pass `scopedContext` and `handleNavigateToSource` to all editors |

---

## Task 1: `parseTextTokens` utility

**Files:**
- Create: `src/lib/text-variables.ts`
- Create: `src/lib/__tests__/text-variables.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/lib/__tests__/text-variables.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { parseTextTokens } from '../text-variables';

describe('parseTextTokens', () => {
  it('returns single text token for plain text', () => {
    expect(parseTextTokens('hello world')).toEqual([
      { type: 'text', content: 'hello world', rawLength: 11 },
    ]);
  });

  it('returns empty array for empty string', () => {
    expect(parseTextTokens('')).toEqual([]);
  });

  it('parses a single variable', () => {
    expect(parseTextTokens('{{token}}')).toEqual([
      { type: 'variable', content: 'token', rawLength: 9 },
    ]);
  });

  it('parses variable surrounded by text', () => {
    expect(parseTextTokens('Bearer {{token}} extra')).toEqual([
      { type: 'text', content: 'Bearer ', rawLength: 7 },
      { type: 'variable', content: 'token', rawLength: 9 },
      { type: 'text', content: ' extra', rawLength: 6 },
    ]);
  });

  it('parses multiple variables', () => {
    expect(parseTextTokens('{{a}}/{{b}}')).toEqual([
      { type: 'variable', content: 'a', rawLength: 5 },
      { type: 'text', content: '/', rawLength: 1 },
      { type: 'variable', content: 'b', rawLength: 5 },
    ]);
  });

  it('trims whitespace inside braces but rawLength covers full match', () => {
    expect(parseTextTokens('{{ key }}')).toEqual([
      { type: 'variable', content: 'key', rawLength: 9 },
    ]);
  });

  it('handles process.env dot notation', () => {
    expect(parseTextTokens('{{process.env.KEY}}')).toEqual([
      { type: 'variable', content: 'process.env.KEY', rawLength: 19 },
    ]);
  });

  it('does not emit empty text tokens', () => {
    const tokens = parseTextTokens('{{a}}{{b}}');
    expect(tokens).toEqual([
      { type: 'variable', content: 'a', rawLength: 5 },
      { type: 'variable', content: 'b', rawLength: 5 },
    ]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn test src/lib/__tests__/text-variables.test.ts
```

Expected: FAIL with `Cannot find module '../text-variables'`

- [ ] **Step 3: Implement `src/lib/text-variables.ts`**

```ts
// Matches {{variable.name}} style placeholders, same pattern as url-variables.ts.
const VAR_REGEX = /\{\{\s*([\w.]+)\s*\}\}/g;

export interface TextToken {
  type: 'text' | 'variable';
  /** Raw text content, or variable name without braces (whitespace trimmed). */
  content: string;
  /** Number of characters consumed in the original string (needed for offset math). */
  rawLength: number;
}

/**
 * Tokenizes arbitrary text into plain-text and {{variable}} segments.
 * Example: `"Bearer {{token}}"` → [{type:'text', content:'Bearer ', rawLength:7}, {type:'variable', content:'token', rawLength:9}]
 */
export function parseTextTokens(text: string): TextToken[] {
  const tokens: TextToken[] = [];
  let lastIndex = 0;

  for (const match of text.matchAll(VAR_REGEX)) {
    const matchStart = match.index ?? 0;
    if (matchStart > lastIndex) {
      const content = text.slice(lastIndex, matchStart);
      tokens.push({ type: 'text', content, rawLength: content.length });
    }
    tokens.push({ type: 'variable', content: match[1], rawLength: match[0].length });
    lastIndex = matchStart + match[0].length;
  }

  if (lastIndex < text.length) {
    const content = text.slice(lastIndex);
    tokens.push({ type: 'text', content, rawLength: content.length });
  }

  return tokens;
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test src/lib/__tests__/text-variables.test.ts
```

Expected: All 8 tests PASS.

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/text-variables.ts src/lib/__tests__/text-variables.test.ts
git commit -m "feat(variables): add parseTextTokens utility for arbitrary text tokenization"
```

---

## Task 2: `VariableAwareInput` component

**Files:**
- Create: `src/components/request/VariableAwareInput.tsx`
- Create: `src/components/request/__tests__/VariableAwareInput.test.tsx`

This component mirrors the overlay pattern from `VariableAwareUrlInput` but handles plain text fields (no URL/path/query tokenization). When `variableContext` is absent it falls back to a plain `<Input>`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/request/__tests__/VariableAwareInput.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { VariableAwareInput } from '../VariableAwareInput';

function makeContext(entries: Record<string, VariableScopeEntry>): Map<string, VariableScopeEntry> {
  return new Map(Object.entries(entries));
}

describe('VariableAwareInput', () => {
  it('renders a plain input when variableContext is undefined', () => {
    render(<VariableAwareInput value='hello' onChange={vi.fn()} />);
    const input = screen.getByRole('textbox');
    expect(input).toBeDefined();
    // No overlay present when no context.
    expect(document.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('renders overlay when variableContext is provided', () => {
    render(
      <VariableAwareInput
        value='Bearer {{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc123', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    expect(document.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });

  it('renders resolved variable with source color span', () => {
    render(
      <VariableAwareInput
        value='{{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    expect(overlay?.textContent).toContain('{{token}}');
    // The variable span should have a highlight class.
    const span = overlay?.querySelector('.rounded-sm');
    expect(span).not.toBeNull();
  });

  it('renders unresolved variable with destructive color span', () => {
    render(
      <VariableAwareInput
        value='{{missing}}'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    const span = overlay?.querySelector('.text-destructive');
    expect(span).not.toBeNull();
  });

  it('renders plain input when value has no variables', () => {
    render(
      <VariableAwareInput
        value='plain text'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const overlay = document.querySelector('[aria-hidden="true"]');
    // Overlay renders but contains only a text span.
    expect(overlay?.textContent).toBe('plain text');
  });
});
```

- [ ] **Step 2: Run to verify they fail**

```bash
yarn test src/components/request/__tests__/VariableAwareInput.test.tsx
```

Expected: FAIL with `Cannot find module '../VariableAwareInput'`

- [ ] **Step 3: Implement `src/components/request/VariableAwareInput.tsx`**

```tsx
import { useCallback, useRef, useState } from 'react';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { parseTextTokens } from '@/lib/text-variables';
import { sourceBadgeClass, type VariableScopeEntry, type VariableSource } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

export interface VariableAwareInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

// Human-readable label and badge icon for source type.
function sourceMeta(entry: VariableScopeEntry) {
  return {
    icon: entry.source.charAt(0).toUpperCase(),
    iconClass: cn(
      'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
      sourceBadgeClass(entry.source),
    ),
    label: entry.label,
  };
}

// Nav link label for a variable source.
function navLinkLabel(source: VariableSource): string | null {
  switch (source) {
    case 'request':
    case 'runtime':
      return 'Request Variables \u2192';
    case 'environment':
      return 'Collection Environments \u2192';
    case 'global':
      return 'Global Environments \u2192';
    case 'collection':
      return 'Collection Variables \u2192';
    default:
      return null; // folder, process — no navigation
  }
}

export function VariableAwareInput({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: VariableAwareInputProps) {
  // Fall back to a plain Input when no context is provided to avoid overhead.
  if (!variableContext) {
    return (
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={cn('text-xs', className)}
        disabled={disabled}
      />
    );
  }

  return (
    <VariableAwareInputInner
      value={value}
      onChange={onChange}
      placeholder={placeholder}
      className={className}
      disabled={disabled}
      variableContext={variableContext}
      onNavigateToSource={onNavigateToSource}
    />
  );
}

// Separated so the hook rules are not broken by the early conditional return above.
function VariableAwareInputInner({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: Required<Pick<VariableAwareInputProps, 'variableContext'>> &
  Omit<VariableAwareInputProps, 'variableContext'>) {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  const [openVarKey, setOpenVarKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const editingScopeRef = useRef<VariableSource | null>(null);

  const tokens = parseTextTokens(value);

  const handleTokenHover = useCallback((varKey: string, entry: VariableScopeEntry | undefined) => {
    setOpenVarKey(varKey);
    setEditValue(entry?.secret ? '' : (entry?.value ?? ''));
    editingScopeRef.current = entry?.source ?? null;
  }, []);

  const handleCommit = useCallback(async () => {
    if (!openVarKey) return;
    const scope = editingScopeRef.current;

    if (scope === 'global' && globalEnv) {
      const vars = globalEnv.variables.map((v) =>
        v.key === openVarKey ? { ...v, value: editValue } : v,
      );
      if (!globalEnv.variables.some((v) => v.key === openVarKey)) {
        vars.push({ key: openVarKey, value: editValue, enabled: true, secret: false });
      }
      await updateGlobalEnvironment({ ...globalEnv, variables: vars });
    } else if ((scope === 'environment' || scope === null) && activeEnvId) {
      const env = environments.find((e) => e.name === activeEnvId);
      if (env) {
        const vars = env.variables.map((v) =>
          v.key === openVarKey ? { ...v, value: editValue } : v,
        );
        if (!env.variables.some((v) => v.key === openVarKey)) {
          vars.push({ key: openVarKey, value: editValue, enabled: true, secret: false });
        }
        await updateEnvironment({ ...env, variables: vars });
      }
    }

    setOpenVarKey(null);
  }, [openVarKey, editValue, activeEnvId, environments, updateEnvironment, globalEnv, updateGlobalEnvironment]);

  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  return (
    <div className={cn('relative', className)}>
      {/* Transparent input receives keystrokes and shows caret. */}
      <input
        type='text'
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className={cn(
          'h-8 w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs',
          'text-transparent caret-foreground outline-none ring-ring/50',
          'focus-visible:ring-[3px] focus-visible:border-ring',
          'disabled:cursor-not-allowed disabled:opacity-50',
        )}
      />

      {/* Overlay renders token highlights; pointer-events disabled except on variable spans. */}
      <div
        className='absolute inset-0 flex items-center px-3 py-1 font-mono text-xs pointer-events-none overflow-hidden whitespace-nowrap'
        aria-hidden='true'
      >
        {tokens.length > 0 ? (
          tokens.map((token, idx) => {
            if (token.type === 'text') {
              // biome-ignore lint/suspicious/noArrayIndexKey: tokens have no stable id
              return <span key={idx}>{token.content}</span>;
            }

            const entry = variableContext.get(token.content);
            const badgeClass = entry
              ? sourceBadgeClass(entry.source)
              : 'bg-destructive/15 text-destructive';

            const isReadOnly =
              entry !== undefined &&
              entry.source !== 'environment' &&
              entry.source !== 'global';

            const linkLabel = entry ? navLinkLabel(entry.source) : null;
            const meta = entry ? sourceMeta(entry) : null;

            return (
              <Popover
                // biome-ignore lint/suspicious/noArrayIndexKey: tokens have no stable id
                key={idx}
                open={openVarKey === token.content}
                onOpenChange={(open) => {
                  if (!open) setOpenVarKey(null);
                }}
              >
                <PopoverTrigger asChild>
                  <button
                    type='button'
                    className={cn(
                      'rounded-sm px-0.5 cursor-pointer pointer-events-auto bg-transparent border-0',
                      badgeClass,
                    )}
                    onMouseEnter={() => handleTokenHover(token.content, entry)}
                  >
                    {`{{${token.content}}}`}
                  </button>
                </PopoverTrigger>
                <PopoverContent className='w-80 p-0' side='bottom' align='start'>
                  <div className='p-2'>
                    <Input
                      autoFocus
                      className='h-7 text-xs font-mono'
                      value={entry?.secret ? '●●●●' : editValue}
                      placeholder='Value'
                      readOnly={isReadOnly || entry?.secret}
                      onChange={(e) => {
                        if (isReadOnly || entry?.secret) return;
                        setEditValue(e.target.value);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void handleCommitRef.current();
                        if (e.key === 'Escape') setOpenVarKey(null);
                      }}
                      onBlur={() => void handleCommitRef.current()}
                    />
                  </div>
                  <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                    {meta ? (
                      <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                        <span className={meta.iconClass}>{meta.icon}</span>
                        <span>{meta.label}</span>
                      </div>
                    ) : (
                      <div className='text-2xs text-muted-foreground'>Unresolved</div>
                    )}
                    {onNavigateToSource && entry && linkLabel && (
                      <button
                        type='button'
                        className='text-2xs text-primary hover:underline cursor-pointer'
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={async () => {
                          await handleCommitRef.current();
                          onNavigateToSource(entry.source, token.content);
                        }}
                      >
                        {linkLabel}
                      </button>
                    )}
                  </div>
                </PopoverContent>
              </Popover>
            );
          })
        ) : (
          <span className='text-muted-foreground'>{placeholder}</span>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test src/components/request/__tests__/VariableAwareInput.test.tsx
```

Expected: All 5 tests PASS. (Note: `useEnvStore` will fail in jsdom unless mocked. If so, add a vi.mock at the top of the test file as shown below. Only add if tests fail due to store access.)

If the store causes JSDOM errors, add at the top of the test file after the imports:
```ts
vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: { activeEnvId: null; environments: []; globalEnv: null; updateEnvironment: () => {}; updateGlobalEnvironment: () => {} }) => unknown) =>
    selector({ activeEnvId: null, environments: [], globalEnv: null, updateEnvironment: () => {}, updateGlobalEnvironment: () => {} }),
}));
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: no new errors.

- [ ] **Step 6: Lint**

```bash
yarn check 2>&1 | head -30
```

Fix any reported issues (unused imports, biome lint errors).

- [ ] **Step 7: Commit**

```bash
git add src/components/request/VariableAwareInput.tsx src/components/request/__tests__/VariableAwareInput.test.tsx
git commit -m "feat(variables): add VariableAwareInput component with dual-layer overlay and popover"
```

---

## Task 3: Wire `KeyValueEditor`, `HeadersEditor`, `QueryParamsEditor`

**Files:**
- Modify: `src/components/request/KeyValueEditor.tsx`
- Modify: `src/components/request/HeadersEditor.tsx`
- Modify: `src/components/request/QueryParamsEditor.tsx`

- [ ] **Step 1: Update `KeyValueEditor.tsx`**

Replace the entire file content:

```tsx
import { Plus, X } from 'lucide-react';
import { useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';
import { VariableAwareInput } from './VariableAwareInput';

interface KeyValueEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  label?: string;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

export function KeyValueEditor({
  entries,
  onChange,
  keyPlaceholder = 'Key',
  valuePlaceholder = 'Value',
  addLabel = 'Add Entry',
  label,
  variableContext,
  onNavigateToSource,
}: KeyValueEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [entries, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(entries.filter((e) => e.id !== id));
    },
    [entries, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([...entries, { id: crypto.randomUUID(), key: '', value: '', enabled: true }]);
  }, [entries, onChange]);

  return (
    <div className='space-y-2'>
      {label && <div className='text-xs font-medium text-muted-foreground'>{label}</div>}
      {entries.length > 0 && (
        <div className='flex gap-2 items-center text-[10px] text-muted-foreground uppercase tracking-wider'>
          <div className='w-4' />
          <div className='flex-1'>{keyPlaceholder}</div>
          <div className='flex-1'>{valuePlaceholder}</div>
          <div className='w-7' />
        </div>
      )}
      {entries.map((entry) => (
        <div key={entry.id} className='flex gap-2 items-center'>
          <Checkbox
            checked={entry.enabled}
            onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
          />
          <Input
            placeholder={keyPlaceholder}
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className='flex-1 text-xs'
          />
          <VariableAwareInput
            placeholder={valuePlaceholder}
            value={entry.value}
            onChange={(val) => updateEntry(entry.id, { value: val })}
            className='flex-1'
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
          <Button
            variant='ghost'
            size='icon'
            onClick={() => removeEntry(entry.id)}
            className='h-7 w-7'
            aria-label={`Remove ${entry.key || 'unnamed'}`}
          >
            <X className='h-3.5 w-3.5' />
          </Button>
        </div>
      ))}
      <Button variant='ghost' size='sm' onClick={addEntry} className='text-xs'>
        <Plus className='h-3.5 w-3.5 mr-1' />
        {addLabel}
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: Update `HeadersEditor.tsx`**

```tsx
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

interface HeadersEditorProps {
  headers: KeyValueEntry[];
  onChange: (headers: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

export function HeadersEditor({ headers, onChange, variableContext, onNavigateToSource }: HeadersEditorProps) {
  return (
    <KeyValueEditor
      entries={headers}
      onChange={onChange}
      keyPlaceholder='Header name'
      valuePlaceholder='Value'
      addLabel='Add Header'
      variableContext={variableContext}
      onNavigateToSource={onNavigateToSource}
    />
  );
}
```

- [ ] **Step 3: Update `QueryParamsEditor.tsx`**

```tsx
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

interface QueryParamsEditorProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

export function QueryParamsEditor({ params, onChange, variableContext, onNavigateToSource }: QueryParamsEditorProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder='Param name'
      valuePlaceholder='Value'
      addLabel='Add Query Param'
      label='Query'
      variableContext={variableContext}
      onNavigateToSource={onNavigateToSource}
    />
  );
}
```

- [ ] **Step 4: TypeScript check + lint**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 5: Run all tests**

```bash
yarn test
```

Expected: all pass (KeyValueEditor has no unit tests, but existing tests should still pass).

- [ ] **Step 6: Commit**

```bash
git add src/components/request/KeyValueEditor.tsx src/components/request/HeadersEditor.tsx src/components/request/QueryParamsEditor.tsx
git commit -m "feat(variables): wire variableContext through KeyValueEditor, HeadersEditor, QueryParamsEditor"
```

---

## Task 4: `PathParamsPanel` — swap value input

**Files:**
- Modify: `src/components/request/PathParamsPanel.tsx`

- [ ] **Step 1: Update `PathParamsPanel.tsx`**

```tsx
import { useCallback } from 'react';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';
import { VariableAwareInput } from './VariableAwareInput';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

/**
 * Read-only path params display. Keys are derived from the URL
 * (e.g. `:id` segments) and cannot be added, removed, or renamed.
 * Only values and the enabled toggle are editable.
 */
export function PathParamsPanel({ params, onChange, variableContext, onNavigateToSource }: PathParamsPanelProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(params.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [params, onChange],
  );

  return (
    <div className='space-y-2'>
      <div className='text-xs font-medium text-muted-foreground'>Path</div>
      {params.length === 0 ? (
        <p className='text-xs text-muted-foreground italic px-1'>
          No path params. Add <span className='font-mono'>:param</span> segments to the URL.
        </p>
      ) : (
        params.map((entry) => (
          <div key={entry.id} className='flex gap-2 items-center'>
            <Checkbox
              checked={entry.enabled}
              onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
              aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key}`}
            />
            <Input
              value={entry.key}
              readOnly
              tabIndex={-1}
              className='flex-1 text-xs bg-muted/50 cursor-default'
            />
            <VariableAwareInput
              placeholder='Value'
              value={entry.value}
              onChange={(val) => updateEntry(entry.id, { value: val })}
              className='flex-1'
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
            {/* No remove button — path params are controlled by the URL. */}
            <div className='w-7' />
          </div>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check + lint**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/request/PathParamsPanel.tsx
git commit -m "feat(variables): add variable highlighting to PathParamsPanel value field"
```

---

## Task 5: `AuthEditor` — swap all text value inputs

**Files:**
- Modify: `src/components/request/AuthEditor.tsx`

The auth editor has many `<Input>` fields across basic, bearer, api-key, oauth2, and aws-sig-v4 sections. All text value inputs (not dropdowns, not read-only access token displays, not password inputs that store secrets) get swapped to `<VariableAwareInput>`.

Fields to swap (value inputs only, not type-password security fields):
- Basic: `username` field (password stays `type='password'` — use plain Input)
- Bearer: `token`
- API Key: `key` field, `value` field
- OAuth2: `authorizationUrl`, `tokenUrl`, `callbackUrl`, `clientId`, `scope`, `state`, `username`, `headerPrefix`
- AWS SigV4: `accessKey`, `region`, `service`

Fields that stay as plain `<Input>`:
- All `type='password'` inputs (basic password, oauth2 clientSecret/password, AWS secretKey/sessionToken)
- Read-only `accessToken` and `refreshToken` display fields (already `readOnly`)

- [ ] **Step 1: Add props and import to `AuthEditor.tsx`**

At the top of the file, add the imports and update the props interface:

In `AuthEditor.tsx`, change:
```tsx
import type { AuthState } from '@/types/pane-types';
```
to:
```tsx
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';
import { VariableAwareInput } from './VariableAwareInput';
```

Change the `AuthEditorProps` interface from:
```tsx
interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
}
```
to:
```tsx
interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}
```

Change the function signature from:
```tsx
export function AuthEditor({ auth, onChange }: AuthEditorProps) {
```
to:
```tsx
export function AuthEditor({ auth, onChange, variableContext, onNavigateToSource }: AuthEditorProps) {
```

- [ ] **Step 2: Swap Basic auth username field**

Find:
```tsx
      {auth.authType === 'basic' && auth.basic && (
        <div className='space-y-2'>
          <div className='flex items-center gap-2'>
            <User className='h-3.5 w-3.5 text-muted-foreground' />
            <Input
              placeholder='Username'
              className='flex-1 text-sm'
              value={auth.basic.username}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic, username: e.target.value } as NonNullable<
                    AuthState['basic']
                  >,
                })
              }
            />
          </div>
```

Replace with:
```tsx
      {auth.authType === 'basic' && auth.basic && (
        <div className='space-y-2'>
          <div className='flex items-center gap-2'>
            <User className='h-3.5 w-3.5 text-muted-foreground' />
            <VariableAwareInput
              placeholder='Username'
              className='flex-1'
              value={auth.basic.username}
              onChange={(val) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic, username: val } as NonNullable<AuthState['basic']>,
                })
              }
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
```

- [ ] **Step 3: Swap Bearer token field**

Find:
```tsx
      {auth.authType === 'bearer' && auth.bearer && (
        <div className='flex items-center gap-2'>
          <Key className='h-3.5 w-3.5 text-muted-foreground' />
          <Input
            placeholder='Token'
            className='flex-1 text-sm'
            value={auth.bearer.token}
            onChange={(e) =>
              onChange({
                ...auth,
                bearer: { token: e.target.value },
              })
            }
          />
        </div>
      )}
```

Replace with:
```tsx
      {auth.authType === 'bearer' && auth.bearer && (
        <div className='flex items-center gap-2'>
          <Key className='h-3.5 w-3.5 text-muted-foreground' />
          <VariableAwareInput
            placeholder='Token'
            className='flex-1'
            value={auth.bearer.token}
            onChange={(val) =>
              onChange({
                ...auth,
                bearer: { token: val },
              })
            }
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
        </div>
      )}
```

- [ ] **Step 4: Swap API Key fields**

Find the API Key section (both `key` and `value` inputs):
```tsx
      {auth.authType === 'api-key' && auth.apiKey && (
        <div className='space-y-2'>
          <Input
            placeholder='Key'
            className='text-sm'
            value={auth.apiKey.key}
            onChange={(e) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey, key: e.target.value } as NonNullable<AuthState['apiKey']>,
              })
            }
          />
          <Input
            placeholder='Value'
            className='text-sm'
            value={auth.apiKey.value}
            onChange={(e) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey, value: e.target.value } as NonNullable<
                  AuthState['apiKey']
                >,
              })
            }
          />
```

Replace with:
```tsx
      {auth.authType === 'api-key' && auth.apiKey && (
        <div className='space-y-2'>
          <VariableAwareInput
            placeholder='Key'
            value={auth.apiKey.key}
            onChange={(val) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey, key: val } as NonNullable<AuthState['apiKey']>,
              })
            }
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
          <VariableAwareInput
            placeholder='Value'
            value={auth.apiKey.value}
            onChange={(val) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey, value: val } as NonNullable<AuthState['apiKey']>,
              })
            }
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
```

- [ ] **Step 5: Swap OAuth2 text value fields**

In the OAuth2 section, replace these `<Input>` fields with `<VariableAwareInput>`:

**Authorization URL** — find:
```tsx
                  <Input
                    className='text-sm font-mono'
                    placeholder='https://auth.example.com/authorize'
                    value={o.authorizationUrl}
                    onChange={(e) => patchOAuth2({ authorizationUrl: e.target.value })}
                  />
```
Replace with:
```tsx
                  <VariableAwareInput
                    placeholder='https://auth.example.com/authorize'
                    value={o.authorizationUrl}
                    onChange={(val) => patchOAuth2({ authorizationUrl: val })}
                    variableContext={variableContext}
                    onNavigateToSource={onNavigateToSource}
                  />
```

**Token URL** — find:
```tsx
                  <Input
                    className='text-sm font-mono'
                    placeholder='https://auth.example.com/token'
                    value={o.tokenUrl}
                    onChange={(e) => patchOAuth2({ tokenUrl: e.target.value })}
                  />
```
Replace with:
```tsx
                  <VariableAwareInput
                    placeholder='https://auth.example.com/token'
                    value={o.tokenUrl}
                    onChange={(val) => patchOAuth2({ tokenUrl: val })}
                    variableContext={variableContext}
                    onNavigateToSource={onNavigateToSource}
                  />
```

**Callback URL input** (inside the flex div, not the Copy button) — find:
```tsx
                      <Input
                        className='text-sm font-mono flex-1'
                        value={o.callbackUrl}
                        onChange={(e) => patchOAuth2({ callbackUrl: e.target.value })}
                      />
```
Replace with:
```tsx
                      <VariableAwareInput
                        className='flex-1'
                        value={o.callbackUrl}
                        onChange={(val) => patchOAuth2({ callbackUrl: val })}
                        variableContext={variableContext}
                        onNavigateToSource={onNavigateToSource}
                      />
```

**State field** — find:
```tsx
                    <Input
                      className='text-sm'
                      placeholder='Leave empty for auto-generated'
                      value={o.state}
                      onChange={(e) => patchOAuth2({ state: e.target.value })}
                    />
```
Replace with:
```tsx
                    <VariableAwareInput
                      placeholder='Leave empty for auto-generated'
                      value={o.state}
                      onChange={(val) => patchOAuth2({ state: val })}
                      variableContext={variableContext}
                      onNavigateToSource={onNavigateToSource}
                    />
```

**Client ID** — find:
```tsx
                  <Input
                    className='text-sm'
                    placeholder='client-id'
                    value={o.clientId}
                    onChange={(e) => patchOAuth2({ clientId: e.target.value })}
                  />
```
Replace with:
```tsx
                  <VariableAwareInput
                    placeholder='client-id'
                    value={o.clientId}
                    onChange={(val) => patchOAuth2({ clientId: val })}
                    variableContext={variableContext}
                    onNavigateToSource={onNavigateToSource}
                  />
```

**Scope** — find:
```tsx
                <Input
                  className='text-sm'
                  placeholder='read write'
                  value={o.scope}
                  onChange={(e) => patchOAuth2({ scope: e.target.value })}
                />
```
Replace with:
```tsx
                <VariableAwareInput
                  placeholder='read write'
                  value={o.scope}
                  onChange={(val) => patchOAuth2({ scope: val })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
```

**Username (password grant)** — find:
```tsx
                    <Input
                      className='text-sm'
                      placeholder='user@example.com'
                      value={o.username}
                      onChange={(e) => patchOAuth2({ username: e.target.value })}
                    />
```
Replace with:
```tsx
                    <VariableAwareInput
                      placeholder='user@example.com'
                      value={o.username}
                      onChange={(val) => patchOAuth2({ username: val })}
                      variableContext={variableContext}
                      onNavigateToSource={onNavigateToSource}
                    />
```

**Header Prefix (advanced)** — find:
```tsx
                    <Input
                      className='text-sm'
                      value={o.headerPrefix}
                      onChange={(e) => patchOAuth2({ headerPrefix: e.target.value })}
                    />
```
Replace with:
```tsx
                    <VariableAwareInput
                      value={o.headerPrefix}
                      onChange={(val) => patchOAuth2({ headerPrefix: val })}
                      variableContext={variableContext}
                      onNavigateToSource={onNavigateToSource}
                    />
```

- [ ] **Step 6: Swap AWS SigV4 text fields**

**Access Key** — find:
```tsx
          <Input
            className='text-sm'
            placeholder='AKIAIOSFODNN7EXAMPLE'
            value={auth.awsSigV4.accessKey}
            onChange={(e) => patchAWS({ accessKey: e.target.value })}
          />
```
Replace with:
```tsx
          <VariableAwareInput
            placeholder='AKIAIOSFODNN7EXAMPLE'
            value={auth.awsSigV4.accessKey}
            onChange={(val) => patchAWS({ accessKey: val })}
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
```

**Region** — find:
```tsx
              <Input
                className='text-sm'
                placeholder='us-east-1'
                value={auth.awsSigV4.region}
                onChange={(e) => patchAWS({ region: e.target.value })}
              />
```
Replace with:
```tsx
              <VariableAwareInput
                placeholder='us-east-1'
                value={auth.awsSigV4.region}
                onChange={(val) => patchAWS({ region: val })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
```

**Service** — find:
```tsx
              <Input
                className='text-sm'
                placeholder='execute-api'
                value={auth.awsSigV4.service}
                onChange={(e) => patchAWS({ service: e.target.value })}
              />
```
Replace with:
```tsx
              <VariableAwareInput
                placeholder='execute-api'
                value={auth.awsSigV4.service}
                onChange={(val) => patchAWS({ service: val })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
```

- [ ] **Step 7: TypeScript check + lint**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/components/request/AuthEditor.tsx
git commit -m "feat(variables): add variable highlighting to all AuthEditor text value fields"
```

---

## Task 6: `BodyEditor` + `MonacoWrapper` variable integration

**Files:**
- Modify: `src/components/request/BodyEditor.tsx`
- Modify: `src/components/editor/MonacoWrapper.tsx`

### Part A — `BodyEditor.tsx`

- [ ] **Step 1: Add props and pass-throughs in `BodyEditor.tsx`**

Change the import block at the top to add:
```tsx
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
```

Change the `BodyEditorProps` interface from:
```tsx
interface BodyEditorProps {
  body: BodyState;
  onChange: (body: BodyState) => void;
}
```
to:
```tsx
interface BodyEditorProps {
  body: BodyState;
  onChange: (body: BodyState) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}
```

Change the function signature from:
```tsx
export function BodyEditor({ body, onChange }: BodyEditorProps) {
```
to:
```tsx
export function BodyEditor({ body, onChange, variableContext, onNavigateToSource }: BodyEditorProps) {
```

Pass `variableContext` to `MonacoWrapper` — find:
```tsx
            <MonacoWrapper
              value={body.content}
              onChange={(val) => setContent(val)}
              bodyMode={body.mode}
              height='100%'
            />
```
Replace with:
```tsx
            <MonacoWrapper
              value={body.content}
              onChange={(val) => setContent(val)}
              bodyMode={body.mode}
              height='100%'
              variableContext={variableContext}
            />
```

Pass `variableContext` to the formdata `KeyValueEditor` — find:
```tsx
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
        />
```
Replace with:
```tsx
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
```

### Part B — `MonacoWrapper.tsx` — decorations + hover provider

- [ ] **Step 2: Rewrite `MonacoWrapper.tsx`**

Replace the entire file content:

```tsx
import Editor, { type OnMount } from '@monaco-editor/react';
import { useEffect, useRef } from 'react';
import { parseTextTokens } from '@/lib/text-variables';
import { sourceBadgeClass, type VariableScopeEntry } from '@/lib/url-variables';
import { EditorSkeleton } from './EditorSkeleton';
import { BASE_EDITOR_OPTIONS, detectLanguage, READONLY_OPTIONS } from './monaco-config';
import { useMonacoTheme } from './useMonacoTheme';

interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
  variableContext?: Map<string, VariableScopeEntry>;
}

// CSS class names injected by deltaDecorations for each variable source.
// These must be registered as Monaco CSS classes via addExtraLib or injected directly.
// We inject them as a <style> tag on first mount to avoid Monaco API complexity.
const VAR_DECORATION_CLASSES: Record<string, string> = {
  environment: 'var-deco-environment',
  collection: 'var-deco-collection',
  global: 'var-deco-global',
  folder: 'var-deco-folder',
  request: 'var-deco-request',
  process: 'var-deco-process',
  runtime: 'var-deco-runtime',
  unresolved: 'var-deco-unresolved',
};

// Source → approximate Tailwind color values for the injected style tag.
const VAR_DECO_STYLES: Record<string, string> = {
  'var-deco-environment': 'background:hsl(var(--primary)/0.15);color:hsl(var(--primary));border-radius:3px;',
  'var-deco-collection': 'background:rgb(59 130 246/0.15);color:rgb(59 130 246);border-radius:3px;',
  'var-deco-global': 'background:rgb(20 184 166/0.15);color:rgb(20 184 166);border-radius:3px;',
  'var-deco-folder': 'background:rgb(245 158 11/0.15);color:rgb(245 158 11);border-radius:3px;',
  'var-deco-request': 'background:rgb(168 85 247/0.15);color:rgb(168 85 247);border-radius:3px;',
  'var-deco-process': 'background:hsl(var(--muted));color:hsl(var(--muted-foreground));border-radius:3px;',
  'var-deco-runtime': 'background:rgb(249 115 22/0.15);color:rgb(249 115 22);border-radius:3px;',
  'var-deco-unresolved': 'background:hsl(var(--destructive)/0.15);color:hsl(var(--destructive));border-radius:3px;',
};

let decoStyleInjected = false;

function injectDecoStyles() {
  if (decoStyleInjected) return;
  decoStyleInjected = true;
  const style = document.createElement('style');
  style.textContent = Object.entries(VAR_DECO_STYLES)
    .map(([cls, css]) => `.${cls}{${css}}`)
    .join('\n');
  document.head.appendChild(style);
}

export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  variableContext,
}: MonacoWrapperProps) {
  const { themeName, defineThemes } = useMonacoTheme();
  const resolvedLanguage = language ?? detectLanguage(bodyMode, contentType);
  const options = readOnly ? READONLY_OPTIONS : BASE_EDITOR_OPTIONS;

  // Keep a stable ref to variableContext so hover provider always reads current value.
  const variableContextRef = useRef(variableContext);
  variableContextRef.current = variableContext;

  // Refs to hold the editor instance and disposables from onMount.
  type EditorInstance = Parameters<OnMount>[0];
  type MonacoInstance = Parameters<OnMount>[1];
  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoInstance | null>(null);
  const decorationIdsRef = useRef<string[]>([]);
  const disposablesRef = useRef<{ dispose(): void }[]>([]);

  const handleMount: OnMount = (editor, monaco) => {
    defineThemes(monaco);
    editorRef.current = editor;
    monacoRef.current = monaco;

    injectDecoStyles();

    // Register hover providers once per language ID.
    const LANG_IDS = ['json', 'xml', 'plaintext'];
    for (const langId of LANG_IDS) {
      const d = monaco.languages.registerHoverProvider(langId, {
        provideHover: (model, position) => {
          const ctx = variableContextRef.current;
          if (!ctx) return null;

          const text = model.getValue();
          const tokens = parseTextTokens(text);
          // Convert line/column to a linear character offset.
          const lines = text.split('\n');
          let offset = 0;
          for (let l = 0; l < position.lineNumber - 1; l++) {
            offset += (lines[l]?.length ?? 0) + 1; // +1 for the newline character
          }
          offset += position.column - 1;

          // Walk tokens to find which variable (if any) the cursor is inside.
          let charPos = 0;
          for (const token of tokens) {
            if (token.type === 'variable' && offset >= charPos && offset < charPos + token.rawLength) {
              const entry = ctx.get(token.content);
              const resolved = entry?.secret ? '●●●●' : (entry?.value ?? '*(not set)*');
              const source = entry ? ` *(${entry.label})*` : '';
              return {
                contents: [{ value: `**\`{{${token.content}}}\`** → \`${resolved}\`${source}` }],
              };
            }
            charPos += token.rawLength;
          }
          return null;
        },
      });
      disposablesRef.current.push(d);
    }

    // Apply initial decorations and subscribe to content changes.
    applyDecorations(editor, monaco, variableContextRef.current);
    const cd = editor.onDidChangeModelContent(() => {
      applyDecorations(editor, monaco, variableContextRef.current);
    });
    disposablesRef.current.push(cd);
  };

  // Re-apply decorations when variableContext changes.
  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (editor && monaco) {
      applyDecorations(editor, monaco, variableContext);
    }
  }, [variableContext]);

  // Dispose all providers and listeners on unmount.
  useEffect(() => {
    return () => {
      for (const d of disposablesRef.current) d.dispose();
      disposablesRef.current = [];
    };
  }, []);

  function applyDecorations(
    editor: EditorInstance,
    monaco: MonacoInstance,
    ctx: Map<string, VariableScopeEntry> | undefined,
  ) {
    const model = editor.getModel();
    if (!model) return;
    if (!ctx) {
      decorationIdsRef.current = editor.deltaDecorations(decorationIdsRef.current, []);
      return;
    }

    const text = model.getValue();
    const tokens = parseTextTokens(text);
    const newDecorations: Parameters<typeof editor.deltaDecorations>[1] = [];
    let charOffset = 0;

    for (const token of tokens) {
      if (token.type === 'variable') {
        const startPos = model.getPositionAt(charOffset);
        const endPos = model.getPositionAt(charOffset + token.rawLength);
        const entry = ctx.get(token.content);
        const cssClass = entry
          ? (VAR_DECORATION_CLASSES[entry.source] ?? 'var-deco-unresolved')
          : VAR_DECORATION_CLASSES.unresolved;
        newDecorations.push({
          range: new monaco.Range(
            startPos.lineNumber,
            startPos.column,
            endPos.lineNumber,
            endPos.column,
          ),
          options: { inlineClassName: cssClass },
        });
      }
      charOffset += token.rawLength;
    }

    decorationIdsRef.current = editor.deltaDecorations(decorationIdsRef.current, newDecorations);
  }

  return (
    <Editor
      height={height}
      language={resolvedLanguage}
      value={value}
      onChange={(val) => onChange?.(val ?? '')}
      onMount={handleMount}
      theme={themeName}
      options={options}
      loading={<EditorSkeleton />}
    />
  );
}
```

- [ ] **Step 3: TypeScript check + lint**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
```

Expected: no errors. If TypeScript complains about `editor.deltaDecorations` second parameter type, cast `newDecorations` as `import('monaco-editor').editor.IModelDeltaDecoration[]`.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/BodyEditor.tsx src/components/editor/MonacoWrapper.tsx
git commit -m "feat(variables): add variable decorations and hover tooltips to Monaco body editor"
```

---

## Task 7: `RequestPanel` — wire `scopedContext` to all editors

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

`scopedContext` and `handleNavigateToSource` are already built in `RequestPanel`. This task passes them through to the newly-upgraded editors.

`handleNavigateToSource` currently takes `source: VariableSource | 'pathParam'`. The editors use a different signature: `(source: VariableSource, key: string)`. We need to adapt the call.

- [ ] **Step 1: Add a wrapped navigate handler for editors**

In `RequestPanel.tsx`, after the `handleNavigateToSource` callback, add a new callback:

Find (within the `handleNavigateToSource` block, this is after line ~440):
```tsx
    [tab.source?.collection],
  );
```

Add after it:
```tsx
  // Adapter for components that pass (source, key) — key is not used for navigation routing.
  const handleEditorNavigateToSource = useCallback(
    (source: VariableSource, _key: string) => {
      handleNavigateToSource(source);
    },
    [handleNavigateToSource],
  );
```

- [ ] **Step 2: Pass context to `PathParamsPanel`**

Find:
```tsx
                <PathParamsPanel params={request.pathParams} onChange={handlePathParamsChange} />
```

Replace with:
```tsx
                <PathParamsPanel
                  params={request.pathParams}
                  onChange={handlePathParamsChange}
                  variableContext={scopedContext}
                  onNavigateToSource={handleEditorNavigateToSource}
                />
```

- [ ] **Step 3: Pass context to `QueryParamsEditor`**

Find:
```tsx
                <QueryParamsEditor params={request.queryParams} onChange={handleParamsChange} />
```

Replace with:
```tsx
                <QueryParamsEditor
                  params={request.queryParams}
                  onChange={handleParamsChange}
                  variableContext={scopedContext}
                  onNavigateToSource={handleEditorNavigateToSource}
                />
```

- [ ] **Step 4: Pass context to `HeadersEditor`**

Find:
```tsx
              <HeadersEditor headers={request.headers} onChange={handleHeadersChange} />
```

Replace with:
```tsx
              <HeadersEditor
                headers={request.headers}
                onChange={handleHeadersChange}
                variableContext={scopedContext}
                onNavigateToSource={handleEditorNavigateToSource}
              />
```

- [ ] **Step 5: Pass context to `BodyEditor`**

Find:
```tsx
              <BodyEditor body={request.body} onChange={handleBodyChange} />
```

Replace with:
```tsx
              <BodyEditor
                body={request.body}
                onChange={handleBodyChange}
                variableContext={scopedContext}
                onNavigateToSource={handleEditorNavigateToSource}
              />
```

- [ ] **Step 6: Pass context to `AuthEditor`**

Find:
```tsx
              <AuthEditor auth={request.auth} onChange={handleAuthChange} />
```

Replace with:
```tsx
              <AuthEditor
                auth={request.auth}
                onChange={handleAuthChange}
                variableContext={scopedContext}
                onNavigateToSource={handleEditorNavigateToSource}
              />
```

- [ ] **Step 7: TypeScript check + lint + tests**

```bash
yarn tsc --noEmit 2>&1 | head -30
yarn check 2>&1 | head -30
yarn test
```

Expected: all pass, no type errors, no lint errors.

- [ ] **Step 8: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(variables): thread scopedContext from RequestPanel to all request editors"
```

---

## Final: Build Verification

- [ ] **Step 1: Full build check**

```bash
yarn tsc --noEmit
yarn check
yarn build
```

Expected: no type errors, no lint errors, build succeeds.

- [ ] **Step 2: Run all tests**

```bash
yarn test
```

Expected: all pass.
