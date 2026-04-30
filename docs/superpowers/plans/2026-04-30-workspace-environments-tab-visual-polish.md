# WorkspaceEnvironmentsTab Visual Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove visual noise from `WorkspaceEnvironmentsTab` by reducing borders, improving spacing, switching to CSS grid for equal-width fields, and replacing the custom button-as-checkbox with the shadcn `Checkbox` primitive.

**Architecture:** All changes are confined to a single component file. The left panel loses its outer border and button-bar separator. The right panel's `Card` wrapper is removed. Variable rows switch from `flex` to `grid` with `grid-template-columns: 20px 1fr 1fr 52px` so Key and Value fields are always equal width. The enabled toggle is replaced with the shadcn `Checkbox` primitive.

**Tech Stack:** React, TypeScript, Tailwind CSS, shadcn/ui (`Checkbox`)

---

### Task 1: Fix left panel borders and button bar

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

The left panel currently has `border border-border` (a full four-sided border) plus a right-side divider, causing a double-border where the panels meet. The button bar at the bottom has an unnecessary `border-t`.

- [ ] **Step 1: Open the file and locate the left panel div**

  In `src/components/workspace/WorkspaceEnvironmentsTab.tsx`, find line ~153:
  ```tsx
  <div className='w-52 border border-border flex flex-col bg-card/50'>
  ```

- [ ] **Step 2: Replace the full border with a right-side divider only**

  Change to:
  ```tsx
  <div className='w-52 border-r border-border flex flex-col bg-card/50'>
  ```

- [ ] **Step 3: Remove the border-t from the button bar**

  Find line ~190:
  ```tsx
  <div className='p-2 border-t border-border/60 flex gap-1'>
  ```

  Change to:
  ```tsx
  <div className='p-2 flex gap-1'>
  ```

- [ ] **Step 4: Verify TypeScript compiles**

  ```bash
  yarn tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
  git commit -m "fix(ui): remove double-border and button-bar separator from env tab left panel"
  ```

---

### Task 2: Remove Card wrapper from right panel

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

The right panel is wrapped in `Card` / `CardContent` with `rounded-none` and `.border-0` (a no-op class). This adds an extra card surface. Replace with a plain `div`. Also remove `border-t border-border` from the outer right panel div — the left panel's right-side border already provides the visual separation.

- [ ] **Step 1: Remove the Card import if it becomes unused**

  First check whether `Card` or `CardContent` are used anywhere else in this file. They are not — they only appear in the right panel. Remove them from the import line at the top:

  Change:
  ```tsx
  import { Card, CardContent } from '@/components/ui/card';
  ```
  To: delete this line entirely.

- [ ] **Step 2: Replace the Card wrapper with a plain div**

  Find the right panel section starting at line ~214:
  ```tsx
  {/* Right panel: variable editor. */}
  <div className='flex-1 flex flex-col min-w-0 border-t border-border'>
    {selectedName ? (
      <Card className='flex-1 flex flex-col min-w-0 overflow-hidden .border-0 rounded-none'>
        <CardContent className='p-0 flex flex-col h-full'>
  ```

  Replace with:
  ```tsx
  {/* Right panel: variable editor. */}
  <div className='flex-1 flex flex-col min-w-0'>
    {selectedName ? (
      <div className='flex-1 flex flex-col min-w-0 overflow-hidden'>
        <div className='p-0 flex flex-col h-full'>
  ```

- [ ] **Step 3: Close the replaced tags at the bottom of the selected-env branch**

  Find the closing tags (around line ~333):
  ```tsx
        </CardContent>
      </Card>
  ```

  Replace with:
  ```tsx
        </div>
      </div>
  ```

- [ ] **Step 4: Remove the border-b from the column header row**

  Find (around line ~219):
  ```tsx
  <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0'>
  ```

  Change to:
  ```tsx
  <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 shrink-0'>
  ```

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  yarn tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
  git commit -m "fix(ui): remove Card wrapper and redundant borders from env tab right panel"
  ```

---

### Task 3: Switch variable rows to CSS grid for equal-width fields

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

Variable rows and column headers currently use `flex`. Replace with `grid-template-columns: 20px 1fr 1fr 52px` so Key and Value fields always receive identical computed width. Also increase row height from `h-7` to `h-8` for better vertical rhythm.

- [ ] **Step 1: Update the column header row to use grid**

  Find (around line ~219, after the border-b was removed in Task 2):
  ```tsx
  <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 shrink-0'>
    {/* checkbox placeholder */}
    <div className='w-4 shrink-0' />
    <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
      Key
    </p>
    <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
      Value
    </p>
    <div className='w-[52px] shrink-0' />
  </div>
  ```

  Replace with:
  ```tsx
  <div className='grid items-center gap-1.5 px-3 pt-3 pb-1.5 shrink-0' style={{ gridTemplateColumns: '20px 1fr 1fr 52px' }}>
    <div />
    <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
      Key
    </p>
    <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
      Value
    </p>
    <div />
  </div>
  ```

- [ ] **Step 2: Update each variable row to use grid**

  Find the variable row div (around line ~234):
  ```tsx
  <div
    // biome-ignore lint/suspicious/noArrayIndexKey: env variables may share keys; index is the correct identity
    key={idx}
    className={cn(
      'flex gap-1.5 items-center py-0.5 group',
      !variable.enabled && 'opacity-50',
    )}
  >
  ```

  Replace with:
  ```tsx
  <div
    // biome-ignore lint/suspicious/noArrayIndexKey: env variables may share keys; index is the correct identity
    key={idx}
    className={cn(
      'grid items-center gap-1.5 h-8 group',
      !variable.enabled && 'opacity-50',
    )}
    style={{ gridTemplateColumns: '20px 1fr 1fr 52px' }}
  >
  ```

- [ ] **Step 3: Remove shrink-0 from inputs (no longer needed in grid)**

  In the Key input (around line ~259):
  ```tsx
  <Input
    placeholder='Key'
    value={variable.key}
    onChange={(e) => updateVar(idx, { key: e.target.value })}
    className='flex-1 text-xs h-7 font-mono'
  />
  ```

  Change to:
  ```tsx
  <Input
    placeholder='Key'
    value={variable.key}
    onChange={(e) => updateVar(idx, { key: e.target.value })}
    className='text-xs h-7 font-mono'
  />
  ```

  In the Value input (around line ~267):
  ```tsx
  <Input
    placeholder='Value'
    type={variable.secret ? 'password' : 'text'}
    value={variable.value}
    onChange={(e) => updateVar(idx, { value: e.target.value })}
    className='flex-1 text-xs h-7 font-mono'
  />
  ```

  Change to:
  ```tsx
  <Input
    placeholder='Value'
    type={variable.secret ? 'password' : 'text'}
    value={variable.value}
    onChange={(e) => updateVar(idx, { value: e.target.value })}
    className='text-xs h-7 font-mono'
  />
  ```

- [ ] **Step 4: Wrap the secret toggle and delete buttons in a single div that occupies the 52px column**

  The action buttons currently sit as two sibling elements in the row. They need to share the 52px grid cell:

  Find (around line ~276):
  ```tsx
  {/* Secret toggle. */}
  <Button
    variant='ghost'
    size='icon'
    className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
    onClick={() => updateVar(idx, { secret: !variable.secret })}
    title={variable.secret ? 'Show value' : 'Hide value'}
  >
    {variable.secret ? (
      <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
    ) : (
      <Eye className='h-3.5 w-3.5 text-muted-foreground' />
    )}
  </Button>

  {/* Delete row. */}
  <Button
    variant='ghost'
    size='icon'
    className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
    onClick={() => removeVar(idx)}
    title='Delete variable'
  >
    <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
  </Button>
  ```

  Replace with:
  ```tsx
  {/* Secret toggle + delete row. */}
  <div className='flex items-center gap-1 justify-end'>
    <Button
      variant='ghost'
      size='icon'
      className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
      onClick={() => updateVar(idx, { secret: !variable.secret })}
      title={variable.secret ? 'Show value' : 'Hide value'}
    >
      {variable.secret ? (
        <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
      ) : (
        <Eye className='h-3.5 w-3.5 text-muted-foreground' />
      )}
    </Button>
    <Button
      variant='ghost'
      size='icon'
      className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
      onClick={() => removeVar(idx)}
      title='Delete variable'
    >
      <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
    </Button>
  </div>
  ```

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  yarn tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
  git commit -m "fix(ui): use CSS grid for equal-width key/value fields in env tab"
  ```

---

### Task 4: Replace custom Button-as-checkbox with shadcn Checkbox

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

The enabled toggle is currently a `Button` component styled to mimic a checkbox. Replace it with the shadcn `Checkbox` primitive to match `EnvironmentDialog`.

- [ ] **Step 1: Add Checkbox to the import list**

  Find the existing import block at the top. Add `Checkbox`:
  ```tsx
  import { Checkbox } from '@/components/ui/checkbox';
  ```

- [ ] **Step 2: Remove the Check icon import if it is only used by the toggle**

  Search the file for all uses of `Check`. It is used in two places: the enabled toggle and the save button's success state. Keep the `Check` import — it is still needed for the save button.

- [ ] **Step 3: Replace the Button-as-checkbox with Checkbox**

  Find (around line ~243):
  ```tsx
  {/* Enabled toggle. */}
  <Button
    variant='ghost'
    size='icon'
    onClick={() => updateVar(idx, { enabled: !variable.enabled })}
    className={cn(
      'w-4 h-4 rounded border p-0 shrink-0',
      variable.enabled
        ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
        : 'border-border hover:bg-muted',
    )}
    title={variable.enabled ? 'Disable variable' : 'Enable variable'}
  >
    {variable.enabled && <Check className='h-3 w-3' />}
  </Button>
  ```

  Replace with:
  ```tsx
  {/* Enabled toggle. */}
  <Checkbox
    checked={variable.enabled}
    onCheckedChange={(checked) => updateVar(idx, { enabled: !!checked })}
    aria-label={variable.enabled ? 'Disable variable' : 'Enable variable'}
    className='shrink-0'
  />
  ```

- [ ] **Step 4: Verify TypeScript compiles and biome is clean**

  ```bash
  yarn tsc --noEmit && yarn check
  ```
  Expected: no errors, no lint warnings.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
  git commit -m "fix(ui): replace custom button-as-checkbox with shadcn Checkbox in env tab"
  ```

---

### Task 5: Final verification

- [ ] **Step 1: Run TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 2: Run biome lint**

  ```bash
  yarn check
  ```
  Expected: no errors or warnings.

- [ ] **Step 3: Run the app and visually verify both themes**

  ```bash
  yarn tauri dev
  ```

  Open Workspace Settings → Environments tab. Verify:
  - Left panel has a single right-side divider, no outer border, no separator above the add/delete buttons.
  - Right panel has no card surface or rounded corners.
  - Column header labels align directly above their respective input fields.
  - Key and Value inputs are visually equal in width.
  - Enabled toggle renders as a standard shadcn checkbox.
  - Hover on a row reveals the eye and delete buttons as before.
  - Toggle theme (light ↔ dark) — layout looks correct in both.
  - Save button still enables/disables correctly on changes.
