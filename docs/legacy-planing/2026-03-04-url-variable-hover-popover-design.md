# URL Variable Hover Popover Design (Postman-like)

## Context
URL variables are now highlighted inline, but the editor UI is shown as a separate inline panel. User wants Postman-like non-modal floating popover anchored to hovered variable tokens.

## Goal
Use a shadcn-style non-modal floating popover anchored to each URL variable token for quick value editing.

## Decision
Adopt HoverCard-style anchored popover behavior (non-modal) for `{{var}}` tokens.

## UX Contract
- Variables remain highlighted inline inside URL field only.
- Hover/focus on a token opens anchored floating popover.
- Popover includes variable name/source/value editor + Save/Cancel.
- Save behavior remains env-first with collection fallback.
- No center modal dialog.

## Technical Design
1. Add shadcn-style `HoverCard` wrapper component in `frontend/src/components/ui/hover-card.tsx` (Radix hover card primitives).
2. Refactor `VariableAwareUrlInput` token rendering to wrap each token with `HoverCardTrigger` and `HoverCardContent`.
3. Move current editor controls into hover card content.
4. Keep `onSaveVariable` contract and save flow unchanged.

## Validation
- `cd frontend && yarn -s lint`
- `cd frontend && yarn -s test`
- `cd frontend && yarn -s build`

## Acceptance Criteria
- Hovering a URL variable opens anchored non-modal popover.
- Editing and saving variable works as before.
- Inline URL highlight behavior remains intact.
