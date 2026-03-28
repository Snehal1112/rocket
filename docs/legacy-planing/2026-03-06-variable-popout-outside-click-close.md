# Implementation Plan: Variable Popout Outside-Click Close

## Objective
Fix `VariableAwareUrlInput` so the token edit popout closes immediately on outside click and `Escape`, discarding unsaved edits.

## Step 1: Add close-boundary refs
- In `VariableAwareUrlInput.tsx`, add refs for:
  - component wrapper container
  - active dropdown content container
- Attach refs in JSX to support containment checks when content is portaled.

## Step 2: Implement outside-click dismissal
- Add an effect active only while `editingTokenKey` is set.
- Register `document` `pointerdown` listener.
- If event target is outside both wrapper and dropdown content, call `closeEditor()` immediately.

## Step 3: Implement Escape dismissal
- In the same open-state effect, register `document` `keydown` listener.
- On `Escape`, call `closeEditor()` and prevent default bubbling side effects where appropriate.

## Step 4: Preserve existing hover behavior
- Keep current hover open/close timer logic.
- Ensure outside-click path bypasses timer delay and does not trigger accidental reopen.

## Step 5: Tests
- Update `VariableAwareUrlInput.test.tsx`:
  - open popup, click outside, verify popup closes
  - open popup, press Escape, verify popup closes
  - verify outside-dismiss does not call save handlers
  - keep existing save-path tests passing

## Step 6: Verification
- Run:
  - `cd frontend && yarn lint`
  - `cd frontend && yarn test`
- Manually verify in URL input:
  - hover token -> popup opens
  - click outside -> closes immediately
  - unsaved value is discarded

## Rollback
- If regressions appear, revert only `VariableAwareUrlInput` and related test changes.
