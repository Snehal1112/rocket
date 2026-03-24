# Splash Screen Enhancement — Design Spec

**Date:** 2026-03-24
**Status:** Approved
**Builds on:** `2026-03-24-splash-screen-design.md`

## Problem

The current splash screen rocket Lottie animation is too large (300x300px) relative to the screen, making it feel cartoonish and unpolished compared to the professional API workspace UI it introduces.

## Changes

All changes are confined to `src/components/SplashScreen.tsx`. No new files or dependencies.

### 1. Shrink Animation Container

- Reduce from `w-[300px] h-[300px]` to `w-[160px] h-[160px]`
- Keep `overflow-hidden` to clip the animation cleanly

### 2. Radial Glow Behind Rocket

- Add a decorative container behind the animation with a soft radial gradient
- Light blue-to-transparent circular glow, roughly 200x200px
- CSS only, using a pseudo-element or a wrapper div with `bg-radial-gradient`
- Gives depth without adding visual clutter

### 3. Typography Upgrade

- Keep "Rocket" as `text-2xl font-bold text-foreground tracking-tight`
- Add "API Workspace" subtitle: `text-sm text-muted-foreground tracking-wide`
- Reduce spacing between animation and text from `mt-4` to `mt-2`

### 4. Entrance Animation

- Wrap the animation + text group in a container that starts at `scale-95 opacity-0`
- Transition to `scale-100 opacity-100` over 300ms on mount using a `mounted` state
- Gives the splash a smooth entrance feel

### 5. Fade-Out (No Change)

- Keep existing 500ms opacity fade-out on Lottie `complete` event
- Keep existing 500ms `setTimeout` before calling `onComplete`

## Implementation Detail

```tsx
// New state for entrance animation
const [mounted, setMounted] = useState(false);

useEffect(() => {
  // Trigger entrance animation on next frame
  requestAnimationFrame(() => setMounted(true));
}, []);
```

The radial glow is a sibling/wrapper div behind the Lottie container:
```tsx
{/* Outer wrapper with entrance animation */}
<div className={cn(
  'transition-all duration-300',
  mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
)}>
  <div className="relative flex flex-col items-center">
    {/* Radial glow */}
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
      <div className="w-[200px] h-[200px] rounded-full bg-blue-500/5 blur-2xl" />
    </div>
    {/* Lottie animation */}
    <div ref={containerRef} className="w-[160px] h-[160px] overflow-hidden relative z-10" />
    {/* Text */}
    <p className="mt-2 text-2xl font-bold text-foreground tracking-tight">Rocket</p>
    <p className="text-sm text-muted-foreground tracking-wide">API Workspace</p>
  </div>
</div>
```

## Files

- Modify: `src/components/SplashScreen.tsx`
