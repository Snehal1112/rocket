---
description: "Use when editing React and UI files in src. Enforces shadcn primitives, lucide icons, editor selection rules, and Zustand usage guardrails."
name: "Rocket Frontend Component Guardrails"
applyTo:
  - "src/**/*.ts"
  - "src/**/*.tsx"
---

# Frontend Component Guardrails

Use these rules for [src](../../src).

## UI Primitives and Icons

- Use shadcn/ui primitives for interactive controls.
- Use lucide-react icons.
- Do not add raw form/dialog/button/select/input primitives.

## Editor Selection Rules

- Single-line variable-aware fields: SingleLineEditor.
- Multi-line editor surfaces: Monaco.

## Zustand Usage

- Prefer narrow selectors.
- Do not fully destructure store state at component top level.

## Consistency and Boundaries

- Keep frontend concerns in frontend files.
- Route backend logic through Tauri commands.

## Verification Before Completion

- yarn tsc --noEmit
- yarn check
- targeted yarn test when needed
