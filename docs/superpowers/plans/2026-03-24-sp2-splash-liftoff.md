# Splash Screen Liftoff Animation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the splash screen fade-out with a rocket liftoff animation where the rocket flies upward off screen before the background fades.

**Architecture:** Single-file edit replacing the `fading` state with `liftingOff`, updating the Lottie complete handler timing, and changing CSS classes on two elements (outer container and inner content wrapper).

**Tech Stack:** React, Tailwind CSS, Lottie (existing)

**Spec:** `docs/superpowers/specs/2026-03-24-sp2-splash-liftoff-design.md`

---

### File Structure

- Modify: `src/components/SplashScreen.tsx`

No new files. No new dependencies.

---

### Task 1: Replace fade-out with liftoff animation

**Files:**
- Modify: `src/components/SplashScreen.tsx:10,35-37,51-54,56-60`

- [ ] **Step 1: Replace `fading` state with `liftingOff`**

On line 10, replace:
```tsx
const [fading, setFading] = useState(false);
```
with:
```tsx
const [liftingOff, setLiftingOff] = useState(false);
```

- [ ] **Step 2: Update Lottie complete handler**

On lines 35-37, replace:
```tsx
      anim.addEventListener('complete', () => {
        setFading(true);
        setTimeout(onComplete, 500);
      });
```
with:
```tsx
      anim.addEventListener('complete', () => {
        setLiftingOff(true);
        setTimeout(onComplete, 1000);
      });
```

- [ ] **Step 3: Update outer container className**

On lines 51-54, replace:
```tsx
      className={cn(
        'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background transition-opacity duration-500',
        fading ? 'opacity-0' : 'opacity-100',
      )}
```
with:
```tsx
      className={cn(
        'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background',
        liftingOff
          ? 'opacity-0 transition-opacity duration-[400ms] delay-[600ms]'
          : 'opacity-100',
      )}
```

Key changes:
- Removed always-present `transition-opacity duration-500` (transition only applies during liftoff)
- Added `delay-[600ms]` so background fades after rocket clears screen
- Changed duration from 500ms to 400ms

- [ ] **Step 4: Update inner content wrapper className**

On lines 57-60, replace:
```tsx
        className={cn(
          'transition-all duration-300',
          mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
        )}
```
with:
```tsx
        className={cn(
          mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
          liftingOff
            ? 'transition-transform duration-[800ms] ease-in -translate-y-[100vh]'
            : 'transition-all duration-300',
        )}
```

Key changes:
- When `liftingOff` is false: entrance animation uses `transition-all duration-300` (same as before)
- When `liftingOff` is true: overrides to `transition-transform duration-[800ms] ease-in` with `-translate-y-[100vh]` to fly the rocket upward
- `ease-in` makes it start slow and accelerate (like real thrust)

- [ ] **Step 5: Verify visually**

Run the dev server and confirm:
1. Splash appears with smooth scale-in entrance (unchanged)
2. Lottie rocket animation plays (unchanged)
3. After animation completes, rocket + text + glow fly upward off screen
4. The upward motion starts slow and accelerates
5. Background fades out after rocket clears, revealing the app
6. Total transition is ~1 second

- [ ] **Step 6: Verify build**

Run: `npx tsc --noEmit && npx vite build`
Expected: Exit 0, no errors

- [ ] **Step 7: Commit**

```bash
git add src/components/SplashScreen.tsx
git commit -m "feat(splash): replace fade-out with rocket liftoff animation"
```

---

### Final State

The complete file after all changes:

```tsx
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface SplashScreenProps {
  onComplete: () => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [liftingOff, setLiftingOff] = useState(false);
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
        setLiftingOff(true);
        setTimeout(onComplete, 1000);
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
        'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background',
        liftingOff
          ? 'opacity-0 transition-opacity duration-[400ms] delay-[600ms]'
          : 'opacity-100',
      )}
    >
      <div
        className={cn(
          mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
          liftingOff
            ? 'transition-transform duration-[800ms] ease-in -translate-y-[100vh]'
            : 'transition-all duration-300',
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
