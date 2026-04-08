# Save Feedback UX — Design Spec

**Date:** 2026-04-08
**Status:** Approved

## Problem

Save actions across the app give no acknowledgement to the user. The Save button stays enabled and unchanged before and after saving, so users have no way to know whether their change was persisted. This affects Collection Variables, Collection Auth, Collection Settings, Workspace Environments, Environment Dialog, and Workspace doc.

---

## Three Feedback Patterns

### Pattern 1 — Explicit Save Button (`useSaveButton` hook)

**Applies to:**
- Collection Variables tab (CollectionOverviewTab.tsx, variables section)
- Collection Auth tab (CollectionOverviewTab.tsx, auth section)
- Collection Settings dialog (CollectionSettingsDialog.tsx)
- Workspace doc save (WorkspaceOverviewTab.tsx)

**Button state machine:** `idle → saving → success → idle`

| State | Icon | Label | Disabled | Style |
|---|---|---|---|---|
| `idle` | `Save` (Lucide) | Save | no | normal |
| `saving` | `Loader2` animate-spin | Save | yes | normal |
| `success` | `Check` | Saved | yes | green tint (text-green-600) |
| → `idle` after 2 s | `Save` | Save | no | normal |

- On **error**: `toast.error('Failed to save')` from Sonner; button returns to `idle` immediately.
- On **success**: no toast — the button state is the confirmation.

**Shared hook — `useSaveButton`:**

```ts
// src/hooks/use-save-button.ts
type SaveButtonState = 'idle' | 'saving' | 'success';

function useSaveButton(fn: () => Promise<void>, errorMessage = 'Failed to save') {
  const [state, setState] = useState<SaveButtonState>('idle');

  const trigger = useCallback(async () => {
    if (state !== 'idle') return;
    setState('saving');
    try {
      await fn();
      setState('success');
      setTimeout(() => setState('idle'), 2000);
    } catch (err) {
      console.error(err);
      toast.error(errorMessage);
      setState('idle');
    }
  }, [fn, state, errorMessage]);

  return { state, trigger };
}
```

**Button rendering:**

```tsx
const { state, trigger } = useSaveButton(saveSettings, 'Failed to save settings');

<Button
  size='sm'
  onClick={trigger}
  disabled={state !== 'idle'}
  className={cn('gap-1.5', state === 'success' && 'text-green-600')}
>
  {state === 'saving' ? (
    <Loader2 className='h-3.5 w-3.5 animate-spin' />
  ) : state === 'success' ? (
    <Check className='h-3.5 w-3.5' />
  ) : (
    <Save className='h-3.5 w-3.5' />
  )}
  {state === 'success' ? 'Saved' : 'Save'}
</Button>
```

---

### Pattern 2 — Auto-save Status Pill

**Applies to:**
- Workspace Environments tab (WorkspaceEnvironmentsTab.tsx)
- Environment Dialog (EnvironmentDialog.tsx)

Both components auto-save via a 400–500 ms debounce on every variable change. There is no explicit Save button and none will be added.

**Behaviour:**
- After each debounced write resolves successfully, a muted pill **"All changes saved"** with a checkmark appears in the section header.
- The pill fades out after 3 s.
- On error: `toast.error('Failed to save changes')` — the pill does not appear.

**Implementation:**

A local `savedAt` timestamp state drives visibility:

```tsx
const [savedAt, setSavedAt] = useState<number | null>(null);

// After successful debounced write:
setSavedAt(Date.now());

// Pill (shown for 3s after savedAt):
{savedAt && <SavedPill key={savedAt} />}
```

`SavedPill` is a tiny component that self-removes via a CSS fade-out or `useEffect` timeout:

```tsx
function SavedPill() {
  return (
    <span className='flex items-center gap-1 text-xs text-muted-foreground animate-fade-out'>
      <Check className='h-3 w-3' />
      All changes saved
    </span>
  );
}
```

The `key={savedAt}` ensures re-mount (and re-animation) on each save.

---

### Pattern 3 — Create Actions (no change)

Create Collection (CollectionsSidebar.tsx) and Create Environment (EnvironmentSwitcher.tsx) remain silent on success. The new item appearing in the list is sufficient confirmation. Submit buttons already disable during the request. No changes required.

---

## Files to Modify

| File | Change |
|---|---|
| `src/hooks/use-save-button.ts` | **New file** — `useSaveButton` hook |
| `src/components/collections/CollectionOverviewTab.tsx` | Replace manual `saving` state on variables + auth save buttons with `useSaveButton` |
| `src/components/collections/CollectionSettingsDialog.tsx` | Add `useSaveButton` to Save button |
| `src/components/workspace/WorkspaceOverviewTab.tsx` | Add `useSaveButton` to doc Save button |
| `src/components/workspace/WorkspaceEnvironmentsTab.tsx` | Add `SavedPill` + `savedAt` state wired to debounced write |
| `src/components/environments/EnvironmentDialog.tsx` | Add `SavedPill` + `savedAt` state wired to debounced write |

---

## Out of Scope

- `SaveRequestButton` and `RequestVariablesPanel` — both have existing non-standard patterns and are not part of this change.
- `CreateRequestDialog` and `SaveToCollectionDialog` — dialog-scoped flows, not included.
- Adding explicit Save buttons to auto-save sections.
- Any changes to create actions (collection, environment).

---

## Success Criteria

1. Clicking Save on Collection Variables, Auth, Collection Settings, or Workspace doc shows a spinner during save, then a green "Saved" label for 2 s.
2. A failed save on any of the above shows `toast.error(...)` and the button returns to idle.
3. After editing a variable in Workspace Environments or Environment Dialog, "All changes saved" appears briefly in the section header once the debounce write completes.
4. A failed auto-save shows `toast.error(...)`.
5. Create actions (collection, environment) behave unchanged.
6. No regressions in existing `cargo check`, `yarn tsc --noEmit`, `yarn check`.
