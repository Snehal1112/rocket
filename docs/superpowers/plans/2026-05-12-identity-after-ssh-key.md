# Identity Setup After SSH Key Change Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a user saves SSH key credentials in `GitCredentialsDialog`, automatically prompt them to confirm/update their git commit identity (name + email) before the push/pull is retried — so commits always show the correct GitHub author.

**Architecture:** The orchestration lives in `git-store` (not in `GitCredentialsDialog`). When `setCredentials` is called with `sshKey` type, the store fetches the current identity via `gitGetIdentity`, stores pre-populated values, and raises a new `showIdentitySetupDialog` flag. `GitPanel` observes this flag and renders `GitIdentityDialog`; on confirm it calls `gitSetIdentity` then `activatePendingCredentials` (which activates the credentials and retries the pending push/pull). `GitIdentityDialog` gains optional `initialName`, `initialEmail`, and `confirmLabel` props — backward-compatible with its existing use in `GitCommitForm`.

**Tech Stack:** TypeScript, React, Zustand (`git-store`), Tauri IPC (`gitGetIdentity`, `gitSetIdentity`), shadcn/ui, Vitest

---

## File Map

| File | Change |
|---|---|
| `src/components/git/GitIdentityDialog.tsx` | Add `initialName?`, `initialEmail?`, `confirmLabel?` props + sync `useEffect` |
| `src/stores/git-store.ts` | Add 4 state fields + `activatePendingCredentials` action + modify `setCredentials` for SSH key flow |
| `src/components/git/GitPanel.tsx` | Import `GitIdentityDialog` + `gitSetIdentity`; render identity setup dialog from store state |
| `src/stores/__tests__/git-store.test.ts` | Add `gitGetIdentity` mock + 5 new tests for identity setup flow |

---

## Task 1: Make GitIdentityDialog reusable

**Files:**
- Modify: `src/components/git/GitIdentityDialog.tsx`

`GitIdentityDialog` currently always starts with empty fields and its button always says "Save & Commit". We need it to accept pre-populated values and a configurable button label for use outside the commit flow — without breaking `GitCommitForm`.

- [ ] **Step 1: Write failing type-check**

Run:
```bash
yarn tsc --noEmit 2>&1 | head -5
```
Expected: passes. This confirms baseline before our change.

- [ ] **Step 2: Update the component**

Replace the entire contents of `src/components/git/GitIdentityDialog.tsx` with:

```tsx
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface Props {
  open: boolean;
  onConfirm: (name: string, email: string) => void;
  onCancel: () => void;
  initialName?: string;
  initialEmail?: string;
  confirmLabel?: string;
}

export function GitIdentityDialog({
  open,
  onConfirm,
  onCancel,
  initialName = '',
  initialEmail = '',
  confirmLabel = 'Save & Commit',
}: Props) {
  const [name, setName] = useState(initialName);
  const [email, setEmail] = useState(initialEmail);

  // Sync fields each time the dialog opens so pre-populated values are fresh.
  useEffect(() => {
    if (open) {
      setName(initialName);
      setEmail(initialEmail);
    }
  }, [open]);

  const isValid = name.trim().length > 0 && email.includes('@');

  const handleConfirm = () => {
    if (!isValid) return;
    onConfirm(name.trim(), email.trim());
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) onCancel();
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className='w-auto min-w-[22rem] max-w-[min(90vw,_36rem)]'>
        <DialogHeader>
          <DialogTitle>Git Author Identity</DialogTitle>
        </DialogHeader>

        <p className='text-sm text-muted-foreground'>
          Git needs your name and email to record commit authorship.
        </p>

        <div className='space-y-3'>
          <div>
            <Label htmlFor='git-identity-name' className='text-sm'>
              Name
            </Label>
            <Input
              id='git-identity-name'
              value={name}
              onChange={(e) => setName(e.target.value)}
              className='h-8 text-sm'
              placeholder='Your Name'
              autoComplete='name'
              autoFocus
            />
          </div>

          <div>
            <Label htmlFor='git-identity-email' className='text-sm'>
              Email
            </Label>
            <Input
              id='git-identity-email'
              type='email'
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className='h-8 text-sm'
              placeholder='you@example.com'
              autoComplete='email'
              onKeyDown={(e) => {
                if (e.key === 'Enter' && isValid) handleConfirm();
              }}
            />
          </div>

          <div className='flex gap-2'>
            <Button onClick={handleConfirm} disabled={!isValid} className='flex-1' size='sm'>
              {confirmLabel}
            </Button>
            <Button onClick={onCancel} variant='outline' className='flex-1' size='sm'>
              Cancel
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Verify type-check still passes**

```bash
yarn tsc --noEmit 2>&1 | head -10
```
Expected: zero new errors. `GitCommitForm` passes no optional props so it keeps current behaviour (empty fields, "Save & Commit" label).

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitIdentityDialog.tsx
git commit -m "feat: add initialName/initialEmail/confirmLabel props to GitIdentityDialog"
```

---

## Task 2: Add identity setup state and flow to git-store

**Files:**
- Modify: `src/stores/git-store.ts`
- Test: `src/stores/__tests__/git-store.test.ts`

The store gains 4 new state fields, one new action (`activatePendingCredentials`), and a modified `setCredentials` that intercepts SSH key credentials to trigger the identity setup flow.

- [ ] **Step 1: Write failing tests first**

Add `gitGetIdentity: vi.fn().mockResolvedValue({ name: 'Test User', email: 'test@example.com' })` to the existing `vi.mock('@/lib/tauri-api', ...)` block in `src/stores/__tests__/git-store.test.ts`.

Then add a new `describe` block at the bottom of the test file:

```typescript
describe('git-store identity setup flow', () => {
  beforeEach(() => {
    useGitStore.setState({
      collectionPath: null,
      credentials: null,
      showCredentialsDialog: false,
      showIdentitySetupDialog: false,
      identitySetupInitialName: '',
      identitySetupInitialEmail: '',
      pendingCredentialsForIdentitySetup: null,
      pendingNetworkOp: null,
      remotes: [],
      error: null,
    });
    vi.clearAllMocks();
  });

  it('setCredentials with sshKey and collectionPath shows identity setup dialog', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockResolvedValue({ name: 'Snehal', email: 'snehal@example.com' });
    useGitStore.setState({ collectionPath: '/some/repo' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = useGitStore.getState();
    expect(state.credentials).toBeNull();
    expect(state.showCredentialsDialog).toBe(false);
    expect(state.showIdentitySetupDialog).toBe(true);
    expect(state.pendingCredentialsForIdentitySetup).toEqual(creds);
    expect(state.identitySetupInitialName).toBe('Snehal');
    expect(state.identitySetupInitialEmail).toBe('snehal@example.com');
  });

  it('setCredentials with sshKey but no collectionPath activates immediately', () => {
    useGitStore.setState({ collectionPath: null });
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with token creds activates immediately without identity dialog', () => {
    useGitStore.setState({ collectionPath: '/some/repo' });
    const creds: GitCredentials = { type: 'token', token: 'ghp_xxx' };

    useGitStore.getState().setCredentials(creds);

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('setCredentials with sshKey falls back to immediate activation when gitGetIdentity throws', async () => {
    const { gitGetIdentity } = await import('@/lib/tauri-api');
    vi.mocked(gitGetIdentity).mockRejectedValue(new Error('no repo'));
    useGitStore.setState({ collectionPath: '/some/repo' });

    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.getState().setCredentials(creds);
    await new Promise((r) => setTimeout(r, 0));

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
  });

  it('activatePendingCredentials sets credentials and clears identity setup state', () => {
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.setState({
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      identitySetupInitialName: 'Snehal',
      identitySetupInitialEmail: 'snehal@example.com',
      pendingNetworkOp: null,
    });

    useGitStore.getState().activatePendingCredentials();

    const state = useGitStore.getState();
    expect(state.credentials).toEqual(creds);
    expect(state.showIdentitySetupDialog).toBe(false);
    expect(state.pendingCredentialsForIdentitySetup).toBeNull();
    expect(state.identitySetupInitialName).toBe('');
    expect(state.identitySetupInitialEmail).toBe('');
  });

  it('activatePendingCredentials retries pending push after activating', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockResolvedValue(undefined);
    const creds: GitCredentials = { type: 'sshKey', privateKeyPath: '~/.ssh/id_ed25519' };
    useGitStore.setState({
      collectionPath: '/some/repo',
      pendingCredentialsForIdentitySetup: creds,
      showIdentitySetupDialog: true,
      pendingNetworkOp: 'push',
      remotes: [{ name: 'origin', url: 'git@github.com:test/test.git' }],
    });

    useGitStore.getState().activatePendingCredentials();
    await new Promise((r) => setTimeout(r, 0));

    expect(vi.mocked(gitPush)).toHaveBeenCalledWith('/some/repo', 'origin', creds);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
yarn test git-store 2>&1 | grep -E "identity setup|FAIL|×" | head -15
```
Expected: 6 new test failures (state fields + actions not yet defined).

- [ ] **Step 3: Implement the store changes**

In `src/stores/git-store.ts`, make these changes:

**A. Add `gitGetIdentity` to the import from `@/lib/tauri-api`:**

Find the existing import block from `@/lib/tauri-api` and add `gitGetIdentity` to it.

**B. Add 4 new fields to the `GitState` interface** (after the `pendingNetworkOp` line):

```typescript
showIdentitySetupDialog: boolean;
identitySetupInitialName: string;
identitySetupInitialEmail: string;
pendingCredentialsForIdentitySetup: GitCredentials | null;
```

**C. Add `activatePendingCredentials` to the `GitState` interface** (after `clearPendingNetworkOp`):

```typescript
activatePendingCredentials: () => void;
```

**D. Add initial values** in the `create` call (after `pendingNetworkOp: null`):

```typescript
showIdentitySetupDialog: false,
identitySetupInitialName: '',
identitySetupInitialEmail: '',
pendingCredentialsForIdentitySetup: null,
```

**E. Replace the existing `setCredentials` action** with:

```typescript
setCredentials: (creds) => {
  const { pendingNetworkOp, collectionPath } = get();

  // SSH key: prompt user to confirm/update git identity before activating.
  if (creds.type === 'sshKey' && collectionPath) {
    gitGetIdentity(collectionPath)
      .then((identity) => {
        set({
          showCredentialsDialog: false,
          pendingCredentialsForIdentitySetup: creds,
          showIdentitySetupDialog: true,
          identitySetupInitialName: identity.name,
          identitySetupInitialEmail: identity.email,
        });
      })
      .catch(() => {
        // Identity fetch failed — activate creds immediately rather than blocking.
        set({ credentials: creds, showCredentialsDialog: false, pendingNetworkOp: null });
        if (pendingNetworkOp) get()[pendingNetworkOp]();
      });
    return;
  }

  // All other credential types: activate immediately.
  set({ credentials: creds, showCredentialsDialog: false, pendingNetworkOp: null });
  if (pendingNetworkOp) get()[pendingNetworkOp]();
},
```

**F. Add `activatePendingCredentials` action** (after `clearPendingNetworkOp`):

```typescript
activatePendingCredentials: () => {
  const { pendingCredentialsForIdentitySetup, pendingNetworkOp } = get();
  const creds = pendingCredentialsForIdentitySetup;
  set({
    credentials: creds,
    showIdentitySetupDialog: false,
    pendingCredentialsForIdentitySetup: null,
    identitySetupInitialName: '',
    identitySetupInitialEmail: '',
    pendingNetworkOp: null,
  });
  if (pendingNetworkOp && creds) {
    get()[pendingNetworkOp]();
  }
},
```

**G. Update `reset`** — add the 4 new fields to the `set({...})` call inside `reset`:

```typescript
showIdentitySetupDialog: false,
identitySetupInitialName: '',
identitySetupInitialEmail: '',
pendingCredentialsForIdentitySetup: null,
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
yarn test git-store 2>&1 | tail -15
```
Expected: all tests pass including the 6 new ones.

- [ ] **Step 5: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
git commit -m "feat: trigger identity setup dialog after SSH key credential change"
```

---

## Task 3: Render identity setup dialog in GitPanel

**Files:**
- Modify: `src/components/git/GitPanel.tsx`

`GitPanel` already renders `GitCredentialsDialog` when `showCredentialsDialog` is true. We add the identity setup dialog alongside it, driven by `showIdentitySetupDialog` from the store.

- [ ] **Step 1: Add imports**

In `src/components/git/GitPanel.tsx`, add these two imports alongside the existing git imports:

```typescript
import { GitIdentityDialog } from '@/components/git/GitIdentityDialog';
import { gitSetIdentity } from '@/lib/tauri-api';
```

- [ ] **Step 2: Extend the useGitStore destructure**

The existing destructure (starting around line 51) includes `showCredentialsDialog`. Add 4 more fields:

```typescript
showIdentitySetupDialog,
identitySetupInitialName,
identitySetupInitialEmail,
activatePendingCredentials,
```

- [ ] **Step 3: Add the two handlers**

Inside the `GitPanel` function body, after the existing local state declarations, add:

```typescript
const handleIdentitySetupConfirm = async (name: string, email: string) => {
  try {
    await gitSetIdentity(collectionPath, name, email);
  } catch {
    // Non-blocking — don't prevent the push from being retried if identity save fails.
  }
  activatePendingCredentials();
};

const handleIdentitySetupCancel = () => {
  activatePendingCredentials();
};
```

- [ ] **Step 4: Render the dialog — first occurrence (line ~178)**

The file has `{showCredentialsDialog && <GitCredentialsDialog />}` at two locations. At **each** location, add the identity dialog immediately after:

```tsx
{showCredentialsDialog && <GitCredentialsDialog />}
{showIdentitySetupDialog && (
  <GitIdentityDialog
    open={showIdentitySetupDialog}
    onConfirm={handleIdentitySetupConfirm}
    onCancel={handleIdentitySetupCancel}
    initialName={identitySetupInitialName}
    initialEmail={identitySetupInitialEmail}
    confirmLabel='Save Identity'
  />
)}
```

Do this at both occurrences of `{showCredentialsDialog && <GitCredentialsDialog />}` in the file.

- [ ] **Step 5: Type-check**

```bash
yarn tsc --noEmit 2>&1 | head -10
```
Expected: zero errors.

- [ ] **Step 6: Run full test suite**

```bash
yarn test 2>&1 | tail -10
```
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/components/git/GitPanel.tsx
git commit -m "feat: render identity setup dialog in GitPanel after SSH key change"
```

---

## Task 4: Manual smoke test

- [ ] **Step 1: Start the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Open Git panel for a collection with a remote**

Open a collection whose git remote uses SSH (`git@github.com:...`).

- [ ] **Step 3: Trigger credentials dialog**

Click Push (or Pull) in the Git panel. The Credentials dialog should open.

- [ ] **Step 4: Select SSH Key, browse to your snehaldangroshiya key, click Connect**

After clicking Connect, the Credentials dialog should close and the **Git Author Identity** dialog should open, pre-populated with the current identity (e.g. `Snehal Dangroshiya` / `Snehaldangroshiya@gmail.com`).

- [ ] **Step 5: Update the email to match the snehaldangroshiya GitHub account**

Change the email to the one registered with your `snehaldangroshiya` GitHub account, then click **Save Identity**.

- [ ] **Step 6: Verify push proceeds and commit shows correct author**

The push should proceed automatically. Check GitHub — the next commit should show `snehaldangroshiya` as the author.

- [ ] **Step 7: Verify Cancel doesn't block the push**

Repeat the flow, but click Cancel on the identity dialog. The push should still proceed (identity is skipped, not required).

---

## Self-Review

**Spec coverage:**
- ✅ Identity dialog shows after SSH key change — `setCredentials` triggers it for `sshKey` type
- ✅ Dialog pre-populated with current identity — `gitGetIdentity` fetched before showing
- ✅ Identity saved to repo-local `.git/config` on confirm — `gitSetIdentity(collectionPath, ...)` in `GitPanel`
- ✅ Push/pull still retried after identity confirm/cancel — `activatePendingCredentials` calls `get()[pendingNetworkOp]()`
- ✅ Non-SSH credentials unaffected — `token`, `userPass`, `sshAgent` activate immediately
- ✅ No collection path = no identity dialog — `if (creds.type === 'sshKey' && collectionPath)` guard
- ✅ `gitGetIdentity` failure = graceful fallback — `.catch()` activates creds immediately
- ✅ `GitCommitForm` unchanged — new props are optional, default values preserve existing behaviour
- ✅ Single responsibility: `GitCredentialsDialog` only saves credentials; store orchestrates the flow; `GitPanel` handles IPC side-effects

**Placeholder scan:** None — all steps contain full code.

**Type consistency:**
- `activatePendingCredentials: () => void` — matches across interface (Task 2) and usage in `GitPanel` (Task 3)
- `showIdentitySetupDialog`, `identitySetupInitialName`, `identitySetupInitialEmail`, `pendingCredentialsForIdentitySetup` — same names used in interface, initial state, actions, and test assertions
- `confirmLabel='Save Identity'` in `GitPanel` — matches `confirmLabel?: string` prop added in Task 1
