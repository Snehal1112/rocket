# VSCode 2026 Theme — Plan 02: Scaffold `apps/theme-playground`

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `apps/theme-playground` — a standalone Vite + React 18 + TypeScript browser app that shares the main frontend's shadcn/ui components and `globals.css`, starts in under 1 second, and has a working light/dark toggle.

**Architecture:** Vite with a path alias `@/ui` → `../../frontend/src/components/ui`. The playground imports `frontend/src/globals.css` directly so there is a single source of truth. No Tauri, no Rust, no build-time Tauri IPC stubs needed.

**Tech Stack:** Vite 5, React 18, TypeScript 5, Tailwind CSS (shared config), shadcn/ui (shared components via alias)

**Spec:** `docs/superpowers/specs/2026-04-26-vscode2026-theme-design.md`
**Branch:** `feat/vscode-2026-theme` — **continue in the worktree created in Plan 01**
**Prerequisite:** Plan 01 complete (globals.css updated with 2026 tokens)

---

## Chunk 0: Enter the existing worktree

- [ ] **Step 1: Switch into the worktree created in Plan 01**

```bash
cd .worktrees/vscode-2026-theme
git branch --show-current
# Expected: feat/vscode-2026-theme
```

All tasks in this plan run from inside `.worktrees/vscode-2026-theme/`.

---

## Task 1: Create the playground package scaffold

**Files:**
- Create: `apps/theme-playground/package.json`
- Create: `apps/theme-playground/vite.config.ts`
- Create: `apps/theme-playground/tsconfig.json`
- Create: `apps/theme-playground/index.html`

- [ ] **Step 1: Create the directory**

```bash
mkdir -p apps/theme-playground/src/sections
```

- [ ] **Step 2: Create `apps/theme-playground/package.json`**

```json
{
  "name": "@rocket/theme-playground",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "lucide-react": "^0.469.0",
    "class-variance-authority": "^0.7.1",
    "clsx": "^2.1.1",
    "tailwind-merge": "^2.6.0",
    "@radix-ui/react-checkbox": "^1.1.3",
    "@radix-ui/react-collapsible": "^1.1.2",
    "@radix-ui/react-dialog": "^1.1.4",
    "@radix-ui/react-dropdown-menu": "^2.1.4",
    "@radix-ui/react-label": "^2.1.1",
    "@radix-ui/react-progress": "^1.1.1",
    "@radix-ui/react-radio-group": "^1.2.2",
    "@radix-ui/react-select": "^2.1.4",
    "@radix-ui/react-separator": "^1.1.1",
    "@radix-ui/react-slot": "^1.1.1",
    "@radix-ui/react-switch": "^1.1.2",
    "@radix-ui/react-tabs": "^1.1.2",
    "@radix-ui/react-tooltip": "^1.1.6"
  },
  "devDependencies": {
    "@types/react": "^18.3.18",
    "@types/react-dom": "^18.3.5",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.3",
    "vite": "^6.0.11",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0"
  }
}
```

- [ ] **Step 3: Create `apps/theme-playground/vite.config.ts`**

```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { resolve } from 'path';

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      // Share the exact same shadcn/ui components from the main frontend
      '@/ui': resolve(__dirname, '../../frontend/src/components/ui'),
      // Share cn utility
      '@/lib': resolve(__dirname, '../../frontend/src/lib'),
    },
  },
  server: {
    port: 5174,
    strictPort: false,
  },
});
```

- [ ] **Step 4: Create `apps/theme-playground/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": false,
    "noUnusedParameters": false,
    "paths": {
      "@/ui/*": ["../../frontend/src/components/ui/*"],
      "@/lib/*": ["../../frontend/src/lib/*"]
    }
  },
  "include": ["src"]
}
```

- [ ] **Step 5: Create `apps/theme-playground/index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>RocketAPI Design System</title>
    <script>
      // Apply dark class synchronously to prevent flash
      const stored = localStorage.getItem('rocket-theme');
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      if (stored === 'dark' || (!stored && prefersDark)) {
        document.documentElement.classList.add('dark');
      }
    </script>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Install dependencies**

```bash
cd apps/theme-playground
yarn install
```

Expected: installs without errors.

- [ ] **Step 7: Commit scaffold**

```bash
cd ../..
git add apps/theme-playground/
git commit -m "feat(playground): scaffold theme-playground Vite + React app

Shares frontend/src/components/ui via @/ui path alias.
Shares frontend/src/globals.css for single-source theming.
Runs at localhost:5174 independent of Tauri."
```

---

## Task 2: Create entry point files and theme infrastructure

**Files:**
- Create: `apps/theme-playground/src/main.tsx`
- Create: `apps/theme-playground/src/globals.css`
- Create: `apps/theme-playground/src/ThemeToggle.tsx`

- [ ] **Step 1: Create `apps/theme-playground/src/globals.css`**

This file re-imports the main frontend's `globals.css` so all shadcn CSS variables and font imports are available. It also adds playground-specific layout:

```css
/* Import the canonical theme — single source of truth */
@import "../../../frontend/src/globals.css";

/* Playground-specific layout utilities */
html, body, #root {
  height: 100%;
}

body {
  font-family: var(--font-sans, -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif);
}
```

- [ ] **Step 2: Create `apps/theme-playground/src/main.tsx`**

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import './globals.css';
import { App } from './App';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 3: Create `apps/theme-playground/src/ThemeToggle.tsx`**

```tsx
import { useEffect, useState } from 'react';
import { Sun, Moon } from 'lucide-react';

export function ThemeToggle() {
  const [isDark, setIsDark] = useState(
    () => document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    if (isDark) {
      document.documentElement.classList.add('dark');
      localStorage.setItem('rocket-theme', 'dark');
    } else {
      document.documentElement.classList.remove('dark');
      localStorage.setItem('rocket-theme', 'light');
    }
  }, [isDark]);

  return (
    <button
      onClick={() => setIsDark((d) => !d)}
      className="inline-flex items-center gap-2 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
      aria-label="Toggle theme"
    >
      {isDark ? (
        <>
          <Sun className="h-3.5 w-3.5" />
          Light
        </>
      ) : (
        <>
          <Moon className="h-3.5 w-3.5" />
          Dark
        </>
      )}
    </button>
  );
}
```

- [ ] **Step 4: Verify Vite starts with these entry files**

```bash
cd apps/theme-playground
yarn dev
```

Expected: Vite starts at `http://localhost:5174`. The page may show a blank div or an error about missing `App` — that is fine at this stage. The build must not crash.

Kill the dev server with Ctrl+C.

- [ ] **Step 5: Commit**

```bash
cd ../..
git add apps/theme-playground/src/
git commit -m "feat(playground): add entry point, globals.css import, ThemeToggle

globals.css re-imports frontend/src/globals.css as single source of truth.
ThemeToggle persists preference to localStorage and toggles .dark class."
```

---

## Task 3: Create `App.tsx` — sidebar navigation shell

**Files:**
- Create: `apps/theme-playground/src/App.tsx`

- [ ] **Step 1: Create `apps/theme-playground/src/App.tsx`**

This is the navigation shell. Sections are lazy-loaded. The sidebar is styled with shadcn/ui CSS variables directly (Tailwind classes).

```tsx
import { useState } from 'react';
import { ThemeToggle } from './ThemeToggle';

// Section registry — add new sections here as they are created
const SECTIONS = [
  { id: 'buttons',      label: 'Button',                group: 'Primitives' },
  { id: 'inputs',       label: 'Input / Label / Select', group: 'Primitives' },
  { id: 'checkbox',     label: 'Checkbox / Switch',      group: 'Primitives' },
  { id: 'badges',       label: 'Badge',                  group: 'Primitives' },
  { id: 'tabs',         label: 'Tabs',                   group: 'Primitives' },
  { id: 'table',        label: 'Table',                  group: 'Primitives' },
  { id: 'cards',        label: 'Card / Dialog',          group: 'Overlays' },
  { id: 'alerts',       label: 'Alert',                  group: 'Overlays' },
  { id: 'dropdown',     label: 'DropdownMenu',           group: 'Overlays' },
  { id: 'collapsible',  label: 'Collapsible',            group: 'Overlays' },
  { id: 'progress',     label: 'Progress',               group: 'Overlays' },
  { id: 'separator',    label: 'Separator',              group: 'Overlays' },
  { id: 'requestbar',   label: 'Request Bar',            group: 'RocketAPI Patterns' },
  { id: 'kveditor',     label: 'Key-Value Editor',       group: 'RocketAPI Patterns' },
  { id: 'gitstatus',    label: 'Git Status',             group: 'RocketAPI Patterns' },
  { id: 'envswitcher',  label: 'Environment Switcher',   group: 'RocketAPI Patterns' },
] as const;

type SectionId = typeof SECTIONS[number]['id'];

const groups = Array.from(new Set(SECTIONS.map((s) => s.group)));

export function App() {
  const [active, setActive] = useState<SectionId>('buttons');

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      {/* Sidebar */}
      <aside className="w-52 shrink-0 border-r border-border bg-card overflow-y-auto">
        <div className="sticky top-0 bg-card border-b border-border p-4 z-10">
          <div className="flex items-center gap-2 mb-1">
            <div className="w-6 h-6 rounded bg-primary flex items-center justify-center text-primary-foreground text-xs font-bold">R</div>
            <span className="text-sm font-semibold text-foreground">Design System</span>
          </div>
          <p className="text-[10px] text-muted-foreground">VSCode 2026 × shadcn/ui</p>
        </div>
        <nav className="p-2">
          {groups.map((group) => (
            <div key={group} className="mb-4">
              <p className="px-2 py-1 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                {group}
              </p>
              {SECTIONS.filter((s) => s.group === group).map((section) => (
                <button
                  key={section.id}
                  onClick={() => setActive(section.id)}
                  className={[
                    'w-full text-left px-3 py-1.5 rounded-md text-xs transition-colors',
                    active === section.id
                      ? 'bg-accent text-primary font-medium'
                      : 'text-muted-foreground hover:bg-muted hover:text-foreground',
                  ].join(' ')}
                >
                  {section.label}
                </button>
              ))}
            </div>
          ))}
        </nav>
      </aside>

      {/* Main */}
      <div className="flex flex-col flex-1 overflow-hidden">
        {/* Header */}
        <header className="shrink-0 border-b border-border bg-card px-6 py-3 flex items-center justify-between">
          <div>
            <h1 className="text-sm font-semibold text-foreground">
              {SECTIONS.find((s) => s.id === active)?.label}
            </h1>
            <p className="text-[11px] text-muted-foreground">
              {SECTIONS.find((s) => s.id === active)?.group}
            </p>
          </div>
          <ThemeToggle />
        </header>

        {/* Content */}
        <main className="flex-1 overflow-y-auto p-8">
          <SectionRenderer id={active} />
        </main>
      </div>
    </div>
  );
}

function SectionRenderer({ id }: { id: SectionId }) {
  return (
    <div className="rounded-lg border border-dashed border-border p-8 text-center text-muted-foreground text-sm">
      <p className="text-base font-medium text-foreground mb-1">{id}</p>
      <p>Component showcase coming in Plan 03.</p>
    </div>
  );
}
```

- [ ] **Step 2: Start dev server and verify navigation works**

```bash
cd apps/theme-playground
yarn dev
```

Open `http://localhost:5174`. Expected:
- Sidebar renders with all section names grouped correctly
- Clicking a sidebar item updates the main header title
- ThemeToggle button switches between light and dark — all colors update
- No console errors

Kill the dev server.

- [ ] **Step 3: Commit**

```bash
cd ../..
git add apps/theme-playground/src/App.tsx
git commit -m "feat(playground): add App shell with sidebar navigation and section router

All 16 sections registered. SectionRenderer shows placeholder until Plan 03.
ThemeToggle wired — light/dark works end-to-end."
```
