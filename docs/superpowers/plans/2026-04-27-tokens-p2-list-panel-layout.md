# VSCode 2026 Tokens — P2: List, Panel & Layout

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire List/Tree selection states, Panel surfaces, Sidebar background, and Title Bar to their VSCode 2026 design tokens.

**Architecture:** Add CSS vars to `globals.css` and Tailwind entries to `tailwind.config.js`, then update `tree.tsx`, `ConsolePanel.tsx`, `CollectionsSidebar.tsx`, and `TitleBar.tsx`. No logic changes — pure styling alignment.

**Tech Stack:** Tailwind CSS v4, CSS custom properties, shadcn/ui Tree primitive.

---

### Task 1: Add CSS vars for List, Panel, Sidebar, Title Bar

**Files:**
- Modify: `src/globals.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add CSS vars to `:root` in `globals.css`**

After the scrollbar vars in `:root`, add:

```css
    /* VSCode 2026 Light: list tokens */
    --list-active-selection-bg:   210 100% 40% / 0.1; /* list.activeSelectionBackground  #0069CC1A */
    --list-active-selection-fg:   0 0% 12.5%;          /* list.activeSelectionForeground  #202020 */
    --list-inactive-selection-bg: 0 0% 85.5%;          /* list.inactiveSelectionBackground #DADADA99 on white */
    --list-hover-bg:              0 0% 84.7% / 0.31;   /* list.hoverBackground            #DADADA4f */
    --list-focus-bg:              210 100% 40% / 0.1;  /* list.focusBackground            #0069CC1A */
    --list-highlight-fg:          210 100% 40%;        /* list.highlightForeground        #0069CC */
    --list-error-fg:              3 93% 35%;            /* list.errorForeground            #ad0707 */
    --list-warning-fg:            72 74% 22%;           /* list.warningForeground          #667309 */
    /* VSCode 2026 Light: panel tokens */
    --panel-bg:                   240 50% 99%;          /* panel.background                #FAFAFD */
    --panel-border:               210 7% 85%;           /* panel.border                    #F0F1F2 */
    --panel-title-active-border:  0 0% 0%;              /* panelTitle.activeBorder         #000000 */
    --panel-title-active-fg:      0 0% 12.5%;           /* panelTitle.activeForeground     #202020 */
    --panel-title-inactive-fg:    0 0% 37.6%;           /* panelTitle.inactiveForeground   #606060 */
    /* VSCode 2026 Light: sidebar tokens */
    --sidebar-bg:                 240 50% 99%;          /* sideBar.background              #FAFAFD */
    --sidebar-border:             210 7% 85%;           /* sideBar.border                  #F0F1F2 */
    /* VSCode 2026 Light: title bar tokens */
    --titlebar-bg:                240 50% 99%;          /* titleBar.activeBackground       #FAFAFD */
    --titlebar-fg:                0 0% 37.6%;           /* titleBar.activeForeground       #606060 */
    --titlebar-border:            210 7% 85%;           /* titleBar.border                 #F0F1F2 */
```

- [ ] **Step 2: Add CSS vars to `.dark` in `globals.css`**

After the scrollbar vars in `.dark`, add:

```css
    /* VSCode 2026 Dark: list tokens */
    --list-active-selection-bg:   199 53% 48% / 0.149; /* list.activeSelectionBackground  #3994BC26 */
    --list-active-selection-fg:   0 0% 93%;             /* list.activeSelectionForeground  #ededed */
    --list-inactive-selection-bg: 210 2% 18%;           /* list.inactiveSelectionBackground #2C2D2E */
    --list-hover-bg:              0 0% 100% / 0.051;    /* list.hoverBackground            #FFFFFF0D */
    --list-focus-bg:              199 53% 48% / 0.149;  /* list.focusBackground            #3994BC26 */
    --list-highlight-fg:          199 47% 54%;          /* list.highlightForeground        #48A0C7 */
    --list-error-fg:              9 85% 70%;             /* list.errorForeground            #f48771 */
    --list-warning-fg:            36 69% 69%;            /* list.warningForeground          #e5ba7d */
    /* VSCode 2026 Dark: panel tokens */
    --panel-bg:                   210 5% 10%;            /* panel.background                #191A1B */
    --panel-border:               210 2% 17%;            /* panel.border                    #2A2B2C */
    --panel-title-active-border:  199 53% 48%;           /* panelTitle.activeBorder         #3994BC */
    --panel-title-active-fg:      0 0% 75%;              /* panelTitle.activeForeground     #bfbfbf */
    --panel-title-inactive-fg:    0 0% 55%;              /* panelTitle.inactiveForeground   #8C8C8C */
    /* VSCode 2026 Dark: sidebar tokens */
    --sidebar-bg:                 210 5% 10%;            /* sideBar.background              #191A1B */
    --sidebar-border:             210 2% 17%;            /* sideBar.border                  #2A2B2C */
    /* VSCode 2026 Dark: title bar tokens */
    --titlebar-bg:                210 5% 10%;            /* titleBar.activeBackground       #191A1B */
    --titlebar-fg:                0 0% 55%;              /* titleBar.activeForeground       #8C8C8C */
    --titlebar-border:            210 2% 17%;            /* titleBar.border                 #2A2B2C */
```

- [ ] **Step 3: Register in `tailwind.config.js`**

After the `badge` group, add:

```js
  			list: {
  				'active-bg':   'hsl(var(--list-active-selection-bg))',
  				'active-fg':   'hsl(var(--list-active-selection-fg))',
  				'inactive-bg': 'hsl(var(--list-inactive-selection-bg))',
  				'hover-bg':    'hsl(var(--list-hover-bg))',
  				'focus-bg':    'hsl(var(--list-focus-bg))',
  				'highlight-fg':'hsl(var(--list-highlight-fg))',
  				'error-fg':    'hsl(var(--list-error-fg))',
  				'warning-fg':  'hsl(var(--list-warning-fg))',
  			},
  			panel: {
  				bg:                    'hsl(var(--panel-bg))',
  				border:                'hsl(var(--panel-border))',
  				'title-active-border': 'hsl(var(--panel-title-active-border))',
  				'title-active-fg':     'hsl(var(--panel-title-active-fg))',
  				'title-inactive-fg':   'hsl(var(--panel-title-inactive-fg))',
  			},
  			sidebar: {
  				bg:     'hsl(var(--sidebar-bg))',
  				border: 'hsl(var(--sidebar-border))',
  			},
  			titlebar: {
  				bg:     'hsl(var(--titlebar-bg))',
  				fg:     'hsl(var(--titlebar-fg))',
  				border: 'hsl(var(--titlebar-border))',
  			},
```

- [ ] **Step 4: Verify TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/globals.css tailwind.config.js
git commit -m "feat(theme): add list/panel/sidebar/titlebar CSS vars from VSCode 2026"
```

---

### Task 2: Update Tree (List) primitive

**Files:**
- Modify: `src/components/ui/tree.tsx`

- [ ] **Step 1: Update TreeItem classes**

In `src/components/ui/tree.tsx`, find the `TreeItem` `className` string (around line 114–118):

```tsx
            'flex items-center gap-1 px-1 py-1 text-sm cursor-pointer pl-(--tree-indent) w-full text-left bg-transparent border-l-2 border-transparent',
            'hover:bg-accent/50',
            'data-selected:bg-accent/30',
            'data-active:border-primary data-active:bg-accent/60',
```

Replace with:

```tsx
            'flex items-center gap-1 px-1 py-1 text-sm cursor-pointer pl-(--tree-indent) w-full text-left bg-transparent border-l-2 border-transparent',
            'hover:bg-list-hover-bg',
            'data-selected:bg-list-inactive-bg',
            'data-active:border-list-highlight-fg data-active:bg-list-active-bg data-active:text-list-active-fg',
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep tree.tsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/tree.tsx
git commit -m "feat(theme): wire tree list item states to VSCode 2026 list tokens"
```

---

### Task 3: Update Panel surfaces

**Files:**
- Modify: `src/components/layout/ConsolePanel.tsx`
- Modify: `src/components/git/GitPanel.tsx`

- [ ] **Step 1: Update ConsolePanel background and border**

In `src/components/layout/ConsolePanel.tsx`, find:

```tsx
    <div className='shrink-0 border-t border-border bg-card flex flex-col' style={{ height }}>
```

Replace with:

```tsx
    <div className='shrink-0 border-t border-panel-border bg-panel-bg flex flex-col' style={{ height }}>
```

- [ ] **Step 2: Update GitPanel border**

In `src/components/git/GitPanel.tsx`, find all occurrences of `border-border` used as panel dividers (not within list items). Replace structural panel border classes `border-border` → `border-panel-border` where they separate panel sections (check for `border-r border-border` and `border-b border-border` on container divs).

Run to find exact lines:
```bash
grep -n "border-border\|bg-card" src/components/git/GitPanel.tsx | head -20
```

Replace structural container borders from `border-border` to `border-panel-border` on the top-level divs only (not on list rows or file row items).

- [ ] **Step 3: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep -E "ConsolePanel|GitPanel"
```

Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/ConsolePanel.tsx src/components/git/GitPanel.tsx
git commit -m "feat(theme): wire panel surfaces to VSCode 2026 panel tokens"
```

---

### Task 4: Update Sidebar and Title Bar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`
- Modify: `src/components/title-bar/TitleBar.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Update CollectionsSidebar background**

In `src/components/layout/CollectionsSidebar.tsx`, find:

```tsx
    <div className='h-full flex flex-col bg-background' role='navigation' aria-label='Collections'>
```

Replace with:

```tsx
    <div className='h-full flex flex-col bg-sidebar-bg' role='navigation' aria-label='Collections'>
```

- [ ] **Step 2: Update sidebar separator in App.tsx**

In `src/App.tsx`, the resizable divider uses `bg-border`. Update to use `bg-sidebar-border` since it separates the sidebar from the content area:

Find:
```tsx
              <div className='w-px h-full bg-border transition-colors group-hover:bg-primary/60' />
```

Replace with:
```tsx
              <div className='w-px h-full bg-sidebar-border transition-colors group-hover:bg-primary/60' />
```

- [ ] **Step 3: Update TitleBar**

In `src/components/title-bar/TitleBar.tsx`, find:

```tsx
      className='relative flex h-10 w-full items-center shrink-0 border-b border-border bg-background'
```

Replace with:

```tsx
      className='relative flex h-10 w-full items-center shrink-0 border-b border-titlebar-border bg-titlebar-bg text-titlebar-fg'
```

- [ ] **Step 4: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep -E "CollectionsSidebar|TitleBar|App.tsx"
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx src/components/title-bar/TitleBar.tsx src/App.tsx
git commit -m "feat(theme): wire sidebar and title bar to VSCode 2026 tokens"
```
