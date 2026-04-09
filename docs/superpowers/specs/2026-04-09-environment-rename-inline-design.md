# Environment Inline Rename Design

## Goal

Allow users to rename collection and global environment names by double-clicking the name in the left panel list. A text input with accept (✓) and cancel (✗) buttons replaces the name row while editing.

## Problem

Neither `EnvironmentDialog` (collection envs) nor `WorkspaceEnvironmentsTab` (global envs) expose any rename affordance. The only way to rename is to delete and recreate the environment, losing nothing but requiring manual re-entry.

## Approved Approach

Frontend-only copy+delete: save a new env file with the new name, delete the old file. No new Rust or Tauri commands needed. Acceptable risk for a local tool.

---

## Component: `InlineEnvName`

**File**: `src/components/environments/InlineEnvName.tsx` (new)

**Props**:
```tsx
interface InlineEnvNameProps {
  name: string;
  isSelected: boolean;
  onClick: () => void;
  onRename: (newName: string) => Promise<void>;
}
```

### Normal state

Renders the existing `<button>` element (same classes as the current env list buttons) with `onDoubleClick` added. Double-click calls `onClick` (selects the env) and enters edit mode in one gesture.

### Edit state

Replaces the button with an inline row:

```
[ input (pre-filled, autofocus) ] [✓] [✗]
```

- Input: `h-7 text-sm`, takes up remaining width
- ✓ button: `h-6 w-6` ghost icon button, `text-green-500`, `Check` icon from lucide-react
- ✗ button: `h-6 w-6` ghost icon button, `text-destructive`, `X` icon from lucide-react
- `Enter` key → accept
- `Escape` key → cancel
- `onBlur` on input → cancel
- ✓ button uses `onMouseDown={(e) => e.preventDefault()}` to prevent blur-before-click race before the `onClick` accept fires

### Accept logic

1. Trim the input value
2. If empty or equal to current name → cancel (no-op, exit edit mode)
3. If name already exists in the siblings list → `toast.warning('An environment with that name already exists')`, keep edit mode open
4. Otherwise → call `onRename(newName)`; on error keep edit mode open, on success exit edit mode

---

## Integration: `EnvironmentDialog.tsx`

**File**: `src/components/environments/EnvironmentDialog.tsx`

Replace the `<button>` inside `environments.map(...)` (lines 138–151) with `<InlineEnvName>`.

**`onRename` implementation**:
```tsx
async (newName: string) => {
  const env = environments.find((e) => e.name === oldName);
  if (!env || !activeCollection) return;
  await saveEnvironment(activeCollection, { ...env, name: newName });
  await deleteEnvironment(activeCollection, oldName);
  // Update store: swap env object.
  useEnvStore.setState((s) => ({
    environments: s.environments.map((e) => e.name === oldName ? { ...e, name: newName } : e),
    activeEnvId: s.activeEnvId === oldName ? newName : s.activeEnvId,
  }));
  setSelectedName(newName);
}
```

Error handling: wrap in try/catch, call `toast.error('Failed to rename environment')` on failure, keep edit mode open.

---

## Integration: `WorkspaceEnvironmentsTab.tsx`

**File**: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

Replace the `<button>` inside `environments.map(...)` (lines 151–165) with `<InlineEnvName>`.

**`onRename` implementation**:
```tsx
async (newName: string) => {
  const env = environments.find((e) => e.name === oldName);
  if (!env) return;
  await saveGlobalEnvironment({ ...env, name: newName });
  await deleteGlobalEnvironment(oldName);
  // Update store: swap env object and active global name.
  useEnvStore.setState((s) => ({
    globalEnvironments: s.globalEnvironments.map((e) => e.name === oldName ? { ...e, name: newName } : e),
    globalEnvName: s.globalEnvName === oldName ? newName : s.globalEnvName,
  }));
  setSelectedName(newName);
}
```

Error handling: same pattern — `toast.error` on failure, keep edit mode open.

---

## Files

| File | Action |
|---|---|
| `src/components/environments/InlineEnvName.tsx` | Create |
| `src/components/environments/EnvironmentDialog.tsx` | Modify (replace env name buttons) |
| `src/components/workspace/WorkspaceEnvironmentsTab.tsx` | Modify (replace env name buttons) |

No backend changes. No new Tauri commands.

---

## Out of Scope

- Rename from the `EnvironmentSwitcher` popover list (names are shown there but editing inline in a compact dropdown is awkward)
- Undo/redo for rename
- Renaming the environment that is currently open in a separate dialog/tab (the dialog already tracks by name so the `selectedName` update handles this)
