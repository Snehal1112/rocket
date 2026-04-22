# Collection Header Scroll Elevation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the tab body inside `CollectionOverviewTab` is scrolled, the header div picks up a drop shadow so it visually floats above the content; shadow disappears when scrolled back to the top.

**Architecture:** Add an `isScrolled` boolean state driven by a `scroll` listener on the Radix `ScrollArea` viewport element. The viewport is found by querying `[data-radix-scroll-area-viewport]` inside a wrapper ref. The shadow class is applied conditionally to the existing header div. No new files, no new components.

**Tech Stack:** React (useState, useEffect, useRef), Tailwind CSS arbitrary shadow value, Radix UI scroll area DOM attribute.

---

### Task 1: Add scroll-driven elevation to the collection header

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

- [ ] **Step 1: Add `useRef` to imports**

In `src/components/collections/CollectionOverviewTab.tsx`, line 2, change:

```typescript
import { useCallback, useEffect, useState } from 'react';
```

to:

```typescript
import { useCallback, useEffect, useRef, useState } from 'react';
```

- [ ] **Step 2: Add `isScrolled` state and `scrollContainerRef`**

Inside the component function body, near the other `useState` declarations (around line 220–260), add:

```typescript
const [isScrolled, setIsScrolled] = useState(false);
const scrollContainerRef = useRef<HTMLDivElement>(null);
```

- [ ] **Step 3: Add the scroll listener effect**

After the state declarations, add this `useEffect`:

```typescript
useEffect(() => {
  const container = scrollContainerRef.current;
  if (!container) return;
  const viewport = container.querySelector<HTMLElement>(
    '[data-radix-scroll-area-viewport]',
  );
  if (!viewport) return;
  const handleScroll = () => setIsScrolled(viewport.scrollTop > 0);
  viewport.addEventListener('scroll', handleScroll, { passive: true });
  return () => viewport.removeEventListener('scroll', handleScroll);
}, []);
```

- [ ] **Step 4: Attach the ref to a wrapper div around `ScrollArea`**

Find the `<ScrollArea className='flex-1'>` line (around line 423). Wrap it in a `div` with the ref:

```tsx
<div ref={scrollContainerRef} className='flex-1 min-h-0'>
  <ScrollArea className='h-full'>
    {/* existing content unchanged */}
  </ScrollArea>
</div>
```

Note: Change `className='flex-1'` on the `ScrollArea` to `className='h-full'` since the `flex-1` moves to the wrapper div. This keeps the layout identical.

- [ ] **Step 5: Apply the shadow class conditionally to the header div**

Find the existing header div (around line 391):

```tsx
<div className='shrink-0 border-b border-border/70 px-6 pt-4 pb-0'>
```

Change it to:

```tsx
<div
  className={`shrink-0 border-b border-border/70 px-6 pt-4 pb-0 transition-shadow duration-200${isScrolled ? ' shadow-[0_4px_16px_rgba(0,0,0,0.5)]' : ''}`}
>
```

- [ ] **Step 6: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Verify the app runs and the effect works**

```bash
yarn tauri dev
```

1. Open a collection tab.
2. Scroll down in the tab body — the header should gain a shadow.
3. Scroll back to top — shadow should disappear with a 200ms transition.
4. Verify no other visual changes (colors, borders, tab styles unchanged).

- [ ] **Step 8: Commit**

```bash
git add src/components/collections/CollectionOverviewTab.tsx
git commit -m "feat(ui): add scroll elevation shadow to collection header tab bar"
```
