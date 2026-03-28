# Handle Save URL Variable TDZ Design

## Summary

Fix the `Cannot access 'handleSaveUrlVariable' before initialization` runtime error in the request builder without changing any existing variable persistence behavior.

## Problem

`handleSubmit` references `handleSaveUrlVariable` before the callback is initialized. Because both are declared as `const` callbacks inside `useRequestBuilderState`, React evaluates the `handleSubmit` declaration while `handleSaveUrlVariable` is still in the temporal dead zone.

## Goals

- Remove the runtime initialization error.
- Preserve current behavior for script-driven variable writes.
- Preserve current behavior for toolbar-driven variable saves.
- Keep the change narrow to the request builder hook.

## Non-Goals

- No changes to collection or environment persistence rules.
- No refactor of store APIs.
- No UI changes.

## Options Considered

### Option 1: Reorder callback declarations

Move `handleSaveUrlVariable` above `handleSubmit` so every callback is initialized before first use.

Pros:
- Smallest possible fix.
- No behavior change.
- Easy to verify and low risk.

Cons:
- Does not improve reuse beyond this hook.

### Option 2: Extract a helper function

Move the save logic into a helper declared earlier in the module or hook, then have callbacks call that helper.

Pros:
- Removes ordering sensitivity.
- Could improve reuse later.

Cons:
- Adds indirection without solving a broader problem today.

### Option 3: Duplicate the save logic at call sites

Inline the save behavior into `handleSubmit` and the toolbar path separately.

Pros:
- Removes the shared callback dependency.

Cons:
- Duplicates logic and increases regression risk.

## Decision

Use Option 1. The bug is caused by declaration order, so the fix should address declaration order directly.

## Implementation Sketch

1. Move `handleSaveUrlVariable` above `handleSubmit` in `frontend/src/components/request-builder/useRequestBuilderState.ts`.
2. Keep the callback body unchanged.
3. Add a regression test that renders the hook path and verifies initialization no longer throws.

## Verification

- Run the targeted frontend test covering the request builder hook.
- Run the relevant frontend test suite if a new focused test is added.
