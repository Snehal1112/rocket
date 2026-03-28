# Variable Popout Outside-Click Close Design

## Problem
In `VariableAwareUrlInput`, the variable/path/query edit popout does not close when the user clicks outside the popout. Current close behavior is tied to hover-leave timers and explicit Cancel/Save actions.

## Goal
Match expected editor behavior: when the popout is open and the user clicks outside, close it immediately and discard unsaved edits.

## Scope
- Component: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`
- Interaction only; no backend or API contract changes.

## Approved Behavior
- Outside click closes popout immediately.
- `Escape` closes popout immediately.
- Unsaved edits are discarded on outside click / Escape.
- `Save` persists and closes (existing behavior).
- `Cancel` discards and closes (existing behavior).

## Options Considered
1. Document-level outside-click + Escape handling (recommended)
2. Rely only on Radix `onOpenChange`
3. Switch to click-only opening and remove hover-open

Chosen: **Option 1** to preserve current hover interaction while fixing the close bug reliably.

## Technical Design
1. Add refs for open popout context:
   - root wrapper ref for the URL input region
   - dropdown content ref for the currently open menu
2. When `editingTokenKey` is set, register:
   - `document` `pointerdown` listener for outside-click detection
   - `document` `keydown` listener for `Escape`
3. Outside click condition:
   - if target is outside both wrapper and dropdown content, call `closeEditor()` directly (no delay timer)
4. Keep existing hover timer logic unchanged for hover-only transitions.
5. Ensure listeners are removed on close/unmount.

## Risks & Mitigations
- Risk: false outside detection with portal content.
  - Mitigation: include explicit dropdown-content ref in containment checks.
- Risk: reopen flicker due to hover suppression logic.
  - Mitigation: outside-close path sets closed state directly and preserves suppression guard behavior.

## Testing Strategy
- Update/add unit tests in `VariableAwareUrlInput.test.tsx`:
  - closes on document outside click
  - closes on Escape
  - does not call save handlers when closed via outside click/Escape
  - existing save flow remains green
- Run `yarn lint` and `yarn test`.

## Success Criteria
- User can open a token popout, click anywhere outside, and popout closes immediately.
- No warning popover remains open after outside interaction.
- Existing variable and param save flows are unaffected.
