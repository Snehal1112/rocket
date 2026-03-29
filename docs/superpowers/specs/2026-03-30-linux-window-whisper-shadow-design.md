# Linux Window Whisper Shadow

**Date:** 2026-03-30
**Status:** Approved

## Problem

The current Linux window box-shadow is heavy and directional — two downward-offset layers at 0.25 and 0.35 opacity. This reads as "shadow cast over the desktop" rather than a subtle floating-window effect.

## Solution

Replace the box-shadow on `html.linux #root` in `src/index.css` with a near-invisible two-layer whisper:

```css
box-shadow: 0 0 0 1px rgba(0,0,0,0.04), 0 4px 20px rgba(0,0,0,0.08);
```

- Layer 1: `0 0 0 1px rgba(0,0,0,0.04)` — a barely-visible outer ring for edge crispness. Sits just outside the existing `border` property.
- Layer 2: `0 4px 20px rgba(0,0,0,0.08)` — a faint ambient haze giving the window a gentle elevation feel without a strong directional bias.

## Scope

Single file, single property change:

- `src/index.css` — line 193, `box-shadow` value inside `html.linux #root`

No Tauri config changes, no Rust changes, no component changes.
