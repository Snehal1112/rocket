# Error Boundary Migration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled class-based `ErrorBoundary` with `react-error-boundary` library while preserving the exact existing visual design and public API.

**Architecture:** Install the `react-error-boundary` package, extract the fallback UI into a typed `ErrorFallback` component using the library's `FallbackProps`, and re-export a thin `ErrorBoundary` wrapper that wires `FallbackComponent`, `onError` logging, and forwards `resetKeys` for future use. Call sites remain unchanged.

**Tech Stack:** React 18, TypeScript, `react-error-boundary` v6, shadcn/ui, Lucide icons

---

### Task 1: Install react-error-boundary

**Files:**
- Modify: `frontend/package.json` (dependency added by yarn)

- [ ] **Step 1: Install the package**

```bash
cd frontend
yarn add react-error-boundary
```

Expected: `react-error-boundary` appears in `package.json` dependencies.

- [ ] **Step 2: Verify types are bundled** (the package ships its own types — no `@types/` needed)

```bash
grep "react-error-boundary" package.json
```

Expected: a version line like `"react-error-boundary": "^6.x.x"`.

- [ ] **Step 3: Commit**

```bash
git add frontend/package.json frontend/yarn.lock
git commit -m "chore: add react-error-boundary dependency"
```

---

### Task 2: Rewrite ErrorBoundary.tsx

**Files:**
- Modify: `frontend/src/components/ErrorBoundary.tsx`

The new file replaces the class component with two exports:
- `ErrorFallback` — the fallback UI component (same design as before)
- `ErrorBoundary` — a thin functional wrapper over the library's `<ErrorBoundary>`

- [ ] **Step 1: Replace the file contents**

```tsx
import { AlertTriangle, RefreshCw } from 'lucide-react'
import { type ReactNode } from 'react'
import {
  ErrorBoundary as ReactErrorBoundary,
  type FallbackProps,
} from 'react-error-boundary'

import { Button } from '@/components/ui/button'

// ── Fallback UI ───────────────────────────────────────────────────────────────

export function ErrorFallback({ error, resetErrorBoundary }: FallbackProps) {
  return (
    <div className="flex h-screen w-full items-center justify-center bg-background p-8">
      <div className="max-w-md text-center">
        <AlertTriangle className="mx-auto h-12 w-12 text-destructive/70" />
        <h1 className="mt-4 text-lg font-semibold text-foreground">
          Something went wrong
        </h1>
        <p className="mt-2 text-sm text-muted-foreground">
          An unexpected error occurred. You can try reloading or resetting the
          application state.
        </p>
        {error instanceof Error && (
          <pre className="mt-4 max-h-32 overflow-auto rounded-md border bg-muted p-3 text-left text-xs text-muted-foreground">
            {error.message}
          </pre>
        )}
        <div className="mt-6 flex items-center justify-center gap-3">
          <Button variant="outline" size="sm" onClick={resetErrorBoundary}>
            <RefreshCw className="mr-1.5 h-3.5 w-3.5" />
            Try Again
          </Button>
          <Button size="sm" onClick={() => window.location.reload()}>
            Reload Page
          </Button>
        </div>
      </div>
    </div>
  )
}

// ── Boundary wrapper ──────────────────────────────────────────────────────────

interface ErrorBoundaryProps {
  children: ReactNode
  /** When any of these keys change the boundary resets automatically */
  resetKeys?: unknown[]
}

export function ErrorBoundary({ children, resetKeys }: ErrorBoundaryProps) {
  function handleError(error: Error, info: { componentStack?: string | null }) {
    console.error('Uncaught error:', error, info.componentStack)
  }

  return (
    <ReactErrorBoundary
      FallbackComponent={ErrorFallback}
      onError={handleError}
      resetKeys={resetKeys}
    >
      {children}
    </ReactErrorBoundary>
  )
}
```

- [ ] **Step 2: Verify TypeScript compiles cleanly**

```bash
cd frontend
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Smoke-test in dev**

```bash
yarn tauri dev
```

Open the app and verify it launches normally (no crash, error UI does not appear at startup).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/ErrorBoundary.tsx
git commit -m "feat: migrate ErrorBoundary to react-error-boundary library"
```
