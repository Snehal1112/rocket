# Sandbox Developer Mode Frontend UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repoint `SandboxPopover.tsx` from the cosmetic global `localStorage` toggle it reads today to the active collection's real, persisted `sandbox_mode` setting (wired end-to-end by the previous three plans in this sequence), with a confirmation gate before enabling Developer Mode.

**Architecture:** `SandboxPopover` reads the currently active collection from `usePaneStore` (the same store `GitToolbarButton`, its sibling in the same toolbar, already uses for exactly this purpose) and fetches/saves that collection's `sandboxMode` via the existing `getCollectionSettings`/`saveCollectionSettings` IPC calls — no new Tauri command, since `CollectionSettings` already carries every other per-collection setting this way. Enabling Developer Mode routes through a shadcn `AlertDialog` (the same controlled-dialog pattern `GitLandingPanel.tsx`'s force-push confirmation already uses) before it saves; disabling back to Safe Mode saves immediately. The old global `src/stores/sandbox-store.ts` is deleted — nothing else depends on it.

**Tech Stack:** React 18, TypeScript, Zustand (`usePaneStore`), shadcn/ui (`Popover`, `AlertDialog`, `Button`), Vitest, React Testing Library, `@testing-library/user-event`.

**Spec:** `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md`

## Global Constraints

- All UI components use shadcn/ui primitives only — the confirmation dialog is `AlertDialog`, not a raw `confirm()` or a hand-rolled modal (spec §5).
- Zustand: never fully destructure store state at component top level — use a narrow selector (`usePaneStore((s) => s.activeCollection)`), matching `GitToolbarButton.tsx`'s existing pattern exactly.
- No new Tauri command — `sandboxMode` rides the existing `get_collection_settings`/`save_collection_settings` IPC path (spec §5).
- **`saveCollectionSettings` is a full replace, not a merge, despite its `Partial<CollectionSettings>` TypeScript type.** Verified by controller investigation (2026-09-21, during Plan 1's final review): `save_collection_settings` (`src-tauri/src/commands/collections.rs:232-239`) takes the whole `CollectionSettings` struct and writes it wholesale — any field the caller omits from the JSON payload deserializes via `#[serde(default)]` to its Rust default and overwrites whatever was on disk. **Every `saveCollectionSettings` call in this plan must send the complete settings object** (spread the most recently loaded/known settings, then override only the field being changed) — never a bare `{ sandboxMode: ... }` literal. This constraint shaped Tasks 1-3 below; do not "simplify" a save call back to a partial literal during implementation.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

**Next Plan:** None — this is the final plan (4 of 4) in the sandbox-developer-mode sequence. Once it is implemented and verified, the sequence is complete.

---

### Task 1: Delete the global store, extend the TS type

**Files:**
- Delete: `src/stores/sandbox-store.ts`
- Delete: `src/stores/__tests__/sandbox-store.test.ts`
- Modify: `src/lib/tauri-api.ts:59-64` (`CollectionSettings` interface)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:252-260` (`saveSettings` callback — pre-existing bug fix, see Step 2)

**Interfaces:**
- Produces: `SandboxMode` (`'safe' | 'developer'`, exported from `src/lib/tauri-api.ts`), `CollectionSettings.sandboxMode: SandboxMode` — consumed by Tasks 2-3 (this plan).

- [ ] **Step 1: Confirm nothing else depends on the old store, then delete it**

Run: `grep -rln "sandbox-store\|useSandboxStore" src/` — expected output is exactly two files: `src/components/layout/SandboxPopover.tsx` (updated in Task 2, not this task) and `src/stores/sandbox-store.ts` itself. If anything else appears, stop — this task's premise (nothing else depends on the old store) doesn't hold, and the plan needs revisiting before proceeding.

```bash
rm src/stores/sandbox-store.ts src/stores/__tests__/sandbox-store.test.ts
```

- [ ] **Step 2: Add `SandboxMode` and extend `CollectionSettings`**

In `src/lib/tauri-api.ts`, find:

```ts
export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
}
```

Replace with:

```ts
export type SandboxMode = 'safe' | 'developer';

export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
  sandboxMode: SandboxMode;
}
```

Run: `yarn tsc --noEmit`
Expected: PASS — no file constructs a full `CollectionSettings` object literal that `sandboxMode` being required would break (`saveCollectionSettings`'s parameter type is `Partial<CollectionSettings>`, so TypeScript itself won't flag a call site that omits the field — that's a runtime data-loss risk, not a compile error, and it's real: `CollectionOverviewTab.tsx`'s save call omits `sandboxMode` today, which under `save_collection_settings`'s full-replace semantics (see this plan's Global Constraints) silently resets it to `Safe` on every save from that tab. Fix that call site now, in this same step, while you're already touching this type.

In `src/components/collections/CollectionOverviewTab.tsx`, find the `saveSettings` callback:

```tsx
  const saveSettings = useCallback(async () => {
    await saveCollectionSettings(collectionName, {
      auth: toPersistedAuth(auth),
      headers: toPersistedHeaders(headers),
      docs: docs || undefined,
      variables,
    });
    setIsDirty(false);
  }, [collectionName, auth, headers, docs, variables]);
```

Replace with:

```tsx
  const saveSettings = useCallback(async () => {
    await saveCollectionSettings(collectionName, {
      auth: toPersistedAuth(auth),
      headers: toPersistedHeaders(headers),
      docs: docs || undefined,
      variables,
      sandboxMode: collection?.settings.sandboxMode ?? 'safe',
    });
    setIsDirty(false);
  }, [collectionName, auth, headers, docs, variables, collection]);
```

(`collection` is this component's existing `useState<Collection | null>` holding the last-loaded collection, set in the `getCollection(collectionName).then(...)` effect a few lines above — it already carries `.settings.sandboxMode` once the type change above lands, since `getCollection`'s Rust-side response includes every `CollectionSettings` field. This preserves whatever sandbox mode was last saved — including a change made concurrently via `SandboxPopover` — rather than resetting it to `Safe` every time this tab's Save button is used.)

Run: `yarn tsc --noEmit` again — expect PASS.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add -A src/stores/sandbox-store.ts src/stores/__tests__/sandbox-store.test.ts src/lib/tauri-api.ts src/components/collections/CollectionOverviewTab.tsx
```

(The `-A` flag here is only to stage the two deletions correctly alongside the modified file — `git add` on a path that no longer exists on disk stages the deletion.)

Commit message along the lines of: `feat(sandbox): add SandboxMode type, remove cosmetic global store`.

---

### Task 2: Repoint `SandboxPopover` at the active collection

**Files:**
- Modify: `src/components/layout/SandboxPopover.tsx` (full rewrite of the data source; JSX structure/styling unchanged except for the new "no collection open" state)
- Test: `src/components/layout/__tests__/SandboxPopover.test.tsx` (new file)

**Interfaces:**
- Consumes: `SandboxMode`, `CollectionSettings` (Task 1), `usePaneStore` (existing), `getCollectionSettings`/`saveCollectionSettings` (existing).
- Produces: nothing new — `SandboxPopover` keeps its existing no-argument export signature, so `WorkspaceToolbar.tsx` (its only caller) needs no change.

- [ ] **Step 1: Write the failing tests**

Create `src/components/layout/__tests__/SandboxPopover.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SandboxPopover } from '@/components/layout/SandboxPopover';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSettings: vi.fn(),
    saveCollectionSettings: vi.fn(),
  };
});

function baseSettings(overrides: Partial<tauriApi.CollectionSettings> = {}): tauriApi.CollectionSettings {
  return { headers: [], variables: [], sandboxMode: 'safe', ...overrides };
}

describe('SandboxPopover', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.getCollectionSettings).mockReset();
    vi.mocked(tauriApi.saveCollectionSettings).mockReset();
  });

  it('is disabled with no active collection', () => {
    render(<SandboxPopover />);
    expect(screen.getByRole('button', { name: /JavaScript Sandbox/i })).toBeDisabled();
    expect(tauriApi.getCollectionSettings).not.toHaveBeenCalled();
  });

  it('loads and displays the active collection\'s sandbox mode', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(baseSettings({ sandboxMode: 'developer' }));

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalledWith('my-api'));

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    const popover = await screen.findByText('JavaScript Sandbox', { selector: 'p' });
    const dot = within(popover.closest('div')!.parentElement!.parentElement!).queryByText('Developer Mode');
    expect(dot).toBeInTheDocument();
  });

  it('saves immediately when switching to Safe Mode, preserving the rest of the loaded settings', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'developer', docs: 'hello' }),
    );
    vi.mocked(tauriApi.saveCollectionSettings).mockResolvedValue(undefined);

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Safe Mode'));

    // Must send the whole settings object back, not a bare `{ sandboxMode }` literal —
    // saveCollectionSettings is a full replace on the backend (see this plan's Global
    // Constraints), so a partial payload would silently wipe `docs`/`headers`/etc.
    await waitFor(() =>
      expect(tauriApi.saveCollectionSettings).toHaveBeenCalledWith(
        'my-api',
        baseSettings({ sandboxMode: 'safe', docs: 'hello' }),
      ),
    );
  });
});
```

Run: `yarn test SandboxPopover`
Expected: FAIL — `SandboxPopover` still imports the deleted `useSandboxStore` (compile/import error), and doesn't read `activeCollection` or call `getCollectionSettings`/`saveCollectionSettings` at all yet.

- [ ] **Step 2: Rewrite the component**

Replace the entire contents of `src/components/layout/SandboxPopover.tsx`:

```tsx
import { Lock, ShieldCheck, Unlock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import {
  type CollectionSettings,
  getCollectionSettings,
  saveCollectionSettings,
  type SandboxMode,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

export function SandboxPopover() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  // Holds the FULL loaded settings, not just the mode — saveCollectionSettings is a
  // full replace on the backend (see this plan's Global Constraints), so every save
  // below must spread this object rather than send a bare `{ sandboxMode }` literal.
  const [settings, setSettings] = useState<CollectionSettings | null>(null);
  const mode: SandboxMode = settings?.sandboxMode ?? 'safe';

  useEffect(() => {
    if (!activeCollection) {
      setSettings(null);
      return;
    }
    let cancelled = false;
    void getCollectionSettings(activeCollection).then((loaded) => {
      if (!cancelled) setSettings(loaded);
    });
    return () => {
      cancelled = true;
    };
  }, [activeCollection]);

  async function selectSafeMode() {
    if (!activeCollection || !settings) return;
    const next: CollectionSettings = { ...settings, sandboxMode: 'safe' };
    await saveCollectionSettings(activeCollection, next);
    setSettings(next);
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7 hover:bg-toolbar-hover'
          title='JavaScript Sandbox'
          aria-label='JavaScript Sandbox'
          disabled={!activeCollection}
        >
          <ShieldCheck
            fill='currentColor'
            className={cn(
              'h-4 w-4 transition-colors duration-200',
              mode === 'safe'
                ? 'text-green-500 dark:text-green-400'
                : 'text-amber-500 dark:text-amber-400',
            )}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-68 p-0 overflow-hidden' align='end'>
        {!activeCollection ? (
          <div className='px-4 py-3 text-[11px] text-muted-foreground'>
            Open a collection to configure its sandbox mode.
          </div>
        ) : (
          <>
            {/* Header */}
            <div className='flex items-center gap-2 px-4 py-2.5 border-b border-border/60'>
              <ShieldCheck
                className={cn(
                  'h-3 w-3 shrink-0',
                  mode === 'safe'
                    ? 'text-green-500 dark:text-green-400'
                    : 'text-amber-500 dark:text-amber-400',
                )}
              />
              <p className='text-[11px] font-semibold tracking-wider uppercase text-muted-foreground'>
                JavaScript Sandbox
              </p>
            </div>

            {/* Mode options */}
            <div className='p-1.5 space-y-0.5'>
              {/* Safe Mode */}
              <button
                type='button'
                onClick={() => void selectSafeMode()}
                className={cn(
                  'w-full rounded-md p-2.5 text-left transition-all duration-150 group border',
                  mode === 'safe'
                    ? 'border-green-500/30 dark:border-green-400/20 bg-green-500/5 dark:bg-green-400/5'
                    : 'border-transparent hover:border-border hover:bg-accent/50',
                )}
              >
                <div className='flex items-start gap-2.5'>
                  <div
                    className={cn(
                      'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded transition-colors',
                      mode === 'safe'
                        ? 'bg-green-500/15 dark:bg-green-400/10'
                        : 'bg-muted group-hover:bg-muted/70',
                    )}
                  >
                    <Lock
                      className={cn(
                        'h-2.5 w-2.5',
                        mode === 'safe'
                          ? 'text-green-500 dark:text-green-400'
                          : 'text-muted-foreground',
                      )}
                    />
                  </div>
                  <div className='flex-1 min-w-0'>
                    <div className='flex items-center gap-1.5 mb-0.5'>
                      <span className='text-[13px] font-medium text-foreground'>Safe Mode</span>
                      <span className='text-[9px] font-semibold tracking-wide uppercase text-green-600 dark:text-green-400 bg-green-500/10 dark:bg-green-500/15 px-1.5 py-px rounded-sm'>
                        Default
                      </span>
                    </div>
                    <p className='text-[11px] leading-relaxed text-muted-foreground'>
                      Sandboxed. No filesystem or system access.
                    </p>
                  </div>
                  {mode === 'safe' && (
                    <div className='mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-green-500 dark:bg-green-400' />
                  )}
                </div>
              </button>

              {/* Developer Mode */}
              <button
                type='button'
                className={cn(
                  'w-full rounded-md p-2.5 text-left transition-all duration-150 group border',
                  mode === 'developer'
                    ? 'border-amber-500/30 dark:border-amber-400/20 bg-amber-500/5 dark:bg-amber-400/5'
                    : 'border-transparent hover:border-border hover:bg-accent/50',
                )}
              >
                <div className='flex items-start gap-2.5'>
                  <div
                    className={cn(
                      'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded transition-colors',
                      mode === 'developer'
                        ? 'bg-amber-500/15 dark:bg-amber-400/10'
                        : 'bg-muted group-hover:bg-muted/70',
                    )}
                  >
                    <Unlock
                      className={cn(
                        'h-2.5 w-2.5',
                        mode === 'developer'
                          ? 'text-amber-500 dark:text-amber-400'
                          : 'text-muted-foreground',
                      )}
                    />
                  </div>
                  <div className='flex-1 min-w-0'>
                    <div className='flex items-center gap-1.5 mb-0.5'>
                      <span className='text-[13px] font-medium text-foreground'>Developer Mode</span>
                    </div>
                    <p className='text-[11px] leading-relaxed text-muted-foreground'>
                      Full filesystem and system command access.
                    </p>
                  </div>
                  {mode === 'developer' && (
                    <div className='mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 dark:bg-amber-400' />
                  )}
                </div>
              </button>
            </div>

            {/* Warning footer — only visible in developer mode */}
            {mode === 'developer' && (
              <div className='mx-1.5 mb-1.5 rounded border border-amber-500/20 dark:border-amber-400/15 bg-amber-500/8 dark:bg-amber-400/8 px-3 py-2'>
                <p className='text-[10px] leading-relaxed text-amber-600 dark:text-amber-400'>
                  Only enable for collections from trusted authors.
                </p>
              </div>
            )}
          </>
        )}
      </PopoverContent>
    </Popover>
  );
}
```

Note: the Developer Mode `<button>` deliberately has no `onClick` yet in this task — Task 3 adds the confirmation-dialog trigger. Clicking it right now does nothing, which is an intentional, temporary mid-sequence state within this task, not a bug to chase — Task 3 completes it in the same plan.

Run: `yarn test SandboxPopover`
Expected: the "disabled with no active collection" and "loads and displays" tests PASS; the "saves immediately when switching to Safe Mode, preserving the rest of the loaded settings" test PASSES too (Safe Mode's `onClick` is wired in this step). All three should be green after this step.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/layout/SandboxPopover.tsx src/components/layout/__tests__/SandboxPopover.test.tsx
```

Commit message along the lines of: `feat(sandbox): read/write SandboxPopover from the active collection's settings`.

---

### Task 3: Confirmation dialog for enabling Developer Mode

**Files:**
- Modify: `src/components/layout/SandboxPopover.tsx` (add `AlertDialog`, wire Developer Mode's `onClick`)
- Modify: `src/components/layout/__tests__/SandboxPopover.test.tsx` (add tests)

**Interfaces:**
- Consumes: `AlertDialog`/`AlertDialogAction`/`AlertDialogCancel`/`AlertDialogContent`/`AlertDialogDescription`/`AlertDialogFooter`/`AlertDialogHeader`/`AlertDialogTitle` from `@/components/ui/alert-dialog` (existing shadcn primitive, same one `GitLandingPanel.tsx`'s force-push confirmation already uses).

- [ ] **Step 1: Write the failing tests**

In `src/components/layout/__tests__/SandboxPopover.test.tsx`, add these tests inside the existing `describe('SandboxPopover', ...)` block, after `'saves immediately when switching to Safe Mode, preserving the rest of the loaded settings'`:

```tsx
  it('requires confirmation before enabling Developer Mode, and does not save on cancel', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(baseSettings({ sandboxMode: 'safe' }));

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    expect(tauriApi.saveCollectionSettings).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: /Cancel/i }));
    expect(tauriApi.saveCollectionSettings).not.toHaveBeenCalled();
  });

  it('saves Developer Mode only after the confirmation dialog is accepted, preserving the rest of the loaded settings', async () => {
    usePaneStore.setState({ activeCollection: 'my-api' });
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      baseSettings({ sandboxMode: 'safe', docs: 'hello' }),
    );
    vi.mocked(tauriApi.saveCollectionSettings).mockResolvedValue(undefined);

    render(<SandboxPopover />);
    await waitFor(() => expect(tauriApi.getCollectionSettings).toHaveBeenCalled());

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /JavaScript Sandbox/i }));
    await user.click(await screen.findByText('Developer Mode'));

    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /Enable/i }));

    // Same full-replace concern as Safe Mode's save above — must send the complete
    // settings object, not a bare `{ sandboxMode }` literal.
    await waitFor(() =>
      expect(tauriApi.saveCollectionSettings).toHaveBeenCalledWith(
        'my-api',
        baseSettings({ sandboxMode: 'developer', docs: 'hello' }),
      ),
    );
  });
```

Run: `yarn test SandboxPopover`
Expected: FAIL — the Developer Mode button has no `onClick` yet, so no dialog ever opens; `screen.findByRole('alertdialog')` times out.

- [ ] **Step 2: Wire the dialog**

In `src/components/layout/SandboxPopover.tsx`, find the import block (note: this reflects the actual output of Task 2 after Biome's import-sort auto-fix ran on commit — `cn` sorts after the `tauri-api` import by path, and `SandboxMode`/`saveCollectionSettings` swap order within the named-import group — if what you see differs from this, trust the file on disk and adapt the same logical change rather than this literal text):

```tsx
import { Lock, ShieldCheck, Unlock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  type CollectionSettings,
  getCollectionSettings,
  type SandboxMode,
  saveCollectionSettings,
} from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
```

Replace with:

```tsx
import { Lock, ShieldCheck, Unlock } from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  type CollectionSettings,
  getCollectionSettings,
  type SandboxMode,
  saveCollectionSettings,
} from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
```

Find:

```tsx
  async function selectSafeMode() {
    if (!activeCollection || !settings) return;
    const next: CollectionSettings = { ...settings, sandboxMode: 'safe' };
    await saveCollectionSettings(activeCollection, next);
    setSettings(next);
  }
```

Replace with:

```tsx
  async function selectSafeMode() {
    if (!activeCollection || !settings) return;
    const next: CollectionSettings = { ...settings, sandboxMode: 'safe' };
    await saveCollectionSettings(activeCollection, next);
    setSettings(next);
  }

  const [showDevConfirm, setShowDevConfirm] = useState(false);

  async function confirmDeveloperMode() {
    if (!activeCollection || !settings) return;
    const next: CollectionSettings = { ...settings, sandboxMode: 'developer' };
    await saveCollectionSettings(activeCollection, next);
    setSettings(next);
    setShowDevConfirm(false);
  }
```

Find the Developer Mode `<button>`:

```tsx
              {/* Developer Mode */}
              <button
                type='button'
                className={cn(
```

Replace with:

```tsx
              {/* Developer Mode */}
              <button
                type='button'
                onClick={() => setShowDevConfirm(true)}
                className={cn(
```

Find the closing of the component's returned JSX:

```tsx
          </>
        )}
      </PopoverContent>
    </Popover>
  );
}
```

Replace with:

```tsx
          </>
        )}
      </PopoverContent>
      <AlertDialog open={showDevConfirm} onOpenChange={setShowDevConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Enable Developer Mode?</AlertDialogTitle>
            <AlertDialogDescription>
              Scripts in this collection will get real filesystem read/write and command
              execution access on your machine — no restrictions on which files or commands.
              Only enable this for a collection you wrote yourself or trust completely; an
              imported collection can carry a script that does anything a normal program on
              your machine could do.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDeveloperMode()}>Enable</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Popover>
  );
}
```

Run: `yarn test SandboxPopover`
Expected: PASS (all 5 tests). Then run `yarn tsc --noEmit` and `yarn check` to confirm no type errors and no lint/format issues in the rewritten component and its test file.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/layout/SandboxPopover.tsx src/components/layout/__tests__/SandboxPopover.test.tsx
```

Commit message along the lines of: `feat(sandbox): confirm before enabling Developer Mode`.

---

## Final verification (after all 3 tasks)

- [ ] Run `yarn test SandboxPopover` — expect PASS (5 tests).
- [ ] Run `yarn tsc --noEmit` — expect PASS.
- [ ] Run `yarn check` — expect PASS (no new lint/format issues).
- [ ] Run `cargo test --workspace -j 4` — expect PASS (confirms the whole sequence, backend and frontend, is green together).
- [ ] Manual smoke test (`yarn tauri dev`): open a collection, click the sandbox icon in the toolbar, switch to Developer Mode (confirm the dialog appears and blocks the save until accepted), write a script using `fs.writeFile`/`fs.readFile`/`process.exec`, confirm it works. Switch back to Safe Mode, confirm the same script now fails with `fs is not defined`. Close the collection (or check one with no collection open) and confirm the sandbox icon is disabled.
