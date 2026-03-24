# Splash Screen with Rocket Lottie Animation — Design Spec

**Date:** 2026-03-24
**Status:** Approved

## Goal

Show a full-screen splash screen with the rocket Lottie animation when the app launches. The animation plays once, fades out on completion, and reveals the main workspace.

## Decisions

- Use existing `lottie.min.js` from `public/` (no new npm dependencies)
- Animation plays once, dismiss on `complete` event
- Fade-out transition (~500ms) after animation ends

## Implementation

### New file: `src/components/SplashScreen.tsx`

- Full-screen overlay: `fixed inset-0 z-50` with dark background (`bg-background`)
- Loads `/lottie.min.js` via dynamic `<script>` tag in a `useEffect`
- Once script loads, calls `window.lottie.loadAnimation()` with:
  - `container`: a ref'd `<div>`
  - `renderer`: `'svg'`
  - `loop`: `false`
  - `autoplay`: `true`
  - `path`: `'/rocket-launch.json'`
- Listens for `'complete'` event on the animation instance
- On complete: sets a `fading` state to true, which adds `opacity-0 transition-opacity duration-500`
- After the 500ms transition, calls `onComplete` prop to unmount
- Animation container: centered (flex, items-center, justify-center), max size ~300x300px
- "Rocket" text centered below animation: `text-2xl font-bold text-foreground`

### Modified file: `src/App.tsx`

- Add `const [showSplash, setShowSplash] = useState(true)`
- Render `{showSplash && <SplashScreen onComplete={() => setShowSplash(false)} />}` as the last child (highest z-index) inside the root `<div>`
- Main app renders underneath and initializes in parallel

### TypeScript typing

Declare `window.lottie` in a type augmentation or inline cast to avoid TS errors:
```typescript
const lottie = (window as any).lottie;
```

## Files

- Create: `src/components/SplashScreen.tsx`
- Modify: `src/App.tsx` (add splash state + render)
- Existing assets: `public/lottie.min.js`, `public/rocket-launch.json`
