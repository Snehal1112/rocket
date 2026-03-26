# Variable-Aware URL Input — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Highlight `{{variable}}` tokens inline in the request URL input and support hover-edit via popover, updating the active environment.

## Current State

- `useEnvStore.resolveVariables(text)` already resolves `{{var}}` at send time using the active environment's variables.
- The URL input is a plain `<Input>` in `RequestPanel.tsx` with no visual indication of variables.
- Only environment variables exist — no collection-level variable store.

## What We're Building

### 1. URL variable parser (`src/lib/url-variables.ts`)

Pure utility that extracts `{{var}}` tokens from a URL string and resolves them against a variables map.

```ts
interface UrlToken {
  type: 'text' | 'variable';
  value: string;        // raw text or variable name
  start: number;        // character offset in URL
  end: number;          // character offset end
  resolved?: string;    // resolved value (only for variables)
  source?: string;      // environment name (only for resolved variables)
}

function parseUrlTokens(url: string, variables: Record<string, string>, envName?: string): UrlToken[]
```

Returns an array of tokens alternating between plain text and variable segments. Variable tokens include their resolved value and source.

### 2. VariableAwareUrlInput component (`src/components/request/VariableAwareUrlInput.tsx`)

Controlled input with an overlay layer that renders colored highlights over `{{var}}` patterns.

**Architecture:** A hidden real `<input>` handles all keyboard/focus/selection. An absolutely-positioned overlay `<div>` renders the same text but with `<span>` wrappers around variable tokens for styling. The overlay is `pointer-events-none` so all interactions go to the real input underneath.

**Props:**
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  placeholder?: string;
  className?: string;
}
```

**Token styles:**
- Resolved: `bg-primary/15 text-primary rounded-sm px-0.5`
- Unresolved: `bg-destructive/15 text-destructive rounded-sm px-0.5`

**Hover popover:**
- Triggered by hovering over a variable token in the overlay
- Shows: variable name, source environment, editable value input
- Save updates the variable in the active environment via `useEnvStore`
- Cancel/blur closes the popover
- If no active environment selected, popover shows "No active environment" with no edit field

### 3. Integration in RequestPanel

Replace the plain `<Input>` in the URL bar with `<VariableAwareUrlInput>`. No other changes to send/save/validation flow.

---

## Token Style Reference

| State | Background | Text | Border |
|---|---|---|---|
| Resolved (env variable) | `bg-primary/15` | `text-primary` | `border-primary/30` |
| Unresolved (no match) | `bg-destructive/15` | `text-destructive` | `border-destructive/30` |

## Hover Popover Content

```
┌─────────────────────────────────┐
│  host                           │
│  Source: Production             │
│  ┌───────────────────────┐      │
│  │ api.example.com       │      │
│  └───────────────────────┘      │
│  [Save]  [Cancel]               │
└─────────────────────────────────┘
```

For unresolved variables:
```
┌─────────────────────────────────┐
│  missing_var                    │
│  ⚠ Not found in any environment│
└─────────────────────────────────┘
```

## Files Changed

| File | Changes |
|---|---|
| `src/lib/url-variables.ts` | Create — token parser and resolution |
| `src/components/request/VariableAwareUrlInput.tsx` | Create — overlay input with highlights + popover |
| `src/components/request/RequestPanel.tsx` | Modify — swap Input for VariableAwareUrlInput |

## Out of Scope

- Collection-level variables (store doesn't exist yet)
- Variable autocomplete/suggestions while typing
- ContentEditable-based URL field rewrite
- Separate resolved URL preview line
- Variable creation from the popover (only edit existing)
