# Linux Window Drop Shadow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a CSS drop shadow and rounded corners to the Rocket app window on Linux by enabling window transparency and applying a scoped CSS shadow rule.

**Architecture:** Three files change. `tauri.linux.conf.json` gains `transparent: true` so the webview background is clear. `App.tsx` detects Linux at runtime via `osType()` and adds a `linux` class to `<html>`. `index.css` uses that class to paint `#app` with a background, shadow, and border-radius, and gives `body` 8px transparent padding where the shadow falls.

**Tech Stack:** Tauri 2 (window config), React 19 (useEffect), `@tauri-apps/plugin-os` (osType), TailwindCSS 4.2 / plain CSS.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/tauri.linux.conf.json` | Modify | Enable transparent window on Linux |
| `src/index.css` | Modify | CSS shadow + rounded corners scoped to `html.linux` |
| `src/App.tsx` | Modify | Detect Linux and add `linux` class to `<html>` |

---

## Task 1: Enable transparent window and add CSS shadow

**Files:**
- Modify: `src-tauri/tauri.linux.conf.json`
- Modify: `src/index.css`

- [ ] **Step 1: Add `transparent: true` to the Linux Tauri config**

Open `src-tauri/tauri.linux.conf.json`. The current content is:

```json
{
  "app": {
    "windows": [
      {
        "decorations": false
      }
    ]
  }
}
```

Replace with:

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

- [ ] **Step 2: Add the Linux-scoped CSS shadow block to `src/index.css`**

Open `src/index.css`. After the closing `}` of the `@layer base { ... }` block (currently the last block in the file, ending around line 180), append:

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

How this works:
- `body` gets `padding: 8px` — creates a transparent ring around the app content (the ring shows the desktop, which is where the shadow appears).
- `background: transparent` on body ensures the ring is genuinely clear, not painted.
- `#app` paints the real background color, draws the shadow into the transparent padding, and clips content with `overflow: hidden` + `border-radius: 8px` for rounded corners.
- `height: 100%` keeps `#app` filling the body content box (body is `100vh` minus 16px of padding; `#app` fills the remaining 100%).

- [ ] **Step 3: TypeScript check**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: no errors (these are config/CSS changes — tsc should be clean).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.linux.conf.json src/index.css
git commit -m "feat(linux): enable transparent window and CSS drop shadow"
```

---

## Task 2: Detect Linux and apply the CSS class

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add the `osType` import**

Open `src/App.tsx`. The current import block starts at line 1. Add the `osType` import as the first line (before existing imports):

```tsx
import { type as osType } from '@tauri-apps/plugin-os';
```

The full import block should then begin:

```tsx
import { type as osType } from '@tauri-apps/plugin-os';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { TitleBar } from '@/components/title-bar';
// ... rest of existing imports unchanged
```

- [ ] **Step 2: Add the OS detection effect**

Inside the `App` function, after the existing `useEffect` that subscribes to `usePaneStore` (the one at line 48–51), add:

```tsx
useEffect(() => {
  if (osType() === 'linux') {
    document.documentElement.classList.add('linux');
  }
}, []);
```

The effect runs once on mount. `osType()` is synchronous and returns `'linux'` | `'macos'` | `'windows'` | `'android'` | `'ios'`. On non-Linux platforms the class is never added and all `html.linux` CSS rules remain inactive.

- [ ] **Step 3: TypeScript check**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Build check**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn build
```

Expected: successful build with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "feat(linux): add OS detection to apply window shadow CSS class"
```

---

## Manual Smoke Test (Linux only)

Run `yarn tauri dev` and verify:

1. The window has a visible drop shadow against the desktop background.
2. The window has slightly rounded corners (visible when dragged over a contrasting wallpaper).
3. Toggle light/dark mode — the window background color (`var(--background)`) updates correctly.
4. The 8px transparent border around the window is invisible to the user (appears as the shadow fading out).
5. Window dragging, resizing, minimize/maximize/close all work normally.
