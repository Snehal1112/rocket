# Splash Screen Liftoff Animation — Design Spec

**Date:** 2026-03-24
**Status:** Approved
**Builds on:** `2026-03-24-sp2-splash-enhancement-design.md`

## Problem

The current splash screen dismisses with a simple opacity fade, which feels flat. The rocket should lift off the screen to match the app's identity and create a more engaging transition.

## Changes

All changes confined to `src/components/SplashScreen.tsx`. No new files or dependencies.

### 1. Replace fade-out with liftoff animation

Remove the `fading` state. Add a `liftingOff` state that triggers on Lottie `complete`.

When `liftingOff` is true:
- The inner content group (rocket, glow, text) translates upward via `translateY(-100vh)` over 800ms with `ease-in` timing (starts slow, accelerates like real thrust)
- The outer background container fades from `opacity-100` to `opacity-0` starting at 600ms delay, over 400ms
- After 1000ms total, `onComplete` fires to unmount the splash

### 2. State changes

- Remove: `const [fading, setFading] = useState(false)`
- Add: `const [liftingOff, setLiftingOff] = useState(false)`
- Lottie `complete` handler: `setLiftingOff(true); setTimeout(onComplete, 1000)`

### 3. Outer container className

```tsx
className={cn(
  'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background',
  liftingOff ? 'opacity-0 transition-opacity duration-[400ms] delay-[600ms]' : 'opacity-100',
)}
```

### 4. Inner content wrapper className

```tsx
className={cn(
  'transition-all duration-300',
  mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
  liftingOff && 'translate-y-[-100vh] transition-transform duration-[800ms] ease-in',
)}
```

Note: When `liftingOff` activates, it overrides the `transition-all duration-300` with `transition-transform duration-[800ms]` to control the liftoff speed independently.

### 5. Unchanged

- Entrance animation (`mounted` state with scale-in)
- Radial glow, 160x160 animation size, typography
- Lottie script loading logic

## Files

- Modify: `src/components/SplashScreen.tsx`
