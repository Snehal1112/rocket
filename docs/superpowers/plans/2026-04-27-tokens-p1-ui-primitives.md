# VSCode 2026 Tokens — P1: UI Primitives

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Button, Checkbox, Dropdown/Select, Badge, and Scrollbar to their VSCode 2026 design tokens.

**Architecture:** Add CSS vars to `globals.css` and Tailwind groups to `tailwind.config.js` first, then update each shadcn primitive to use the new token classes. No logic changes — pure styling alignment.

**Tech Stack:** Tailwind CSS v4, CSS custom properties, shadcn/ui primitives.

---

### Task 1: Add CSS vars for Button, Checkbox, Dropdown, Badge, Scrollbar

**Files:**
- Modify: `src/globals.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add CSS vars to `:root` in `globals.css`**

After the `--breadcrumb-focus-fg` light line, add:

```css
    /* VSCode 2026 Light: button tokens */
    --button-hover-bg:              210 100% 38%;  /* button.hoverBackground  #0063C1 */
    --button-border:                240 100% 96%;  /* button.border           #EEEEF1 */
    --button-secondary-bg:          0 0% 91.7%;    /* button.secondaryBackground #EAEAEA */
    --button-secondary-hover-bg:    210 33% 95%;   /* button.secondaryHoverBackground #F2F3F4 */
    /* VSCode 2026 Light: checkbox tokens */
    --checkbox-bg:                  0 0% 91.7%;    /* checkbox.background     #EAEAEA */
    --checkbox-border:              0 0% 84.7%;    /* checkbox.border         #D8D8D8 */
    --checkbox-fg:                  0 0% 37.6%;    /* checkbox.foreground     #606060 */
    /* VSCode 2026 Light: dropdown tokens */
    --dropdown-bg:                  0 0% 100%;     /* dropdown.background     #FFFFFF */
    --dropdown-border:              0 0% 84.7%;    /* dropdown.border         #D8D8D8 */
    --dropdown-fg:                  0 0% 12.5%;    /* dropdown.foreground     #202020 */
    /* VSCode 2026 Light: badge tokens */
    --badge-bg:                     210 100% 40%;  /* badge.background        #0069CC */
    --badge-fg:                     0 0% 100%;     /* badge.foreground        #FFFFFF */
    /* VSCode 2026 Light: scrollbar tokens */
    --scrollbar-thumb:    rgba(153, 153, 153, 0.149); /* scrollbarSlider.background    #99999926 */
    --scrollbar-thumb-hover: rgba(153, 153, 153, 0.251); /* scrollbarSlider.hoverBackground #99999940 */
    --scrollbar-thumb-active: rgba(153, 153, 153, 0.333); /* scrollbarSlider.activeBackground #99999955 */
```

- [ ] **Step 2: Add CSS vars to `.dark` in `globals.css`**

After the `--breadcrumb-focus-fg` dark line, add:

```css
    /* VSCode 2026 Dark: button tokens */
    --button-hover-bg:              200 57% 40%;   /* button.hoverBackground  #2B7DA3 */
    --button-border:                210 2% 21%;    /* button.border           #333536 */
    --button-secondary-bg:          210 3% 13%;    /* button.secondaryBackground (same as secondary) */
    --button-secondary-hover-bg:    0 0% 100%;     /* button.secondaryHoverBackground #FFFFFF10 */
    /* VSCode 2026 Dark: checkbox tokens */
    --checkbox-bg:                  210 3% 14.5%;  /* checkbox.background     #242526 */
    --checkbox-border:              210 2% 21%;    /* checkbox.border         #333536 */
    --checkbox-fg:                  0 0% 55%;      /* checkbox.foreground     #8C8C8C */
    /* VSCode 2026 Dark: dropdown tokens */
    --dropdown-bg:                  210 5% 10%;    /* dropdown.background     #191A1B */
    --dropdown-border:              210 2% 21%;    /* dropdown.border         #333536 */
    --dropdown-fg:                  0 0% 75%;      /* dropdown.foreground     #bfbfbf */
    /* VSCode 2026 Dark: badge tokens */
    --badge-bg:                     199 53% 48%;   /* badge.background        #3994BC */
    --badge-fg:                     0 0% 100%;     /* badge.foreground        #FFFFFF */
    /* VSCode 2026 Dark: scrollbar tokens */
    --scrollbar-thumb:    rgba(131, 132, 133, 0.200); /* scrollbarSlider.background    #83848533 */
    --scrollbar-thumb-hover: rgba(131, 132, 133, 0.400); /* scrollbarSlider.hoverBackground #83848566 */
    --scrollbar-thumb-active: rgba(131, 132, 133, 0.600); /* scrollbarSlider.activeBackground #83848599 */
```

- [ ] **Step 3: Register in `tailwind.config.js`**

After the `breadcrumb` group, add:

```js
  			'button-hover-bg':        'hsl(var(--button-hover-bg))',
  			'button-border':          'hsl(var(--button-border))',
  			'button-secondary-bg':    'hsl(var(--button-secondary-bg))',
  			'button-secondary-hover': 'hsl(var(--button-secondary-hover-bg))',
  			checkbox: {
  				bg:     'hsl(var(--checkbox-bg))',
  				border: 'hsl(var(--checkbox-border))',
  				fg:     'hsl(var(--checkbox-fg))',
  			},
  			dropdown: {
  				bg:     'hsl(var(--dropdown-bg))',
  				border: 'hsl(var(--dropdown-border))',
  				fg:     'hsl(var(--dropdown-fg))',
  			},
  			badge: {
  				bg: 'hsl(var(--badge-bg))',
  				fg: 'hsl(var(--badge-fg))',
  			},
```

- [ ] **Step 4: Update scrollbar CSS in `globals.css`**

Find the existing scrollbar section:

```css
* {
  scrollbar-width: thin;
  scrollbar-color: hsl(var(--border)) transparent;
}

*:hover {
  scrollbar-color: hsl(var(--muted-foreground) / 0.4) transparent;
}
```

Replace with:

```css
* {
  scrollbar-width: thin;
  scrollbar-color: var(--scrollbar-thumb) transparent;
}

*:hover {
  scrollbar-color: var(--scrollbar-thumb-hover) transparent;
}
```

Find:

```css
::-webkit-scrollbar-thumb {
  background-color: hsl(var(--border));
  border-radius: 3px;
}

::-webkit-scrollbar-thumb:hover {
  background-color: hsl(var(--muted-foreground) / 0.5);
}
```

Replace with:

```css
::-webkit-scrollbar-thumb {
  background-color: var(--scrollbar-thumb);
  border-radius: 3px;
}

::-webkit-scrollbar-thumb:hover {
  background-color: var(--scrollbar-thumb-hover);
}

::-webkit-scrollbar-thumb:active {
  background-color: var(--scrollbar-thumb-active);
}
```

- [ ] **Step 5: Verify TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add src/globals.css tailwind.config.js
git commit -m "feat(theme): add button/checkbox/dropdown/badge/scrollbar CSS vars from VSCode 2026"
```

---

### Task 2: Update Button primitive

**Files:**
- Modify: `src/components/ui/button.tsx`

- [ ] **Step 1: Update button variants**

Replace the full `buttonVariants` cva call:

```ts
const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground shadow-xs hover:bg-button-hover-bg border border-button-border',
        destructive:
          'bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60',
        outline:
          'border border-input bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50',
        secondary: 'bg-button-secondary-bg text-secondary-foreground shadow-xs hover:bg-button-secondary-hover',
        ghost: 'hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-9 px-4 py-2 has-[>svg]:px-3',
        sm: 'h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5',
        lg: 'h-10 rounded-md px-6 has-[>svg]:px-4',
        icon: 'size-9',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
);
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep button.tsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/button.tsx
git commit -m "feat(theme): wire button variants to VSCode 2026 tokens"
```

---

### Task 3: Update Checkbox primitive

**Files:**
- Modify: `src/components/ui/checkbox.tsx`

- [ ] **Step 1: Update checkbox classes**

Find the `className={cn(` call and replace its content:

```tsx
      className={cn(
        'peer h-4 w-4 shrink-0 rounded-sm border border-checkbox-border bg-checkbox-bg shadow focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground',
        className,
      )}
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep checkbox.tsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/checkbox.tsx
git commit -m "feat(theme): wire checkbox to VSCode 2026 tokens"
```

---

### Task 4: Update Dropdown Menu and Select primitives

**Files:**
- Modify: `src/components/ui/dropdown-menu.tsx`
- Modify: `src/components/ui/select.tsx`

- [ ] **Step 1: Update dropdown-menu content background and border**

In `src/components/ui/dropdown-menu.tsx`, find all occurrences of:
```
bg-card/50 backdrop-blur-sm
```
And the border:
```
border border-border
```

In the `DropdownMenuContent` and `DropdownMenuSubContent` className, replace:
```
border border-border bg-card/50 backdrop-blur-sm
```
with:
```
border border-dropdown-border bg-dropdown-bg text-dropdown-fg backdrop-blur-sm
```

Also update the `DropdownMenuItem` focus state — find:
```
focus:bg-accent
```
Replace with:
```
focus:bg-accent focus:text-accent-foreground
```

- [ ] **Step 2: Update select content background and border**

In `src/components/ui/select.tsx`, in `SelectContent` className, replace:
```
border border-border bg-card/50 backdrop-blur-sm text-popover-foreground
```
with:
```
border border-dropdown-border bg-dropdown-bg text-dropdown-fg backdrop-blur-sm
```

Also update `SelectTrigger` — find:
```
border border-input bg-card dark:bg-input/30
```
Replace with:
```
border border-dropdown-border bg-dropdown-bg text-dropdown-fg dark:bg-dropdown-bg
```

- [ ] **Step 3: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep -E "dropdown-menu|select.tsx"
```

Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add src/components/ui/dropdown-menu.tsx src/components/ui/select.tsx
git commit -m "feat(theme): wire dropdown and select to VSCode 2026 tokens"
```

---

### Task 5: Update Badge primitive

**Files:**
- Modify: `src/components/ui/badge.tsx`

- [ ] **Step 1: Update badge default variant**

Find the `default` variant:
```ts
default: 'border-transparent bg-primary text-primary-foreground shadow hover:bg-primary/80',
```

Replace with:
```ts
default: 'border-transparent bg-badge-bg text-badge-fg shadow hover:bg-badge-bg/90',
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep badge.tsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/badge.tsx
git commit -m "feat(theme): wire badge default variant to VSCode 2026 tokens"
```
