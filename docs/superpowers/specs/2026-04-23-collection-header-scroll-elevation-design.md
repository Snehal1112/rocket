# Collection Header Scroll Elevation

## Goal

When the user scrolls the tab body content inside `CollectionOverviewTab`, the header (which contains the title, stats, and tab bar) picks up a drop shadow to visually float above the scrolling content. No other visual changes.

## Behaviour

- **At rest (scrollTop === 0):** Header looks exactly as today — no shadow.
- **Scrolled (scrollTop > 0):** Header gains `shadow-[0_4px_16px_rgba(0,0,0,0.5)]`. Transitions in over 200ms.
- **Scroll back to top:** Shadow transitions out over 200ms.

## Implementation

**File:** `src/components/collections/CollectionOverviewTab.tsx`

1. Add `isScrolled` state: `const [isScrolled, setIsScrolled] = useState(false)`.
2. Get a ref to the `ScrollArea` viewport using shadcn's `ScrollAreaPrimitive.Viewport` or by passing a `ref` to the `ScrollArea` component and querying its inner viewport element via `useEffect`.
3. Attach a `scroll` listener on the viewport that sets `setIsScrolled(el.scrollTop > 0)`.
4. On the existing header `<div>`, add classes conditionally:
   - Always: `transition-shadow duration-200`
   - When `isScrolled`: `shadow-[0_4px_16px_rgba(0,0,0,0.5)]`
5. Clean up the scroll listener on unmount.

## Constraints

- No changes to colors, borders, typography, layout, or tab styles.
- Shadow only — no backdrop blur, no gradient, no background color change.
- The existing `border-b border-border/70` remains unchanged at all times.
