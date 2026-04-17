# ContentEditable Input Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fragile "transparent `<input>` + absolute overlay `<div>`" architecture in `VariableAwareInput` and `VariableAwareUrlInput` with a single `<div contenteditable>` that both accepts keyboard input and renders styled token highlights natively.

**Architecture:** A new `useContentEditableInput` hook owns all contenteditable complexity — caret save/restore, DOM sync from state, IME composition guard, and paste normalization. Both variable-aware components are rewritten to use this hook, eliminating the overlay layer entirely. All caller APIs (`onChange: (value: string) => void` and all other props) remain identical.

**Tech Stack:** React 19, TypeScript 5.8, Tailwind 4, Radix UI (shadcn/ui), Vitest 4 + @testing-library/react 16, jsdom

---

## Background: Why This Refactor

The current architecture renders two stacked layers:

1. A real `<input>` with `text-transparent` — handles keystrokes, caret, selection, accessibility
2. An `absolute inset-0 pointer-events-none` overlay `<div>` — renders colored token badges on top

These layers cannot stay perfectly aligned. Known bugs already patched with workarounds:
- Trailing whitespace shows as phantom space (`trimEnd()` workaround added)
- Text selection invisible (`selection:bg-primary/30` hack added)
- Font rendering can diverge between `<input>` and `<div>` across OS/browsers

Every future padding, font, or border change risks re-introducing misalignment. The fix is a single rendering layer.

---

## New Architecture

```
<div>                                       ← wrapper: focus ring, border, sizing
  <div contenteditable="true">              ← editor: typing + caret + highlights
    "Bearer "                               ← Text node
    <span data-badge data-token-idx="0">    ← badge span (variable/pathParam)
      {{token}}
    </span>
    " rest"                                 ← Text node
  </div>
  <!-- Popovers rendered as siblings outside the editor div via Radix -->
</div>
```

No overlay. No `text-transparent`. No alignment hacks. Selection works natively.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/hooks/useContentEditableInput.ts` | **Create** | Core hook: caret helpers, DOM sync effect, IME guard, paste normalization |
| `src/components/request/VariableAwareInput.tsx` | **Rewrite** | Single-field input with variable token highlighting |
| `src/components/request/VariableAwareUrlInput.tsx` | **Rewrite** | URL bar with variable/pathParam/queryKey highlighting + cURL paste |
| `src/components/request/__tests__/VariableAwareInput.test.tsx` | **Rewrite** | Remove overlay assertions; add contenteditable assertions |
| `src/components/request/__tests__/useContentEditableInput.test.ts` | **Create** | Unit tests for the hook in isolation |

**Files that do NOT change** (all callers):
- `src/components/request/AuthEditor.tsx`
- `src/components/request/KeyValueEditor.tsx`
- `src/components/request/PathParamsPanel.tsx`
- `src/components/request/RequestPanel.tsx`
- `src/lib/text-variables.ts`
- `src/lib/url-variables.ts`

---

## Task 1: Write Caret Helper Utilities (Inside the Hook File)

**Files:**
- Create: `src/hooks/useContentEditableInput.ts`
- Test: `src/components/request/__tests__/useContentEditableInput.test.ts`

These two functions are the foundation. `saveCaret` converts the browser's cursor position into a flat character offset. `restoreCaret` puts it back. They treat the entire editor div as a flat text stream — text nodes contribute their length, badge spans contribute their `textContent.length`.

- [ ] **Step 1: Create the hook file with only the caret helpers**

Create `src/hooks/useContentEditableInput.ts`:

```ts
/**
 * Converts the browser's current cursor position inside `el` to a flat
 * character offset, treating the whole subtree as a plain-text stream.
 * Text nodes contribute their length; element nodes contribute their
 * textContent length (badge spans fall into this category).
 * Returns 0 if no selection is active or el is empty.
 */
export function saveCaret(el: HTMLElement): number {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return 0;

  const range = sel.getRangeAt(0);
  let offset = 0;

  function walk(node: Node): boolean {
    if (node === range.startContainer) {
      offset += range.startOffset;
      return true; // done
    }
    if (node.nodeType === Node.TEXT_NODE) {
      offset += (node as Text).length;
      return false;
    }
    for (const child of Array.from(node.childNodes)) {
      if (walk(child)) return true;
    }
    return false;
  }

  walk(el);
  return offset;
}

/**
 * Places the browser cursor at `targetOffset` (flat character offset) inside `el`.
 * If the offset falls inside a badge span, the caret is placed at the text node
 * immediately before the span to prevent the caret from getting stuck inside it.
 */
export function restoreCaret(el: HTMLElement, targetOffset: number): void {
  const sel = window.getSelection();
  if (!sel) return;

  let remaining = targetOffset;
  let targetNode: Node | null = null;
  let localOffset = 0;

  function walk(node: Node): boolean {
    if (node.nodeType === Node.TEXT_NODE) {
      const len = (node as Text).length;
      if (remaining <= len) {
        targetNode = node;
        localOffset = remaining;
        return true;
      }
      remaining -= len;
      return false;
    }
    // Element node (badge span or other): traverse children but never
    // place the caret inside a [data-badge] span — land before it instead.
    if ((node as Element).hasAttribute?.('data-badge')) {
      const len = (node.textContent ?? '').length;
      if (remaining < len) {
        // Caret would land inside this badge — place it at the preceding text
        // node boundary instead. Find the previous sibling text node.
        let prev = node.previousSibling;
        while (prev && prev.nodeType !== Node.TEXT_NODE) {
          prev = prev.previousSibling;
        }
        if (prev) {
          targetNode = prev;
          localOffset = (prev as Text).length;
        } else {
          // No preceding text node — place at start of parent.
          targetNode = node.parentNode;
          localOffset = 0;
        }
        return true;
      }
      remaining -= len;
      return false;
    }
    for (const child of Array.from(node.childNodes)) {
      if (walk(child)) return true;
    }
    return false;
  }

  walk(el);

  if (!targetNode) {
    // Offset past end of content — place at last text node.
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    let last: Node | null = null;
    while (walker.nextNode()) last = walker.currentNode;
    if (last) {
      targetNode = last;
      localOffset = (last as Text).length;
    } else {
      return; // Empty element.
    }
  }

  try {
    const range = document.createRange();
    range.setStart(targetNode, localOffset);
    range.collapse(true);
    sel.removeAllRanges();
    sel.addRange(range);
  } catch {
    // Ignore stale node errors during rapid updates.
  }
}

/**
 * Reads the editor div's current DOM and returns the raw user string,
 * collapsing the node structure back into plain text.
 * Text nodes → their text. Element nodes → their textContent.
 */
export function serializeToText(el: HTMLElement): string {
  let result = '';
  for (const node of Array.from(el.childNodes)) {
    result += node.textContent ?? '';
  }
  return result;
}
```

- [ ] **Step 2: Write tests for the caret helpers**

Create `src/components/request/__tests__/useContentEditableInput.test.ts`:

```ts
import { describe, expect, it, beforeEach } from 'vitest';
import { saveCaret, restoreCaret, serializeToText } from '@/hooks/useContentEditableInput';

function makeEditor(html: string): HTMLDivElement {
  const div = document.createElement('div');
  div.contentEditable = 'true';
  div.innerHTML = html;
  document.body.appendChild(div);
  return div;
}

function placeCaretAt(node: Node, offset: number) {
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
}

describe('serializeToText', () => {
  it('reads plain text node', () => {
    const el = makeEditor('hello');
    expect(serializeToText(el)).toBe('hello');
    el.remove();
  });

  it('reads text + badge span', () => {
    const el = makeEditor('Bearer <span data-badge>{{token}}</span>');
    expect(serializeToText(el)).toBe('Bearer {{token}}');
    el.remove();
  });
});

describe('saveCaret', () => {
  it('returns flat offset in plain text node', () => {
    const el = makeEditor('hello world');
    placeCaretAt(el.firstChild!, 5);
    expect(saveCaret(el)).toBe(5);
    el.remove();
  });

  it('returns flat offset past a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    // 'ab' = 2 chars, '{{x}}' = 5 chars, place caret at offset 1 in 'cd'
    const cdNode = el.lastChild!;
    placeCaretAt(cdNode, 1);
    expect(saveCaret(el)).toBe(8); // 2 + 5 + 1
    el.remove();
  });
});

describe('restoreCaret', () => {
  it('places caret at correct offset in plain text', () => {
    const el = makeEditor('hello');
    document.body.appendChild(el);
    restoreCaret(el, 3);
    const sel = window.getSelection()!;
    expect(sel.anchorOffset).toBe(3);
    el.remove();
  });

  it('does not place caret inside a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    document.body.appendChild(el);
    // Offset 3 falls inside the badge (ab=2, then char 0 of {{x}})
    restoreCaret(el, 3);
    const sel = window.getSelection()!;
    // Caret should be at the text node 'ab', offset 2 (end of it)
    expect(sel.anchorNode?.textContent).toBe('ab');
    expect(sel.anchorOffset).toBe(2);
    el.remove();
  });
});
```

- [ ] **Step 3: Run the tests — expect them to fail because the file has no exports yet (they will fail with import errors)**

```bash
yarn test --run useContentEditableInput
```

Expected: import errors or test failures — confirms test wiring works.

- [ ] **Step 4: Run tests again — they should pass now that the file exists**

```bash
yarn test --run useContentEditableInput
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useContentEditableInput.ts src/components/request/__tests__/useContentEditableInput.test.ts
git commit -m "feat(input): add caret helpers for contenteditable refactor"
```

---

## Task 2: Add `renderTokens` and the Hook Itself

**Files:**
- Modify: `src/hooks/useContentEditableInput.ts`
- Modify: `src/components/request/__tests__/useContentEditableInput.test.ts`

`renderTokens` imperatively diffs the editor div's DOM against a desired token list. The hook wires everything together: `onInput`, `onPaste`, `onCompositionStart/End`, and the `useEffect` that syncs the DOM when `value` changes from outside.

- [ ] **Step 1: Write a failing test for the hook**

Add to `src/components/request/__tests__/useContentEditableInput.test.ts`:

```ts
import { renderHook, act } from '@testing-library/react';
import { useContentEditableInput, type EditorToken } from '@/hooks/useContentEditableInput';

describe('useContentEditableInput', () => {
  it('calls onChange when the user types', () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    document.body.appendChild(el);

    const onChange = vi.fn();
    const tokens: EditorToken[] = [{ type: 'text', content: 'hello', rawLength: 5 }];

    const { result } = renderHook(() =>
      useContentEditableInput({ editorEl: el, value: 'hello', onChange, tokens }),
    );

    // Simulate a user typing: modify DOM then fire input event.
    act(() => {
      el.textContent = 'hello!';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    });

    expect(onChange).toHaveBeenCalledWith('hello!');
    el.remove();
  });

  it('does not call onChange during IME composition', () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    document.body.appendChild(el);

    const onChange = vi.fn();
    const { result } = renderHook(() =>
      useContentEditableInput({ editorEl: el, value: '', onChange, tokens: [] }),
    );

    act(() => {
      el.dispatchEvent(new CompositionEvent('compositionstart'));
      el.textContent = 'interim';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    });

    expect(onChange).not.toHaveBeenCalled();

    act(() => {
      el.dispatchEvent(new CompositionEvent('compositionend'));
    });

    expect(onChange).toHaveBeenCalledWith('interim');
    el.remove();
  });
});
```

- [ ] **Step 2: Run — expect failures (hook not yet exported)**

```bash
yarn test --run useContentEditableInput
```

Expected: import error on `useContentEditableInput`.

- [ ] **Step 3: Add `renderTokens`, `EditorToken`, and the hook to `useContentEditableInput.ts`**

Append to `src/hooks/useContentEditableInput.ts`:

```ts
import { useEffect, useRef } from 'react';

export interface EditorToken {
  type: 'text' | 'badge';
  /** Display text — for badge tokens this is the full `{{name}}` or `:param` string. */
  content: string;
  rawLength: number;
  /** CSS classes applied to the badge span. Only present when type === 'badge'. */
  badgeClass?: string;
  /** Index stored as data-token-idx on the span, for popover targeting. */
  tokenIdx?: number;
}

/**
 * Imperatively diffs `el.childNodes` against `tokens` and mutates the DOM
 * to match. Unchanged nodes (same type and content) are left in place to
 * preserve the browser's internal caret tracking. After mutation, restores
 * the caret to `caretOffset`.
 */
export function renderTokens(
  el: HTMLElement,
  tokens: EditorToken[],
  caretOffset: number,
): void {
  const desired: Node[] = tokens.map((token) => {
    if (token.type === 'text') {
      return document.createTextNode(token.content);
    }
    const span = document.createElement('span');
    span.setAttribute('data-badge', '');
    span.setAttribute('data-token-idx', String(token.tokenIdx ?? 0));
    if (token.badgeClass) span.className = token.badgeClass;
    span.textContent = token.content;
    return span;
  });

  const current = Array.from(el.childNodes);

  // Replace or insert nodes that differ.
  desired.forEach((node, i) => {
    const existing = current[i];
    if (!existing) {
      el.appendChild(node);
      return;
    }
    const sameType =
      node.nodeType === existing.nodeType &&
      (node.nodeType !== Node.ELEMENT_NODE ||
        (node as Element).tagName === (existing as Element).tagName);
    const sameContent = node.textContent === existing.textContent;
    const sameClass =
      node.nodeType !== Node.ELEMENT_NODE ||
      (node as Element).className === (existing as Element).className;

    if (sameType && sameContent && sameClass) return; // Unchanged — leave it alone.
    el.replaceChild(node, existing);
  });

  // Remove extra nodes.
  while (el.childNodes.length > desired.length) {
    el.removeChild(el.lastChild!);
  }

  restoreCaret(el, caretOffset);
}

export interface UseContentEditableInputOptions {
  /** The editor div DOM element (must be stable across renders — use a ref). */
  editorEl: HTMLElement | null;
  value: string;
  onChange: (value: string) => void;
  tokens: EditorToken[];
  /** Called before the hook's paste handler. Return true if the event was fully handled. */
  onBeforePaste?: (e: ClipboardEvent) => boolean;
}

/**
 * Wires a `contenteditable` div to a React-controlled string value.
 * Attach the returned event handlers to the editor div.
 */
export function useContentEditableInput({
  editorEl,
  value,
  onChange,
  tokens,
  onBeforePaste,
}: UseContentEditableInputOptions) {
  const isComposing = useRef(false);

  // Sync DOM → state when the user types (not during IME composition).
  function onInput() {
    if (isComposing.current) return;
    if (!editorEl) return;
    onChange(serializeToText(editorEl));
  }

  function onCompositionStart() {
    isComposing.current = true;
  }

  function onCompositionEnd() {
    isComposing.current = false;
    if (!editorEl) return;
    onChange(serializeToText(editorEl));
  }

  function onPaste(e: ClipboardEvent) {
    if (onBeforePaste?.(e)) return; // Caller handled it (e.g. cURL import).
    e.preventDefault();
    const plain = e.clipboardData?.getData('text/plain') ?? '';
    // insertText is the standard cross-browser way to insert at caret in contenteditable.
    document.execCommand('insertText', false, plain);
    if (!editorEl) return;
    onChange(serializeToText(editorEl));
  }

  // Sync state → DOM when value changes from outside. The equality guard
  // prevents a DOM rewrite (and caret reset) when the change originated
  // from onInput — in that case the DOM is already correct.
  useEffect(() => {
    if (!editorEl) return;
    if (serializeToText(editorEl) === value) return;
    const offset = saveCaret(editorEl);
    renderTokens(editorEl, tokens, offset);
  }, [value, tokens, editorEl]);

  return { onInput, onCompositionStart, onCompositionEnd, onPaste };
}
```

- [ ] **Step 4: Run tests — all should pass**

```bash
yarn test --run useContentEditableInput
```

Expected: all tests pass.

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useContentEditableInput.ts src/components/request/__tests__/useContentEditableInput.test.ts
git commit -m "feat(input): add renderTokens and useContentEditableInput hook"
```

---

## Task 3: Rewrite `VariableAwareInput`

**Files:**
- Modify: `src/components/request/VariableAwareInput.tsx`
- Modify: `src/components/request/__tests__/VariableAwareInput.test.tsx`

Replace the inner component's overlay with a `contenteditable` div wired to the hook. The outer component (the early-return for `!variableContext` or `type === 'password'`) stays almost identical.

- [ ] **Step 1: Rewrite the failing tests first**

Replace the contents of `src/components/request/__tests__/VariableAwareInput.test.tsx`:

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { VariableAwareInput } from '../VariableAwareInput';

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (
    selector: (s: {
      activeEnvId: null;
      environments: never[];
      globalEnv: null;
      updateEnvironment: () => Promise<void>;
      updateGlobalEnvironment: () => Promise<void>;
    }) => unknown,
  ) =>
    selector({
      activeEnvId: null,
      environments: [],
      globalEnv: null,
      updateEnvironment: async () => {},
      updateGlobalEnvironment: async () => {},
    }),
}));

function makeContext(entries: Record<string, VariableScopeEntry>): Map<string, VariableScopeEntry> {
  return new Map(Object.entries(entries));
}

describe('VariableAwareInput', () => {
  it('renders a plain input when variableContext is undefined', () => {
    render(<VariableAwareInput value='hello' onChange={vi.fn()} />);
    // Plain <input> renders as textbox; no contenteditable present.
    expect(screen.getByRole('textbox')).toBeDefined();
    expect(document.querySelector('[contenteditable]')).toBeNull();
  });

  it('renders a plain input for type=password even with variableContext', () => {
    render(
      <VariableAwareInput
        value='secret'
        onChange={vi.fn()}
        type='password'
        variableContext={makeContext({})}
      />,
    );
    const input = document.querySelector('input[type="password"]');
    expect(input).not.toBeNull();
    expect(document.querySelector('[contenteditable]')).toBeNull();
  });

  it('renders a contenteditable editor when variableContext is provided', () => {
    render(
      <VariableAwareInput
        value='Bearer {{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc123', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    expect(document.querySelector('[contenteditable]')).not.toBeNull();
    // No overlay div with aria-hidden.
    expect(document.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  it('renders plain text content in the editor', () => {
    render(
      <VariableAwareInput
        value='plain text'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const editor = document.querySelector('[contenteditable]')!;
    expect(editor.textContent).toBe('plain text');
  });

  it('renders a badge span for a resolved variable', () => {
    render(
      <VariableAwareInput
        value='{{token}}'
        onChange={vi.fn()}
        variableContext={makeContext({
          token: { value: 'abc', source: 'environment', label: 'Dev', secret: false },
        })}
      />,
    );
    const badge = document.querySelector('[data-badge]');
    expect(badge).not.toBeNull();
    expect(badge?.textContent).toBe('{{token}}');
  });

  it('renders a badge span with destructive class for an unresolved variable', () => {
    render(
      <VariableAwareInput
        value='{{missing}}'
        onChange={vi.fn()}
        variableContext={makeContext({})}
      />,
    );
    const badge = document.querySelector('[data-badge]');
    expect(badge?.className).toContain('text-destructive');
  });

  it('calls onChange when the editor content changes', () => {
    const onChange = vi.fn();
    render(
      <VariableAwareInput
        value='hello'
        onChange={onChange}
        variableContext={makeContext({})}
      />,
    );
    const editor = document.querySelector('[contenteditable]') as HTMLElement;
    editor.textContent = 'hello!';
    fireEvent(editor, new Event('input', { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith('hello!');
  });
});
```

- [ ] **Step 2: Run tests — expect failures (overlay assertions removed but component not yet rewritten)**

```bash
yarn test --run VariableAwareInput
```

Expected: tests that check for `[contenteditable]` fail, `aria-hidden` tests also fail.

- [ ] **Step 3: Rewrite `VariableAwareInput.tsx`**

Replace the entire file `src/components/request/VariableAwareInput.tsx`:

```tsx
import { useCallback, useMemo, useRef, useState } from 'react';
import {
  type EditorToken,
  useContentEditableInput,
} from '@/hooks/useContentEditableInput';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { parseTextTokens } from '@/lib/text-variables';
import {
  sourceBadgeClass,
  type VariableScopeEntry,
  type VariableSource,
} from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

export interface VariableAwareInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  type?: 'text' | 'password';
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

// Navigation link label for a variable source.
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
      return null;
  }
}

export function VariableAwareInput({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  type = 'text',
  variableContext,
  onNavigateToSource,
}: VariableAwareInputProps) {
  // No variableContext or password field: render a plain input.
  if (!variableContext || type === 'password') {
    return (
      <Input
        type={type}
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

// Inner component holds hooks; separated so the outer can do an early return.
function VariableAwareInputInner({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
}: Required<Pick<VariableAwareInputProps, 'variableContext'>> &
  Omit<VariableAwareInputProps, 'variableContext' | 'type'>) {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  const editorRef = useRef<HTMLDivElement>(null);

  // Index of the token whose popover is currently open.
  const [openTokenIdx, setOpenTokenIdx] = useState<number | null>(null);
  const [openVarKey, setOpenVarKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const editingScopeRef = useRef<VariableSource | null>(null);

  // Parse value into tokens and build EditorToken list for the hook.
  const rawTokens = useMemo(() => parseTextTokens(value), [value]);

  const tokens: EditorToken[] = useMemo(() =>
    rawTokens.map((token, idx) => {
      if (token.type === 'text') {
        return { type: 'text' as const, content: token.content, rawLength: token.rawLength };
      }
      const entry = variableContext.get(token.content);
      const badgeClass = cn(
        'rounded-sm px-0.5 cursor-pointer',
        entry ? sourceBadgeClass(entry.source) : 'bg-destructive/15 text-destructive',
      );
      return {
        type: 'badge' as const,
        content: `{{${token.content}}}`,
        rawLength: token.rawLength,
        badgeClass,
        tokenIdx: idx,
      };
    }),
  [rawTokens, variableContext]);

  const { onInput, onCompositionStart, onCompositionEnd, onPaste } = useContentEditableInput({
    editorEl: editorRef.current,
    value,
    onChange,
    tokens,
  });

  const handleBadgeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const span = (e.target as Element).closest('[data-badge]');
      if (!span) return;
      e.preventDefault(); // Prevents caret jumping into the span.
      const idx = Number(span.getAttribute('data-token-idx'));
      const rawToken = rawTokens[idx];
      if (!rawToken || rawToken.type !== 'variable') return;
      const entry = variableContext.get(rawToken.content);
      setOpenTokenIdx(idx);
      setOpenVarKey(rawToken.content);
      setEditValue(entry?.secret ? '' : (entry?.value ?? ''));
      editingScopeRef.current = entry?.source ?? null;
    },
    [rawTokens, variableContext],
  );

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
    setOpenTokenIdx(null);
    setOpenVarKey(null);
  }, [openVarKey, editValue, activeEnvId, environments, updateEnvironment, globalEnv, updateGlobalEnvironment]);

  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  // Refs for popover anchoring — populated after renderTokens writes badge spans.
  const badgeRefsMap = useRef<Map<number, HTMLSpanElement>>(new Map());

  // Refresh badge ref map after each render.
  const refreshBadgeRefs = useCallback(() => {
    if (!editorRef.current) return;
    badgeRefsMap.current.clear();
    for (const span of Array.from(editorRef.current.querySelectorAll('[data-badge]'))) {
      const idx = Number((span as HTMLElement).getAttribute('data-token-idx'));
      badgeRefsMap.current.set(idx, span as HTMLSpanElement);
    }
  }, []);

  return (
    <div
      className={cn(
        'relative h-8 w-full rounded-md border border-input bg-background px-3 py-1',
        'font-mono text-xs ring-ring/50 focus-within:ring-[3px] focus-within:border-ring',
        disabled && 'opacity-50 pointer-events-none cursor-not-allowed',
        className,
      )}
      onMouseDown={handleBadgeMouseDown}
    >
      {/* Placeholder shown when value is empty. */}
      {value === '' && (
        <span
          aria-hidden
          className='absolute inset-0 flex items-center px-3 py-1 text-muted-foreground pointer-events-none'
        >
          {placeholder}
        </span>
      )}

      {/* The contenteditable editor. */}
      <div
        ref={editorRef}
        contentEditable={!disabled}
        suppressContentEditableWarning
        role='textbox'
        aria-label={placeholder}
        aria-multiline={false}
        aria-disabled={disabled}
        spellCheck={false}
        className='outline-none h-full flex items-center'
        onInput={() => { onInput(); refreshBadgeRefs(); }}
        onCompositionStart={onCompositionStart}
        onCompositionEnd={() => { onCompositionEnd(); refreshBadgeRefs(); }}
        onPaste={(e) => onPaste(e.nativeEvent as ClipboardEvent)}
      />

      {/* Popovers rendered as siblings, outside the contenteditable div. */}
      {rawTokens.map((token, idx) => {
        if (token.type !== 'variable') return null;
        const entry = variableContext.get(token.content);
        const isReadOnly =
          entry !== undefined && entry.source !== 'environment' && entry.source !== 'global';
        const linkLabel = entry ? navLinkLabel(entry.source) : null;

        return (
          <Popover
            key={idx}
            open={openTokenIdx === idx}
            onOpenChange={(open) => { if (!open) setOpenTokenIdx(null); }}
          >
            {/* Invisible trigger positioned over the badge span via JS anchor. */}
            <PopoverTrigger asChild>
              <span style={{ display: 'none' }} />
            </PopoverTrigger>
            <PopoverContent
              className='w-80 p-0'
              side='bottom'
              align='start'
              // Anchor to the badge span's bounding rect.
              style={
                badgeRefsMap.current.get(idx)
                  ? {
                      position: 'absolute',
                      left: badgeRefsMap.current.get(idx)!.getBoundingClientRect().left,
                      top: badgeRefsMap.current.get(idx)!.getBoundingClientRect().bottom,
                    }
                  : undefined
              }
            >
              <div className='p-2'>
                <Input
                  autoFocus
                  className='h-7 text-xs font-mono'
                  value={entry?.secret ? '●●●●' : editValue}
                  placeholder={entry ? 'Value' : 'Not set'}
                  readOnly={isReadOnly || entry?.secret}
                  onChange={(e) => {
                    if (isReadOnly || entry?.secret) return;
                    setEditValue(e.target.value);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void handleCommitRef.current();
                    if (e.key === 'Escape') {
                      setOpenTokenIdx(null);
                      setOpenVarKey(null);
                    }
                  }}
                  onBlur={() => void handleCommitRef.current()}
                />
              </div>
              {(entry || linkLabel) && (
                <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                  {entry ? (
                    <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                      <span className={cn(
                        'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
                        sourceBadgeClass(entry.source),
                      )}>
                        {entry.source.charAt(0).toUpperCase()}
                      </span>
                      <span>{entry.label}</span>
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
              )}
            </PopoverContent>
          </Popover>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Run tests**

```bash
yarn test --run VariableAwareInput
```

Expected: all tests pass.

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/VariableAwareInput.tsx src/components/request/__tests__/VariableAwareInput.test.tsx
git commit -m "feat(input): replace overlay with contenteditable in VariableAwareInput"
```

---

## Task 4: Rewrite `VariableAwareUrlInput`

**Files:**
- Modify: `src/components/request/VariableAwareUrlInput.tsx`

The URL input has more token types (pathParam, queryKey, queryValue) and the cURL paste interception. The `onKeyDown` prop type changes from `KeyboardEventHandler<HTMLInputElement>` to `KeyboardEventHandler<HTMLDivElement>` — the single call site in `RequestPanel.tsx` uses an inline arrow on `e.key` which is compatible without touching that file.

- [ ] **Step 1: Rewrite `VariableAwareUrlInput.tsx`**

Replace the entire file `src/components/request/VariableAwareUrlInput.tsx`:

```tsx
import { useCallback, useMemo, useRef, useState } from 'react';
import {
  type EditorToken,
  useContentEditableInput,
} from '@/hooks/useContentEditableInput';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { type ParsedCurl, parseCurl } from '@/lib/curl-parser';
import {
  parseUrlTokens,
  sourceBadgeClass,
  type UrlToken,
  type VariableScopeEntry,
  type VariableSource,
} from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLDivElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  collectionVariables?: Record<string, string>;
  pathParams?: Record<string, string>;
  queryParams?: Record<string, string>;
  onPathParamChange?: (key: string, value: string) => void;
  onNavigateToSource?: (source: VariableSource | 'pathParam') => void;
  placeholder?: string;
  className?: string;
  scopedContext?: Map<string, VariableScopeEntry>;
}

function navLinkLabel(source: VariableSource | 'pathParam'): string | null {
  switch (source) {
    case 'pathParam': return 'Params \u2192';
    case 'request':
    case 'runtime': return 'Request Variables \u2192';
    case 'environment': return 'Collection Environments \u2192';
    case 'global': return 'Global Environments \u2192';
    case 'collection': return 'Collection Variables \u2192';
    default: return null;
  }
}

export function VariableAwareUrlInput({
  value,
  onChange,
  onKeyDown,
  onCurlImport,
  collectionVariables,
  pathParams,
  queryParams,
  onPathParamChange,
  onNavigateToSource,
  placeholder,
  className,
  scopedContext,
}: VariableAwareUrlInputProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);

  const [editingToken, setEditingToken] = useState<UrlToken | null>(null);
  const [editValue, setEditValue] = useState('');
  const editingScopeRef = useRef<VariableSource | null>(null);

  const variables = useMemo(() => {
    if (!activeEnvId) return {};
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return {};
    const vars: Record<string, string> = {};
    for (const v of env.variables) {
      if (v.enabled) vars[v.key] = v.value;
    }
    return vars;
  }, [activeEnvId, environments]);

  const urlTokens = useMemo(
    () => parseUrlTokens(value, variables, activeEnvId ?? undefined, collectionVariables, pathParams, queryParams),
    [value, variables, activeEnvId, collectionVariables, pathParams, queryParams],
  );

  // Map UrlTokens to EditorTokens for the hook.
  const editorTokens: EditorToken[] = useMemo(() =>
    urlTokens.map((token, idx) => {
      if (token.type === 'text') {
        return { type: 'text' as const, content: token.value, rawLength: token.value.length };
      }
      if (token.type === 'queryValue') {
        return { type: 'badge' as const, content: token.value, rawLength: token.value.length, badgeClass: 'text-muted-foreground', tokenIdx: idx };
      }
      if (token.type === 'queryKey') {
        const isResolved = token.resolved !== undefined;
        return {
          type: 'badge' as const,
          content: token.value,
          rawLength: token.value.length,
          badgeClass: cn('rounded-sm px-0.5', isResolved ? 'bg-amber-500/15 text-amber-500' : 'text-muted-foreground'),
          tokenIdx: idx,
        };
      }
      if (token.type === 'pathParam') {
        const isResolved = token.resolved !== undefined;
        return {
          type: 'badge' as const,
          content: `:${token.value}`,
          rawLength: token.value.length + 1,
          badgeClass: cn('rounded-sm px-0.5 cursor-pointer', isResolved ? 'bg-violet-500/15 text-violet-500' : 'bg-destructive/15 text-destructive'),
          tokenIdx: idx,
        };
      }
      // variable token
      const scopeEntry = scopedContext?.get(token.value);
      const badgeClass = cn(
        'rounded-sm px-0.5 cursor-pointer',
        scopeEntry
          ? sourceBadgeClass(scopeEntry.source)
          : token.resolved !== undefined
          ? 'bg-primary/15 text-primary'
          : 'bg-destructive/15 text-destructive',
      );
      return {
        type: 'badge' as const,
        content: `{{${token.value}}}`,
        rawLength: token.value.length + 4,
        badgeClass,
        tokenIdx: idx,
      };
    }),
  [urlTokens, scopedContext]);

  // cURL paste interception — must run before the hook's onPaste.
  const handleBeforePaste = useCallback((e: ClipboardEvent): boolean => {
    if (!onCurlImport) return false;
    const text = e.clipboardData?.getData('text/plain').trim() ?? '';
    if (!/^curl\s/i.test(text)) return false;
    e.preventDefault();
    const parsed = parseCurl(text);
    if (parsed) onCurlImport(parsed);
    return true;
  }, [onCurlImport]);

  const { onInput, onCompositionStart, onCompositionEnd, onPaste } = useContentEditableInput({
    editorEl: editorRef.current,
    value,
    onChange,
    tokens: editorTokens,
    onBeforePaste: handleBeforePaste,
  });

  const badgeRefsMap = useRef<Map<number, HTMLSpanElement>>(new Map());

  const refreshBadgeRefs = useCallback(() => {
    if (!editorRef.current) return;
    badgeRefsMap.current.clear();
    for (const span of Array.from(editorRef.current.querySelectorAll('[data-badge]'))) {
      const idx = Number((span as HTMLElement).getAttribute('data-token-idx'));
      badgeRefsMap.current.set(idx, span as HTMLSpanElement);
    }
  }, []);

  const handleBadgeMouseDown = useCallback((e: React.MouseEvent) => {
    const span = (e.target as Element).closest('[data-badge]');
    if (!span) return;
    const idx = Number(span.getAttribute('data-token-idx'));
    const token = urlTokens[idx];
    if (!token || (token.type !== 'variable' && token.type !== 'pathParam')) return;
    e.preventDefault();
    const scopeEntry = token.type === 'variable' ? scopedContext?.get(token.value) : undefined;
    setEditingToken(token);
    setEditValue(scopeEntry?.secret ? '' : (scopeEntry?.value ?? token.resolved ?? ''));
    editingScopeRef.current = scopeEntry?.source ?? null;
  }, [urlTokens, scopedContext]);

  const handleCommit = useCallback(async () => {
    if (!editingToken) return;
    if (editingToken.type === 'pathParam' && onPathParamChange) {
      onPathParamChange(editingToken.value, editValue);
    } else if (editingToken.type === 'variable') {
      const scope = editingScopeRef.current;
      if (scope === 'global' && globalEnv) {
        const vars = globalEnv.variables.map((v) =>
          v.key === editingToken.value ? { ...v, value: editValue } : v,
        );
        if (!globalEnv.variables.some((v) => v.key === editingToken.value)) {
          vars.push({ key: editingToken.value, value: editValue, enabled: true, secret: false });
        }
        await updateGlobalEnvironment({ ...globalEnv, variables: vars });
      } else if ((scope === 'environment' || scope === null) && activeEnvId) {
        const env = environments.find((e) => e.name === activeEnvId);
        if (env) {
          const vars = env.variables.map((v) =>
            v.key === editingToken.value ? { ...v, value: editValue } : v,
          );
          if (!env.variables.some((v) => v.key === editingToken.value)) {
            vars.push({ key: editingToken.value, value: editValue, enabled: true, secret: false });
          }
          await updateEnvironment({ ...env, variables: vars });
        }
      }
    }
    setEditingToken(null);
  }, [editingToken, editValue, activeEnvId, environments, updateEnvironment, globalEnv, updateGlobalEnvironment, onPathParamChange]);

  const handleCommitRef = useRef(handleCommit);
  handleCommitRef.current = handleCommit;

  return (
    <div
      className={cn('relative flex-1 h-8', className)}
      onMouseDown={handleBadgeMouseDown}
    >
      {value === '' && (
        <span
          aria-hidden
          className='absolute inset-0 flex items-center px-3 py-1 font-mono text-xs text-muted-foreground pointer-events-none'
        >
          {placeholder}
        </span>
      )}

      <div
        ref={editorRef}
        contentEditable
        suppressContentEditableWarning
        role='textbox'
        aria-label={placeholder}
        aria-multiline={false}
        spellCheck={false}
        className='h-full w-full rounded-md border border-input bg-background px-3 py-1 font-mono text-xs outline-none ring-ring/50 focus-visible:ring-[3px] focus-visible:border-ring flex items-center'
        onInput={() => { onInput(); refreshBadgeRefs(); }}
        onKeyDown={onKeyDown}
        onCompositionStart={onCompositionStart}
        onCompositionEnd={() => { onCompositionEnd(); refreshBadgeRefs(); }}
        onPaste={(e) => onPaste(e.nativeEvent as ClipboardEvent)}
      />

      {/* Popovers for interactive tokens (variable, pathParam). */}
      {urlTokens.map((token, idx) => {
        if (token.type !== 'variable' && token.type !== 'pathParam') return null;
        const scopeEntry = token.type === 'variable' ? scopedContext?.get(token.value) : undefined;
        const isReadOnlyVar =
          token.type === 'variable' &&
          scopeEntry !== undefined &&
          scopeEntry.source !== 'environment' &&
          scopeEntry.source !== 'global';
        const navSource: VariableSource | 'pathParam' | null =
          token.type === 'pathParam' ? 'pathParam' : (scopeEntry?.source ?? null);
        const linkLabel = navSource !== null ? navLinkLabel(navSource) : null;

        return (
          <Popover
            key={token.start}
            open={editingToken?.start === token.start}
            onOpenChange={(open) => { if (!open) setEditingToken(null); }}
          >
            <PopoverTrigger asChild>
              <span style={{ display: 'none' }} />
            </PopoverTrigger>
            <PopoverContent className='w-80 p-0' side='bottom' align='start'>
              <div className='p-2'>
                <Input
                  autoFocus
                  className='h-7 text-xs font-mono'
                  value={scopeEntry?.secret ? '●●●●' : editValue}
                  onChange={(e) => {
                    if (scopeEntry?.secret || isReadOnlyVar) return;
                    const v = e.target.value;
                    setEditValue(v);
                    if (token.type === 'pathParam' && onPathParamChange) {
                      onPathParamChange(token.value, v);
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void handleCommitRef.current();
                    if (e.key === 'Escape') setEditingToken(null);
                  }}
                  onBlur={() => void handleCommitRef.current()}
                  placeholder='Value'
                  readOnly={isReadOnlyVar || scopeEntry?.secret}
                />
              </div>
              <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
                <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
                  {token.type === 'pathParam' ? (
                    <span className='text-violet-500 font-bold text-xs'>:</span>
                  ) : scopeEntry ? (
                    <span className={cn(
                      'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
                      sourceBadgeClass(scopeEntry.source),
                    )}>
                      {scopeEntry.source.charAt(0).toUpperCase()}
                    </span>
                  ) : null}
                  <span>{scopeEntry?.label ?? (token.type === 'pathParam' ? 'Path Variable' : 'Unresolved')}</span>
                </div>
                {onNavigateToSource && navSource !== null && linkLabel !== null && (
                  <button
                    type='button'
                    className='text-2xs text-primary hover:underline cursor-pointer'
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={async () => {
                      await handleCommitRef.current();
                      onNavigateToSource(navSource);
                    }}
                  >
                    {linkLabel}
                  </button>
                )}
              </div>
            </PopoverContent>
          </Popover>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. The `onKeyDown` type change from `HTMLInputElement` to `HTMLDivElement` is compatible with the inline arrow in `RequestPanel.tsx` because `e.key` is on `KeyboardEvent` regardless of element type.

- [ ] **Step 3: Run full test suite**

```bash
yarn test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/VariableAwareUrlInput.tsx
git commit -m "feat(input): replace overlay with contenteditable in VariableAwareUrlInput"
```

---

## Task 5: Add `selectionchange` Guard (Caret-in-Badge Protection)

**Files:**
- Modify: `src/hooks/useContentEditableInput.ts`

When the user arrow-keys through a badge span, the browser moves the caret inside the span's interior. This `selectionchange` listener detects that and ejects the caret to the nearest text node boundary.

- [ ] **Step 1: Write a failing test for the guard**

Add to `src/components/request/__tests__/useContentEditableInput.test.ts`:

```ts
describe('selectionchange caret guard', () => {
  it('ejects caret from inside a badge span when arrow key moves into it', async () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    el.innerHTML = 'ab<span data-badge data-token-idx="0">{{x}}</span>cd';
    document.body.appendChild(el);

    const onChange = vi.fn();
    const tokens: EditorToken[] = [
      { type: 'text', content: 'ab', rawLength: 2 },
      { type: 'badge', content: '{{x}}', rawLength: 5, tokenIdx: 0 },
      { type: 'text', content: 'cd', rawLength: 2 },
    ];

    renderHook(() =>
      useContentEditableInput({ editorEl: el, value: 'ab{{x}}cd', onChange, tokens }),
    );

    // Simulate caret landing inside the badge span (as if arrow-keyed in).
    const badge = el.querySelector('[data-badge]')!;
    const range = document.createRange();
    range.setStart(badge.firstChild!, 1); // Inside '{{x}}', offset 1.
    range.collapse(true);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);

    // Focus the editor so the guard is active.
    el.focus();
    el.dispatchEvent(new Event('selectionchange', { bubbles: true }));

    // Wait for the guard to move the caret.
    await new Promise((r) => setTimeout(r, 0));

    const finalSel = window.getSelection()!;
    // Caret should now be outside the badge span.
    expect((finalSel.anchorNode as Element).hasAttribute?.('data-badge')).toBe(false);
    el.remove();
  });
});
```

- [ ] **Step 2: Run — expect failure**

```bash
yarn test --run useContentEditableInput
```

Expected: the selectionchange test fails (no guard implemented yet).

- [ ] **Step 3: Add the guard to `useContentEditableInput`**

Add inside the `useContentEditableInput` function, after the `onPaste` function:

```ts
  const isFocused = useRef(false);

  // Ejects caret from badge spans when arrow-key navigation moves into one.
  useEffect(() => {
    const el = editorEl;
    if (!el) return;

    const onFocus = () => { isFocused.current = true; };
    const onBlur = () => { isFocused.current = false; };

    const onSelectionChange = () => {
      if (!isFocused.current) return;
      const sel = window.getSelection();
      if (!sel || sel.rangeCount === 0) return;
      const anchor = sel.anchorNode;
      if (!anchor) return;
      // Check if the anchor node is inside a [data-badge] span.
      const badge = anchor.nodeType === Node.ELEMENT_NODE
        ? (anchor as Element).closest('[data-badge]')
        : anchor.parentElement?.closest('[data-badge]');
      if (!badge) return;
      // Eject: place caret at end of preceding text node.
      let prev = badge.previousSibling;
      while (prev && prev.nodeType !== Node.TEXT_NODE) prev = prev.previousSibling;
      if (prev) {
        const range = document.createRange();
        range.setStart(prev, (prev as Text).length);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
      }
    };

    el.addEventListener('focus', onFocus);
    el.addEventListener('blur', onBlur);
    document.addEventListener('selectionchange', onSelectionChange);
    return () => {
      el.removeEventListener('focus', onFocus);
      el.removeEventListener('blur', onBlur);
      document.removeEventListener('selectionchange', onSelectionChange);
    };
  }, [editorEl]);
```

- [ ] **Step 4: Run tests**

```bash
yarn test --run useContentEditableInput
```

Expected: all tests pass including the new guard test.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useContentEditableInput.ts src/components/request/__tests__/useContentEditableInput.test.ts
git commit -m "feat(input): add selectionchange guard to eject caret from badge spans"
```

---

## Task 6: Remove Old Workaround Code and Final Verification

**Files:**
- Modify: `src/components/request/VariableAwareInput.tsx` (already rewritten — verify `trimEnd` workaround is gone)
- Modify: `src/components/request/VariableAwareUrlInput.tsx` (already rewritten — verify same)

The `trimEnd()` and `selection:bg-primary/30 selection:text-transparent` workarounds added for the overlay architecture no longer exist in the rewritten files. This task confirms they are gone and runs full verification.

- [ ] **Step 1: Confirm workaround code is absent**

```bash
grep -n "trimEnd\|selection:bg-primary" src/components/request/VariableAwareInput.tsx src/components/request/VariableAwareUrlInput.tsx
```

Expected: no output (both workarounds removed by the rewrite).

- [ ] **Step 2: Run full verification suite**

```bash
cargo check && yarn tsc --noEmit && yarn check && yarn test
```

Expected:
- `cargo check` — pass (no Rust changes)
- `yarn tsc --noEmit` — no errors
- `yarn check` — only pre-existing lint warnings (in `sync-tauri-version.cjs` and `CollectionOverviewTab.tsx`), none new
- `yarn test` — all tests pass

- [ ] **Step 3: Final commit**

```bash
git add src/components/request/VariableAwareInput.tsx src/components/request/VariableAwareUrlInput.tsx
git commit -m "chore(input): confirm overlay workarounds removed after contenteditable refactor"
```

---

## Known Risks and Mitigations

| Risk | Mitigation |
|---|---|
| `document.execCommand('insertText')` is deprecated | All Chromium/WebKit still support it. Tauri 2 uses WebKit (macOS) and Chromium (Windows/Linux). If it breaks, fallback: `Range.deleteContents(); range.insertNode(document.createTextNode(plain))` |
| Caret drift on fast typing | The `serializeToText(el) === value` guard prevents most cases. If drift is observed, debounce `onChange` by one animation frame: wrap in `requestAnimationFrame(() => onChange(text))` |
| Screen readers reading badge spans | `role="textbox"` causes screen readers to expose the flat `textContent` of the editor — badge spans have no extra role and are transparent to the accessibility tree. Correct behavior |
| Popover positioning off-screen | Badge spans can be clipped by `overflow-hidden` on parent containers. If popover anchoring via `getBoundingClientRect` positions outside the viewport, Radix's built-in collision detection handles repositioning |

---

## What This Permanently Fixes

| Bug | Status after refactor |
|---|---|
| Trailing whitespace shows as phantom space | Gone — contenteditable renders text identically to native input |
| Text selection invisible | Gone — browser native selection, no `text-transparent` trick |
| Font rendering misalignment | Gone — one rendering layer, impossible to diverge |
| Future padding/border changes breaking alignment | Gone — no overlay to keep in sync |
