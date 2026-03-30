# VS Code Modern Style: Dropdown, Popover, Context Menu — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the three menu/overlay primitives to match VS Code's modern menu aesthetic — frosted-glass containers, sharp full-width item highlights, compact row height.

**Architecture:** Pure Tailwind class changes across three files. No logic changes, no new files, no consumer changes. Each file is self-contained and can be changed and verified independently.

**Tech Stack:** React 19, TypeScript 5.8, TailwindCSS 4.2, Radix UI primitives, shadcn/ui patterns.

---

## File Map

| File | Change type |
|---|---|
| `src/components/ui/popover.tsx` | Container classes only (1 component) |
| `src/components/ui/dropdown-menu.tsx` | Container classes (2 components) + item classes (6 components) |
| `src/components/ui/context-menu.tsx` | Container classes (2 components) + item classes (6 components) |

---

## Task 1: Update `popover.tsx`

**Files:**
- Modify: `src/components/ui/popover.tsx`

- [ ] **Step 1: Apply container class changes**

Replace the `PopoverContent` className string. The full updated file:

```tsx
import * as React from "react";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { cn } from "@/lib/utils";

const Popover = PopoverPrimitive.Root;
const PopoverTrigger = PopoverPrimitive.Trigger;
const PopoverAnchor = PopoverPrimitive.Anchor;

function PopoverContent({
  className,
  align = "center",
  sideOffset = 4,
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Content>) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        data-slot="popover-content"
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "z-50 w-72 rounded-sm border border-border/60 bg-popover/95 backdrop-blur-sm p-0 text-popover-foreground shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
          className
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  );
}

export { Popover, PopoverTrigger, PopoverAnchor, PopoverContent };
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/popover.tsx
git commit -m "style(popover): apply VS Code modern container styles"
```

---

## Task 2: Update `dropdown-menu.tsx` containers

**Files:**
- Modify: `src/components/ui/dropdown-menu.tsx`

- [ ] **Step 1: Update `DropdownMenuContent` className**

Replace the className string inside `DropdownMenuContent`:

```tsx
className={cn(
  "z-50 min-w-[8rem] overflow-hidden rounded-sm border border-border/60 bg-popover/95 backdrop-blur-sm p-1 text-popover-foreground shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
  className
)}
```

- [ ] **Step 2: Update `DropdownMenuSubContent` className**

Replace the className string inside `DropdownMenuSubContent`:

```tsx
className={cn(
  "z-50 min-w-[8rem] overflow-hidden rounded-sm border border-border/60 bg-popover/95 backdrop-blur-sm p-1 text-popover-foreground shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
  className
)}
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/ui/dropdown-menu.tsx
git commit -m "style(dropdown-menu): apply VS Code modern container styles"
```

---

## Task 3: Update `dropdown-menu.tsx` items

**Files:**
- Modify: `src/components/ui/dropdown-menu.tsx`

- [ ] **Step 1: Update `DropdownMenuSubTrigger` className**

```tsx
className={cn(
  "flex cursor-default select-none items-center gap-2 rounded-none px-3 py-1 text-sm outline-none focus:bg-accent data-[state=open]:bg-accent [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  inset && "pl-8",
  className
)}
```

- [ ] **Step 2: Update `DropdownMenuItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center gap-2 rounded-none px-3 py-1 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  inset && "pl-8",
  className
)}
```

- [ ] **Step 3: Update `DropdownMenuCheckboxItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center rounded-none py-1 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
  className
)}
```

- [ ] **Step 4: Update `DropdownMenuRadioItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center rounded-none py-1 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
  className
)}
```

- [ ] **Step 5: Update `DropdownMenuSeparator` className**

```tsx
className={cn("-mx-1 my-1 h-px bg-border/60", className)}
```

- [ ] **Step 6: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/dropdown-menu.tsx
git commit -m "style(dropdown-menu): apply VS Code modern item styles"
```

---

## Task 4: Update `context-menu.tsx` containers

**Files:**
- Modify: `src/components/ui/context-menu.tsx`

- [ ] **Step 1: Update `ContextMenuContent` className**

```tsx
className={cn(
  "z-50 min-w-[8rem] overflow-hidden rounded-sm border border-border/60 bg-popover/95 backdrop-blur-sm p-1 text-popover-foreground shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] animate-in fade-in-80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
  className
)}
```

- [ ] **Step 2: Update `ContextMenuSubContent` className**

```tsx
className={cn(
  "z-50 min-w-[8rem] overflow-hidden rounded-sm border border-border/60 bg-popover/95 backdrop-blur-sm p-1 text-popover-foreground shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
  className
)}
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/ui/context-menu.tsx
git commit -m "style(context-menu): apply VS Code modern container styles"
```

---

## Task 5: Update `context-menu.tsx` items

**Files:**
- Modify: `src/components/ui/context-menu.tsx`

- [ ] **Step 1: Update `ContextMenuSubTrigger` className**

```tsx
className={cn(
  "flex cursor-default select-none items-center gap-2 rounded-none px-3 py-1 text-sm outline-none focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  inset && "pl-8",
  className
)}
```

- [ ] **Step 2: Update `ContextMenuItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center gap-2 rounded-none px-3 py-1 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  inset && "pl-8",
  className
)}
```

- [ ] **Step 3: Update `ContextMenuCheckboxItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center rounded-none py-1 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
  className
)}
```

- [ ] **Step 4: Update `ContextMenuRadioItem` className**

```tsx
className={cn(
  "relative flex cursor-default select-none items-center rounded-none py-1 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
  className
)}
```

- [ ] **Step 5: Update `ContextMenuSeparator` className**

```tsx
className={cn("-mx-1 my-1 h-px bg-border/60", className)}
```

- [ ] **Step 6: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/context-menu.tsx
git commit -m "style(context-menu): apply VS Code modern item styles"
```

---

## Task 6: Final build verification

**Files:** none

- [ ] **Step 1: Full TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 2: Build frontend**

```bash
yarn build
```

Expected: build completes with no errors.

- [ ] **Step 3: Visual smoke test**

Launch the app with `yarn tauri dev` and verify:
- Right-click a request in the sidebar — context menu has subtle frosted background, sharp item highlights, compact row height.
- Click the "..." on a request — dropdown menu matches the same style.
- Click the shield icon in the toolbar — popover opens as a bare shell (no extra padding from the container itself).
- Toggle dark mode — shadow deepens, blur still visible, border remains subtle.
