# Splash Screen Enhancement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refine the splash screen by shrinking the rocket animation, adding a radial glow, upgrading typography, and adding a smooth entrance animation.

**Architecture:** Single-file edit to `SplashScreen.tsx`. Add a `mounted` state for entrance animation, restructure JSX to wrap content in a glow container, shrink the Lottie container, and add a subtitle.

**Tech Stack:** React, Tailwind CSS, Lottie (existing)

**Spec:** `docs/superpowers/specs/2026-03-24-sp2-splash-enhancement-design.md`

---

### File Structure

- Modify: `src/components/SplashScreen.tsx`

No new files. No new dependencies.

---

### Task 1: Add mounted state for entrance animation

**Files:**
- Modify: `src/components/SplashScreen.tsx:10-11`

- [ ] **Step 1: Add mounted state and effect**

After line 10 (`const [fading, setFading] = useState(false);`), add:

```tsx
const [mounted, setMounted] = useState(false);

useEffect(() => {
  requestAnimationFrame(() => setMounted(true));
}, []);
```

- [ ] **Step 2: Verify the app still builds**

Run: `yarn dev` (or existing dev command) — confirm no errors in console.

- [ ] **Step 3: Commit**

```bash
git add src/components/SplashScreen.tsx
git commit -m "feat(splash): add mounted state for entrance animation"
```

---

### Task 2: Restructure JSX with glow, smaller animation, and entrance wrapper

**Files:**
- Modify: `src/components/SplashScreen.tsx:44-56`

- [ ] **Step 1: Replace the return JSX**

Replace the entire `return (...)` block (lines 44-56) with:

```tsx
return (
  <div
    className={cn(
      'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background transition-opacity duration-500',
      fading ? 'opacity-0' : 'opacity-100',
    )}
  >
    <div
      className={cn(
        'transition-all duration-300',
        mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
      )}
    >
      <div className="relative flex flex-col items-center">
        {/* Radial glow behind rocket. */}
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          <div className="w-[200px] h-[200px] rounded-full bg-blue-500/5 blur-2xl" />
        </div>
        {/* Lottie animation container. */}
        <div
          ref={containerRef}
          className="w-[160px] h-[160px] overflow-hidden relative z-10"
        />
        {/* App name and subtitle. */}
        <p className="mt-2 text-2xl font-bold text-foreground tracking-tight">
          Rocket
        </p>
        <p className="text-sm text-muted-foreground tracking-wide">
          API Workspace
        </p>
      </div>
    </div>
  </div>
);
```

Key changes from the original:
- `w-[300px] h-[300px]` shrunk to `w-[160px] h-[160px]`
- New wrapper div with `scale-95 opacity-0` → `scale-100 opacity-100` entrance transition
- New `relative` container with radial glow div behind the animation
- `mt-4` reduced to `mt-2`
- Added "API Workspace" subtitle in `text-sm text-muted-foreground`

- [ ] **Step 2: Verify visually**

Run the dev server and confirm:
1. Splash appears with a smooth scale-in entrance (not instant)
2. Rocket animation is noticeably smaller than before
3. Soft blue glow is visible behind the rocket
4. "Rocket" text appears with "API Workspace" below it
5. Fade-out still works after animation completes

- [ ] **Step 3: Commit**

```bash
git add src/components/SplashScreen.tsx
git commit -m "feat(splash): shrink animation, add glow, upgrade typography"
```

---

### Final State

The complete file after all tasks:

```tsx
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface SplashScreenProps {
  onComplete: () => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [fading, setFading] = useState(false);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    requestAnimationFrame(() => setMounted(true));
  }, []);

  useEffect(() => {
    // Load lottie.min.js from public/ via a script tag.
    const script = document.createElement('script');
    script.src = '/lottie.min.js';
    script.async = true;

    script.onload = () => {
      const lottie = (window as any).lottie;
      if (!lottie || !containerRef.current) return;

      const anim = lottie.loadAnimation({
        container: containerRef.current,
        renderer: 'svg',
        loop: false,
        autoplay: true,
        path: '/rocket-launch.json',
      });

      anim.addEventListener('complete', () => {
        setFading(true);
        setTimeout(onComplete, 500);
      });

      return () => anim.destroy();
    };

    document.head.appendChild(script);
    return () => {
      script.remove();
    };
  }, [onComplete]);

  return (
    <div
      className={cn(
        'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background transition-opacity duration-500',
        fading ? 'opacity-0' : 'opacity-100',
      )}
    >
      <div
        className={cn(
          'transition-all duration-300',
          mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
        )}
      >
        <div className="relative flex flex-col items-center">
          {/* Radial glow behind rocket. */}
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="w-[200px] h-[200px] rounded-full bg-blue-500/5 blur-2xl" />
          </div>
          {/* Lottie animation container. */}
          <div
            ref={containerRef}
            className="w-[160px] h-[160px] overflow-hidden relative z-10"
          />
          {/* App name and subtitle. */}
          <p className="mt-2 text-2xl font-bold text-foreground tracking-tight">
            Rocket
          </p>
          <p className="text-sm text-muted-foreground tracking-wide">
            API Workspace
          </p>
        </div>
      </div>
    </div>
  );
}
```
