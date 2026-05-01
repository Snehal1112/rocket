# Load Test Tab — Sidebar Redesign

**Date:** 2026-05-01
**Status:** Approved

---

## Goal

Fix two layout problems in the Load Test tab:

1. The `PhaseBuilder` rows overflow the 208px sidebar — the combined width of kind selector, two number inputs, and three icon buttons exceeds the available space.
2. The sidebar has a fixed width with no way to resize it.

---

## Scope

Pure frontend change. Only two files are touched:

| File | Change |
|---|---|
| `src/components/request/load-test/PhaseBuilder.tsx` | Redesign phase rows into labeled grid cards with drag-to-reorder |
| `src/components/request/load-test/LoadTestTab.tsx` | Add drag handle and resizable sidebar logic |

No Rust changes. No store changes. No other components touched.

---

## PhaseBuilder Redesign

### Current problem

Each phase row renders inline: dot + Select (90px) + Input (50px) + "s @" + Input (50px) + ArrowUp (28px) + ArrowDown (28px) + Trash (28px) = ~290px total inside a 208px sidebar. Content overflows or wraps badly.

### New layout — labeled grid card

Each phase becomes a self-contained card:

```
┌─────────────────────────────────────────┐
│  ● [Kind selector ▼]           [× delete] │
│                                           │
│  Duration (s)      Concurrency            │
│  [  30  ]          [  10  ]               │
└─────────────────────────────────────────┘
```

- Top row: color dot + kind `Select` (flex-1) + `×` delete `Button` (ghost, destructive, icon-only)
- Bottom row: 2-column grid with labeled `Input` fields
  - Left: label "Duration (s)", `Input type="number" min={1}`
  - Right: label "Concurrency", `Input type="number" min={0}`
- Labels use `text-[10px] uppercase tracking-wider text-muted-foreground` — matches existing section header style
- Card background: `bg-muted/30`, border: `border border-border/60`, rounded: `rounded-md`
- Up/down reorder buttons are **removed** — replaced by drag-to-reorder

### Drag-to-reorder

Use the HTML5 Drag and Drop API — no new library required.

- Each card has `draggable={true}`
- `onDragStart`: store the dragged index in a `useRef`
- `onDragOver`: `e.preventDefault()` to allow drop; add a visual highlight to the target card (`ring-1 ring-primary`)
- `onDrop`: swap the dragged index with the drop target index via `onChange`
- Drag handle indicator: a `GripVertical` icon (Lucide, `h-3 w-3 text-muted-foreground`) on the left edge of each card — purely visual, the whole card is draggable
- Disabled during a running test (`disabled` prop disables all inputs and drag)

### "Add phase" button

Unchanged — `Button variant="ghost" size="sm"` with `Plus` icon at the bottom of the list.

---

## Resizable Sidebar

### Drag handle

A 5px-wide divider element sits between the `<aside>` and the main `<div>`. It renders a `GripVertical`-style visual indicator centered vertically.

```tsx
<div
  className="w-[5px] shrink-0 cursor-col-resize bg-border/40 hover:bg-border transition-colors flex items-center justify-center"
  onMouseDown={handleDragStart}
>
  <div className="h-8 w-[2px] rounded-full bg-border" />
</div>
```

### Resize logic

Implemented with `useRef` + `useEffect` inside `LoadTestTab`:

```ts
const sidebarWidth = useRef<number>(260);  // default px
const isDragging = useRef(false);

const handleDragStart = (e: React.MouseEvent) => {
  isDragging.current = true;
  const startX = e.clientX;
  const startW = sidebarWidth.current;

  const onMove = (e: MouseEvent) => {
    if (!isDragging.current) return;
    const next = Math.min(480, Math.max(180, startW + e.clientX - startX));
    sidebarWidth.current = next;
    // Apply directly to DOM for zero-lag resize
    if (asideRef.current) asideRef.current.style.width = `${next}px`;
  };

  const onUp = () => {
    isDragging.current = false;
    localStorage.setItem('load-test-sidebar-width', String(sidebarWidth.current));
    window.removeEventListener('mousemove', onMove);
    window.removeEventListener('mouseup', onUp);
  };

  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', onUp);
};
```

- `asideRef` is a `useRef<HTMLDivElement>` attached to the `<aside>`
- Width is applied directly via `style` (not React state) to avoid re-renders during drag
- On mount: read `localStorage.getItem('load-test-sidebar-width')` and apply if present
- Min: 180px, Max: 480px, Default: 260px

### Sidebar initial width

The `<aside>` className changes from `w-52` (fixed 208px) to no width class — width is set via inline `style={{ width: sidebarWidth.current }}` on mount and updated by the drag handler directly.

---

## What Is NOT in Scope

- Collapsed/icon-only sidebar state
- Preset size buttons
- Any changes to `StatBar`, `LatencyChart`, `ThroughputChart`, `ErrorRateChart`, `HistogramChart`, `ConcurrencyChart`, `RequestLogTable`, `ExportMenu`, or the load test store
- Keyboard reordering of phases
- Touch/mobile drag support
