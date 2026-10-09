# Frontend Component Guardrails

## UI Primitives and Icons

- Use shadcn/ui primitives and lucide-react icons.
- Do not add raw form/dialog/button/select/input primitives.

## Editor Selection

- Single-line variable-aware fields: SingleLineEditor.
- Multi-line editor surfaces: Monaco.
- Approved exception (2026-10-09): the AI Assistant prompt editor may use CodeMirror 6 (`PromptEditor`). It is prose with mentions, a growing field with a max height. The exception covers that one component only. Any other multi-line editor stays Monaco, and a new use of CodeMirror for multi-line text needs a new approval.

## Zustand Constraints

- Prefer narrow selectors.
- Do not fully destructure store state at component top level.

## Verification

- yarn tsc --noEmit
- yarn check
