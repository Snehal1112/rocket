# Custom Title Bar Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the native OS title bar with a custom title bar showing logo + app name left, workspace switcher centered, and platform-appropriate window controls.

**Architecture:** The title bar is a standalone React component tree rendered above the main app layout. Tauri decorations are disabled per-platform (overlay on macOS, fully custom on Windows/Linux). The workspace switcher reads from the existing global store and uses shadcn DropdownMenu.

**Tech Stack:** React, TypeScript, Tauri v2, `@tauri-apps/api/window`, `@tauri-apps/plugin-os`, shadcn/ui (DropdownMenu, Button)

---

## Chunk 1: Tauri configuration

### Task 1: Disable native title bar via Tauri config

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/tauri.macos.conf.json`
- Create: `src-tauri/tauri.windows.conf.json`
- Create: `src-tauri/tauri.linux.conf.json`

- [ ] **Step 1: Update base `tauri.conf.json`**

Remove or leave `decorations` unset in the base config — platform overrides will handle it. Add `hiddenTitle` for macOS. The base `app.windows[0]` block should look like:

```json
{
  "title": "Rocket",
  "width": 1440,
  "height": 900
}
```

- [ ] **Step 2: Create `src-tauri/tauri.macos.conf.json`**

```json
{
  "app": {
    "windows": [
      {
        "titleBarStyle": "Overlay",
        "hiddenTitle": true
      }
    ]
  }
}
```

This preserves native traffic lights while letting custom HTML fill the title bar area.

- [ ] **Step 3: Create `src-tauri/tauri.windows.conf.json`**

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

- [ ] **Step 4: Create `src-tauri/tauri.linux.conf.json`**

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

- [ ] **Step 5: Verify app still launches**

```bash
yarn tauri dev
```

Expected: App opens. Native title bar is gone (or replaced by traffic lights on macOS). Window may not be draggable yet — that's fine, drag region comes in Task 3.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/tauri.macos.conf.json src-tauri/tauri.windows.conf.json src-tauri/tauri.linux.conf.json
git commit -m "feat(titlebar): disable native decorations via platform tauri configs"
```

---

## Chunk 2: WindowControls component (Windows/Linux)

### Task 2: Build `WindowControls` component

**Files:**
- Create: `src/components/title-bar/WindowControls.tsx`

The `WindowControls` component renders minimize / maximize / close buttons. It is only mounted on Windows and Linux — macOS uses native traffic lights.

- [ ] **Step 1: Install `@tauri-apps/api` if not already present**

```bash
yarn add @tauri-apps/api
```

Check `package.json` first — skip if already listed.

- [ ] **Step 2: Create `src/components/title-bar/WindowControls.tsx`**

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Button } from '@/components/ui/button'

export function WindowControls() {
  const win = getCurrentWindow()

  return (
    <div className="flex items-center">
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.minimize()}
        aria-label="Minimize"
      >
        <span className="text-xs">─</span>
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.toggleMaximize()}
        aria-label="Maximize"
      >
        <span className="text-xs">▢</span>
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"
        onClick={() => win.close()}
        aria-label="Close"
      >
        <span className="text-xs">✕</span>
      </Button>
    </div>
  )
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/title-bar/WindowControls.tsx
git commit -m "feat(titlebar): add WindowControls component for Windows/Linux"
```

---

## Chunk 3: WorkspaceSwitcher component

### Task 3: Build `WorkspaceSwitcher` component

**Files:**
- Create: `src/components/title-bar/WorkspaceSwitcher.tsx`

The switcher reads workspace state from the existing global store. Check the store file (likely `src/store/` or `src/state/`) and adapt the import paths below to match.

- [ ] **Step 1: Locate the existing store**

```bash
find src -name "*.ts" -o -name "*.tsx" | xargs grep -l "workspace" | head -10
```

Note the file path and the exported hook/selector names for workspace list and active workspace.

- [ ] **Step 2: Create `src/components/title-bar/WorkspaceSwitcher.tsx`**

Replace `useWorkspaceStore` and its selectors with whatever your store exposes.

```tsx
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { ChevronDown } from 'lucide-react'
import { useWorkspaceStore } from '@/store/workspace' // adjust path

export function WorkspaceSwitcher() {
  const workspaces = useWorkspaceStore((s) => s.workspaces)
  const activeId = useWorkspaceStore((s) => s.activeWorkspaceId)
  const setActive = useWorkspaceStore((s) => s.setActiveWorkspace)

  const activeWorkspace = workspaces.find((w) => w.id === activeId)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="gap-1.5 font-medium">
          {activeWorkspace?.name ?? 'Select workspace'}
          <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center" className="min-w-[180px]">
        {workspaces.map((ws) => (
          <DropdownMenuItem
            key={ws.id}
            onSelect={() => setActive(ws.id)}
            className={ws.id === activeId ? 'bg-accent text-accent-foreground' : ''}
          >
            {ws.name}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => {/* open add workspace flow */}}>
          + Add workspace
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
```

- [ ] **Step 3: Verify shadcn DropdownMenu is installed**

```bash
grep -r "DropdownMenu" src/components/ui/ | head -3
```

If missing, install it:

```bash
yarn dlx shadcn@latest add dropdown-menu
```

- [ ] **Step 4: Commit**

```bash
git add src/components/title-bar/WorkspaceSwitcher.tsx
git commit -m "feat(titlebar): add WorkspaceSwitcher component with dropdown"
```

---

## Chunk 4: TitleBar component

### Task 4: Build the `TitleBar` component

**Files:**
- Create: `src/components/title-bar/TitleBar.tsx`
- Create: `src/components/title-bar/index.ts`

- [ ] **Step 1: Install `@tauri-apps/plugin-os` if not already present**

```bash
yarn add @tauri-apps/plugin-os
```

Also add to `src-tauri/Cargo.toml` if not present:
```toml
tauri-plugin-os = "2"
```

And register in `src-tauri/src/lib.rs`:
```rust
.plugin(tauri_plugin_os::init())
```

- [ ] **Step 2: Create `src/components/title-bar/TitleBar.tsx`**

```tsx
import { useEffect, useState } from 'react'
import { type } from '@tauri-apps/plugin-os'
import { WorkspaceSwitcher } from './WorkspaceSwitcher'
import { WindowControls } from './WindowControls'

export function TitleBar() {
  const [platform, setPlatform] = useState<string>('linux')

  useEffect(() => {
    type().then(setPlatform)
  }, [])

  const isMac = platform === 'macos'

  return (
    <div
      className="flex h-11 w-full items-center shrink-0 border-b bg-background"
      data-tauri-drag-region
    >
      {/* macOS: traffic lights are native, leave space on the left */}
      {isMac && <div className="w-[72px] shrink-0" data-tauri-drag-region />}

      {/* Logo + App name */}
      <div className="flex items-center gap-2 px-3 shrink-0">
        <img src="/icons/32x32.png" alt="Rocket" className="h-4 w-4" />
        <span className="text-sm font-medium">Rocket</span>
      </div>

      {/* Left spacer — drag region */}
      <div className="flex-1" data-tauri-drag-region />

      {/* Workspace switcher — centered */}
      <WorkspaceSwitcher />

      {/* Right spacer — drag region */}
      <div className="flex-1" data-tauri-drag-region />

      {/* Window controls — Windows/Linux only */}
      {!isMac && <WindowControls />}
    </div>
  )
}
```

- [ ] **Step 3: Create `src/components/title-bar/index.ts`**

```ts
export { TitleBar } from './TitleBar'
```

- [ ] **Step 4: Commit**

```bash
git add src/components/title-bar/TitleBar.tsx src/components/title-bar/index.ts
git commit -m "feat(titlebar): add TitleBar component with platform detection"
```

---

## Chunk 5: Wire TitleBar into the app

### Task 5: Mount TitleBar at the top of the app layout

**Files:**
- Modify: `src/App.tsx` (or the root layout file — check your router setup)

- [ ] **Step 1: Find the root layout**

```bash
grep -r "return (" src/App.tsx src/main.tsx src/layout/ 2>/dev/null | head -20
```

Identify the outermost layout component that wraps all routes.

- [ ] **Step 2: Import and mount `TitleBar`**

In the root layout file, add `TitleBar` as the first child:

```tsx
import { TitleBar } from '@/components/title-bar'

// Inside the root layout return:
return (
  <div className="flex h-screen flex-col overflow-hidden">
    <TitleBar />
    <div className="flex flex-1 overflow-hidden">
      {/* existing sidebar + main content */}
    </div>
  </div>
)
```

The outer div must be `flex-col` with `h-screen` so the title bar takes its natural height and the rest fills remaining space.

- [ ] **Step 3: Run the app and verify**

```bash
yarn tauri dev
```

Expected:
- macOS: native traffic lights visible, logo + "Rocket" + workspace switcher in bar, window draggable
- Windows/Linux: fully custom bar with win32-style controls on the right
- No double title bar (native should be gone)
- Workspace dropdown opens and lists workspaces

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx  # or whichever file you modified
git commit -m "feat(titlebar): mount TitleBar in root layout"
```

---

## Chunk 6: Polish & edge cases

### Task 6: Handle maximized state on Windows

On Windows, when the window is maximized the restore button should show a "restore down" icon instead of the maximize icon.

**Files:**
- Modify: `src/components/title-bar/WindowControls.tsx`

- [ ] **Step 1: Update `WindowControls.tsx` to track maximized state**

```tsx
import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Button } from '@/components/ui/button'

export function WindowControls() {
  const win = getCurrentWindow()
  const [isMaximized, setIsMaximized] = useState(false)

  useEffect(() => {
    win.isMaximized().then(setIsMaximized)
    const unlisten = win.onResized(() => {
      win.isMaximized().then(setIsMaximized)
    })
    return () => { unlisten.then((fn) => fn()) }
  }, [win])

  return (
    <div className="flex items-center">
      <Button
        variant="ghost" size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.minimize()}
        aria-label="Minimize"
      >
        <span className="text-xs">─</span>
      </Button>
      <Button
        variant="ghost" size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.toggleMaximize()}
        aria-label={isMaximized ? 'Restore' : 'Maximize'}
      >
        <span className="text-xs">{isMaximized ? '❐' : '▢'}</span>
      </Button>
      <Button
        variant="ghost" size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"
        onClick={() => win.close()}
        aria-label="Close"
      >
        <span className="text-xs">✕</span>
      </Button>
    </div>
  )
}
```

- [ ] **Step 2: Test maximize/restore cycle**

Run `yarn tauri dev` on Windows or Linux. Maximize the window — button should switch to restore icon. Click restore — should switch back.

- [ ] **Step 3: Commit**

```bash
git add src/components/title-bar/WindowControls.tsx
git commit -m "feat(titlebar): track maximized state in WindowControls"
```

---

## Done

All tasks complete. The custom title bar is fully wired:
- Platform-aware (macOS overlay / Windows+Linux fully custom)
- Draggable via `data-tauri-drag-region`
- Workspace switcher reads from global store
- Window controls handle maximize/restore state
- All interactive elements use shadcn/ui primitives
