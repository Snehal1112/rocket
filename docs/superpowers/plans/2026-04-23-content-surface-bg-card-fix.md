# Content Surface bg-card Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `bg-background` with `bg-card` on all content-level surfaces so they appear elevated above the shell/chrome after the `--background`/`--card` layer swap.

**Architecture:** Pure class replacements across 8 files. No token changes, no layout restructuring. The rule: shell chrome → `bg-background`, content surfaces → `bg-card`.

**Tech Stack:** React/TypeScript, Tailwind CSS, shadcn/ui

---

### Task 1: Fix active tab elevation

**Files:**
- Modify: `src/components/panes/TabItem.tsx:60`
- Modify: `src/components/panes/TabBar.tsx:104`

- [ ] **Step 1: Read both files to confirm current classes**

```bash
grep -n "bg-background" src/components/panes/TabItem.tsx src/components/panes/TabBar.tsx
```

Expected:
```
src/components/panes/TabItem.tsx:60:          ? 'bg-background/95 border-b-2 border-b-primary -mb-px text-foreground'
src/components/panes/TabBar.tsx:104:                <div className='flex items-center px-2 py-1 border-r border-border/70 bg-background/95 border-b-2 border-b-primary -mb-px shrink-0'>
```

- [ ] **Step 2: Fix TabItem.tsx active tab class**

In `src/components/panes/TabItem.tsx`, line 60, change:
```tsx
          ? 'bg-background/95 border-b-2 border-b-primary -mb-px text-foreground'
```
to:
```tsx
          ? 'bg-card border-b-2 border-b-primary -mb-px text-foreground'
```

- [ ] **Step 3: Fix TabBar.tsx rename input tab class**

In `src/components/panes/TabBar.tsx`, line 104, change:
```tsx
                <div className='flex items-center px-2 py-1 border-r border-border/70 bg-background/95 border-b-2 border-b-primary -mb-px shrink-0'>
```
to:
```tsx
                <div className='flex items-center px-2 py-1 border-r border-border/70 bg-card border-b-2 border-b-primary -mb-px shrink-0'>
```

- [ ] **Step 4: Verify**

```bash
grep -n "bg-background" src/components/panes/TabItem.tsx src/components/panes/TabBar.tsx
```

Expected: no output (both instances replaced).

- [ ] **Step 5: Run type check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/panes/TabItem.tsx src/components/panes/TabBar.tsx
git commit -m "fix(tabs): use bg-card for active tab elevation after layer swap"
```

---

### Task 2: Fix response viewer toolbars

**Files:**
- Modify: `src/components/response/ResponseBodyViewer.tsx:234,284`

- [ ] **Step 1: Read the file to confirm current classes**

```bash
grep -n "bg-background" src/components/response/ResponseBodyViewer.tsx
```

Expected:
```
234:      <div className='flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0 bg-background'>
284:      <div className='flex items-center border-b border-border/70 px-1 shrink-0 bg-background'>
```

- [ ] **Step 2: Replace both instances**

In `src/components/response/ResponseBodyViewer.tsx`:

Line 234, change:
```tsx
      <div className='flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0 bg-background'>
```
to:
```tsx
      <div className='flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0 bg-card'>
```

Line 284, change:
```tsx
      <div className='flex items-center border-b border-border/70 px-1 shrink-0 bg-background'>
```
to:
```tsx
      <div className='flex items-center border-b border-border/70 px-1 shrink-0 bg-card'>
```

- [ ] **Step 3: Verify**

```bash
grep -n "bg-background" src/components/response/ResponseBodyViewer.tsx
```

Expected: no output.

- [ ] **Step 4: Run type check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/response/ResponseBodyViewer.tsx
git commit -m "fix(response): use bg-card for response viewer toolbars"
```

---

### Task 3: Fix contract tab panels

**Files:**
- Modify: `src/components/contract/ContractTab.tsx:215,247,274,298`
- Modify: `src/components/contract/ContractTabTopBar.tsx:22`

- [ ] **Step 1: Read both files to confirm current classes**

```bash
grep -n "bg-background" src/components/contract/ContractTab.tsx src/components/contract/ContractTabTopBar.tsx
```

Expected:
```
src/components/contract/ContractTab.tsx:215:      <div className='flex flex-col h-full bg-background'>
src/components/contract/ContractTab.tsx:247:      <div className='flex flex-col h-full bg-background'>
src/components/contract/ContractTab.tsx:274:        <div className='flex items-center justify-end gap-2 px-6 py-3 border-t border-border bg-background shrink-0'>
src/components/contract/ContractTab.tsx:298:      <div className='flex flex-col h-full bg-background'>
src/components/contract/ContractTabTopBar.tsx:22:    <div className='flex items-center justify-between px-5 py-3 border-b border-border shrink-0 bg-background'>
```

- [ ] **Step 2: Replace all instances in ContractTab.tsx**

Replace all 4 occurrences of `bg-background` with `bg-card` in `src/components/contract/ContractTab.tsx`:

Line 215:
```tsx
      <div className='flex flex-col h-full bg-card'>
```
Line 247:
```tsx
      <div className='flex flex-col h-full bg-card'>
```
Line 274:
```tsx
        <div className='flex items-center justify-end gap-2 px-6 py-3 border-t border-border bg-card shrink-0'>
```
Line 298:
```tsx
      <div className='flex flex-col h-full bg-card'>
```

- [ ] **Step 3: Replace in ContractTabTopBar.tsx**

Line 22:
```tsx
    <div className='flex items-center justify-between px-5 py-3 border-b border-border shrink-0 bg-card'>
```

- [ ] **Step 4: Verify**

```bash
grep -n "bg-background" src/components/contract/ContractTab.tsx src/components/contract/ContractTabTopBar.tsx
```

Expected: no output.

- [ ] **Step 5: Run type check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/contract/ContractTab.tsx src/components/contract/ContractTabTopBar.tsx
git commit -m "fix(contract): use bg-card for contract panel surfaces"
```

---

### Task 4: Fix audit log panel

**Files:**
- Modify: `src/components/audit/AuditLogTab.tsx:43,107`

- [ ] **Step 1: Read the file to confirm current classes**

```bash
grep -n "bg-background" src/components/audit/AuditLogTab.tsx
```

Expected:
```
43:    <div className='h-full flex flex-col bg-background'>
107:            <thead className='text-[10px] uppercase tracking-wider text-muted-foreground border-b border-border/60 sticky top-0 bg-background'>
```

- [ ] **Step 2: Replace both instances**

Line 43:
```tsx
    <div className='h-full flex flex-col bg-card'>
```

Line 107:
```tsx
            <thead className='text-[10px] uppercase tracking-wider text-muted-foreground border-b border-border/60 sticky top-0 bg-card'>
```

- [ ] **Step 3: Verify**

```bash
grep -n "bg-background" src/components/audit/AuditLogTab.tsx
```

Expected: no output.

- [ ] **Step 4: Run type check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/audit/AuditLogTab.tsx
git commit -m "fix(audit): use bg-card for audit log panel surface"
```

---

### Task 5: Fix dialogs and alert dialogs

**Files:**
- Modify: `src/components/ui/dialog.tsx`
- Modify: `src/components/ui/alert-dialog.tsx`

- [ ] **Step 1: Read both files to confirm current classes**

```bash
grep -n "bg-background" src/components/ui/dialog.tsx src/components/ui/alert-dialog.tsx
```

Expected:
```
src/components/ui/dialog.tsx:38:          'fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg ... bg-background ...',
src/components/ui/alert-dialog.tsx:36:          'fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg ... bg-background ...',
```

- [ ] **Step 2: Replace in dialog.tsx**

In `src/components/ui/dialog.tsx`, in the `DialogContent` className, replace `bg-background` with `bg-card`.

- [ ] **Step 3: Replace in alert-dialog.tsx**

In `src/components/ui/alert-dialog.tsx`, in the `AlertDialogContent` className, replace `bg-background` with `bg-card`.

- [ ] **Step 4: Verify**

```bash
grep -n "bg-background" src/components/ui/dialog.tsx src/components/ui/alert-dialog.tsx
```

Expected: no output.

- [ ] **Step 5: Run type check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/ui/dialog.tsx src/components/ui/alert-dialog.tsx
git commit -m "fix(ui): use bg-card for dialog and alert-dialog surfaces"
```
