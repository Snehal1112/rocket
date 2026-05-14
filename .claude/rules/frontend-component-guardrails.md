# Frontend Component Guardrails

## UI Primitives and Icons

- Use shadcn/ui primitives and lucide-react icons.
- Do not add raw form/dialog/button/select/input primitives.

## Editor Selection

- Single-line variable-aware fields: SingleLineEditor.
- Multi-line editor surfaces: Monaco.

## Zustand Constraints

- Prefer narrow selectors.
- Do not fully destructure store state at component top level.

## Verification

- yarn tsc --noEmit
- yarn check
