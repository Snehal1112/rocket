# Bruno-Style Collection Tree Panel Redesign

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Redesign the collection tree panel to match Bruno's clean, minimal aesthetic: plain colored method text, no count badge on collections, and a left-bar active state indicator centralised in `tree.tsx`.

## Problems with Current Design

1. **Request method**: Displayed as a bordered `Badge` box (`variant="outline"`, fixed `w-14`, colored border + tinted bg). Feels heavy and boxy.
2. **Collection header**: Shows a `requestCount` Badge on the right edge — adds noise without value at a glance.
3. **Active state**: `bg-accent/50` tint on `TreeItemContent` — too subtle, and applied inconsistently per-node rather than centrally.
4. **Hover state**: `hover:bg-accent` with `rounded-md` — rounded corners don't match Bruno's full-width row highlight.

## Approach

**Systemic (Approach C):** Move hover and active row styles into `tree.tsx` so all tree rows share the same treatment. Individual nodes opt into the active state via an `active` prop on `TreeItem`. Method display and count badge are cleaned up in the relevant components.

---

## Section 1 — `src/components/ui/tree.tsx`

### `TreeItem` prop addition

Add `active?: boolean` to the `TreeItem` prop signature:

```tsx
function TreeItem({
  value,
  className,
  open: openProp,
  onOpenChange,
  active,
  children,
  ...props
}: React.ComponentProps<"li"> & {
  value: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  active?: boolean;
})
```

### `tree-item-row` div changes

| Property | Before | After |
|---|---|---|
| Rounding | `rounded-md` | removed (full-width row) |
| Hover bg | `hover:bg-accent` | `hover:bg-accent/50` |
| Hover text | `hover:text-accent-foreground` | removed |
| Selected | `data-[selected]:bg-accent data-[selected]:text-accent-foreground` | `data-[selected]:bg-accent/30` |
| Active (new) | — | `data-[active]:border-l-2 data-[active]:border-primary data-[active]:bg-accent/60 data-[active]:text-accent-foreground` |

Add `data-active={active || undefined}` to the row div.

---

## Section 2 — `src/components/collections/RequestNode.tsx`

### Method display

Replace `<Badge variant="outline" className={cn('text-[10px] font-semibold w-14 justify-center shrink-0', badgeClass)}>`:

```tsx
<span className={cn('w-10 shrink-0 font-mono text-[10px] font-bold', methodColor)}>
  {method}
</span>
```

### `METHOD_BADGE` → `METHOD_COLOR`

Rename constant and strip border/bg classes — text color only:

```ts
const METHOD_COLOR: Record<string, string> = {
  GET:     'text-emerald-500',
  POST:    'text-amber-500',
  PUT:     'text-blue-500',
  PATCH:   'text-violet-500',
  DELETE:  'text-red-500',
  OPTIONS: 'text-cyan-500',
  HEAD:    'text-pink-500',
};
```

### Active state wiring

- Pass `active={active}` to `<TreeItem value={uid} active={active}>`.
- Remove `active && 'bg-accent/50 text-accent-foreground'` from `TreeItemContent` className.
- Remove `Badge` import.
- Update `badgeClass` variable reference to `methodColor` (renamed from `badgeClass`).

---

## Section 3 — `src/components/collections/CollectionNode.tsx`

- Remove the `<Badge variant="outline" className="ml-auto text-[10px] shrink-0">{summary.requestCount}</Badge>` element from the `TreeItemContent` row.
- Remove the `Badge` import (`import { Badge } from '@/components/ui/badge'`).

---

## Files Changed

| File | Changes |
|---|---|
| `src/components/ui/tree.tsx` | Add `active` prop to `TreeItem`; update row hover/selected/active styles; remove `rounded-md` |
| `src/components/collections/RequestNode.tsx` | Replace `Badge` with `<span>`; rename `METHOD_BADGE` → `METHOD_COLOR`; wire `active` prop to `TreeItem`; remove active class from `TreeItemContent` |
| `src/components/collections/CollectionNode.tsx` | Remove `requestCount` Badge and `Badge` import |

## Out of Scope

- `FolderNode.tsx` — no method badge, no active state, no changes needed
- `RequestList.tsx` — separate component, separate method color system, not touched
- Font sizes, icon sizes, padding (already handled by Approach A/C spacing work)
- Drag overlay styling
