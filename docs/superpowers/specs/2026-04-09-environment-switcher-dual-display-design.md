# Environment Switcher Dual Display Design

## Goal

Replace the teal-dot indicator in the `EnvironmentSwitcher` trigger button with a dual-segment display that shows both the active collection environment and the active global environment simultaneously — matching the reference layout from Bruno.

## Problem

The current trigger button shows:
- A teal dot (present only when a global env is active, no label shown)
- The collection environment name (or "No Environment" if none)

Users cannot tell which global environment is active without opening the popover. The teal dot gives no useful information at a glance.

## Approved Design

### Layout per state

| State | Trigger content |
|---|---|
| Both active | `[Database] DEC  [Globe] qa  [ChevronDown]` |
| Collection only | `[Database] DEC  [ChevronDown]` |
| Global only | `[Globe] qa  [ChevronDown]` |
| Neither | `No Environment  [ChevronDown]` (muted text, no icons) |

### Visual specification

- **Icons**: `Database` (lucide-react) for collection env; `Globe` (already imported) for global env
- **Icon size**: `h-3 w-3 text-muted-foreground`
- **Icon-to-label gap**: `gap-1` within each segment
- **Segment-to-segment gap**: `gap-2.5` between the two segments
- **Name truncation**: `max-w-[80px] truncate` per name label
- **"No Environment" text**: `text-muted-foreground` (same as current)
- **ChevronDown**: unchanged, `h-3 w-3 opacity-50`

### Segment structure (JSX sketch)

```tsx
{/* Both active */}
<span className="flex items-center gap-1">
  <Database className="h-3 w-3 text-muted-foreground shrink-0" />
  <span className="max-w-[80px] truncate">{activeEnvId}</span>
</span>
<span className="flex items-center gap-1">
  <Globe className="h-3 w-3 text-muted-foreground shrink-0" />
  <span className="max-w-[80px] truncate">{globalEnvName}</span>
</span>

{/* Collection only */}
<span className="flex items-center gap-1">
  <Database className="h-3 w-3 text-muted-foreground shrink-0" />
  <span className="max-w-[80px] truncate">{activeEnvId}</span>
</span>

{/* Global only */}
<span className="flex items-center gap-1">
  <Globe className="h-3 w-3 text-muted-foreground shrink-0" />
  <span className="max-w-[80px] truncate">{globalEnvName}</span>
</span>

{/* Neither */}
<span className="text-muted-foreground">No Environment</span>
```

The outer `Button` needs `gap-2.5` to space the two segments from each other. The existing `gap-1.5` and `px-2` on the Button are replaced with `gap-2.5 px-2`.

## Scope

**Only the trigger button JSX changes.** The popover content, tabs, environment lists, create flows, configure links — all unchanged. No behavior changes.

## File to modify

`src/components/layout/EnvironmentSwitcher.tsx`

Changes confined to lines 112–123 (the `<Button>` content inside `<PopoverTrigger>`):
- Add `Database` to lucide-react import
- Replace the teal-dot + single-name JSX with the four-state conditional described above
- Change Button `gap-1.5` to `gap-2.5`
