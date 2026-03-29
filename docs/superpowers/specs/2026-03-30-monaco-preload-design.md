# Monaco Editor Preload Design

**Goal:** Eliminate the cold-load delay when the user opens a request body editor for the first time in a session.

**Problem:** Monaco is a large bundle. Even though `MonacoWrapper` is lazy-loaded via `React.lazy()`, the chunk only starts downloading the moment the user first opens an editor panel — producing a visible blank or text-fallback state. The current fallback (`"Loading editor..."` text) makes the wait feel longer than it is.

---

## Architecture

Two independent improvements applied together:

1. **Preload trigger** — start downloading the Monaco chunk in the background immediately after the app shell renders, before the user navigates anywhere.
2. **Skeleton fallback** — replace the plain text loading state with a styled fake editor so the transition feels instant rather than blank-to-content.

---

## Preload Trigger

**File:** `src/App.tsx`

Add a single `useEffect` that fires once after the app mounts:

```ts
useEffect(() => {
  const id = requestIdleCallback(
    () => { import('@/components/editor/MonacoWrapper'); },
    { timeout: 2000 }
  );
  return () => cancelIdleCallback(id);
}, []);
```

**How it works:**
- `requestIdleCallback` waits for the browser's first idle period — after the critical UI has painted — then triggers the dynamic import.
- `{ timeout: 2000 }` forces the import to run within 2 seconds even if the browser stays busy.
- The import path is identical to what `React.lazy()` uses, so when the component actually renders it hits the already-populated module registry cache with no second download.
- No Vite config changes, no new dependencies, no build changes.

**TypeScript note:** `requestIdleCallback` is not in TypeScript's default `lib.dom.d.ts` for all targets. If the compiler does not recognize it, add a local type declaration:

```ts
declare function requestIdleCallback(cb: IdleRequestCallback, opts?: IdleRequestOptions): number;
declare function cancelIdleCallback(id: number): void;
```

---

## EditorSkeleton Component

**File:** `src/components/editor/EditorSkeleton.tsx`

A new component that mimics the Monaco editor's visual chrome while the real editor loads.

**Visual spec:**
- Full-width, full-height container matching the editor area
- Background: `#ffffff` in light mode, `#1f1f1f` in dark mode (matching `--monaco-bg`)
- Left column (~40px): 3–4 faint vertical bars simulating line number digits
- Content area: 5–7 horizontally-varying shimmer bars simulating code lines
- All shimmer uses Tailwind `animate-pulse` — no new animation CSS
- Uses the existing `useTheme` hook (or a `dark` class check) to select background

**Usage:** Three callsites replace their existing text fallback with `<EditorSkeleton />`:
1. `MonacoWrapper.tsx` — the `loading` prop (Monaco initializing after chunk loads)
2. `BodyEditor.tsx` — the `<Suspense>` fallback (chunk downloading)
3. `ResponseBodyViewer.tsx` — the `<Suspense>` fallback (chunk downloading)

---

## File Change Summary

| Action | File | Change |
|--------|------|--------|
| Modify | `src/App.tsx` | Add `requestIdleCallback` preload `useEffect` |
| Create | `src/components/editor/EditorSkeleton.tsx` | New skeleton component |
| Modify | `src/components/editor/MonacoWrapper.tsx` | Replace `loading` text with `<EditorSkeleton>` |
| Modify | `src/components/request/BodyEditor.tsx` | Replace Suspense fallback text with `<EditorSkeleton>` |
| Modify | `src/components/response/ResponseBodyViewer.tsx` | Replace Suspense fallback text with `<EditorSkeleton>` |

---

## Out of Scope

- Vite manual chunk splitting for Monaco (adds build complexity, marginal gain on a desktop app with reliable local caching)
- `<link rel="modulepreload">` in `index.html` (requires knowing the hashed chunk filename at build time)
- Hover/interaction-triggered preload (smaller window of opportunity than app-startup preload)
- ConflictResolver and DiffViewer (direct Monaco imports, not lazy — different performance profile)
