---
layout: doc
---

# EnvironmentDialog Blur Backdrop Design

**Date:** 2026-04-07

## Goal
Apply a blurred, semi‑transparent backdrop to the `EnvironmentDialog` modal, matching the visual style used for the dropdown overlay in `WorkspaceSwitcher`.

## Target Element
The backdrop (overlay) rendered by the `Dialog` component – *not* the dialog content itself.

## Styling
Use the Tailwind utility string used elsewhere:

```tsx
className="bg-card/50 backdrop-blur-sm border border-border/70"
```

- `bg-card/50` – 50 % opaque background that respects the current theme.
- `backdrop-blur-sm` – Small blur effect.
- `border border-border/70` – Light border that also adapts to the theme.

## Implementation Approach (Recommended)
1. **Pass `className` to `<Dialog>`** – the `Dialog` component from `ui/dialog` forwards this prop to its overlay.
2. Verify the class reaches the overlay; if not, create a thin `DialogOverlay` wrapper component that applies the classes.
3. Keep dark‑mode responsiveness (Tailwind automatically swaps colors).

## Steps
1. Update `src/components/environments/EnvironmentDialog.tsx`:
   ```tsx
   <Dialog open={open} onOpenChange={onOpenChange}
           className="bg-card/50 backdrop-blur-sm border border-border/70">
       ...
   </Dialog>
   ```
2. Run the app locally, open the environment dialog, and confirm the backdrop looks identical to the dropdown blur on both light and dark themes.
3. Ensure focus‑trap and accessibility attributes remain intact (the blur does not affect interaction).
4. Add visual regression test (snapshot) for the dialog with the new backdrop.

## Testing
- **Manual visual check** on desktop and mobile viewports for both themes.
- **Automated test** using Playwright to capture a screenshot of the dialog and compare against a baseline.

## Accessibility
- The blur is purely decorative; it does not hide content or affect contrast ratios.
- All ARIA roles and focus management from `Dialog` remain unchanged.

## Specification Self‑Review
- No placeholders – all details are concrete.
- Consistent with existing design language.
- Scoped to a single implementation (no extra features).
- No ambiguous wording.

---
