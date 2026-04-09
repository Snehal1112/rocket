# Environment Inline Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow users to rename collection and global environment names by double-clicking the name row — shows an inline input with ✓ accept and ✗ cancel buttons.

**Architecture:** A new shared `InlineEnvName` component handles the double-click-to-edit UX. Both `EnvironmentDialog` (collection envs) and `WorkspaceEnvironmentsTab` (global envs) replace their plain env-name `<button>` elements with this component. Rename persists via frontend copy+delete: write a new env file with the new name, delete the old one, then update the Zustand store in one `setState` call.

**Tech Stack:** React, TypeScript, Tailwind CSS, lucide-react, sonner (toasts), Zustand (`useEnvStore`), Tauri IPC (`saveEnvironment`, `deleteEnvironment`, `saveGlobalEnvironment`, `deleteGlobalEnvironment`)

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/components/environments/InlineEnvName.tsx` | Create | Shared double-click-to-rename row component |
| `src/components/environments/EnvironmentDialog.tsx` | Modify | Swap env name buttons → InlineEnvName; wire collection rename |
| `src/components/workspace/WorkspaceEnvironmentsTab.tsx` | Modify | Swap env name buttons → InlineEnvName; wire global rename |

---

### Task 1: Create the `InlineEnvName` component

**Files:**
- Create: `src/components/environments/InlineEnvName.tsx`

- [ ] **Step 1: Create the file with this exact content**

```tsx
// src/components/environments/InlineEnvName.tsx

import { Check, X } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

interface InlineEnvNameProps {
  name: string;
  isSelected: boolean;
  existingNames: string[];
  onClick: () => void;
  onRename: (newName: string) => Promise<void>;
}

export function InlineEnvName({
  name,
  isSelected,
  existingNames,
  onClick,
  onRename,
}: InlineEnvNameProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [draftName, setDraftName] = useState('');

  const enterEdit = () => {
    onClick();
    setDraftName(name);
    setIsEditing(true);
  };

  const cancel = () => {
    setIsEditing(false);
    setDraftName('');
  };

  const accept = async () => {
    const trimmed = draftName.trim();
    if (!trimmed || trimmed === name) {
      cancel();
      return;
    }
    if (existingNames.some((n) => n !== name && n === trimmed)) {
      toast.warning('An environment with that name already exists');
      return;
    }
    try {
      await onRename(trimmed);
      setIsEditing(false);
    } catch {
      // Keep edit mode open — caller shows the toast.
    }
  };

  if (isEditing) {
    return (
      <div className='flex items-center gap-1 px-1'>
        <Input
          autoFocus
          className='h-7 text-sm flex-1 min-w-0'
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void accept();
            if (e.key === 'Escape') cancel();
          }}
          onBlur={cancel}
        />
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-green-500 hover:text-green-600'
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => void accept()}
          title='Accept rename'
        >
          <Check className='h-3.5 w-3.5' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-destructive hover:text-destructive/80'
          onMouseDown={(e) => e.preventDefault()}
          onClick={cancel}
          title='Cancel rename'
        >
          <X className='h-3.5 w-3.5' />
        </Button>
      </div>
    );
  }

  return (
    <button
      type='button'
      onClick={onClick}
      onDoubleClick={enterEdit}
      className={cn(
        'w-full text-left px-2 py-1.5 text-sm rounded-sm truncate',
        isSelected
          ? 'bg-accent text-accent-foreground'
          : 'text-foreground hover:bg-muted/60',
      )}
    >
      {name}
    </button>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/environments/InlineEnvName.tsx
git commit -m "feat(ux): add InlineEnvName component for double-click rename"
```

---

### Task 2: Wire inline rename into `EnvironmentDialog` (collection envs)

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`

The env list button block is at lines 137–151. We need to:
- Import `InlineEnvName`
- Import `deleteEnvironment` from tauri-api (it is not currently imported; `saveEnvironment` already is)
- Replace the `<button>` with `<InlineEnvName>`
- Add a `handleRenameEnv` callback

- [ ] **Step 1: Update imports at the top of `EnvironmentDialog.tsx`**

Current line 13:
```tsx
import { saveEnvironment } from '@/lib/tauri-api';
```

Replace with:
```tsx
import { deleteEnvironment, saveEnvironment } from '@/lib/tauri-api';
import { InlineEnvName } from './InlineEnvName';
```

- [ ] **Step 2: Add the `handleRenameEnv` callback inside the component**

Add this block after the `handleDeleteEnv` callback (after line 58). The component already has `activeCollection` read from the store (line 63):

```tsx
const handleRenameEnv = useCallback(
  async (oldName: string, newName: string) => {
    const env = environments.find((e) => e.name === oldName);
    if (!env || !activeCollection) return;
    try {
      await saveEnvironment(activeCollection, { ...env, name: newName });
      await deleteEnvironment(activeCollection, oldName);
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) =>
          e.name === oldName ? { ...e, name: newName } : e,
        ),
        activeEnvId: s.activeEnvId === oldName ? newName : s.activeEnvId,
      }));
      setSelectedName(newName);
    } catch (err) {
      console.error('[EnvironmentDialog] rename failed:', err);
      toast.error('Failed to rename environment');
      throw err;
    }
  },
  [environments, activeCollection],
);
```

- [ ] **Step 3: Replace the `<button>` in the env list with `<InlineEnvName>`**

Current block (lines 137–151):
```tsx
{environments.map((env) => (
  <button
    key={env.name}
    type='button'
    onClick={() => setSelectedName(env.name)}
    className={cn(
      'w-full text-left px-2 py-1.5 text-sm rounded-sm truncate',
      selectedName === env.name
        ? 'bg-accent text-accent-foreground'
        : 'text-foreground hover:bg-muted/60',
    )}
  >
    {env.name}
  </button>
))}
```

Replace with:
```tsx
{environments.map((env) => (
  <InlineEnvName
    key={env.name}
    name={env.name}
    isSelected={selectedName === env.name}
    existingNames={environments.map((e) => e.name)}
    onClick={() => setSelectedName(env.name)}
    onRename={(newName) => handleRenameEnv(env.name, newName)}
  />
))}
```

- [ ] **Step 4: Remove the now-unused `cn` import if it is no longer used elsewhere in the file**

Search the file for remaining uses of `cn(`. If none remain, remove `cn` from the import on line 14:
```tsx
import { cn } from '@/lib/utils';
```

**Note:** `cn` may still be used in the right-panel variable editor — check before removing. If it is still used, leave the import as-is.

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Lint check**

```bash
yarn check
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/environments/EnvironmentDialog.tsx
git commit -m "feat(ux): inline rename for collection environments"
```

---

### Task 3: Wire inline rename into `WorkspaceEnvironmentsTab` (global envs)

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

The env list button block is at lines 151–165. We need to:
- Import `InlineEnvName`
- Import `saveGlobalEnvironment` and `deleteGlobalEnvironment` from tauri-api (currently only types are imported from tauri-api)
- Replace the `<button>` with `<InlineEnvName>`
- Add a `handleRenameEnv` callback

- [ ] **Step 1: Update imports at the top of `WorkspaceEnvironmentsTab.tsx`**

Current lines 10–11:
```tsx
import type { Environment, Variable } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
```

Replace with:
```tsx
import type { Environment, Variable } from '@/lib/tauri-api';
import { deleteGlobalEnvironment, saveGlobalEnvironment } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { InlineEnvName } from '@/components/environments/InlineEnvName';
```

- [ ] **Step 2: Add the `handleRenameEnv` callback inside the component**

Add this block after the `handleDeleteEnv` callback (after line 143):

```tsx
const handleRenameEnv = useCallback(
  async (oldName: string, newName: string) => {
    const env = environments.find((e) => e.name === oldName);
    if (!env) return;
    try {
      await saveGlobalEnvironment({ ...env, name: newName });
      await deleteGlobalEnvironment(oldName);
      useEnvStore.setState((s) => ({
        globalEnvironments: s.globalEnvironments.map((e) =>
          e.name === oldName ? { ...e, name: newName } : e,
        ),
        globalEnvName: s.globalEnvName === oldName ? newName : s.globalEnvName,
      }));
      setSelectedName(newName);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] rename failed:', err);
      toast.error('Failed to rename environment');
      throw err;
    }
  },
  [environments],
);
```

- [ ] **Step 3: Replace the `<button>` in the env list with `<InlineEnvName>`**

Current block (lines 151–165):
```tsx
{environments.map((env) => (
  <button
    key={env.name}
    type='button'
    onClick={() => setSelectedName(env.name)}
    className={cn(
      'w-full text-left px-2 py-1.5 text-sm rounded-sm truncate',
      selectedName === env.name
        ? 'bg-accent text-accent-foreground'
        : 'text-foreground hover:bg-muted/60',
    )}
  >
    {env.name}
  </button>
))}
```

Replace with:
```tsx
{environments.map((env) => (
  <InlineEnvName
    key={env.name}
    name={env.name}
    isSelected={selectedName === env.name}
    existingNames={environments.map((e) => e.name)}
    onClick={() => setSelectedName(env.name)}
    onRename={(newName) => handleRenameEnv(env.name, newName)}
  />
))}
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Lint check**

```bash
yarn check
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
git commit -m "feat(ux): inline rename for global environments"
```

---

## Manual verification checklist

Run the app:
```bash
yarn tauri dev
```

Test these scenarios in **EnvironmentDialog** (collection envs, opened via the gear icon in the env switcher):

| Scenario | Expected |
|---|---|
| Double-click an env name | Input appears with ✓ and ✗ buttons |
| Type new name, press Enter | Env renamed, list updates, edit mode exits |
| Type new name, click ✓ | Env renamed, list updates, edit mode exits |
| Press Escape | Edit cancelled, original name restored |
| Click ✗ button | Edit cancelled, original name restored |
| Click outside input (blur) | Edit cancelled, original name restored |
| Type existing env name, click ✓ | Warning toast, edit stays open |
| Clear the input, click ✓ | No-op, edit mode exits |
| Rename active env | `activeEnvId` updates; switcher trigger shows new name |
| Double-click unselected env | Env becomes selected AND enters edit mode |

Repeat all scenarios in **WorkspaceEnvironmentsTab** (global envs, opened via workspace settings → Environments tab).
