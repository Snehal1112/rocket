# Environment Switcher Dual Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the teal-dot global environment indicator in the EnvironmentSwitcher trigger button with a dual-segment display showing both the active collection environment (Database icon) and the active global environment (Globe icon).

**Architecture:** Single file change — modify the `<Button>` content inside `<PopoverTrigger>` in `EnvironmentSwitcher.tsx`. Add a `Database` icon import and replace the current teal-dot + single-name JSX with a four-state conditional that renders one or two icon+name segments. No behavior, popover, or store changes.

**Tech Stack:** React, TypeScript, Tailwind CSS, lucide-react

---

### Task 1: Update the trigger button to show dual environment segments

**Files:**
- Modify: `src/components/layout/EnvironmentSwitcher.tsx:1` (import line)
- Modify: `src/components/layout/EnvironmentSwitcher.tsx:112-123` (Button content)

This is a visual-only change. There are no unit-testable logic paths added — validation is done by running the app and checking all four states.

- [ ] **Step 1: Add `Database` to the lucide-react import**

Open `src/components/layout/EnvironmentSwitcher.tsx`. Line 1 currently reads:

```tsx
import { Check, ChevronDown, Globe, Plus, Settings } from 'lucide-react';
```

Replace with:

```tsx
import { Check, ChevronDown, Database, Globe, Plus, Settings } from 'lucide-react';
```

- [ ] **Step 2: Replace the Button content**

Lines 112–123 currently read:

```tsx
<Button variant='ghost' size='sm' className='h-7 gap-1.5 px-2 text-xs'>
  {globalEnvName && (
    <span
      className='h-2 w-2 rounded-full bg-teal-500 shrink-0'
      title={`Global: ${globalEnvName}`}
    />
  )}
  <span className={cn(!activeEnvId && 'text-muted-foreground')}>
    {activeEnvId ?? 'No Environment'}
  </span>
  <ChevronDown className='h-3 w-3 opacity-50' />
</Button>
```

Replace with:

```tsx
<Button variant='ghost' size='sm' className='h-7 gap-2.5 px-2 text-xs'>
  {activeEnvId && (
    <span className='flex items-center gap-1'>
      <Database className='h-3 w-3 text-muted-foreground shrink-0' />
      <span className='max-w-[80px] truncate'>{activeEnvId}</span>
    </span>
  )}
  {globalEnvName && (
    <span className='flex items-center gap-1'>
      <Globe className='h-3 w-3 text-muted-foreground shrink-0' />
      <span className='max-w-[80px] truncate'>{globalEnvName}</span>
    </span>
  )}
  {!activeEnvId && !globalEnvName && (
    <span className='text-muted-foreground'>No Environment</span>
  )}
  <ChevronDown className='h-3 w-3 opacity-50' />
</Button>
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Lint check**

```bash
yarn check
```

Expected: no errors or warnings.

- [ ] **Step 5: Visual verification**

Run the app and verify all four states:

```bash
yarn tauri dev
```

Check each state in the toolbar:

| State | How to reach | Expected trigger |
|---|---|---|
| Both active | Select a collection env AND a global env | `[Database] EnvName  [Globe] GlobalName  [v]` |
| Collection only | Select a collection env, deselect global | `[Database] EnvName  [v]` |
| Global only | Deselect collection env, keep global | `[Globe] GlobalName  [v]` |
| Neither | Deselect both | `No Environment  [v]` |

- [ ] **Step 6: Commit**

```bash
git add src/components/layout/EnvironmentSwitcher.tsx
git commit -m "feat(ux): show collection and global env names in switcher trigger"
```
