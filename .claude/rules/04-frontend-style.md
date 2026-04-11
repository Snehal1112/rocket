# Frontend style rules

TypeScript + React + Tailwind + shadcn/ui.

## Package manager

- Yarn, not npm. Never run `npm install`.

## File naming

- Components: `PascalCase.tsx`.
- Stores: `kebab-case-store.ts` under `src/stores/`.
- Hooks: `useCamelCase.ts` under `src/hooks/`.
- Utilities: `kebab-case.ts`.
- Test files: colocated under `__tests__/` with `.test.ts(x)` suffix.

## Components

- Named exports only. No `default export` for components.
- Props interfaces named `<Component>Props`, declared in the same file.
- Destructure props in the signature, not inside the body.
- Keep feature components in domain folders: `collections/`, `environments/`, `git/`, `workspace/`, `request/`, `response/`, `contract/`.
- `src/components/ui/` holds shadcn primitives. Do not edit primitives for feature styling — compose them in feature components.

## State

- Server-side state lives in Zustand stores under `src/stores/`. Do not mirror it in component local state.
- Local UI state (open/closed, input value, hover) lives in the component.
- Zustand selectors that return new arrays/objects on every call break referential equality and cause re-renders. Use a module-level `EMPTY_<X>` sentinel for empty cases — see `CollectionNode.tsx` for the pattern.
- `isDirty` on a tab triggers `scheduleAutoSave()` before close or switch. Do not bypass it.

## Types

- Types from `src/lib/tauri-api.ts` are the single source of truth for wire shapes. Import from there, do not redeclare.
- No `any`. Use `unknown` and narrow at the boundary.
- Prefer discriminated unions (`type: 'foo' | 'bar'`) over optional-field combinations.

## Hooks and effects

- Every `useEffect` with a subscription or listener must return a cleanup function.
- Dependencies arrays are exhaustive. If eslint-disable is tempting, extract a callback via `useCallback` instead.
- `useMemo` / `useCallback` only when a downstream `React.memo` or `useEffect` depends on stable identity. No speculative memoization.

## Styling

- Tailwind utility classes, grouped by concern: layout → spacing → typography → color → state.
- CSS variables (`var(--color-background-secondary)`, etc.) for theme tokens — never hardcode hex values unless the design spec explicitly calls for them.
- shadcn primitives first. Raw `<div>` + Tailwind only when no primitive fits.

## Strings and UX

- No emojis in source or UI strings unless the design explicitly asks.
- User-facing errors go through a toast/notification system (when available). Do not rely on `console.error` as the only feedback — see `.claude/sidebar-known-issues.md` for the backlog.
