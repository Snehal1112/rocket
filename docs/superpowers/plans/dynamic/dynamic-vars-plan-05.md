# Dynamic Variables Plan 05: VariableAwareInput — Dynamic Source in Popover

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `VariableAwareInput` to display `$`-prefixed dynamic variables with a `D` badge in cyan, a read-only preview value, and no edit/navigate controls.

**Architecture:** When the overlay detects a `$`-prefixed variable name, it generates a preview value via `generateDynamicVar` and renders the token with `source: 'dynamic'` styling. The popover shows a read-only display with no edit input and no "navigate to source" link.

**Tech Stack:** TypeScript, React, shadcn/ui

**Spec:** Before starting, read `docs/superpowers/specs/2026-04-21-dynamic-variables-design.md`.

**Depends on:** Plan 02 (`dynamic-vars.ts`), Plan 04 (`'dynamic'` source type)

---

### Task 1: Update overlay rendering for `$`-prefixed tokens

**Files:**
- Modify: `src/components/request/VariableAwareInput.tsx`

- [ ] **Step 1: Add import**

At the top of `VariableAwareInput.tsx`, add:

```typescript
import { generateDynamicVar, isDynamicVar } from '@/lib/dynamic-vars';
```

- [ ] **Step 2: Update variable token detection in the overlay**

Find the section where `parseTextTokens` results are mapped to styled spans. When a token of type `'variable'` is encountered, the existing code looks up the variable name in `variableContext`. Before that lookup, add a check for `$` prefix:

```typescript
// Inside the overlay rendering loop, when token.type === 'variable':
if (token.content.startsWith('$')) {
  const stripped = token.content.slice(1);
  if (isDynamicVar(stripped)) {
    // Render with dynamic source styling
    const previewValue = generateDynamicVar(stripped) ?? 'Dynamic';
    // Use the dynamic badge class: bg-cyan-500/15 text-cyan-600
    // Set entry to { value: previewValue, source: 'dynamic', label: 'Dynamic', secret: false }
  }
}
```

The exact integration depends on how the overlay currently constructs its `VariableScopeEntry`. The pattern is:

1. Check if variable name starts with `$`
2. If yes and `isDynamicVar(stripped)` → create a synthetic `VariableScopeEntry` with `source: 'dynamic'`
3. If yes but unknown → treat as unresolved (red styling)
4. If no `$` prefix → fall through to existing `variableContext` lookup

- [ ] **Step 3: Update popover content for dynamic variables**

Find the popover component that renders when a variable token is clicked/hovered. Add a condition for `source === 'dynamic'`:

```typescript
// Inside the popover content:
if (entry?.source === 'dynamic') {
  // Show:
  // - Preview value (regenerated each time popover opens)
  // - "D" badge with cyan colour
  // - No edit input (read-only)
  // - No "navigate to source" link
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-1.5">
        <span className="inline-flex items-center justify-center rounded-sm bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 px-1 text-[10px] font-medium leading-4">
          D
        </span>
        <span className="text-xs text-muted-foreground">Dynamic</span>
      </div>
      <div className="text-xs font-mono break-all">{entry.value}</div>
    </div>
  );
}
```

- [ ] **Step 4: Commit**

```bash
git add src/components/request/VariableAwareInput.tsx
git commit -m "feat: display dynamic variables with D badge in VariableAwareInput popover"
```

---

### Task 2: Update MonacoWrapper decorations for dynamic source

**Files:**
- Modify: `src/components/editor/MonacoWrapper.tsx`

- [ ] **Step 1: Add the dynamic decoration class**

Find the `VAR_DECORATION_CLASSES` object. Add:

```typescript
dynamic: 'var-deco-dynamic',
```

Find the `VAR_DECO_STYLES` object. Add:

```typescript
'var-deco-dynamic': 'background:rgb(6 182 212/0.15);color:rgb(6 182 212);border-radius:3px;',
```

These values correspond to Tailwind's `cyan-500`.

- [ ] **Step 2: Update decoration logic for `$`-prefix**

Find the section where Monaco decorations are applied based on `variableContext`. When iterating over matched `{{varName}}` tokens, add a check:

```typescript
import { isDynamicVar } from '@/lib/dynamic-vars';

// Inside the decoration application loop:
if (varName.startsWith('$')) {
  const stripped = varName.slice(1);
  const decoClass = isDynamicVar(stripped)
    ? VAR_DECORATION_CLASSES.dynamic
    : VAR_DECORATION_CLASSES.unresolved;
  // Apply decoration with decoClass
} else {
  // Existing variableContext lookup
}
```

- [ ] **Step 3: Update hover provider for `$`-prefix**

Find the Monaco hover provider registration. When a `$`-prefixed variable is hovered, show:

```typescript
import { generateDynamicVar } from '@/lib/dynamic-vars';

// Inside hover provider:
if (varName.startsWith('$')) {
  const stripped = varName.slice(1);
  if (isDynamicVar(stripped)) {
    const preview = generateDynamicVar(stripped) ?? '';
    return {
      contents: [
        { value: `**Dynamic Variable** \`{{$${stripped}}}\`` },
        { value: `Preview: \`${preview}\`` },
        { value: '_Generates a fresh value on each request send._' },
      ],
    };
  }
}
```

- [ ] **Step 4: Commit**

```bash
git add src/components/editor/MonacoWrapper.tsx
git commit -m "feat: add dynamic variable decorations and hover in Monaco editor"
```

---

### Task 3: Verify end-to-end manually

- [ ] **Step 1: Start the dev server**

```bash
yarn tauri dev
```

- [ ] **Step 2: Test in the URL bar**

Type `{{$guid}}` in the URL bar. Verify:
- The token is highlighted in cyan (not red/unresolved)
- Hovering shows a preview UUID with "Dynamic" label and "D" badge

- [ ] **Step 3: Test in the body editor**

Switch to a request body (JSON mode). Type `{"id": "{{$randomUUID}}"}`. Verify:
- `{{$randomUUID}}` is decorated in cyan in Monaco
- Hovering shows a preview UUID with "Dynamic Variable" label

- [ ] **Step 4: Test in header values**

Add a header with value `{{$randomEmail}}`. Verify:
- The VariableAwareInput shows the token highlighted in cyan
- Popover shows "D" badge + "Dynamic" + a preview email address

- [ ] **Step 5: Send a request and verify resolution**

Create a request to `https://httpbin.org/post` with body:
```json
{"id": "{{$randomUUID}}", "name": "{{$randomFullName}}"}
```

Send the request. In the response, verify the echoed body has actual generated values (a real UUID and a real name), not the `{{$...}}` placeholders.
