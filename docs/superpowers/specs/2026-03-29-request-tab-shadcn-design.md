# Spec: Standardize UI to shadcn/ui Conventions (App-wide)

**Date:** 2026-03-29
**Scope:** App-wide — request, response, environments, collections

---

## Problem

The app's UI components use a mix of shadcn/ui primitives and hand-rolled equivalents that diverge from the library's conventions. Key issues:

- Custom toggle buttons used as checkboxes include a hardcoded `border-gray-300` color that breaks dark mode.
- 25+ plain `<label>` elements in `AuthEditor` bypass shadcn `Label` semantics.
- Tab active state uses a custom underline (`border-b-2 border-primary`) instead of shadcn's background + shadow style.
- Input heights are overridden to `h-8` / `h-7` app-wide instead of the shadcn default `h-9`.
- `ResponseBodyViewer` uses raw `<button>` elements for tabs instead of the `Tabs` component.
- `AuthEditor` OAuth2 section uses a raw `<input type="checkbox">` and a `<details>` element.

---

## Goals

1. Replace all custom checkbox button patterns with shadcn `Checkbox`.
2. Replace all plain `<label>` elements with shadcn `Label`.
3. Standardize tab active state to shadcn default (background + shadow) everywhere.
4. Remove `h-8` / `h-7` height overrides and use the shadcn `Input` default (`h-9`).
5. Replace raw HTML elements in `AuthEditor` OAuth2 with proper shadcn components.

---

## Out of Scope

- `VariableAwareUrlInput.tsx` — custom transparent-overlay input is intentional and correct.
- State management, Tauri commands, or backend logic — no changes.
- `PathParamsPanel.tsx`, `QueryParamsEditor.tsx`, `HeadersEditor.tsx`, `SaveRequestButton.tsx` — no changes needed.

---

## Architecture

Purely presentational changes. Four independent layers:

### 1. Checkbox pattern

Replace the custom toggle `Button` (a `Button` with `variant="ghost"` styled to look like a checkbox) in three files. `KeyValueEditor.tsx` is fixed first as the canonical pattern since it is already the shared component for params, query params, and headers tabs.

**Before:**
```tsx
<Button
  variant="ghost"
  size="icon"
  onClick={() => updateEntry(entry.id, { enabled: !entry.enabled })}
  className={`w-4 h-4 rounded border p-0 ${
    entry.enabled
      ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
      : 'border-gray-300 hover:bg-muted'   // <-- breaks dark mode
  }`}
>
  {entry.enabled && <Check className="h-3 w-3" />}
</Button>
```

**After:**
```tsx
<Checkbox
  checked={entry.enabled}
  onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
  aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
/>
```

### 2. Tab styling

Remove custom class overrides from `TabsList` and `TabsTrigger` in `RequestPanel.tsx`. Replace the raw `<button>` tab strip in `ResponseBodyViewer.tsx` with shadcn `Tabs`.

**RequestPanel — before:**
```tsx
<TabsList className="w-full justify-start rounded-none border-b border-border/70 bg-card/60 h-9 px-3">
  <TabsTrigger value="params" className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
```

**RequestPanel — after:**
```tsx
<TabsList>
  <TabsTrigger value="params">
```

**ResponseBodyViewer — before (raw buttons):**
```tsx
<button onClick={() => setActiveView(tab)} className={`... ${activeView === tab ? 'border-b-2 border-primary' : ''}`}>
```

**ResponseBodyViewer — after:**
```tsx
<Tabs value={activeView} onValueChange={(v) => setActiveView(v as ActiveView)}>
  <TabsList>
    <TabsTrigger value="pretty">Pretty</TabsTrigger>
    ...
  </TabsList>
</Tabs>
```

### 3. Labels

Import shadcn `Label` in `AuthEditor.tsx` and replace all instances of:
```tsx
<label className="text-sm font-medium text-muted-foreground mb-1 block">...</label>
```
with:
```tsx
<Label>...</Label>
```

Approximately 25 instances spanning OAuth2 and AWS SigV4 sections.

### 4. Input heights and minor raw HTML cleanup

- Remove explicit `h-8` and `h-7` class overrides from all `Input` components app-wide. Let the shadcn default `h-9` apply.
- In `AuthEditor.tsx` OAuth2 advanced section:
  - Replace `<input type="checkbox" className="rounded" />` with shadcn `Checkbox`.
  - Replace `<details>` / `<summary>` with shadcn `Collapsible` / `CollapsibleTrigger` / `CollapsibleContent`.

---

## Files Changed

| File | Change |
|------|--------|
| `src/components/request/KeyValueEditor.tsx` | Replace toggle `Button` with `Checkbox`; remove `h-8` |
| `src/components/request/RequestPanel.tsx` | Remove tab class overrides; remove `h-8` |
| `src/components/request/AuthEditor.tsx` | Replace `<label>` with `Label`; replace raw checkbox and `<details>` |
| `src/components/request/BodyEditor.tsx` | Remove `h-8` override |
| `src/components/response/ResponseBodyViewer.tsx` | Replace raw button tabs with shadcn `Tabs` |
| `src/components/environments/EnvironmentDialog.tsx` | Replace toggle `Button` with `Checkbox`; remove `h-7`/`h-8` |
| `src/components/collections/CollectionVariablesEditor.tsx` | Replace toggle `Button` with `Checkbox` |

---

## Testing

1. `yarn tsc --noEmit` — catch import errors from new shadcn component usage.
2. `yarn build` — ensure no broken imports.
3. Manual smoke test:
   - Toggle checkboxes in params, headers, query params, environments, collection variables in both light and dark mode.
   - Verify tab active state shows background + shadow in `RequestPanel` and `ResponseBodyViewer`.
   - Verify auth form labels are readable and properly associated with inputs.
   - Verify OAuth2 advanced section collapses/expands correctly.
   - Verify input proportions look correct at `h-9`.
