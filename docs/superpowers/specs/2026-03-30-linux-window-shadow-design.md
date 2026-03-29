# Spec: Linux Window Drop Shadow

**Date:** 2026-03-30
**Scope:** Linux only — Tauri config + CSS

---

## Problem

On Linux, `decorations: false` in `tauri.linux.conf.json` removes native window manager decorations including the drop shadow. The base `tauri.conf.json` has `shadow: true`, but GTK ignores this for undecorated windows, leaving the Rocket window with no visual separation from the desktop background.

---

## Goal

Add a CSS drop shadow to the window on Linux so it looks visually distinct and elevated, matching what native window shadows provide on macOS and Windows.

---

## Out of Scope

- macOS — uses `titleBarStyle: Overlay` with native chrome and shadow already.
- Windows — not reported as an issue; no changes.
- Vibrancy, blur, or frosted-glass effects.

---

## Architecture

Three minimal changes, all scoped to Linux:

### 1. `src-tauri/tauri.linux.conf.json`

Add `"transparent": true`:

```json
{
  "app": {
    "windows": [
      {
        "decorations": false,
        "transparent": true
      }
    ]
  }
}
```

`transparent: true` makes the webview background clear, allowing CSS shadows to appear in the transparent space outside the content area.

### 2. `src/App.tsx`

On mount, detect Linux via `osType()` (from `@tauri-apps/plugin-os`, already used in `TitleBar.tsx`) and add a `linux` class to `<html>`:

```tsx
useEffect(() => {
  if (osType() === 'linux') {
    document.documentElement.classList.add('linux');
  }
}, []);
```

This gates all Linux CSS without touching other platforms.

### 3. `src/index.css`

Add a `html.linux`-scoped block after the existing `@layer base` block:

```css
/* Linux: transparent window with CSS drop shadow and rounded corners. */
html.linux body {
  background: transparent;
  padding: 8px;
}

html.linux #app {
  background: var(--background);
  border-radius: 8px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35), 0 0 0 1px rgba(0, 0, 0, 0.08);
  overflow: hidden;
  height: 100%;
}
```

The 8px `padding` on `body` creates a transparent ring. The shadow on `#app` falls into that ring and appears against the desktop. `border-radius: 8px` gives the window slightly rounded corners (the transparent corners show the desktop through). The `0 0 0 1px rgba(0,0,0,0.08)` inset ring adds a subtle border in light mode.

---

## Files Changed

| File | Change |
|------|--------|
| `src-tauri/tauri.linux.conf.json` | Add `"transparent": true` |
| `src/App.tsx` | Add `useEffect` to set `linux` class on `<html>` |
| `src/index.css` | Add `html.linux` shadow CSS block |

---

## Testing

```bash
yarn tsc --noEmit
yarn build
```

Manual check on Linux:
1. Run `yarn tauri dev` — window shows drop shadow and rounded corners.
2. Move window over a contrasting desktop background — shadow is visible.
3. Toggle light/dark mode — background color updates correctly (CSS variable reference).
4. Verify macOS and Windows behavior is unchanged.
